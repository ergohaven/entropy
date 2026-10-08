//! `entropy --export-layout PATH`: connect to one keyboard, write its
//! `.entlayout`, exit. Meant for backups and scripts; no window is opened.
//!
//! The export goes through the same connect worker and deferred-load
//! pipeline as the GUI, driven from a plain loop instead of egui frames, so
//! the file is byte-for-byte what **Layout → Export layout** would save.

use super::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Everything went through; the layout is at the requested destination.
pub(crate) const EXIT_OK: i32 = 0;
/// The keyboard was found but the export did not complete (connect error,
/// replaced connection, unwritable destination).
pub(crate) const EXIT_FAILED: i32 = 1;
/// No keyboard matched, or several did and `--device` did not settle it.
pub(crate) const EXIT_NO_DEVICE: i32 = 2;
/// Another Entropy instance owns the keyboards; the GUI export still works.
pub(crate) const EXIT_INSTANCE_RUNNING: i32 = 3;
/// The keyboard loaded, but some of its layers or sections did not; nothing
/// was written.
pub(crate) const EXIT_INCOMPLETE: i32 = 4;
/// The command line did not parse.
pub(crate) const EXIT_USAGE: i32 = 64;

const EXPORT_LAYOUT_ARG: &str = "--export-layout";
const DEVICE_ARG: &str = "--device";
const TIMEOUT_ARG: &str = "--timeout";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const DISCOVERY_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) const USAGE: &str = "\
Usage: entropy --export-layout <PATH> [--device <FILTER>] [--timeout <SECONDS>]

  --export-layout <PATH>  write the connected keyboard's .entlayout to PATH
                          (\"-\" writes it to standard output)
  --device <FILTER>       pick the keyboard when several are attached: a
                          case-insensitive part of its name, VID:PID in hex
                          (e126:0071), or its HID path
  --timeout <SECONDS>     give up if the keyboard has not appeared and
                          finished loading by then (default: 60)

Exit status: 0 exported, 1 export failed, 2 no (or ambiguous) keyboard,
3 another Entropy instance is running, 4 incomplete (the missing sections
are listed), 64 usage error. A file is replaced only by a complete export.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExportDestination {
    Stdout,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeadlessExportRequest {
    pub(crate) destination: ExportDestination,
    pub(crate) device: Option<String>,
    pub(crate) timeout: Duration,
}

impl HeadlessExportRequest {
    /// `Ok(None)` when the arguments do not ask for a headless export, so the
    /// GUI starts as usual. `Err` carries the usage message.
    pub(crate) fn from_args<I, S>(args: I) -> Result<Option<Self>, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let args: Vec<String> = args
            .into_iter()
            .skip(1)
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect();
        let mut destination = None;
        let mut device = None;
        let mut timeout = None;
        let mut rest = Vec::new();

        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            let (name, inline_value) = match arg.split_once('=') {
                Some((name, value)) if name.starts_with("--") => (name, Some(value.to_owned())),
                _ => (arg.as_str(), None),
            };
            let take_value = |index: &mut usize| -> Result<String, String> {
                if let Some(value) = inline_value.clone() {
                    return Ok(value);
                }
                *index += 1;
                args.get(*index)
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value\n\n{USAGE}"))
            };
            match name {
                EXPORT_LAYOUT_ARG => {
                    let value = take_value(&mut index)?;
                    destination = Some(if value == "-" {
                        ExportDestination::Stdout
                    } else {
                        ExportDestination::File(PathBuf::from(value))
                    });
                }
                DEVICE_ARG => device = Some(take_value(&mut index)?),
                TIMEOUT_ARG => {
                    let value = take_value(&mut index)?;
                    let seconds: u64 = value
                        .parse()
                        .ok()
                        .filter(|seconds| *seconds > 0)
                        .ok_or_else(|| {
                            format!("{TIMEOUT_ARG} takes a positive number of seconds, not {value:?}\n\n{USAGE}")
                        })?;
                    timeout = Some(Duration::from_secs(seconds));
                }
                _ => rest.push(arg.clone()),
            }
            index += 1;
        }

        let Some(destination) = destination else {
            if device.is_some() || timeout.is_some() {
                return Err(format!(
                    "{DEVICE_ARG} and {TIMEOUT_ARG} only apply together with {EXPORT_LAYOUT_ARG}\n\n{USAGE}"
                ));
            }
            return Ok(None);
        };
        if let Some(unexpected) = rest.first() {
            return Err(format!(
                "unexpected argument {unexpected:?} with {EXPORT_LAYOUT_ARG}\n\n{USAGE}"
            ));
        }
        Ok(Some(Self {
            destination,
            device,
            timeout: timeout.unwrap_or(DEFAULT_TIMEOUT),
        }))
    }
}

