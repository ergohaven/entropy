//! `--export-layout` while the GUI is running: the console process asks the
//! running instance, which owns the keyboards, for the export, and writes the
//! file itself.
//!
//! The channel is a Unix socket in the user's runtime directory. It carries a
//! single command, and the GUI answers it by lending its connection, through a
//! read-only view, to the same pipeline a standalone export runs: whatever the
//! export asks for, only reads reach the keyboard, and nothing it loads
//! reaches the GUI's own state.

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::AtomicU64;
use std::sync::{mpsc, Arc};

const PROTOCOL_VERSION: u32 = 1;
/// A connected client has this long to send its request line, in all.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Far more than any request; a longer line is not one.
const MAX_REQUEST_BYTES: usize = 4096;
/// The longest export the GUI runs, whatever timeout the console asks for.
const MAX_EXPORT_TIME: Duration = Duration::from_secs(10 * 60);
/// How often a request waiting for the GUI checks that its console is there.
const HANGUP_CHECK_INTERVAL: Duration = Duration::from_millis(100);
/// A console has this long to take its answer.
const REPLY_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
enum Request {
    ExportLayout {
        version: u32,
        device: Option<String>,
        timeout_ms: u64,
    },
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub(crate) enum Reply {
    Exported { entlayout: String },
    NoDevice { message: String },
    Incomplete { missing: Vec<String> },
    Failed { message: String },
}

/// `$XDG_RUNTIME_DIR/entropy/export.sock`, or the config directory where the
/// platform has no runtime directory.
pub(crate) fn socket_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)?;
    Some(base.join("entropy").join("export.sock"))
}

/// One console request waiting for the GUI thread. `cancel` is set once its
/// console has gone or run out of time.
pub(crate) struct ExportJob {
    device: Option<String>,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
    reply: mpsc::Sender<Reply>,
}

/// The listening end, owned by the GUI.
pub(crate) struct ExportServer {
    jobs: mpsc::Receiver<ExportJob>,
}

impl ExportServer {
    /// Listens at `path`; each request wakes `ctx` so a backgrounded window
    /// answers without waiting for its next timer.
    pub(crate) fn start(path: &Path, ctx: egui::Context) -> std::io::Result<Self> {
        let listener = bind_private(path)?;
        let (sender, jobs) = mpsc::channel();
        std::thread::Builder::new()
            .name("entropy-export-ipc".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => serve(stream, &sender, &ctx),
                        Err(error) => log::warn!("Export request was not accepted: {error}"),
                    }
                }
            })?;
        Ok(Self { jobs })
    }
}

/// Binds in a directory only the user can enter, replacing a socket that a
/// crashed instance left behind but refusing to steal a live one.
fn bind_private(path: &Path) -> std::io::Result<UnixListener> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

fn serve(stream: UnixStream, jobs: &mpsc::Sender<ExportJob>, ctx: &egui::Context) {
    let read = read_request(&stream, Instant::now() + REQUEST_READ_TIMEOUT);
    let reply = match read {
        Err(error) => Reply::Failed {
            message: format!("request not read: {error}"),
        },
        Ok(line) => match serde_json::from_str::<Request>(&line) {
            Ok(Request::ExportLayout {
                version,
                device,
                timeout_ms,
            }) if version == PROTOCOL_VERSION => {
                let deadline =
                    Instant::now() + Duration::from_millis(timeout_ms).min(MAX_EXPORT_TIME);
                let cancel = Arc::new(AtomicBool::new(false));
                let (reply, answer) = mpsc::channel();
                if jobs
                    .send(ExportJob {
                        device,
                        deadline,
                        cancel: cancel.clone(),
                        reply,
                    })
                    .is_err()
                {
                    return;
                }
                ctx.request_repaint();
                match wait_for_answer(&stream, &answer, deadline) {
                    Some(reply) => reply,
                    None => {
                        // The console has gone, or its time is up: stop the
                        // export and free the channel for the next one.
                        cancel.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                        if Instant::now() < deadline {
                            return;
                        }
                        Reply::Failed {
                            message: "The export did not finish in time".to_owned(),
                        }
                    }
                }
            }
            Ok(Request::ExportLayout { version, .. }) => Reply::Failed {
                message: format!("unsupported export protocol version {version}"),
            },
            Err(error) => Reply::Failed {
                message: format!("unsupported request: {error}"),
            },
        },
    };
    let mut stream = stream;
    if let Ok(mut json) = serde_json::to_string(&reply) {
        json.push('\n');
        let _ = stream
            .set_write_timeout(Some(REPLY_WRITE_TIMEOUT))
            .and_then(|()| stream.write_all(json.as_bytes()));
    }
}

