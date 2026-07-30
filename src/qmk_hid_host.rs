//! Small built-in qmk-hid-host bridge for display presets that expect host data.
//! Sends the same Raw HID packet family as https://github.com/ergohaven/qmk-hid-host.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RAW_HID_PACKET_LEN: usize = 32;
const DATA_TIME: u8 = 0xAA;
const DATA_VOLUME: u8 = 0xAB;
const DATA_LAYOUT: u8 = 0xAC;
const DATA_MEDIA_ARTIST: u8 = 0xAD;
const DATA_MEDIA_TITLE: u8 = 0xAE;
const DEFAULT_LAYOUT_CODES: [&str; 2] = ["en", "ru"];
#[cfg(target_os = "macos")]
const MACOS_AUTOMATION_COMMAND_TIMEOUT: Duration = Duration::from_millis(1_500);
#[cfg(not(target_os = "windows"))]
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostDataMode {
    pub time: bool,
    pub volume: bool,
    pub layout: bool,
    pub media: bool,
}

impl HostDataMode {
    pub fn is_empty(self) -> bool {
        !self.time && !self.volume && !self.layout && !self.media
    }
}

#[derive(Clone, Debug)]
pub struct FeatureCheck {
    pub ok: bool,
    pub label: &'static str,
    pub hint: &'static str,
}

pub fn volume_check() -> FeatureCheck {
    platform_volume_check()
}

pub fn media_check() -> FeatureCheck {
    platform_media_check()
}

pub fn layout_check() -> FeatureCheck {
    platform_layout_check()
}

#[cfg(target_os = "windows")]
fn platform_volume_check() -> FeatureCheck {
    FeatureCheck {
        ok: true,
        label: "native Windows audio",
        hint: "Uses the Windows default output device",
    }
}