/// `--device` matching: a hex `VID:PID`, the exact HID path, or a
/// case-insensitive fragment of the product name.
fn device_matches(device: &Device, filter: &str) -> bool {
    let filter = filter.trim();
    if let Some((vendor, product)) = filter.split_once(':') {
        if let (Ok(vendor), Ok(product)) = (
            u16::from_str_radix(vendor.trim(), 16),
            u16::from_str_radix(product.trim(), 16),
        ) {
            if device.vendor_id == vendor && device.product_id == product {
                return true;
            }
        }
    }
    device.path == filter || device.name.to_lowercase().contains(&filter.to_lowercase())
}

fn describe(device: &Device) -> String {
    format!(
        "{} [{:04x}:{:04x}] {}",
        device.display_name_with_transport(&device.name),
        device.vendor_id,
        device.product_id,
        device.path
    )
}

#[derive(Debug, PartialEq, Eq)]
enum NoDevice {
    Missing,
    Ambiguous(Vec<usize>),
}

/// The index of the one keyboard the request means.
fn select_device(devices: &[Device], filter: Option<&str>) -> Result<usize, NoDevice> {
    let candidates: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| filter.is_none_or(|filter| device_matches(device, filter)))
        .map(|(index, _)| index)
        .collect();
    match candidates.as_slice() {
        [index] => Ok(*index),
        [] => Err(NoDevice::Missing),
        _ => Err(NoDevice::Ambiguous(candidates)),
    }
}

fn report_no_device(devices: &[Device], filter: Option<&str>, no_device: &NoDevice) {
    match no_device {
        NoDevice::Missing if devices.is_empty() => log::error!("No Vial keyboard found"),
        NoDevice::Missing => log::error!(
            "No keyboard matches {DEVICE_ARG} {:?}; attached: {}",
            filter.unwrap_or_default(),
            devices.iter().map(describe).collect::<Vec<_>>().join("; ")
        ),
        NoDevice::Ambiguous(candidates) => log::error!(
            "Several keyboards match; pass {DEVICE_ARG} to pick one of: {}",
            candidates
                .iter()
                .map(|index| describe(&devices[*index]))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

/// Rescans until the requested keyboard shows up or `deadline` passes. A
/// Bluetooth keyboard can appear only after BlueZ discovery catches up, so a
/// scan without it proves nothing; several matches end the wait at once.
fn wait_for_device(
    mut scan: impl FnMut() -> Vec<Device>,
    filter: Option<&str>,
    deadline: Instant,
    interval: Duration,
) -> Result<(Vec<Device>, usize), i32> {
    let mut waiting_logged = false;
    loop {
        let devices = scan();
        let no_device = match select_device(&devices, filter) {
            Ok(index) => return Ok((devices, index)),
            Err(no_device) => no_device,
        };
        let now = Instant::now();
        if matches!(no_device, NoDevice::Ambiguous(_)) || now >= deadline {
            report_no_device(&devices, filter, &no_device);
            return Err(EXIT_NO_DEVICE);
        }
        if !waiting_logged {
            waiting_logged = true;
            log::info!("Waiting for the keyboard to appear…");
        }
        std::thread::sleep(interval.min(deadline - now));
    }
}

/// Why an export produced no bundle.
#[derive(Debug, PartialEq, Eq)]
enum ExportError {
    /// The keyboard never finished loading, or its connection was replaced.
    Failed(String),
    /// The keyboard loaded, but these layers or sections are missing.
    Incomplete(Vec<String>),
}

impl ExportError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::Failed(_) => EXIT_FAILED,
            Self::Incomplete(_) => EXIT_INCOMPLETE,
        }
    }

    fn report(&self) {
        match self {
            Self::Failed(message) => log::error!("{message}"),
            Self::Incomplete(missing) => log::error!(
                "Not exported: the keyboard did not deliver {}",
                missing.join(", ")
            ),
        }
    }
}