/// The request: one line, sent whole by `deadline` and at most
/// `MAX_REQUEST_BYTES` long, however its bytes trickle in.
fn read_request(stream: &UnixStream, deadline: Instant) -> std::io::Result<String> {
    let mut console = stream;
    let mut line = Vec::new();
    let mut chunk = [0; 256];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        console.set_read_timeout(Some(remaining))?;
        let read = match console.read(&mut chunk) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(std::io::ErrorKind::TimedOut.into())
            }
            read => read?,
        };
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        let end = chunk[..read].iter().position(|byte| *byte == b'\n');
        line.extend_from_slice(&chunk[..end.unwrap_or(read)]);
        if line.len() > MAX_REQUEST_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request too long",
            ));
        }
        if end.is_some() {
            return String::from_utf8(line)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error));
        }
    }
}

/// The GUI's answer; `None` once the deadline passes or the console hangs up
/// (or sends anything past its request).
fn wait_for_answer(
    stream: &UnixStream,
    answer: &mpsc::Receiver<Reply>,
    deadline: Instant,
) -> Option<Reply> {
    stream.set_read_timeout(Some(HANGUP_CHECK_INTERVAL)).ok()?;
    let mut console = stream;
    loop {
        match answer.try_recv() {
            Ok(reply) => return Some(reply),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Some(Reply::Failed {
                    message: "Entropy closed before the export finished".to_owned(),
                })
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            return None;
        }
        match console.read(&mut [0; 1]) {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            _ => return None,
        }
    }
}

/// Asks the running instance for the export and writes it. `None` when no
/// instance listens, so the caller keeps reporting a running instance.
pub(crate) fn export_via_running_instance(request: &HeadlessExportRequest) -> Option<i32> {
    export_via_socket(&socket_path()?, request)
}

pub(super) fn export_via_socket(path: &Path, request: &HeadlessExportRequest) -> Option<i32> {
    let deadline = Instant::now() + request.timeout;
    let mut stream = match connect_until(path, deadline) {
        Some(Ok(stream)) => stream,
        Some(Err(_)) => return None,
        None => {
            log::error!("The running Entropy did not take the export request in time");
            return Some(EXIT_INSTANCE_RUNNING);
        }
    };
    log::info!("Asking the running Entropy for the layout");
    let mut line = serde_json::to_string(&Request::ExportLayout {
        version: PROTOCOL_VERSION,
        device: request.device.clone(),
        timeout_ms: request.timeout.as_millis().try_into().unwrap_or(u64::MAX),
    })
    .expect("export request serializes");
    line.push('\n');
    let mut answer = String::new();
    // A zero timeout means none at all; keep at least a millisecond.
    let remaining = || {
        deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1))
    };
    let exchanged = stream
        .set_write_timeout(Some(remaining()))
        .and_then(|()| stream.write_all(line.as_bytes()))
        .and_then(|()| stream.set_read_timeout(Some(remaining())))
        .and_then(|()| BufReader::new(&stream).read_line(&mut answer));
    if let Err(error) = exchanged {
        log::error!("The running Entropy did not answer the export: {error}");
        return Some(EXIT_INSTANCE_RUNNING);
    }
    let reply = match serde_json::from_str::<Reply>(&answer) {
        Ok(reply) => reply,
        Err(error) => {
            log::error!("The running Entropy sent an unreadable answer: {error}");
            return Some(EXIT_FAILED);
        }
    };
    Some(match reply {
        Reply::Exported { entlayout } => match write_export(&request.destination, &entlayout) {
            Ok(destination) => {
                log::info!("Exported layout to {destination}");
                EXIT_OK
            }
            Err(error) => {
                log::error!("Writing the layout failed: {error}");
                EXIT_FAILED
            }
        },
        Reply::NoDevice { message } => {
            log::error!("{message}");
            EXIT_NO_DEVICE
        }
        Reply::Incomplete { missing } => {
            ExportError::Incomplete(missing).report();
            EXIT_INCOMPLETE
        }
        Reply::Failed { message } => {
            log::error!("{message}");
            EXIT_FAILED
        }
    })
}

