//! `entropy --export-layout PATH`: connect to one keyboard, write its
//! `.entlayout`, exit. Meant for backups and scripts; no window is opened.
//!
//! The export goes through the same connect worker and deferred-load
//! pipeline as the GUI, driven from a plain loop instead of egui frames, so
//! the file is byte-for-byte what **Layout → Export layout** would save.

use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
#[path = "headless_export_ipc.rs"]
mod running_instance;
#[cfg(unix)]
pub(crate) use running_instance::{
    export_lent_keyboard, export_via_running_instance, ExportServer, PendingConsoleExport, Reply,
};

/// Everything went through; the layout is at the requested destination.
pub(crate) const EXIT_OK: i32 = 0;
/// The keyboard was found but the export did not complete (connect error,
/// replaced connection, unwritable destination).
pub(crate) const EXIT_FAILED: i32 = 1;
/// No keyboard matched, or several did and `--device` did not settle it.
pub(crate) const EXIT_NO_DEVICE: i32 = 2;
/// Another Entropy instance owns the keyboards and did not export for us.
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
3 another Entropy instance is running and did not answer, 4 incomplete
(the missing sections are listed), 64 usage error. A file is replaced only
by a complete export. A running Entropy exports for this command on Linux
and macOS.";

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
        NoDevice::Missing if devices.is_empty() => log::error!("No compatible keyboard found"),
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
/// has delivered every section, unless `cancelled` is set first.
fn export_entlayout(
    app: &mut EntropyApp,
    device_idx: usize,
    session: &crate::hid::ReadOnlyHidSession,
    deadline: Instant,
    cancelled: &AtomicBool,
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
        if cancelled.load(Ordering::Relaxed) {
            return Err(ExportError::Failed("The export was cancelled".to_owned()));
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

    let exported = export_entlayout(
        &mut app,
        device_idx,
        &session,
        deadline,
        &AtomicBool::new(false),
    );
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
    #[test]
    fn cold_headless_macropad_exports_profiles_and_pictograms_read_only() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, session, recorder) = scripted_export_app(|_| None);
        let mut profiles = crate::application_layouts::DeviceApplicationLayouts::default();
        profiles.create_for_application(&crate::application_layouts::DetectedApplication {
            executable: "entropy".into(),
            display_name: "Entropy".into(),
            ..Default::default()
        });
        app.app_settings
            .application_layouts
            .insert("vial-0000000000000000-m4cr0pad-v2".into(), profiles);
        let json =
            export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 9);
        assert_eq!(value["application_layouts"].as_object().unwrap().len(), 2);
        assert_eq!(
            value["pictograms"][0]["bitmap"],
            serde_json::json!(vec![0x55; crate::app::pictograms::PICTOGRAM_BYTES])
        );
        assert!(recorder.requests().iter().any(|request| request[0] == 0xc0));
        assert!(recorder.requests().iter().all(|request| !is_write(request)));
        assert!(!crate::app::app_storage::app_settings_path().exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn headless_capability_transport_failure_cannot_export_a_truncated_legacy_document() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, session, _) = scripted_export_app(|request| {
            (request[0] == 0xe6).then(|| "injected capability failure".into())
        });
        app.app_settings.application_layouts.insert(
            "vial-0000000000000000-m4cr0pad-v2".into(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        assert!(
            export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).is_err()
        );
    }

    #[cfg(target_os = "linux")]
    fn click_ordinary_export(app: &mut EntropyApp, path: &Path, ctx: &egui::Context) {
        let draw = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.draw_layout_device_dropdown(
                        ui,
                        ctx,
                        crate::i18n::Language::English,
                        Some(egui::Rect::from_min_size(
                            egui::pos2(400.0, 0.0),
                            egui::vec2(80.0, 30.0),
                        )),
                        true,
                        false,
                        false,
                    )
                },
            )
        };
        let _ = draw(app, vec![]);
        let output = draw(app, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Export layout" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("ordinary export menu item");
        crate::app::file_dialog::set_test_dialog_result(Some(path.to_owned()));
        for pressed in [true, false] {
            let _ = draw(
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(
            app.pending_file_dialog.as_ref().map(|pending| &pending.0),
            Some(&crate::app::file_dialog::FileDialogAction::ExportEntlayout)
        );
        app.poll_file_dialog(ctx);
        assert!(app.pending_file_dialog.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn production_gui_connect_capability_timeout_never_downgrades_exports() {
        assert_production_connect_capability_timeout(false);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn production_json_connect_capability_timeout_never_downgrades_exports() {
        assert_production_connect_capability_timeout(true);
    }

    #[cfg(target_os = "linux")]
    fn assert_production_connect_capability_timeout(json_first: bool) {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, _, _) = scripted_export_app(|_| None);
        app.headless = false;
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        scripted_keyboard(&recorder, |request| {
            (request[0] == 0xe6).then(|| "HID timeout on capability probe".into())
        });
        app.connect_hid = Some(hid);
        let mut profiles = crate::application_layouts::DeviceApplicationLayouts::default();
        profiles.create_for_application(&crate::application_layouts::DetectedApplication {
            executable: "entropy".into(),
            display_name: "Entropy".into(),
            ..Default::default()
        });
        app.app_settings
            .application_layouts
            .insert("vial-0000000000000000-m4cr0pad-v2".into(), profiles);
        crate::app::app_storage::save_app_settings(&app.app_settings);
        app.app_settings = crate::app::app_storage::load_app_settings();
        let (mut backup_app, backup_session, _) = scripted_export_app(|_| None);
        backup_app.app_settings = app.app_settings.clone();
        let previous = export_entlayout(
            &mut backup_app,
            0,
            &backup_session,
            deadline(),
            &AtomicBool::default(),
        )
        .unwrap();
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
        assert!(
            app.layout.is_some(),
            "optional probe must not prevent connection"
        );
        assert!(recorder.requests().iter().any(|request| request[0] == 0xe6));
        assert!(
            app.application_layout_device_key().is_none(),
            "failed probe must not grant editor/runtime authority"
        );
        let path = storage.path().join("existing.entlayout");
        std::fs::write(&path, previous.as_bytes()).unwrap();
        if json_first {
            assert!(
                app.entlayout_export_json().unwrap().is_err(),
                "ordinary JSON export must not emit legacy v7 after capability timeout"
            );
        }
        click_ordinary_export(&mut app, &path, &ctx);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            previous,
            "ordinary GUI export must not overwrite a full backup after capability timeout"
        );
        assert!(
            app.entlayout_export_json().unwrap().is_err(),
            "ordinary JSON export must not emit legacy v7 after capability timeout"
        );
        scripted_keyboard(&recorder, |_| None);
        let value: serde_json::Value =
            serde_json::from_str(&app.entlayout_export_json().unwrap().unwrap()).unwrap();
        assert_eq!(value["version"], 9);
        assert_eq!(value["application_layouts"].as_object().unwrap().len(), 2);
        assert_eq!(
            value["pictograms"][0]["bitmap"],
            serde_json::json!(vec![0x55; crate::app::pictograms::PICTOGRAM_BYTES])
        );
        app.write_entlayout_export(&path);
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(written["version"], 9);
        assert_eq!(written["application_layouts"], value["application_layouts"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn production_connect_explicit_unsupported_keeps_legacy_export_policy() {
        // Old firmware echo, explicit unsupported response, incompatible format,
        // and an ordinary K:04 whose optional Macropad probe must not run.
        for mode in 0..4 {
            let storage = crate::app::app_storage::ScopedTempConfig::new();
            let (mut app, _, _) = scripted_export_app(|_| None);
            app.headless = false;
            if mode == 3 {
                let mut device = app.device_manager.devices()[0].clone();
                device.name = "K:04".into();
                device.product_id = 0x0071;
                app.device_manager.replace_devices(vec![device]);
            }
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let mut reply = [0; 32];
            if mode == 0 {
                reply[..3].copy_from_slice(&[0xe6, 1, 0xa5]);
            }
            if mode == 2 {
                reply[..3].copy_from_slice(&[0xe6, 255, 0x5a]);
            }
            scripted_keyboard_with_capability(
                &recorder,
                move |request| {
                    (mode == 3 && request[0] == 0xe6)
                        .then(|| "unexpected K:04 capability probe".into())
                },
                Some(reply),
            );
            app.connect_hid = Some(hid);
            let mut profiles = crate::application_layouts::DeviceApplicationLayouts::default();
            profiles.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "entropy".into(),
                display_name: "Entropy".into(),
                ..Default::default()
            });
            app.app_settings
                .application_layouts
                .insert("vial-0000000000000000-m4cr0pad-v2".into(), profiles.clone());
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
            let info = app.device_about_info.as_ref().unwrap();
            assert!(!info.supports_application_layouts);
            assert!(!info.application_layout_probe_failed);
            let json = app.entlayout_export_json().unwrap().unwrap();
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(value["version"], 7);
            assert!(value.get("application_layouts").is_none());
            let path = storage.path().join("legacy.entlayout");
            click_ordinary_export(&mut app, &path, &ctx);
            let written: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(written["version"], 7);
            assert_eq!(
                app.app_settings.application_layouts["vial-0000000000000000-m4cr0pad-v2"].layouts,
                profiles.layouts
            );
            assert!(!recorder.requests().iter().any(|request| request[0] == 0xc0));
            if mode == 3 {
                assert!(!recorder.requests().iter().any(|request| request[0] == 0xe6));
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn headless_pictogram_failures_and_invalid_profiles_never_produce_json() {
        for opcode in [0xc0, 0xca, 0xcb, 0] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            let (mut app, session, recorder) = scripted_export_app(move |request| {
                (request[0] == opcode && opcode != 0)
                    .then(|| "injected pictogram read failure".into())
            });
            let mut profiles = crate::application_layouts::DeviceApplicationLayouts::default();
            if opcode == 0 {
                profiles
                    .layouts
                    .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                    .unwrap()
                    .layers[15][14] = 0x77ff;
            }
            app.app_settings
                .application_layouts
                .insert("vial-0000000000000000-m4cr0pad-v2".into(), profiles);
            assert!(
                export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default())
                    .is_err()
            );
            assert!(recorder.requests().iter().all(|request| !is_write(request)));
            assert!(!crate::app::app_storage::app_settings_path().exists());
        }
    }

    #[cfg(target_os = "linux")]
    fn scripted_keyboard(
        recorder: &crate::hid::TestHidRecorder,
        fail: impl Fn(&[u8; 32]) -> Option<String> + Send + 'static,
    ) {
        scripted_keyboard_with_capability(recorder, fail, None);
    }

    #[cfg(target_os = "linux")]
    fn scripted_keyboard_with_capability(
        recorder: &crate::hid::TestHidRecorder,
        fail: impl Fn(&[u8; 32]) -> Option<String> + Send + 'static,
        capability_reply: Option<[u8; 32]>,
    ) {
        scripted_keyboard_answering(
            recorder,
            move |request| fail(request).map(Err),
            capability_reply,
        );
    }

    /// The scripted keyboard, with `answer` replying first to (or failing)
    /// whatever it returns `Some` for.
    #[cfg(target_os = "linux")]
    fn scripted_keyboard_answering(
        recorder: &crate::hid::TestHidRecorder,
        answer: impl Fn(&[u8; 32]) -> Option<Result<[u8; 32], String>> + Send + 'static,
        capability_reply: Option<[u8; 32]>,
    ) {
        let definition = serde_json::json!({
            "name": "Export fixture",
            "matrix": { "rows": 1, "cols": 2 },
            "layouts": { "keymap": [["0,0", "0,1"]] }
        })
        .to_string();
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut definition.as_bytes(), &mut compressed).unwrap();

        let mut library = crate::app::pictograms::PictogramLibrary::blank();
        library.set(
            crate::app::pictograms::PictogramKind::Macro,
            0,
            &[0x55; crate::app::pictograms::PICTOGRAM_BYTES],
        );
        let mut pictograms = std::collections::VecDeque::from(
            crate::app::pictograms::test_pictogram_read_responses(&library),
        );
        recorder.respond_by(move |request| {
            if let Some(answer) = answer(request) {
                return Some(answer);
            }
            if matches!(request[0], 0xc0 | 0xca | 0xcb) {
                return Some(Ok(pictograms
                    .pop_front()
                    .expect("unexpected pictogram read")));
            }
            let mut response = [0u8; 32];
            match request[..3] {
                [0xe6, version, 0xa5] => {
                    if let Some(reply) = capability_reply {
                        return Some(Ok(reply));
                    }
                    response[0] = 0xe6;
                    response[1] = version;
                    response[2] = 0x5a;
                }
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
        app.connect_hid = Some(hid);
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
            0xc0 | 0xc1 | 0xca | 0xcb => false,
            0xe6 => {
                !(request[1] == crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION
                    && request[2] == 0xa5
                    && request[3..].iter().all(|byte| *byte == 0))
            }
            0xA0..=0xFD => true,
            _ => false,
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn export_reads_the_whole_keyboard_without_a_single_write() {
        let (mut app, session, recorder) = scripted_export_app(|_| None);

        let json =
            export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).unwrap();

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

        let error = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default())
            .unwrap_err();

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

        let error = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default())
            .unwrap_err();

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
        app.connect_hid = Some(replacement);
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

    /// A GUI instance connected to the scripted keyboard, its console export
    /// channel listening at a temporary socket.
    #[cfg(target_os = "linux")]
    fn running_instance() -> (
        EntropyApp,
        crate::hid::TestHidRecorder,
        tempfile::TempDir,
        PathBuf,
        egui::Context,
    ) {
        running_instance_with(|recorder| scripted_keyboard(recorder, |_| None))
    }

    /// The same with the keyboard `script` sets up. A plain keyboard, so
    /// the GUI runs no display host data of its own.
    #[cfg(target_os = "linux")]
    fn running_instance_with(
        script: impl FnOnce(&crate::hid::TestHidRecorder),
    ) -> (
        EntropyApp,
        crate::hid::TestHidRecorder,
        tempfile::TempDir,
        PathBuf,
        egui::Context,
    ) {
        let (mut app, _, _) = scripted_export_app(|_| None);
        app.headless = false;
        let mut device = crate::device::test_usb_device("3-4", 6, 0x0030);
        device.name = "Export fixture".into();
        app.device_manager.replace_devices(vec![device]);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        script(&recorder);
        app.connect_hid = Some(hid);
        let ctx = egui::Context::default();
        app.start_connect(0);
        let started = Instant::now();
        while !(matches!(app.connect_state, ConnectState::Idle) && app.layout.is_some()) {
            assert!(started.elapsed() < Duration::from_secs(20), "connect hung");
            app.poll_vial_hid_task(&ctx);
            app.poll_connect(&ctx);
            std::thread::sleep(POLL_INTERVAL);
        }
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("entropy").join("export.sock");
        app.console_export_server = Some(ExportServer::start(&socket, ctx.clone()).unwrap());
        (app, recorder, dir, socket, ctx)
    }

    /// One frame of the GUI: its background work and the editor's pending
    /// Key Override write.
    #[cfg(target_os = "linux")]
    fn frame(app: &mut EntropyApp, ctx: &egui::Context, started: Instant) {
        app.tick_layout_import_lifecycle_for_test(ctx, started.elapsed().as_secs_f64());
        app.flush_pending_key_override_writes();
        std::thread::sleep(POLL_INTERVAL);
    }

    /// A console export against `app`, answered on another thread.
    #[cfg(target_os = "linux")]
    fn start_console_export(
        socket: &Path,
        request: HeadlessExportRequest,
    ) -> std::thread::JoinHandle<Option<i32>> {
        let socket = socket.to_owned();
        std::thread::spawn(move || running_instance::export_via_socket(&socket, &request))
    }

    /// Pumps `app`'s frames until the console has its answer.
    #[cfg(target_os = "linux")]
    fn finish_console_export(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        client: std::thread::JoinHandle<Option<i32>>,
    ) -> Option<i32> {
        let started = Instant::now();
        while !client.is_finished() {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "console export hung"
            );
            frame(app, ctx, started);
        }
        client.join().unwrap()
    }

    /// One console export against `app`, pumping its frames meanwhile.
    #[cfg(target_os = "linux")]
    fn console_export(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        socket: &Path,
        request: HeadlessExportRequest,
    ) -> Option<i32> {
        let client = start_console_export(socket, request);
        finish_console_export(app, ctx, client)
    }

    #[cfg(target_os = "linux")]
    fn export_to(path: &Path, device: &str) -> HeadlessExportRequest {
        HeadlessExportRequest {
            destination: ExportDestination::File(path.to_owned()),
            device: Some(device.into()),
            timeout: Duration::from_secs(20),
        }
    }

    /// A keyboard with one Key Override, enabled and triggered by a
    /// modifier: loading it, the editor disables the rule and writes that
    /// repair back.
    #[cfg(target_os = "linux")]
    fn modifier_triggered_key_override(request: &[u8; 32]) -> Option<Result<[u8; 32], String>> {
        let mut response = [0u8; 32];
        match request[..3] {
            [0xFE, 0x0D, 0x00] => response[..3].copy_from_slice(&[1, 1, 1]),
            [0xFE, 0x0D, 0x05] => {
                response[1..3].copy_from_slice(&0x00E1u16.to_le_bytes());
                response[3..5].copy_from_slice(&0x0004u16.to_le_bytes());
                response[5..7].copy_from_slice(&u16::MAX.to_le_bytes());
                response[10] = 0x80 | 0x01;
            }
            _ => return None,
        }
        Some(Ok(response))
    }

    #[cfg(target_os = "linux")]
    fn writes_since(recorder: &crate::hid::TestHidRecorder, before: usize) -> Vec<[u8; 32]> {
        recorder.requests()[before..]
            .iter()
            .filter(|request| is_write(request))
            .copied()
            .collect()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn console_export_never_writes_the_repair_its_load_would_make() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, dir, socket, ctx) = running_instance_with(|recorder| {
            recorder.set_bluetooth();
            scripted_keyboard_answering(recorder, modifier_triggered_key_override, None);
        });
        assert!(app.deferred_device_load.is_staged());
        assert_eq!(
            app.deferred_device_load
                .section_status(DeferredLoadSection::KeyOverrides),
            DeferredLoadStatus::NotLoaded
        );
        let before = recorder.requests().len();
        let path = dir.path().join("backup.entlayout");

        let code = console_export(&mut app, &ctx, &socket, export_to(&path, "export fixture"));
        let started = Instant::now();
        for _ in 0..10 {
            frame(&mut app, &ctx, started);
        }

        assert_eq!(code, Some(EXIT_OK));
        assert!(path.exists());
        let writes = writes_since(&recorder, before);
        assert!(writes.is_empty(), "the console export wrote: {writes:02x?}");
        assert_eq!(
            app.deferred_device_load
                .section_status(DeferredLoadSection::KeyOverrides),
            DeferredLoadStatus::NotLoaded,
            "the export loaded into the editor"
        );
        assert!(!app.key_override_dirty);
    }

    /// Holds the first definition read after `arm` until `open`: the GUI has
    /// read its own by then, so it is an export's connect that waits here.
    #[cfg(target_os = "linux")]
    struct DefinitionGate {
        armed: std::sync::Arc<AtomicBool>,
        entered: std::sync::mpsc::Receiver<()>,
        release: std::sync::mpsc::Sender<()>,
    }

    #[cfg(target_os = "linux")]
    impl DefinitionGate {
        /// The gate, and the keyboard's side of it to run on every request.
        fn new() -> (Self, impl Fn(&[u8; 32]) + Send + 'static) {
            let armed = std::sync::Arc::new(AtomicBool::new(false));
            let (entered_tx, entered) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel::<()>();
            let hold = armed.clone();
            let keyboard = move |request: &[u8; 32]| {
                if request[..2] == [0xFE, 0x01] && hold.swap(false, Ordering::AcqRel) {
                    entered_tx.send(()).unwrap();
                    released.recv().unwrap();
                }
            };
            let gate = Self {
                armed,
                entered,
                release,
            };
            (gate, keyboard)
        }

        fn arm(&self) {
            self.armed.store(true, Ordering::Release);
        }

        /// Pumps `app`'s frames until an export waits at the gate.
        fn wait(&self, app: &mut EntropyApp, ctx: &egui::Context, started: Instant) {
            while self.entered.try_recv().is_err() {
                assert!(
                    started.elapsed() < Duration::from_secs(20),
                    "export never ran"
                );
                frame(app, ctx, started);
            }
        }

        fn open(&self) {
            self.release.send(()).unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_edit_made_while_the_export_runs_is_written_after_it() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (gate, at_gate) = DefinitionGate::new();
        let (mut app, recorder, dir, socket, ctx) = running_instance_with(move |recorder| {
            scripted_keyboard_answering(
                recorder,
                move |request| {
                    at_gate(request);
                    let mut response = [0u8; 32];
                    match request[..3] {
                        [0xFE, 0x0D, 0x00] => response[..3].copy_from_slice(&[1, 1, 1]),
                        [0xFE, 0x0D, 0x05] => {
                            response[1..3].copy_from_slice(&0x0004u16.to_le_bytes());
                            response[3..5].copy_from_slice(&0x0005u16.to_le_bytes());
                        }
                        _ => return None,
                    }
                    Some(Ok(response))
                },
                None,
            );
        });
        gate.arm();
        let before = recorder.requests().len();
        let path = dir.path().join("backup.entlayout");
        let client = start_console_export(&socket, export_to(&path, "export fixture"));
        let started = Instant::now();
        gate.wait(&mut app, &ctx, started);

        // The user edits the Key Override while the export has the keyboard.
        app.key_override_entries[0].replacement = 0x0006;
        app.key_override_dirty = true;
        for _ in 0..5 {
            frame(&mut app, &ctx, started);
        }
        assert!(
            writes_since(&recorder, before).is_empty(),
            "the edit reached the keyboard while the export had it"
        );
        assert!(app.key_override_dirty, "the edit was dropped");

        gate.open();
        assert_eq!(finish_console_export(&mut app, &ctx, client), Some(EXIT_OK));
        while app.key_override_dirty {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "edit never written"
            );
            frame(&mut app, &ctx, started);
        }
        let writes = writes_since(&recorder, before);
        assert!(
            writes
                .iter()
                .any(|request| request[..4] == [0xFE, 0x0D, 0x06, 0] && request[6] == 0x06),
            "the edit was not written: {writes:02x?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_console_that_hangs_up_gives_the_gui_its_connection_back() {
        use std::io::Write;
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (gate, at_gate) = DefinitionGate::new();
        let (mut app, recorder, dir, socket, ctx) = running_instance_with(move |recorder| {
            scripted_keyboard_answering(
                recorder,
                move |request| {
                    at_gate(request);
                    None
                },
                None,
            );
        });
        gate.arm();
        let mut console = std::os::unix::net::UnixStream::connect(&socket).unwrap();
        console
            .write_all(b"{\"command\":\"export-layout\",\"version\":1,\"device\":null,\"timeout_ms\":20000}\n")
            .unwrap();
        let started = Instant::now();
        gate.wait(&mut app, &ctx, started);

        // The export is stuck on the keyboard when its console goes away.
        drop(console);
        while app.pending_console_export.is_some() || app.hid_device.is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the abandoned export kept the connection"
            );
            frame(&mut app, &ctx, started);
        }
        let before = recorder.requests().len();
        gate.open();
        for _ in 0..10 {
            frame(&mut app, &ctx, started);
        }
        assert!(
            recorder.requests()[before..]
                .iter()
                .all(|request| request[0] != 0x12),
            "the abandoned export went on reading the keymap"
        );

        let path = dir.path().join("backup.entlayout");
        let code = console_export(&mut app, &ctx, &socket, export_to(&path, "export fixture"));
        assert_eq!(code, Some(EXIT_OK));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_hung_up_export_never_answers_the_next_console() {
        use std::io::Write;
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (gate, at_gate) = DefinitionGate::new();
        let (mut app, _, dir, socket, ctx) = running_instance_with(move |recorder| {
            scripted_keyboard_answering(
                recorder,
                move |request| {
                    at_gate(request);
                    None
                },
                None,
            );
        });
        gate.arm();
        let mut first = std::os::unix::net::UnixStream::connect(&socket).unwrap();
        first
            .write_all(b"{\"command\":\"export-layout\",\"version\":1,\"device\":null,\"timeout_ms\":20000}\n")
            .unwrap();
        let started = Instant::now();
        gate.wait(&mut app, &ctx, started);

        // A second console asks while the first export holds the keyboard,
        // and the first hangs up: its export is cancelled with the second
        // one already queued behind it.
        let path = dir.path().join("backup.entlayout");
        let second = start_console_export(&socket, export_to(&path, "export fixture"));
        drop(first);
        std::thread::sleep(Duration::from_millis(500));
        for _ in 0..5 {
            frame(&mut app, &ctx, started);
        }
        gate.open();

        assert_eq!(
            finish_console_export(&mut app, &ctx, second),
            Some(EXIT_OK),
            "the second console got the first export's answer"
        );
        assert!(path.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_export_that_loses_the_connection_hands_it_to_recovery() {
        for bluetooth in [false, true] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            let lose = std::sync::Arc::new(AtomicBool::new(false));
            let armed = lose.clone();
            let (mut app, _, dir, socket, ctx) = running_instance_with(move |recorder| {
                if bluetooth {
                    recorder.set_bluetooth();
                }
                let transport = recorder.clone();
                scripted_keyboard_answering(
                    recorder,
                    move |_| {
                        // The helper times out under the export, retiring the
                        // connection the GUI lent; the keyboard stays listed.
                        armed.swap(false, Ordering::AcqRel).then(|| {
                            transport.retire_transport();
                            Err("HID helper timed out".to_owned())
                        })
                    },
                    None,
                );
            });
            lose.store(true, Ordering::Release);
            let path = dir.path().join("backup.entlayout");

            let code = console_export(&mut app, &ctx, &socket, export_to(&path, "export fixture"));

            assert_ne!(code, Some(EXIT_OK), "bluetooth: {bluetooth}");
            assert!(!path.exists());
            assert!(
                app.hid_device.is_none(),
                "the retired connection stayed installed, bluetooth: {bluetooth}"
            );
            // The fixture's device is a USB one: its recovery is the cleanup
            // after which the next scan opens it again.
            assert!(
                app.layout.is_none(),
                "the lost connection was kept, bluetooth: {bluetooth}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn running_instance_answers_a_console_export_without_writes() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, dir, socket, ctx) = running_instance();
        let before = recorder.requests().len();
        let path = dir.path().join("backup.entlayout");

        let code = console_export(
            &mut app,
            &ctx,
            &socket,
            HeadlessExportRequest {
                destination: ExportDestination::File(path.clone()),
                device: Some("export fixture".into()),
                timeout: Duration::from_secs(20),
            },
        );

        assert_eq!(code, Some(EXIT_OK));
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["format"], "entropy.layout");
        let writes: Vec<_> = recorder.requests()[before..]
            .iter()
            .filter(|request| is_write(request))
            .copied()
            .collect();
        assert!(writes.is_empty(), "the console export wrote: {writes:02x?}");
        assert!(app.pending_console_export.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn console_export_keeps_the_backup_when_a_section_read_fails() {
        for bluetooth in [false, true] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            // A combo read fails while the keyboard stays connected: both the
            // connect and the staged load fill in an empty combo and go on.
            let (mut app, _, dir, socket, ctx) = running_instance_with(move |recorder| {
                if bluetooth {
                    recorder.set_bluetooth();
                }
                scripted_keyboard(recorder, |request| {
                    (request[..3] == [0xFE, 0x0D, 0x03]).then(|| "HID timeout".to_owned())
                });
            });
            let path = dir.path().join("backup.entlayout");
            std::fs::write(&path, "previous backup").unwrap();

            let code = console_export(&mut app, &ctx, &socket, export_to(&path, "export fixture"));

            assert_eq!(code, Some(EXIT_INCOMPLETE), "bluetooth: {bluetooth}");
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "previous backup",
                "bluetooth: {bluetooth}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn console_export_of_another_keyboard_reports_no_device() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, _, dir, socket, ctx) = running_instance();
        let path = dir.path().join("backup.entlayout");

        let code = console_export(
            &mut app,
            &ctx,
            &socket,
            HeadlessExportRequest {
                destination: ExportDestination::File(path.clone()),
                device: Some("e126:0071".into()),
                timeout: Duration::from_secs(20),
            },
        );

        assert_eq!(code, Some(EXIT_NO_DEVICE));
        assert!(!path.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn console_export_without_a_listening_instance_keeps_the_old_answer() {
        let dir = tempfile::tempdir().unwrap();
        let request = HeadlessExportRequest {
            destination: ExportDestination::Stdout,
            device: None,
            timeout: Duration::from_secs(1),
        };
        assert_eq!(
            running_instance::export_via_socket(&dir.path().join("export.sock"), &request),
            None
        );
    }
}