/// Follows one connection from connect to a fully loaded layout. Everything
/// in the bundle must come from the connection the connect produced: a
/// reconnect, or any other replacement, ends the export instead of mixing
/// data from two sessions.
#[derive(Default)]
struct ExportConnection {
    generation: Option<u64>,
}

impl ExportConnection {
    /// One poll; `Ok(true)` once every layer and section is loaded.
    fn step(&mut self, app: &mut EntropyApp, ctx: &egui::Context) -> Result<bool, ExportError> {
        match app.connect_state {
            ConnectState::Loading { .. } if self.generation.is_none() => return Ok(false),
            ConnectState::Loading { .. } | ConnectState::Reconnecting(_) => {
                return Err(ExportError::Failed(format!(
                    "The keyboard connection was replaced during the export: {}",
                    app.status_msg
                )));
            }
            ConnectState::SelectingDevice => {
                return Err(ExportError::Failed(format!(
                    "Keyboard selection was lost: {}",
                    app.status_msg
                )));
            }
            ConnectState::Idle if app.layout.is_none() => {
                return Err(ExportError::Failed(format!(
                    "Connect failed: {}",
                    app.status_msg
                )));
            }
            ConnectState::Idle => {}
        }
        let generation = *self.generation.get_or_insert(app.connection_generation);
        if generation != app.connection_generation {
            return Err(ExportError::Failed(
                "The keyboard connection was replaced during the export".to_owned(),
            ));
        }

        // Bluetooth keyboards load layers and dynamic sections after the
        // connect; the pending action asks for all of them, the same way the
        // GUI's export menu item does. A failed load is not retried, so it
        // ends the export at once.
        if app.deferred_full_layout_action.is_none() {
            app.deferred_full_layout_action = Some(DeferredFullLayoutAction::ExportEntlayout);
        }
        app.maybe_start_deferred_device_load(ctx, false);
        let gaps = app.entlayout_export_gaps();
        let failed: Vec<String> = gaps
            .iter()
            .filter(|(_, status)| matches!(status, DeferredLoadStatus::Failed(_)))
            .map(|(name, _)| name.clone())
            .collect();
        if !failed.is_empty() {
            return Err(ExportError::Incomplete(failed));
        }
        Ok(gaps.is_empty()
            && app.deferred_full_layout_action_ready(DeferredFullLayoutAction::ExportEntlayout))
    }
}

/// Layers and sections the current snapshot lacks: not (yet) loaded, or
/// read with a request that failed on the wire and was papered over.
fn missing_sections(app: &EntropyApp, session: &crate::hid::ReadOnlyHidSession) -> Vec<String> {
    let mut missing: Vec<String> = app
        .entlayout_export_gaps()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for request in session.failed_reads() {
        if let Some(section) = crate::hid::hid_protocol::entlayout_section_of_read(&request) {
            if !missing.iter().any(|name| name == section) {
                missing.push(section.to_owned());
            }
        }
    }
    missing
}