#[cfg(target_os = "linux")]
fn platform_volume_check() -> FeatureCheck {
    if command_exists("wpctl") {
        FeatureCheck {
            ok: true,
            label: "wpctl",
            hint: "Uses PipeWire default sink volume",
        }
    } else if command_exists("pactl") {
        FeatureCheck {
            ok: true,
            label: "pactl",
            hint: "Uses PulseAudio/PipeWire Pulse default sink volume",
        }
    } else {
        FeatureCheck {
            ok: false,
            label: "missing wpctl/pactl",
            hint: "Install wireplumber or pulseaudio-utils/pavucontrol package for volume sync",
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_volume_check() -> FeatureCheck {
    FeatureCheck {
        ok: command_exists("osascript"),
        label: "osascript",
        hint: "Uses macOS system output volume",
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn platform_volume_check() -> FeatureCheck {
    FeatureCheck {
        ok: false,
        label: "unsupported OS",
        hint: "Volume sync is implemented for Windows, Linux and macOS",
    }
}

#[cfg(target_os = "windows")]
fn platform_media_check() -> FeatureCheck {
    FeatureCheck {
        ok: true,
        label: "native Windows media session",
        hint: "Uses Windows global media session metadata",
    }
}

#[cfg(target_os = "linux")]
fn platform_media_check() -> FeatureCheck {
    if command_exists("playerctl") {
        FeatureCheck {
            ok: true,
            label: "playerctl",
            hint: "Uses MPRIS metadata from the active player",
        }
    } else if command_exists("gdbus") {
        FeatureCheck {
            ok: true,
            label: "MPRIS via gdbus",
            hint: "Uses GNOME/GIO D-Bus access to read active media metadata",
        }
    } else {
        FeatureCheck {
            ok: false,
            label: "missing playerctl/gdbus",
            hint: "Install playerctl or glib2/gdbus and use an MPRIS-compatible player",
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_media_check() -> FeatureCheck {
    FeatureCheck {
        ok: command_exists("osascript"),
        label: "Spotify / Music via AppleScript",
        hint:
            "macOS may ask for Automation permission for Entropy, System Events, Spotify or Music",
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn platform_media_check() -> FeatureCheck {
    FeatureCheck {
        ok: false,
        label: "unsupported OS",
        hint: "Media sync is implemented for Windows, Linux and macOS",
    }
}

#[cfg(target_os = "windows")]
fn platform_layout_check() -> FeatureCheck {
    FeatureCheck {
        ok: true,
        label: "native Windows input layout",
        hint: "Uses the foreground window keyboard layout",
    }
}

#[cfg(target_os = "linux")]
fn platform_layout_check() -> FeatureCheck {
    if std::env::var_os("DISPLAY").is_some() && x11_dl::xlib::Xlib::open().is_ok() {
        FeatureCheck {
            ok: true,
            label: "X11 / XKB",
            hint: "Uses the active XKB keyboard group",
        }
    } else {
        FeatureCheck {
            ok: false,
            label: "missing X11 / XKB",
            hint: "Layout sync currently needs an X11 session",
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_layout_check() -> FeatureCheck {
    FeatureCheck {
        ok: true,
        label: "macOS input source",
        hint: "Uses the current macOS keyboard input source",
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn platform_layout_check() -> FeatureCheck {
    FeatureCheck {
        ok: false,
        label: "unsupported OS",
        hint: "Layout sync is implemented for Windows, Linux X11 and macOS",
    }
}

#[cfg(not(target_os = "windows"))]
fn command_exists(program: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(program).is_file())
}

pub struct QmkHidHostBridge {
    device: crate::device::Device,
    mode: HostDataMode,
    shared_output: Option<crate::hid::SharedHidOutput>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl QmkHidHostBridge {
    pub fn start(
        device: crate::device::Device,
        mode: HostDataMode,
        shared_output: Option<crate::hid::SharedHidOutput>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_device = device.clone();
        let worker_output = shared_output.clone();
        let thread =
            thread::spawn(move || run_bridge(worker_device, mode, worker_output, worker_stop));
        Self {
            device,
            mode,
            shared_output,
            stop,
            thread: Some(thread),
        }
    }

    pub fn mode(&self) -> HostDataMode {
        self.mode
    }

    pub fn uses_shared_output(&self) -> bool {
        self.shared_output.is_some()
    }

    pub fn stop(&mut self) {
        let was_running = self.thread.is_some();
        self.stop.store(true, Ordering::Relaxed);
        if was_running {
            send_shutdown_payloads(&self.device, self.mode, self.shared_output.as_ref());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread::Builder::new()
                .name("qmk-hid-host-join".to_owned())
                .spawn(move || {
                    let _ = thread.join();
                });
        }
    }
}

impl Drop for QmkHidHostBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_bridge(
    target: crate::device::Device,
    mode: HostDataMode,
    shared_output: Option<crate::hid::SharedHidOutput>,
    stop: Arc<AtomicBool>,
) {
    let mut device: Option<HostDataHid> = None;
    let mut last_open_attempt = Instant::now() - Duration::from_secs(5);
    let mut last_time = None;
    let mut last_volume = None;
    let mut last_layout = None;
    let mut last_artist = String::new();
    let mut last_title = String::new();
    let mut last_time_poll = Instant::now() - Duration::from_secs(60);
    let mut last_volume_poll = Instant::now() - Duration::from_secs(60);
    let mut last_layout_poll = Instant::now() - Duration::from_secs(60);
    let mut last_media_poll = Instant::now() - Duration::from_secs(60);
    let mut last_media_full_send = Instant::now() - Duration::from_secs(60);
    let mut last_layout_tracker_attempt = Instant::now() - Duration::from_secs(60);
    let mut layout_tracker = mode.layout.then(LayoutTracker::new).flatten();

    while !stop.load(Ordering::Relaxed) {
        if device.is_none() && last_open_attempt.elapsed() >= Duration::from_secs(2) {
            last_open_attempt = Instant::now();
            device = open_host_data_hid(&target, shared_output.as_ref())
                .map_err(|e| log::warn!("qmk-hid-host open failed: {e}"))
                .ok();
            if device.is_some() {
                log::info!(
                    "qmk-hid-host bridge started ({})",
                    if device.as_ref().is_some_and(HostDataHid::uses_shared_output) {
                        "shared HID owner"
                    } else {
                        "dedicated HID owner"
                    }
                );
            }
        }

        let Some(dev) = device.as_ref() else {
            thread::sleep(Duration::from_millis(250));
            continue;
        };
        if stop.load(Ordering::Relaxed) {
            break;
        }

        #[cfg(target_os = "linux")]
        if !target.uses_bluez_gatt_transport() && !std::path::Path::new(&target.path).exists() {
            log::warn!("qmk-hid-host device path disappeared; reconnecting");
            device = None;
            thread::sleep(Duration::from_millis(250));
            continue;
        }

        let mut write_failed = false;

        if mode.time && last_time_poll.elapsed() >= Duration::from_secs(1) {
            last_time_poll = Instant::now();
            let now = current_time_payload();
            if last_time != Some(now) {
                last_time = Some(now);
                write_failed |= write_payload(dev, &[DATA_TIME, now.0, now.1]).is_err();
                pause_between_packets();
            }
        }

        if mode.volume && last_volume_poll.elapsed() >= Duration::from_secs(2) {
            last_volume_poll = Instant::now();
            if let Some(volume) = current_volume_percent() {
                if last_volume != Some(volume) {
                    last_volume = Some(volume);
                    write_failed |= write_payload(dev, &[DATA_VOLUME, volume]).is_err();
                    pause_between_packets();
                }
            }
        }

        if mode.layout && last_layout_poll.elapsed() >= Duration::from_millis(100) {
            last_layout_poll = Instant::now();
            if layout_tracker.is_none()
                && last_layout_tracker_attempt.elapsed() >= Duration::from_secs(2)
            {
                last_layout_tracker_attempt = Instant::now();
                layout_tracker = LayoutTracker::new();
            }
            if let Some(layout) = layout_tracker
                .as_mut()
                .and_then(LayoutTracker::current_layout_index)
            {
                if last_layout != Some(layout) {
                    last_layout = Some(layout);
                    write_failed |= write_payload(dev, &[DATA_LAYOUT, layout]).is_err();
                    pause_between_packets();
                }
            }
        }

        if mode.media && last_media_poll.elapsed() >= Duration::from_secs(3) {
            last_media_poll = Instant::now();
            let (artist, title) = current_media_info().unwrap_or_default();
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let full_resend = last_media_full_send.elapsed() >= Duration::from_secs(10);
            if full_resend || artist != last_artist {
                last_artist = artist.clone();
                write_failed |= write_text_payload(dev, DATA_MEDIA_ARTIST, &artist).is_err();
                pause_between_packets();
            }
            if full_resend || title != last_title {
                last_title = title.clone();
                write_failed |= write_text_payload(dev, DATA_MEDIA_TITLE, &title).is_err();
                pause_between_packets();
            }
            if full_resend {
                last_media_full_send = Instant::now();
            }
        }

        if write_failed {
            log::warn!("qmk-hid-host bridge write failed; reconnecting");
            device = None;
            last_time = None;
            last_volume = None;
            last_layout = None;
            last_artist.clear();
            last_title.clear();
            last_media_full_send = Instant::now() - Duration::from_secs(60);
        }

        thread::sleep(Duration::from_millis(200));
    }

    log::info!("qmk-hid-host bridge stopped");
}

fn send_shutdown_payloads(
    target: &crate::device::Device,
    mode: HostDataMode,
    shared_output: Option<&crate::hid::SharedHidOutput>,
) {
    let payloads = shutdown_payloads(mode);
    if payloads.is_empty() {
        return;
    }

    let Ok(device) = open_host_data_hid(target, shared_output).map_err(|e| {
        log::warn!("qmk-hid-host shutdown open failed: {e}");
    }) else {
        return;
    };

    for payload in payloads {
        if let Err(e) = write_payload(&device, &payload) {
            log::warn!("qmk-hid-host shutdown write failed: {e}");
            break;
        }
        pause_between_packets();
    }
}

fn shutdown_payloads(mode: HostDataMode) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    if mode.time {
        payloads.push(vec![DATA_TIME, u8::MAX, u8::MAX]);
    }
    if mode.media {
        payloads.push(vec![DATA_MEDIA_ARTIST, 0]);
        payloads.push(vec![DATA_MEDIA_TITLE, 0]);
    }
    payloads
}

enum HostDataHid {
    Shared(crate::hid::SharedHidOutput),
    Dedicated(crate::hid::HidDevice),
}

impl HostDataHid {
    fn uses_shared_output(&self) -> bool {
        matches!(self, Self::Shared(_))
    }

    fn write_output_report(&self, payload: &[u8]) -> anyhow::Result<()> {
        match self {
            Self::Shared(output) => output.write_output_report(payload),
            Self::Dedicated(device) => device.write_output_report(payload),
        }
    }
}

fn open_host_data_hid(
    device: &crate::device::Device,
    shared_output: Option<&crate::hid::SharedHidOutput>,
) -> anyhow::Result<HostDataHid> {
    if let Some(output) = shared_output.filter(|output| output.is_available()) {
        return Ok(HostDataHid::Shared(output.clone()));
    }
    crate::hid::HidDevice::open_fresh_for(device).map(HostDataHid::Dedicated)
}

fn pause_between_packets() {
    thread::sleep(Duration::from_millis(35));
}

fn write_payload(device: &HostDataHid, payload: &[u8]) -> anyhow::Result<()> {
    device.write_output_report(payload)
}

fn write_text_payload(device: &HostDataHid, data_type: u8, value: &str) -> anyhow::Result<()> {
    let mut payload = Vec::with_capacity(RAW_HID_PACKET_LEN);
    let mut bytes = value.as_bytes().to_vec();
    bytes.truncate(30);
    payload.push(data_type);
    payload.push(bytes.len() as u8);
    payload.extend(bytes);
    write_payload(device, &payload)
}

fn current_time_payload() -> (u8, u8) {
    use chrono::Timelike;
    let now = chrono::Local::now();
    (now.hour() as u8, now.minute() as u8)
}

fn layout_code_index(raw: &str) -> Option<u8> {
    let code = normalize_layout_code(raw)?;
    DEFAULT_LAYOUT_CODES
        .iter()
        .position(|candidate| *candidate == code)
        .map(|idx| idx as u8)
}

fn normalize_layout_code(raw: &str) -> Option<&'static str> {
    let normalized = raw
        .trim()
        .trim_start_matches("com.apple.keylayout.")
        .split(['-', '_', '.', ':', '(', '@'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "en" | "us" | "gb" | "uk" | "au" | "ca" => Some("en"),
        "ru" | "russian" => Some("ru"),
        code if code.starts_with("russian") => Some("ru"),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn current_volume_percent() -> Option<u8> {
    windows_platform::volume_percent()
}

#[cfg(target_os = "linux")]
fn current_volume_percent() -> Option<u8> {
    command_stdout("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .and_then(|out| {
            out.split_whitespace()
                .find_map(|part| part.parse::<f32>().ok())
                .map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u8)
        })
        .or_else(|| {
            command_stdout("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).and_then(|out| {
                out.split_whitespace()
                    .find(|part| part.ends_with('%'))
                    .and_then(|part| part.trim_end_matches('%').parse::<u8>().ok())
            })
        })
}

#[cfg(target_os = "macos")]
fn current_volume_percent() -> Option<u8> {
    macos_automation_stdout(&["-e", "output volume of (get volume settings)"])
        .and_then(|out| out.trim().parse::<u8>().ok())
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn current_volume_percent() -> Option<u8> {
    None
}

#[cfg(target_os = "windows")]
fn current_media_info() -> Option<(String, String)> {
    windows_platform::media_info()
}

#[cfg(target_os = "linux")]
fn current_media_info() -> Option<(String, String)> {
    command_stdout(
        "playerctl",
        &["metadata", "--format", "{{artist}}\t{{title}}"],
    )
    .and_then(|out| split_media_line(&out))
    .or_else(|| {
        command_stdout(
            "playerctl",
            &[
                "-a",
                "metadata",
                "--format",
                "{{status}}\t{{artist}}\t{{title}}",
            ],
        )
        .and_then(|out| split_playerctl_all_metadata(&out))
    })
    .or_else(mpris_media_info_via_gdbus)
}

#[cfg(target_os = "macos")]
fn current_media_info() -> Option<(String, String)> {
    let script = r#"
set mediaArtist to ""
set mediaTitle to ""
tell application "System Events"
    if exists process "Spotify" then
        tell application "Spotify"
            if player state is not stopped then
                set mediaArtist to artist of current track
                set mediaTitle to name of current track
            end if
        end tell
    else if exists process "Music" then
        tell application "Music"
            if player state is not stopped then
                set mediaArtist to artist of current track
                set mediaTitle to name of current track
            end if
        end tell
    end if
end tell
return mediaArtist & tab & mediaTitle
"#;
    macos_automation_stdout(&["-e", script]).and_then(|out| split_media_line(&out))
}

#[cfg(target_os = "macos")]
fn macos_layout_code() -> Option<String> {
    // Carbon's Text Input Source APIs assert that they run on the main queue
    // on current macOS releases. The host bridge itself stays on its worker;
    // only the short system query crosses to the UI-owned queue.
    dispatch2::run_on_main(|_| macos_layout_code_on_main_thread())
}

#[cfg(target_os = "macos")]
fn macos_layout_code_on_main_thread() -> Option<String> {
    use std::ffi::c_void;

    type CFArrayRef = *const c_void;
    type CFIndex = isize;
    type CFStringRef = *const c_void;
    type TISInputSourceRef = *const c_void;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        static kTISPropertyInputSourceLanguages: CFStringRef;
        fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
        fn TISGetInputSourceProperty(
            input_source: TISInputSourceRef,
            property_key: CFStringRef,
        ) -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, index: CFIndex) -> *const c_void;
        fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut i8,
            buffer_size: CFIndex,
            encoding: u32,
        ) -> bool;
        fn CFRelease(cf: *const c_void);
    }

    unsafe fn cf_string_to_string(value: CFStringRef) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let mut buffer = [0i8; 64];
        if !CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }

    unsafe {
        let source = TISCopyCurrentKeyboardInputSource();
        if source.is_null() {
            return None;
        }

        let languages =
            TISGetInputSourceProperty(source, kTISPropertyInputSourceLanguages) as CFArrayRef;
        let code = if languages.is_null() || CFArrayGetCount(languages) <= 0 {
            None
        } else {
            let value = CFArrayGetValueAtIndex(languages, 0) as CFStringRef;
            cf_string_to_string(value)
        };

        CFRelease(source);
        code
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn current_media_info() -> Option<(String, String)> {
    None
}

#[cfg(not(target_os = "windows"))]
#[cfg(target_os = "macos")]
fn macos_automation_stdout(args: &[&str]) -> Option<String> {
    command_stdout_timeout("osascript", args, MACOS_AUTOMATION_COMMAND_TIMEOUT)
}

#[cfg(not(target_os = "windows"))]
#[cfg(target_os = "linux")]
fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    command_stdout_timeout(program, args, Duration::from_secs(10))
}

#[cfg(not(target_os = "windows"))]
fn command_stdout_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    use std::io::Read;

    let mut child = std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    let started_at = Instant::now();
    loop {
        if let Some(status) = child.try_wait().ok()? {
            let mut stdout = Vec::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_end(&mut stdout).ok()?;
            }
            return status.success().then(|| String::from_utf8(stdout).ok())?;
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            log::warn!("{program} timed out after {} ms", timeout.as_millis());
            return None;
        }

        thread::sleep(COMMAND_POLL_INTERVAL);
    }
}

#[cfg(not(target_os = "windows"))]
fn split_media_line(line: &str) -> Option<(String, String)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(2, '\t');
    let artist = parts.next().unwrap_or_default().trim().to_string();
    let title = parts.next().unwrap_or_default().trim().to_string();
    (!artist.is_empty() || !title.is_empty()).then_some((artist, title))
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn layout_live_data_uses_the_connected_hid_owner() {
        let (device, recorder) = crate::hid::HidDevice::test_device();
        let output = device.shared_output().unwrap();
        let target = crate::device::Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "test".to_owned(),
            bus_type: "Bluetooth".to_owned(),
            path: "test".to_owned(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        };
        let host_data_hid = open_host_data_hid(&target, Some(&output)).unwrap();

        assert!(host_data_hid.uses_shared_output());
        write_payload(&host_data_hid, &[DATA_LAYOUT, 1]).unwrap();

        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(&requests[0][..4], &[DATA_LAYOUT, 1, 0, 0]);
    }

    #[test]
    fn layout_code_index_maps_ru_en_aliases() {
        assert_eq!(layout_code_index("en"), Some(0));
        assert_eq!(layout_code_index("us"), Some(0));
        assert_eq!(layout_code_index("gb"), Some(0));
        assert_eq!(layout_code_index("ru"), Some(1));
        assert_eq!(layout_code_index("com.apple.keylayout.RussianWin"), Some(1));
        assert_eq!(layout_code_index("de"), None);
    }

    #[test]
    fn command_stdout_timeout_returns_successful_output() {
        let output =
            command_stdout_timeout("/bin/sh", &["-c", "printf entropy"], Duration::from_secs(1));

        assert_eq!(output.as_deref(), Some("entropy"));
    }

    #[test]
    fn command_stdout_timeout_stops_slow_command() {
        let started_at = Instant::now();
        let output = command_stdout_timeout(
            "/bin/sh",
            &["-c", "sleep 2; printf late"],
            Duration::from_millis(50),
        );

        assert!(output.is_none());
        assert!(started_at.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn shutdown_payloads_clear_time_and_media() {
        let payloads = shutdown_payloads(HostDataMode {
            time: true,
            volume: true,
            layout: true,
            media: true,
        });

        assert_eq!(
            payloads,
            vec![
                vec![DATA_TIME, u8::MAX, u8::MAX],
                vec![DATA_MEDIA_ARTIST, 0],
                vec![DATA_MEDIA_TITLE, 0],
            ]
        );
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn layout_code_index_maps_ru_en_aliases() {
        assert_eq!(layout_code_index("en"), Some(0));
        assert_eq!(layout_code_index("us"), Some(0));
        assert_eq!(layout_code_index("gb"), Some(0));
        assert_eq!(layout_code_index("ru"), Some(1));
        assert_eq!(layout_code_index("com.apple.keylayout.RussianWin"), Some(1));
        assert_eq!(layout_code_index("de"), None);
    }
}

#[cfg(target_os = "linux")]
struct LayoutTracker {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    keyboard: x11_dl::xlib::XkbDescPtr,
    symbols: Vec<String>,
}

#[cfg(target_os = "linux")]
impl LayoutTracker {
    fn new() -> Option<Self> {
        unsafe {
            let xlib = x11_dl::xlib::Xlib::open().ok()?;
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return None;
            }
            let keyboard = (xlib.XkbAllocKeyboard)();
            if keyboard.is_null() {
                (xlib.XCloseDisplay)(display);
                return None;
            }
            let Some(symbols) = linux_xkb_symbols(&xlib, display, keyboard) else {
                (xlib.XkbFreeKeyboard)(keyboard, 0, 1);
                (xlib.XCloseDisplay)(display);
                return None;
            };
            Some(Self {
                xlib,
                display,
                keyboard,
                symbols,
            })
        }
    }

    fn current_layout_index(&mut self) -> Option<u8> {
        const XKB_USE_CORE_KBD: u32 = 0x0100;

        unsafe {
            let mut state: x11_dl::xlib::XkbStateRec = std::mem::zeroed();
            if (self.xlib.XkbGetState)(self.display, XKB_USE_CORE_KBD, &mut state) != 0 {
                return None;
            }
            let group = state.group as usize;
            let raw = self.symbols.get(group + 1)?;
            let layout = raw.split([':', '(']).next().unwrap_or_default();
            layout_code_index(layout)
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for LayoutTracker {
    fn drop(&mut self) {
        unsafe {
            if !self.keyboard.is_null() {
                (self.xlib.XkbFreeKeyboard)(self.keyboard, 0, 1);
            }
            if !self.display.is_null() {
                (self.xlib.XCloseDisplay)(self.display);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_xkb_symbols(
    xlib: &x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    keyboard: x11_dl::xlib::XkbDescPtr,
) -> Option<Vec<String>> {
    const XKB_SYMBOLS_NAME_MASK: u32 = 1 << 2;

    unsafe {
        if (xlib.XkbGetNames)(display, XKB_SYMBOLS_NAME_MASK, keyboard) != 0 {
            return None;
        }
        let names = (*keyboard).names;
        if names.is_null() {
            return None;
        }
        let symbols_atom = (*names).symbols;
        let symbols_ptr = (xlib.XGetAtomName)(display, symbols_atom);
        if symbols_ptr.is_null() {
            return None;
        }
        let symbols = std::ffi::CStr::from_ptr(symbols_ptr)
            .to_string_lossy()
            .into_owned();
        (xlib.XFree)(symbols_ptr.cast());
        Some(symbols.split('+').map(str::to_owned).collect())
    }
}

#[cfg(target_os = "windows")]
struct LayoutTracker;

#[cfg(target_os = "windows")]
impl LayoutTracker {
    fn new() -> Option<Self> {
        Some(Self)
    }

    fn current_layout_index(&mut self) -> Option<u8> {
        windows_platform::layout_code().and_then(|code| layout_code_index(&code))
    }
}

#[cfg(target_os = "macos")]
struct LayoutTracker;

#[cfg(target_os = "macos")]
impl LayoutTracker {
    fn new() -> Option<Self> {
        Some(Self)
    }

    fn current_layout_index(&mut self) -> Option<u8> {
        macos_layout_code().and_then(|code| layout_code_index(&code))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
struct LayoutTracker;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
impl LayoutTracker {
    fn new() -> Option<Self> {
        None
    }

    fn current_layout_index(&mut self) -> Option<u8> {
        None
    }
}

#[cfg(target_os = "linux")]
fn split_playerctl_all_metadata(output: &str) -> Option<(String, String)> {
    let mut fallback = None;
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let status = parts.next().unwrap_or_default().trim();
        let artist = parts.next().unwrap_or_default().trim().to_string();
        let title = parts.next().unwrap_or_default().trim().to_string();
        if artist.is_empty() && title.is_empty() {
            continue;
        }
        if status.eq_ignore_ascii_case("playing") {
            return Some((artist, title));
        }
        fallback.get_or_insert((artist, title));
    }
    fallback
}

#[cfg(target_os = "linux")]
fn mpris_media_info_via_gdbus() -> Option<(String, String)> {
    let names = command_stdout(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.ListNames",
        ],
    )?;

    let mut fallback = None;
    for name in gvariant_quoted_strings(&names)
        .into_iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
    {
        let Some(metadata) = gdbus_get_mpris_property(&name, "Metadata") else {
            continue;
        };
        let Some(media) = split_gdbus_mpris_metadata(&metadata) else {
            continue;
        };
        let is_playing = gdbus_get_mpris_property(&name, "PlaybackStatus")
            .map(|status| status.contains("'Playing'") || status.contains("\"Playing\""))
            .unwrap_or(false);
        if is_playing {
            return Some(media);
        }
        fallback.get_or_insert(media);
    }
    fallback
}

#[cfg(target_os = "linux")]
fn gdbus_get_mpris_property(name: &str, property: &str) -> Option<String> {
    command_stdout(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            name,
            "--object-path",
            "/org/mpris/MediaPlayer2",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.mpris.MediaPlayer2.Player",
            property,
        ],
    )
}

#[cfg(target_os = "linux")]
fn split_gdbus_mpris_metadata(metadata: &str) -> Option<(String, String)> {
    let artist = gvariant_metadata_string(metadata, "xesam:artist").unwrap_or_default();
    let title = gvariant_metadata_string(metadata, "xesam:title").unwrap_or_default();
    (!artist.is_empty() || !title.is_empty()).then_some((artist, title))
}

#[cfg(target_os = "linux")]
fn gvariant_metadata_string(metadata: &str, key: &str) -> Option<String> {
    let key_idx = metadata.find(key)?;
    let tail = &metadata[key_idx + key.len()..];
    let value_idx = tail.find('<').map(|idx| idx + 1).unwrap_or(0);
    gvariant_quoted_strings(&tail[value_idx..])
        .into_iter()
        .find(|value| !value.trim().is_empty())
}

#[cfg(target_os = "linux")]
fn gvariant_quoted_strings(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut current = String::new();

    for ch in text.chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '\'' {
                values.push(current.clone());
                current.clear();
                in_string = false;
            } else {
                current.push(ch);
            }
        } else if ch == '\'' {
            in_string = true;
        }
    }

    values
}

#[cfg(target_os = "windows")]
mod windows_platform {
    use windows::{
        Media::Control::GlobalSystemMediaTransportControlsSessionManager,
        Win32::{
            Media::Audio::{
                eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                MMDeviceEnumerator,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
        },
    };
    use windows_sys::Win32::{
        Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME},
        UI::{
            Input::KeyboardAndMouse::GetKeyboardLayout,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    };

    pub fn volume_percent() -> Option<u8> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER).ok()?;
            let endpoint = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .ok()?;
            let volume = endpoint
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .ok()?;
            let scalar = volume.GetMasterVolumeLevelScalar().ok()?;
            Some((scalar * 100.0).round().clamp(0.0, 100.0) as u8)
        }
    }

    pub fn media_info() -> Option<(String, String)> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .and_then(|request| request.get())
            .ok()?;
        let session = manager.GetCurrentSession().ok()?;
        let props = session
            .TryGetMediaPropertiesAsync()
            .and_then(|request| request.get())
            .ok()?;
        let artist = props.Artist().unwrap_or_default().to_string();
        let title = props.Title().unwrap_or_default().to_string();
        (!artist.is_empty() || !title.is_empty()).then_some((artist, title))
    }

    pub fn layout_code() -> Option<String> {
        unsafe {
            let focused_window = GetForegroundWindow();
            let active_thread = GetWindowThreadProcessId(focused_window, std::ptr::null_mut());
            let layout = GetKeyboardLayout(active_thread);
            let locale_id = (layout as usize & 0xFFFF) as u32;
            let mut buffer = [0u16; 9];
            let len = GetLocaleInfoW(
                locale_id,
                LOCALE_SISO639LANGNAME,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            );
            if len <= 1 {
                return None;
            }
            String::from_utf16(&buffer[..len as usize - 1]).ok()
        }
    }
}
