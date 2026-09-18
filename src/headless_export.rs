//! `entropy --export-layout PATH`: connect to one keyboard, write its
//! `.entlayout`, exit. Meant for backups and scripts; no window is opened.
//!
//! The export goes through the same connect worker and deferred-load
//! pipeline as the GUI, driven from a plain loop instead of egui frames, so
//! the file is byte-for-byte what **Layout → Export layout** would save.

use super::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Everything went through; the layout is at the requested destination.
pub(crate) const EXIT_OK: i32 = 0;
/// The keyboard was found but the export did not complete (connect error,
/// timeout, unwritable destination).
pub(crate) const EXIT_FAILED: i32 = 1;
/// No keyboard matched, or several did and `--device` did not settle it.
pub(crate) const EXIT_NO_DEVICE: i32 = 2;
/// Another Entropy instance owns the keyboards; the GUI export still works.
pub(crate) const EXIT_INSTANCE_RUNNING: i32 = 3;
/// The command line did not parse.
pub(crate) const EXIT_USAGE: i32 = 64;

const EXPORT_LAYOUT_ARG: &str = "--export-layout";
const DEVICE_ARG: &str = "--device";
const TIMEOUT_ARG: &str = "--timeout";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) const USAGE: &str = "\
Usage: entropy --export-layout <PATH> [--device <FILTER>] [--timeout <SECONDS>]

  --export-layout <PATH>  write the connected keyboard's .entlayout to PATH
                          (\"-\" writes it to standard output)
  --device <FILTER>       pick the keyboard when several are attached: a
                          case-insensitive part of its name, VID:PID in hex
                          (e126:0071), or its HID path
  --timeout <SECONDS>     give up if the keyboard has not finished loading
                          (default: 60)

Exit status: 0 exported, 1 export failed, 2 no (or ambiguous) keyboard,
3 another Entropy instance is running, 64 usage error.";

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

/// The index of the one keyboard the request means, or the exit status
/// explaining why there is no such keyboard.
fn select_device(devices: &[Device], filter: Option<&str>) -> Result<usize, i32> {
    let candidates: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| filter.is_none_or(|filter| device_matches(device, filter)))
        .map(|(index, _)| index)
        .collect();
    match candidates.as_slice() {
        [index] => Ok(*index),
        [] if devices.is_empty() => {
            log::error!("No Vial keyboard found");
            Err(EXIT_NO_DEVICE)
        }
        [] => {
            log::error!(
                "No keyboard matches {DEVICE_ARG} {:?}; attached: {}",
                filter.unwrap_or_default(),
                devices.iter().map(describe).collect::<Vec<_>>().join("; ")
            );
            Err(EXIT_NO_DEVICE)
        }
        _ => {
            log::error!(
                "Several keyboards match; pass {DEVICE_ARG} to pick one of: {}",
                candidates
                    .iter()
                    .map(|index| describe(&devices[*index]))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            Err(EXIT_NO_DEVICE)
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

    let mut app = EntropyApp::new_headless();
    // `DeviceManager::new` cannot scan on macOS (see there); the main-thread
    // HID setup above makes a direct scan safe.
    #[cfg(target_os = "macos")]
    app.device_manager.scan();

    let device_idx = match select_device(app.device_manager.devices(), request.device.as_deref()) {
        Ok(index) => index,
        Err(code) => return code,
    };
    log::info!(
        "Exporting layout of {}",
        describe(&app.device_manager.devices()[device_idx])
    );

    // A window-less context still carries the repaint requests and input
    // state the connect and deferred-load pollers consult.
    let ctx = egui::Context::default();
    app.start_connect(device_idx);
    if !matches!(app.connect_state, ConnectState::Loading { .. }) {
        log::error!("Connect did not start: {}", app.status_msg);
        return EXIT_FAILED;
    }

    let deadline = Instant::now() + request.timeout;
    let mut last_status = String::new();
    loop {
        app.poll_vial_hid_task(&ctx);
        app.poll_connect(&ctx);
        if app.status_msg != last_status {
            last_status = app.status_msg.clone();
            log::info!("{last_status}");
        }
        match app.connect_state {
            ConnectState::Loading { .. } | ConnectState::Reconnecting(_) => {}
            ConnectState::SelectingDevice => {
                log::error!("Keyboard selection was lost: {}", app.status_msg);
                return EXIT_FAILED;
            }
            ConnectState::Idle if app.layout.is_none() => {
                log::error!("Connect failed: {}", app.status_msg);
                return EXIT_FAILED;
            }
            ConnectState::Idle => {
                // Bluetooth keyboards load layers and dynamic sections after
                // the connect; the pending action asks for all of them, the
                // same way the GUI's export menu item does.
                if app.deferred_full_layout_action.is_none() {
                    app.deferred_full_layout_action =
                        Some(DeferredFullLayoutAction::ExportEntlayout);
                }
                app.maybe_start_deferred_device_load(&ctx, false);
                if app.deferred_full_layout_action_ready(DeferredFullLayoutAction::ExportEntlayout)
                {
                    break;
                }
            }
        }
        if Instant::now() >= deadline {
            log::error!(
                "Gave up after {:?} while: {}",
                request.timeout,
                app.status_msg
            );
            return EXIT_FAILED;
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    let json = match app.entlayout_export_json() {
        Some(Ok(json)) => json,
        Some(Err(error)) => {
            log::error!("{error:#}");
            return EXIT_FAILED;
        }
        None => {
            log::error!("No keyboard layout to export");
            return EXIT_FAILED;
        }
    };
    let written = match &request.destination {
        ExportDestination::Stdout => {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(json.as_bytes())
                .and_then(|()| stdout.write_all(b"\n"))
                .map(|()| "standard output".to_owned())
        }
        ExportDestination::File(path) => {
            std::fs::write(path, json).map(|()| path.display().to_string())
        }
    };
    match written {
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
        assert_eq!(select_device(&devices, None), Err(EXIT_NO_DEVICE));
        assert_eq!(select_device(&devices, Some("K:04")), Err(EXIT_NO_DEVICE));
        assert_eq!(select_device(&devices, Some("Micro")), Err(EXIT_NO_DEVICE));
        assert_eq!(select_device(&devices[..1], None), Ok(0));
        assert_eq!(select_device(&[], None), Err(EXIT_NO_DEVICE));
    }
}