/// `UnixStream::connect` waits as long as the listener's queue is full; this
/// gives up at `deadline` (`None`) and leaves the attempt behind.
fn connect_until(path: &Path, deadline: Instant) -> Option<std::io::Result<UnixStream>> {
    let (sender, connected) = mpsc::channel();
    let path = path.to_owned();
    std::thread::spawn(move || {
        let _ = sender.send(UnixStream::connect(path));
    });
    connected
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

/// Runs the standalone export over `hid`, the running instance's own
/// connection, lent for the duration through a read-only view: every write
/// is refused at the transport, failed reads are listed instead of exported,
/// and what the export loads stays in its own app. It ends early once
/// `cancelled` is set.
pub(crate) fn export_lent_keyboard(
    hid: &crate::hid::HidDevice,
    device: Device,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Reply {
    let session = super::ReadOnlySessions::default();
    let Some(view) = hid.read_only_view(session.hid.clone()) else {
        return Reply::Failed {
            message: "This keyboard's connection cannot be lent to an export".to_owned(),
        };
    };
    log::info!("Exporting layout of {} for the console", describe(&device));
    let mut app = EntropyApp::new_headless();
    app.device_manager.replace_devices(vec![device]);
    app.connect_hid = Some(view);
    let exported = export_entlayout(&mut app, 0, &session, deadline, cancelled);
    // A worker the export leaves behind must not reach the keyboard once the
    // GUI has it back.
    session.hid.close();
    let refused = session.hid.refused_requests().len();
    if refused > 0 {
        log::info!("Refused {refused} keyboard write(s) during the console export");
    }
    match exported {
        Ok(entlayout) => Reply::Exported { entlayout },
        Err(ExportError::Incomplete(missing)) => Reply::Incomplete { missing },
        Err(ExportError::Failed(message)) => Reply::Failed { message },
    }
}

/// Numbers console exports, so a worker's answer reaches the console whose
/// export it ran and no other.
static NEXT_CONSOLE_EXPORT: AtomicU64 = AtomicU64::new(0);

/// A console export waiting for the connection, or running on it.
pub(crate) struct PendingConsoleExport {
    id: u64,
    reply: mpsc::Sender<Reply>,
    generation: u64,
    device: Device,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
    started: bool,
}

impl EntropyApp {
    pub(crate) fn start_console_export_server(&mut self, ctx: &egui::Context) {
        let Some(path) = socket_path() else {
            return;
        };
        match ExportServer::start(&path, ctx.clone()) {
            Ok(server) => self.console_export_server = Some(server),
            Err(error) => log::warn!(
                "Console exports are unavailable ({}): {error}",
                path.display()
            ),
        }
    }

    /// Answers console exports, one at a time, from the selected keyboard.
    pub(crate) fn poll_console_export(&mut self, ctx: &egui::Context) {
        if self
            .pending_console_export
            .as_ref()
            .is_some_and(|pending| pending.cancel.load(Ordering::Relaxed))
        {
            // Its console has gone: stop the export. One already running keeps
            // the channel until its worker answers, so the next console
            // neither waits behind that worker nor gets its answer.
            self.cancel_console_export();
            if self
                .pending_console_export
                .as_ref()
                .is_some_and(|pending| !pending.started)
            {
                self.pending_console_export = None;
            }
        }
        if self.pending_console_export.is_none() {
            let Some(job) = self
                .console_export_server
                .as_ref()
                .and_then(|server| server.jobs.try_recv().ok())
            else {
                return;
            };
            match self.accept_console_export(job.device.as_deref()) {
                Ok(device) => {
                    self.pending_console_export = Some(PendingConsoleExport {
                        id: NEXT_CONSOLE_EXPORT.fetch_add(1, Ordering::Relaxed),
                        reply: job.reply,
                        generation: self.connection_generation,
                        device,
                        deadline: job.deadline,
                        cancel: job.cancel,
                        started: false,
                    })
                }
                Err(reply) => {
                    let _ = job.reply.send(reply);
                    return;
                }
            }
        }
        if let Some(reply) = self.console_export_progress(ctx) {
            self.answer_console_export(reply);
        }
    }

    fn accept_console_export(&self, filter: Option<&str>) -> Result<Device, Reply> {
        let device = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .filter(|_| self.hid_device.is_some() || self.vial_hid_task_active())
            .filter(|_| self.layout.is_some() && matches!(self.connect_state, ConnectState::Idle))
            .ok_or_else(|| Reply::NoDevice {
                message: "The running Entropy has no keyboard connected".to_owned(),
            })?;
        if let Some(filter) = filter.filter(|filter| !device_matches(device, filter)) {
            return Err(Reply::NoDevice {
                message: format!(
                    "The running Entropy is connected to {}, not {filter:?}",
                    describe(device)
                ),
            });
        }
        Ok(device.clone())
    }

    /// Lends the connection to the export as soon as nothing else holds it;
    /// `None` until then, and while the export runs.
    fn console_export_progress(&mut self, ctx: &egui::Context) -> Option<Reply> {
        let connection_generation = self.connection_generation;
        let connected = matches!(self.connect_state, ConnectState::Idle);
        let pending = self.pending_console_export.as_ref()?;
        if pending.generation != connection_generation || !connected {
            return Some(Reply::Failed {
                message: "The keyboard connection was replaced during the export".to_owned(),
            });
        }
        if pending.started {
            return None;
        }
        if Instant::now() >= pending.deadline {
            return Some(Reply::Failed {
                message: "The keyboard stayed busy until the export timed out".to_owned(),
            });
        }
        let operation = vial_hid_task::VialHidOperation::ConsoleExport {
            device: pending.device.clone(),
            deadline: pending.deadline,
            request: pending.id,
        };
        match self.start_vial_hid_operation(ctx, operation) {
            vial_hid_task::VialHidTaskStart::Started => {
                if let Some(pending) = self.pending_console_export.as_mut() {
                    pending.started = true;
                }
                None
            }
            vial_hid_task::VialHidTaskStart::Busy => {
                ctx.request_repaint_after(POLL_INTERVAL);
                None
            }
            vial_hid_task::VialHidTaskStart::NoDevice => Some(Reply::NoDevice {
                message: "The running Entropy has no keyboard connected".to_owned(),
            }),
        }
    }

    /// Hands the answer to the console waiting for it.
    fn answer_console_export(&mut self, reply: Reply) {
        if let Some(pending) = self.pending_console_export.take() {
            let _ = pending.reply.send(reply);
        }
    }

    /// Hands a worker's answer to the console whose export it ran; an answer
    /// for any other request is dropped.
    pub(crate) fn finish_console_export(&mut self, request: u64, reply: Reply) {
        if self
            .pending_console_export
            .as_ref()
            .is_some_and(|pending| pending.id == request)
        {
            self.answer_console_export(reply);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_channel_answers_nothing_but_an_export() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("export.sock");
        let server = ExportServer::start(&socket, egui::Context::default()).unwrap();
        for line in [
            r#"{"command":"set-keycode"}"#,
            r#"{"command":"export-layout","version":2,"device":null,"timeout_ms":1000}"#,
        ] {
            let mut stream = UnixStream::connect(&socket).unwrap();
            stream.write_all(format!("{line}\n").as_bytes()).unwrap();
            let mut answer = String::new();
            BufReader::new(&stream).read_line(&mut answer).unwrap();
            assert!(answer.contains(r#""status":"failed""#), "{answer}");
        }
        assert!(
            server.jobs.try_recv().is_err(),
            "a rejected request reached the GUI"
        );
    }

    #[test]
    fn the_socket_is_private_to_the_user_and_never_stolen() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("entropy").join("export.sock");
        let _server = ExportServer::start(&socket, egui::Context::default()).unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(socket.parent().unwrap()), 0o700);
        assert_eq!(mode(&socket), 0o600);
        assert!(ExportServer::start(&socket, egui::Context::default()).is_err());
    }

    fn export_request(timeout_ms: u64) -> String {
        format!(
            "{}\n",
            serde_json::to_string(&Request::ExportLayout {
                version: PROTOCOL_VERSION,
                device: None,
                timeout_ms,
            })
            .unwrap()
        )
    }

    /// Sends `line` and returns the answer line.
    fn ask(socket: &Path, line: &str) -> String {
        let mut stream = UnixStream::connect(socket).unwrap();
        stream.write_all(line.as_bytes()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut answer = String::new();
        BufReader::new(&stream).read_line(&mut answer).unwrap();
        answer
    }

    fn listening(dir: &Path) -> (PathBuf, ExportServer) {
        let socket = dir.join("export.sock");
        let server = ExportServer::start(&socket, egui::Context::default()).unwrap();
        (socket, server)
    }

    #[test]
    fn a_request_line_has_one_deadline_however_it_trickles() {
        let (gui, mut console) = UnixStream::pair().unwrap();
        let trickle = std::thread::spawn(move || {
            // A byte well inside any single read's timeout, and no newline.
            while console.write_all(b"{").is_ok() {
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let deadline = Instant::now() + Duration::from_millis(300);

        let error = read_request(&gui, deadline).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(Instant::now() < deadline + Duration::from_millis(200));
        drop(gui);
        trickle.join().unwrap();
    }

    #[test]
    fn a_request_line_longer_than_any_request_is_refused_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, server) = listening(dir.path());
        let started = Instant::now();

        // No newline: the channel must not wait for one.
        let answer = ask(&socket, &"x".repeat(MAX_REQUEST_BYTES + 1));

        assert!(answer.contains("request too long"), "{answer}");
        assert!(started.elapsed() < REQUEST_READ_TIMEOUT);
        assert!(server.jobs.try_recv().is_err());
    }

    #[test]
    fn a_console_that_hangs_up_cancels_its_export_and_frees_the_channel() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, server) = listening(dir.path());
        let mut console = UnixStream::connect(&socket).unwrap();
        console
            .write_all(export_request(60_000).as_bytes())
            .unwrap();
        let job = server.jobs.recv_timeout(Duration::from_secs(5)).unwrap();

        // The GUI never answers; the console gives up.
        drop(console);

        let started = Instant::now();
        while !job.cancel.load(Ordering::Relaxed) {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the export outlived its console"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let answer = ask(&socket, "{\"command\":\"set-keycode\"}\n");
        assert!(answer.contains(r#""status":"failed""#), "{answer}");
    }

    #[test]
    fn an_export_out_of_time_is_cancelled_and_answered() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, server) = listening(dir.path());
        let started = Instant::now();

        let answer = std::thread::spawn(move || ask(&socket, &export_request(200)));
        let job = server.jobs.recv_timeout(Duration::from_secs(5)).unwrap();
        let answer = answer.join().unwrap();

        assert!(answer.contains("did not finish in time"), "{answer}");
        assert!(job.cancel.load(Ordering::Relaxed));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_console_that_never_reads_its_answer_cannot_hold_the_channel() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, server) = listening(dir.path());
        let mut console = UnixStream::connect(&socket).unwrap();
        console
            .write_all(export_request(60_000).as_bytes())
            .unwrap();
        let job = server.jobs.recv_timeout(Duration::from_secs(5)).unwrap();
        // Far more than the socket buffers, and the console reads none of it.
        job.reply
            .send(Reply::Exported {
                entlayout: "x".repeat(16 << 20),
            })
            .unwrap();

        let answer = ask(&socket, "{\"command\":\"set-keycode\"}\n");

        assert!(answer.contains(r#""status":"failed""#), "{answer}");
        drop(console);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_full_listen_queue_cannot_outlast_the_console_timeout() {
        use std::os::fd::AsRawFd;
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("export.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        // Room for one pending connection, taken here: the next connect
        // waits for an accept that never comes.
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let _queued = UnixStream::connect(&socket).unwrap();
        let request = HeadlessExportRequest {
            destination: ExportDestination::Stdout,
            device: None,
            timeout: Duration::from_millis(300),
        };
        let started = Instant::now();

        assert_eq!(
            export_via_socket(&socket, &request),
            Some(EXIT_INSTANCE_RUNNING)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_socket_left_by_a_crashed_instance_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("export.sock");
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        assert!(ExportServer::start(&socket, egui::Context::default()).is_ok());
    }
}