/// Connects to `device_idx` and returns its `.entlayout` once one connection
/// has delivered every section.
fn export_entlayout(
    app: &mut EntropyApp,
    device_idx: usize,
    session: &crate::hid::ReadOnlyHidSession,
    deadline: Instant,
) -> Result<String, ExportError> {
    // A window-less context still carries the repaint requests and input
    // state the connect and deferred-load pollers consult.
    let ctx = egui::Context::default();
    app.start_connect(device_idx);
    if !matches!(app.connect_state, ConnectState::Loading { .. }) {
        return Err(ExportError::Failed(format!(
            "Connect did not start: {}",
            app.status_msg
        )));
    }

    let mut connection = ExportConnection::default();
    let mut last_status = String::new();
    loop {
        app.poll_vial_hid_task(&ctx);
        app.poll_connect(&ctx);
        if app.status_msg != last_status {
            last_status = app.status_msg.clone();
            log::info!("{last_status}");
        }
        if connection.step(app, &ctx)? {
            break;
        }
        if Instant::now() >= deadline {
            if connection.generation.is_some() {
                return Err(ExportError::Incomplete(missing_sections(app, session)));
            }
            return Err(ExportError::Failed(format!(
                "Gave up while: {}",
                app.status_msg
            )));
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    let missing = missing_sections(app, session);
    if !missing.is_empty() {
        return Err(ExportError::Incomplete(missing));
    }
    match app.entlayout_export_json() {
        Some(Ok(json)) => Ok(json),
        Some(Err(error)) => Err(ExportError::Failed(format!("{error:#}"))),
        None => Err(ExportError::Failed(
            "No keyboard layout to export".to_owned(),
        )),
    }
}

/// Replaces `path` only with a complete, durable file: the bundle goes to a
/// sibling temporary file that is fsynced and then renamed over `path`, so a
/// failed or interrupted export never truncates the previous backup.
fn write_file_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".entlayout-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        file.as_file().set_permissions(metadata.permissions())?;
    }
    file.write_all(contents)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    // The rename is durable only once the directory entry is.
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn write_export(destination: &ExportDestination, json: &str) -> std::io::Result<String> {
    match destination {
        ExportDestination::Stdout => {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(json.as_bytes())?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            Ok("standard output".to_owned())
        }
        ExportDestination::File(path) => {
            write_file_atomically(path, json.as_bytes())?;
            Ok(path.display().to_string())
        }
    }
}

/// Runs the export to completion and returns the process exit status.
///
/// Must be called from the main thread before any window exists, and after
/// the single-instance lock is held: a second process talking Vial to the
/// same keyboard would interleave with the GUI's own requests.
pub(crate) fn run_headless_export(request: HeadlessExportRequest) -> i32 {
    #[cfg(target_os = "macos")]
    crate::hid::initialize_macos_hid_on_main_thread();

    // Before any HID handle exists: everything this process opens from here
    // on refuses writes at the transport, whatever pipeline asks for them.
    let session = crate::hid::enforce_read_only_hid();
    let mut app = EntropyApp::new_headless();
    let deadline = Instant::now() + request.timeout;

    let scan = || {
        DeviceManager::scan_devices().unwrap_or_else(|error| {
            log::warn!("{error}");
            Vec::new()
        })
    };
    let (devices, device_idx) = match wait_for_device(
        scan,
        request.device.as_deref(),
        deadline,
        DISCOVERY_INTERVAL,
    ) {
        Ok(found) => found,
        Err(code) => return code,
    };
    log::info!("Exporting layout of {}", describe(&devices[device_idx]));
    app.device_manager.replace_devices(devices);

    let exported = export_entlayout(&mut app, device_idx, &session, deadline);
    let refused = session.refused_requests().len();
    if refused > 0 {
        log::info!("Refused {refused} keyboard write(s) during the export");
    }
    let json = match exported {
        Ok(json) => json,
        Err(error) => {
            error.report();
            return error.exit_code();
        }
    };
    match write_export(&request.destination, &json) {
        Ok(destination) => {
            log::info!("Exported layout to {destination}");
            EXIT_OK
        }
        Err(error) => {
            log::error!("Writing the layout failed: {error}");
            EXIT_FAILED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<HeadlessExportRequest>, String> {
        HeadlessExportRequest::from_args(std::iter::once("entropy").chain(args.iter().copied()))
    }

    #[test]
    fn plain_launch_is_not_an_export() {
        assert_eq!(parse(&[]), Ok(None));
        assert_eq!(parse(&["--minimized"]), Ok(None));
    }

    #[test]
    fn export_takes_path_device_and_timeout() {
        let request = parse(&[
            "--export-layout",
            "backup.entlayout",
            "--device",
            "Qube",
            "--timeout",
            "5",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            request.destination,
            ExportDestination::File(PathBuf::from("backup.entlayout"))
        );
        assert_eq!(request.device.as_deref(), Some("Qube"));
        assert_eq!(request.timeout, Duration::from_secs(5));
    }

    #[test]
    fn inline_values_and_stdout_work() {
        let request = parse(&["--export-layout=-", "--device=e126:0071"])
            .unwrap()
            .unwrap();
        assert_eq!(request.destination, ExportDestination::Stdout);
        assert_eq!(request.device.as_deref(), Some("e126:0071"));
        assert_eq!(request.timeout, DEFAULT_TIMEOUT);
    }

    #[test]
    fn usage_errors_are_reported() {
        assert!(parse(&["--export-layout"]).is_err());
        assert!(parse(&["--device", "Qube"]).is_err());
        assert!(parse(&["--export-layout", "x", "--timeout", "0"]).is_err());
        assert!(parse(&["--export-layout", "x", "--timeout", "soon"]).is_err());
        assert!(parse(&["--export-layout", "x", "--minimized"]).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn device_filter_matches_name_id_and_path() {
        let mut device = crate::device::test_usb_device("3-3", 5, 0x71);
        device.name = "K:04 (Qube)".into();
        assert!(device_matches(&device, "qube"));
        assert!(device_matches(&device, "K:04"));
        assert!(device_matches(&device, "E126:0071"));
        assert!(device_matches(&device, "e126:71"));
        assert!(device_matches(&device, "/dev/hidraw5"));
        assert!(!device_matches(&device, "Mini"));
        assert!(!device_matches(&device, "e126:0072"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn selection_needs_exactly_one_match() {
        let mut qube = crate::device::test_usb_device("3-3", 5, 0x71);
        qube.name = "K:04 (Qube)".into();
        let mut left = crate::device::test_usb_device("3-4", 6, 0x74);
        left.name = "K:04".into();
        let devices = [qube, left];

        assert_eq!(select_device(&devices, Some("Qube")), Ok(0));
        assert_eq!(select_device(&devices, Some("e126:0074")), Ok(1));
        let both = Err(NoDevice::Ambiguous(vec![0, 1]));
        assert_eq!(select_device(&devices, None), both);
        assert_eq!(select_device(&devices, Some("K:04")), both);
        assert_eq!(
            select_device(&devices, Some("Micro")),
            Err(NoDevice::Missing)
        );
        assert_eq!(select_device(&devices[..1], None), Ok(0));
        assert_eq!(select_device(&[], None), Err(NoDevice::Missing));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn discovery_keeps_scanning_until_the_keyboard_appears() {
        let mut qube = crate::device::test_usb_device("3-3", 5, 0x71);
        qube.name = "K:04 (Qube)".into();
        let usb = crate::device::test_usb_device("3-4", 6, 0x74);
        let mut scans = 0;
        let scan = || {
            scans += 1;
            // A Bluetooth keyboard shows up only once BlueZ discovery has run.
            if scans < 4 {
                vec![usb.clone()]
            } else {
                vec![usb.clone(), qube.clone()]
            }
        };

        let deadline = Instant::now() + Duration::from_secs(5);
        let (devices, index) =
            wait_for_device(scan, Some("Qube"), deadline, Duration::from_millis(1)).unwrap();

        assert_eq!(devices[index].name, "K:04 (Qube)");
        assert_eq!(scans, 4);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn discovery_gives_up_at_the_deadline_or_on_ambiguity() {
        let usb = crate::device::test_usb_device("3-4", 6, 0x74);
        let mut scans = 0;
        let started = Instant::now();
        let missing = wait_for_device(
            || {
                scans += 1;
                vec![usb.clone()]
            },
            Some("Qube"),
            started + Duration::from_millis(50),
            Duration::from_millis(5),
        );
        assert_eq!(missing.err(), Some(EXIT_NO_DEVICE));
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert!(scans > 1);

        let mut scans = 0;
        let ambiguous = wait_for_device(
            || {
                scans += 1;
                vec![usb.clone(), usb.clone()]
            },
            None,
            Instant::now() + Duration::from_secs(5),
            Duration::from_millis(5),
        );
        assert_eq!(ambiguous.err(), Some(EXIT_NO_DEVICE));
        assert_eq!(scans, 1);
    }

    #[test]
    fn file_export_replaces_the_backup_only_when_complete() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("k04.entlayout");
        std::fs::write(&path, "previous").unwrap();

        write_file_atomically(&path, b"next").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "next");

        // A destination that cannot be replaced keeps what was there.
        let blocked = directory.path().join("blocked.entlayout");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("inside"), "kept").unwrap();
        assert!(write_file_atomically(&blocked, b"next").is_err());
        assert_eq!(
            std::fs::read_to_string(blocked.join("inside")).unwrap(),
            "kept"
        );

        let mut names: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["blocked.entlayout", "k04.entlayout"]);
    }

    /// A scripted Vial keyboard: a two-key, two-layer definition, one macro,
    /// one combo and one tap dance. `fail` makes chosen requests fail on the
    /// wire the way a timeout or unplug would.
    #[cfg(target_os = "linux")]
    fn scripted_keyboard(
        recorder: &crate::hid::TestHidRecorder,
        fail: impl Fn(&[u8; 32]) -> Option<String> + Send + 'static,
    ) {
        let definition = serde_json::json!({
            "name": "Export fixture",
            "matrix": { "rows": 1, "cols": 2 },
            "layouts": { "keymap": [["0,0", "0,1"]] }
        })
        .to_string();
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut definition.as_bytes(), &mut compressed).unwrap();

        recorder.respond_by(move |request| {
            if let Some(error) = fail(request) {
                return Some(Err(error));
            }
            let mut response = [0u8; 32];
            match request[..3] {
                [0x01, ..] => response[2] = 9,
                [0xFE, 0x00, _] => response[0] = 6,
                [0xFE, 0x05, _] => {
                    response[0] = 1;
                    response[2..4].fill(0xFF);
                }
                [0xFE, 0x01, _] => {
                    response[..4].copy_from_slice(&(compressed.len() as u32).to_le_bytes())
                }
                [0xFE, 0x02, _] => {
                    let block = u32::from_le_bytes(request[2..6].try_into().unwrap()) as usize;
                    let chunk = compressed.iter().skip(block * 32).take(32);
                    for (byte, value) in response.iter_mut().zip(chunk) {
                        *byte = *value;
                    }
                }
                [0xFE, 0x09, _] => response.fill(0xFF),
                [0xFE, 0x0D, 0x00] => response[..2].copy_from_slice(&[1, 1]),
                [0x11, ..] => response[..2].copy_from_slice(&[0x11, 2]),
                [0x12, high, low] => {
                    response[..4].copy_from_slice(&request[..4]);
                    let offset = usize::from(u16::from_be_bytes([high, low]));
                    for index in 0..usize::from(request[3]) {
                        // Big-endian keycodes KC_A, KC_B, … in matrix order.
                        let byte = offset + index;
                        response[4 + index] = if byte % 2 == 0 { 0 } else { 4 + byte as u8 / 2 };
                    }
                }
                // Not an RMK keyboard: echo the native-action capability probe.
                [0x08, 0xE8, 0x02] => response = *request,
                [0x0C, ..] => response[1] = 1,
                [0x0D, ..] => response[2] = 16,
                [0x0E, ..] => response[..4].copy_from_slice(&request[..4]),
                _ => return None,
            }
            Some(Ok(response))
        });
    }

    #[cfg(target_os = "linux")]
    fn scripted_export_app(
        fail: impl Fn(&[u8; 32]) -> Option<String> + Send + 'static,
    ) -> (
        EntropyApp,
        crate::hid::ReadOnlyHidSession,
        crate::hid::TestHidRecorder,
    ) {
        let session = crate::hid::ReadOnlyHidSession::default();
        let (hid, recorder) = crate::hid::HidDevice::test_read_only_device(session.clone());
        scripted_keyboard(&recorder, fail);
        let mut device = crate::device::test_usb_device("3-3", 5, 0x0041);
        // A display macropad: its connect pauses the standby animation.
        device.name = "M4CR0Pad v2".into();
        let mut app = EntropyApp::new_inert_for_test();
        app.headless = true;
        app.device_manager.replace_devices(vec![device]);
        app.test_connect_hid = Some(hid);
        (app, session, recorder)
    }

    #[cfg(target_os = "linux")]
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(20)
    }

    #[cfg(target_os = "linux")]
    /// Every opcode that changes keyboard state. Kept apart from the
    /// transport's read allowlist so the two cannot drift together.
    fn is_write(request: &[u8; 32]) -> bool {
        match request[0] {
            0x03 | 0x05 | 0x06 | 0x07 | 0x09 | 0x0A | 0x0B | 0x0F | 0x10 | 0x13 | 0x15 => true,
            0xFE => {
                matches!(request[1], 0x04 | 0x06 | 0x07 | 0x08 | 0x0B | 0x0C)
                    || (request[1] == 0x0D && request[2] != 0 && request[2].is_multiple_of(2))
            }
            0xA0..=0xFD => true,
            _ => false,
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn export_reads_the_whole_keyboard_without_a_single_write() {
        let (mut app, session, recorder) = scripted_export_app(|_| None);

        let json = export_entlayout(&mut app, 0, &session, deadline()).unwrap();

        let requests = recorder.requests();
        assert!(requests.len() > 10, "{requests:02x?}");
        let writes: Vec<_> = requests
            .iter()
            .filter(|request| is_write(request))
            .collect();
        assert!(
            writes.is_empty(),
            "writes reached the transport: {writes:02x?}"
        );
        // The connect did try to pause the standby animation; the transport
        // refused it before it reached the keyboard.
        assert!(session
            .refused_requests()
            .iter()
            .any(|request| request[..2] == [0xB6, 1]));
        assert!(session.failed_reads().is_empty());

        let bundle: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            bundle["data"]["keymap"],
            serde_json::json!([[4, 5], [6, 7]])
        );
        // An explicit empty preference can clear prior hidden positions on import.
        assert_eq!(
            bundle["data"]["layout_element_visibility"]["hidden_keys"],
            serde_json::json!([])
        );
        assert_eq!(
            bundle["data"]["layout_element_visibility"]["hidden_encoders"],
            serde_json::json!([])
        );
        assert_eq!(
            bundle["data"]["combos"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn failed_reads_are_listed_instead_of_exported() {
        let (mut app, session, _) = scripted_export_app(|request| {
            matches!(request[..3], [0x0E, ..] | [0xFE, 0x0D, 0x03])
                .then(|| "HID timeout".to_owned())
        });

        let error = export_entlayout(&mut app, 0, &session, deadline()).unwrap_err();

        assert_eq!(
            error,
            ExportError::Incomplete(vec!["Macros".to_owned(), "Combos".to_owned()])
        );
        assert_eq!(error.exit_code(), EXIT_INCOMPLETE);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn disconnect_during_the_export_writes_nothing() {
        let (mut app, session, _) = scripted_export_app(|request| {
            (request[0] == 0x11).then(|| "HID device disconnected".to_owned())
        });

        let error = export_entlayout(&mut app, 0, &session, deadline()).unwrap_err();

        assert!(matches!(error, ExportError::Failed(_)), "{error:?}");
        assert_eq!(error.exit_code(), EXIT_FAILED);
    }

    #[cfg(target_os = "linux")]
    fn connected_export_app() -> (EntropyApp, egui::Context, ExportConnection) {
        let (mut app, _, _) = scripted_export_app(|_| None);
        let ctx = egui::Context::default();
        app.start_connect(0);
        let mut connection = ExportConnection::default();
        let started = Instant::now();
        while connection.generation.is_none() {
            assert!(started.elapsed() < Duration::from_secs(20), "connect hung");
            app.poll_connect(&ctx);
            connection.step(&mut app, &ctx).unwrap();
            std::thread::sleep(POLL_INTERVAL);
        }
        (app, ctx, connection)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_replaced_connection_ends_the_export() {
        let (mut app, ctx, mut connection) = connected_export_app();
        assert_eq!(connection.step(&mut app, &ctx), Ok(true));

        // A reconnect (or any replacement) bumps the generation: the loaded
        // data no longer belongs to one connection.
        app.connection_generation = app.connection_generation.wrapping_add(1);
        assert!(matches!(
            connection.step(&mut app, &ctx),
            Err(ExportError::Failed(_))
        ));

        let (mut app, ctx, mut connection) = connected_export_app();
        let (replacement, _) = crate::hid::HidDevice::test_device();
        app.test_connect_hid = Some(replacement);
        app.start_connect(0);
        assert!(matches!(
            connection.step(&mut app, &ctx),
            Err(ExportError::Failed(_))
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_failed_deferred_section_ends_the_export() {
        let (mut app, ctx, mut connection) = connected_export_app();
        app.deferred_device_load.set_section_status(
            DeferredLoadSection::TapDance,
            DeferredLoadStatus::Failed("HID timeout".to_owned()),
        );

        assert_eq!(
            connection.step(&mut app, &ctx),
            Err(ExportError::Incomplete(vec!["TapDance".to_owned()]))
        );
    }
}
