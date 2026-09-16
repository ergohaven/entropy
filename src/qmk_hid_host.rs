//! Small built-in qmk-hid-host bridge for display presets that expect host data.
//! Sends the same Raw HID packet family as https://github.com/ergohaven/qmk-hid-host.

use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc, Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RAW_HID_PACKET_LEN: usize = 32;
const DATA_TIME: u8 = 0xAA;
const DATA_VOLUME: u8 = 0xAB;
const DATA_LAYOUT: u8 = 0xAC;
const DATA_MEDIA_ARTIST: u8 = 0xAD;
const DATA_MEDIA_TITLE: u8 = 0xAE;
const DATA_DATE: u8 = 0xAF;
const DEFAULT_LAYOUT_CODES: [&str; 2] = ["en", "ru"];
const LAYOUT_RESEND_INTERVAL: Duration = Duration::from_secs(10);
// One process-wide desktop sampler, not one uninterruptible query thread per
// bridge/reconnect. It never owns HID or a host-output lease. Demand and cached
// results share one short-held lock; no desktop call executes under that lock.
static HOST_DATA_SERVICE: OnceLock<HostDataService> = OnceLock::new();

pub fn media_snapshot() -> Option<(String, String)> {
    HOST_DATA_SERVICE
        .get()
        .and_then(|service| service.snapshot().media)
        .filter(|(artist, title)| !artist.is_empty() || !title.is_empty())
}

#[derive(Clone, Default)]
struct DesktopSnapshot {
    volume: Option<u8>,
    layout: Option<u8>,
    // None means no completed sample, Some(empty) means playback stopped.
    media: Option<(String, String)>,
    media_query_ms: u128,
    media_sampled_at: Option<Instant>,
}

#[derive(Default)]
struct DesktopState {
    demand: [usize; 3],
    epoch: [u64; 3],
    snapshot: DesktopSnapshot,
    stopped: bool,
}

#[derive(Default)]
struct DesktopShared {
    state: Mutex<DesktopState>,
    wake: std::sync::Condvar,
}

struct DesktopOwner {
    shared: Arc<DesktopShared>,
}

impl Drop for DesktopOwner {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        self.shared.wake.notify_one();
    }
}

#[derive(Clone)]
struct HostDataService {
    owner: Arc<DesktopOwner>,
}

trait DesktopSource {
    fn volume(&mut self) -> Option<u8>;
    fn layout(&mut self) -> Option<u8>;
    fn media(&mut self) -> Option<(String, String)>;
}

struct NativeDesktopSource {
    layout: Option<LayoutTracker>,
    last_layout_attempt: Instant,
}

impl NativeDesktopSource {
    fn new() -> Self {
        Self {
            layout: None,
            last_layout_attempt: Instant::now() - Duration::from_secs(60),
        }
    }
}

impl DesktopSource for NativeDesktopSource {
    fn volume(&mut self) -> Option<u8> {
        current_volume_percent()
    }

    fn layout(&mut self) -> Option<u8> {
        if self.layout.is_none() && self.last_layout_attempt.elapsed() >= Duration::from_secs(2) {
            self.last_layout_attempt = Instant::now();
            self.layout = LayoutTracker::new();
        }
        self.layout
            .as_mut()
            .and_then(LayoutTracker::current_layout_index)
    }

    fn media(&mut self) -> Option<(String, String)> {
        current_media_info()
    }
}

impl HostDataService {
    fn start<S: DesktopSource + 'static>(source: impl FnOnce() -> S + Send + 'static) -> Self {
        let shared = Arc::new(DesktopShared::default());
        let worker = shared.clone();
        // Construct platform objects on their owning thread (not all desktop
        // handles are Send). One sampler lives for the production process;
        // when idle it waits for subscribers, without any OS polling.
        thread::spawn(move || run_desktop_service(worker, source()));
        Self {
            owner: Arc::new(DesktopOwner { shared }),
        }
    }

    fn subscribe(&self, mode: HostDataMode) -> DesktopSubscription {
        let enabled = [mode.volume, mode.layout, mode.media];
        let mut state = self
            .owner
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (index, enabled) in enabled.iter().enumerate() {
            if *enabled {
                if state.demand[index] == 0 {
                    state.epoch[index] = state.epoch[index].wrapping_add(1);
                }
                state.demand[index] += 1;
            }
        }
        self.owner.shared.wake.notify_one();
        DesktopSubscription {
            service: self.clone(),
            enabled,
        }
    }

    fn snapshot(&self) -> DesktopSnapshot {
        self.owner
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .clone()
    }
}

struct DesktopSubscription {
    service: HostDataService,
    enabled: [bool; 3],
}

impl Drop for DesktopSubscription {
    fn drop(&mut self) {
        let shared = &self.service.owner.shared;
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        for (index, enabled) in self.enabled.iter().enumerate() {
            if *enabled {
                state.demand[index] -= 1;
                if state.demand[index] == 0 {
                    match index {
                        0 => state.snapshot.volume = None,
                        1 => state.snapshot.layout = None,
                        _ => {
                            state.snapshot.media = None;
                            state.snapshot.media_sampled_at = None;
                        }
                    }
                }
            }
        }
        shared.wake.notify_one();
    }
}

fn run_desktop_service(shared: Arc<DesktopShared>, mut source: impl DesktopSource) {
    let intervals = [
        VOLUME_POLL_INTERVAL,
        Duration::from_millis(100),
        Duration::from_secs(3),
    ];
    let mut last_poll = [Instant::now() - Duration::from_secs(60); 3];
    let mut last_epoch = [0; 3];
    loop {
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        while !state.stopped && state.demand == [0; 3] {
            state = shared.wake.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        if state.stopped {
            break;
        }
        drop(state);
        for index in 0..3 {
            let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.stopped {
                return;
            }
            let epoch = state.epoch[index];
            let due = state.demand[index] > 0
                && (last_epoch[index] != epoch || last_poll[index].elapsed() >= intervals[index]);
            drop(state);
            if !due {
                continue;
            }
            last_epoch[index] = epoch;
            last_poll[index] = Instant::now();
            let started = Instant::now();
            let mut sample = DesktopSnapshot::default();
            match index {
                0 => sample.volume = source.volume(),
                1 => sample.layout = source.layout(),
                _ => {
                    sample.media = Some(source.media().unwrap_or_default());
                    sample.media_query_ms = started.elapsed().as_millis();
                    sample.media_sampled_at = Some(Instant::now());
                }
            }
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.stopped {
                return;
            }
            // A completed old query may not populate a newly enabled session.
            if state.demand[index] > 0 && state.epoch[index] == epoch {
                match index {
                    0 => state.snapshot.volume = sample.volume,
                    1 => state.snapshot.layout = sample.layout,
                    _ => {
                        state.snapshot.media = sample.media;
                        state.snapshot.media_query_ms = sample.media_query_ms;
                        state.snapshot.media_sampled_at = sample.media_sampled_at;
                    }
                }
            }
        }
        let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            break;
        }
        let _ = shared.wake.wait_timeout(state, Duration::from_millis(20));
    }
}

#[cfg(test)]
struct TestMediaSource<M>(M);

#[cfg(test)]
impl<M: FnMut() -> Option<(String, String)>> DesktopSource for TestMediaSource<M> {
    fn volume(&mut self) -> Option<u8> {
        panic!("media-only fixture must not query real desktop volume")
    }
    fn layout(&mut self) -> Option<u8> {
        panic!("media-only fixture must not query real desktop layout")
    }
    fn media(&mut self) -> Option<(String, String)> {
        (self.0)()
    }
}

#[cfg(target_os = "linux")]
const KDE_LAYOUT_DESTINATION: &str = "org.kde.keyboard";
#[cfg(target_os = "linux")]
const KDE_LAYOUT_PATH: &str = "/Layouts";
#[cfg(target_os = "linux")]
const KDE_LAYOUT_INTERFACE: &str = "org.kde.KeyboardLayouts";
#[cfg(target_os = "linux")]
const IBUS_DESTINATION: &str = "org.freedesktop.IBus";
#[cfg(target_os = "linux")]
const IBUS_PATH: &str = "/org/freedesktop/IBus";
#[cfg(target_os = "linux")]
const IBUS_INTERFACE: &str = "org.freedesktop.IBus";
const DATA_HOST_STATUS: u8 = 0xBA;
// Native Windows audio queries can follow each bridge tick. Only changed
// percentages are sent, allowing the display to retarget small volume steps.
#[cfg(target_os = "windows")]
const VOLUME_POLL_INTERVAL: Duration = Duration::from_millis(20);
#[cfg(any(target_os = "linux", target_os = "macos"))]
const VOLUME_POLL_INTERVAL: Duration = Duration::from_millis(40);
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
const VOLUME_POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(test)]
mod clock_tests;
#[cfg(any(target_os = "macos", test))]
mod macos_volume;
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
    if entropy_ibus_layout_available() {
        FeatureCheck {
            ok: true,
            label: "Entropy Text Expander / IBus",
            hint: "Uses the EN/RU layout exposed by the active Entropy input source",
        }
    } else if kde_layout_available() {
        FeatureCheck {
            ok: true,
            label: "KDE Plasma / D-Bus",
            hint: "Uses the active KDE keyboard layout on Wayland and X11",
        }
    } else if gnome_ibus_layout_available() {
        FeatureCheck {
            ok: true,
            label: "GNOME / IBus D-Bus",
            hint: "Uses the active GNOME keyboard layout on Wayland and X11",
        }
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        FeatureCheck {
            ok: false,
            label: "unsupported Wayland session",
            hint: "Wayland Layout Sync is currently available on GNOME and KDE Plasma",
        }
    } else if std::env::var_os("DISPLAY").is_some() && x11_dl::xlib::Xlib::open().is_ok() {
        FeatureCheck {
            ok: true,
            label: "X11 / XKB",
            hint: "Uses the active XKB keyboard group",
        }
    } else {
        FeatureCheck {
            ok: false,
            label: "missing X11 / XKB",
            hint: "Layout Sync needs GNOME, KDE Plasma or an X11 session",
        }
    }
}

#[cfg(target_os = "linux")]
fn kde_layout_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| KdeLayoutTracker::new().is_some())
}

#[cfg(target_os = "linux")]
fn gnome_ibus_layout_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        if !gnome_desktop_session() {
            return false;
        }
        IbusLayoutTracker::new()
            .and_then(|mut tracker| tracker.current_engine_state())
            .and_then(|state| state.layout)
            .is_some()
    })
}

#[cfg(target_os = "linux")]
fn entropy_ibus_layout_available() -> bool {
    IbusLayoutTracker::new()
        .and_then(|mut tracker| tracker.current_engine_state())
        .is_some_and(|state| state.entropy && state.layout.is_some())
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

/// Stops transport ownership independently of desktop-query progress. This
/// mutex protects only token publication, never HID I/O or desktop queries.
#[derive(Default)]
struct BridgeTransportControl {
    stop: AtomicBool,
    retirement: std::sync::Mutex<Option<crate::hid::HidRetirement>>,
    selected_handoff: std::sync::Mutex<Option<(crate::hid::HidDevice, bool)>>,
}

impl BridgeTransportControl {
    fn handoff_selected(
        &self,
        hid: crate::hid::HidDevice,
        extended: bool,
    ) -> Result<(), crate::hid::HidDevice> {
        if self.stop.load(Ordering::Acquire) {
            return Err(hid);
        }
        let mut handoff = self
            .selected_handoff
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.stop.load(Ordering::Acquire) || handoff.is_some() {
            return Err(hid);
        }
        *handoff = Some((hid, extended));
        Ok(())
    }

    fn take_selected_handoff(&self) -> Option<(crate::hid::HidDevice, bool)> {
        self.selected_handoff
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    fn publish(&self, token: Option<crate::hid::HidRetirement>) -> anyhow::Result<()> {
        if self.stop.load(Ordering::Acquire) {
            if let Some(token) = token {
                token.retire();
            }
            anyhow::bail!("Host bridge stopped during open");
        }
        if let Ok(mut current) = self.retirement.try_lock() {
            *current = token;
        } else {
            if let Some(token) = token {
                token.retire();
            }
            anyhow::bail!("Host bridge transport publication busy");
        }
        // Covers stop racing with publication, including stop's failed try_lock.
        if self.stop.load(Ordering::Acquire) {
            self.retire();
            anyhow::bail!("Host bridge stopped during open");
        }
        Ok(())
    }

    fn retire(&self) {
        self.stop.store(true, Ordering::Release);
        self.selected_handoff
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Ok(mut token) = self.retirement.try_lock() {
            if let Some(token) = token.take() {
                token.retire();
            }
        }
    }
}

// Date settings and the BA/AF receiver ship together. Query the existing Vial
// capability list, never BA/AF themselves (legacy firmware echoes those reports).
pub(crate) fn supports_extended_host_protocol(settings: &[u16]) -> bool {
    crate::app::DATE_QSIDS[..10]
        .iter()
        .all(|id| settings.contains(id))
}

/// Original Ergohaven LCD definitions advertise brightness/timeout, not a
/// `liveFeatures` clock or a clock layout preset. Their home screen consumes
/// legacy AA hours/minutes. Both definition fields and a fresh QMK setting list
/// are required: firmware also lists 318/319 on boards without that LCD.
/// This authorizes time only, never BA/AF, and is not an RMK capability probe.
pub(crate) fn supports_legacy_lcd_clock(
    definition: &serde_json::Value,
    live_settings: &[u16],
) -> bool {
    let fields = [
        (crate::app::DISPLAY_BRIGHTNESS_QSID, "integer"),
        (crate::app::DISPLAY_TIMEOUT_QSID, "select"),
    ];
    fields.iter().all(|(id, _)| live_settings.contains(id))
        && definition
            .get("settings")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tabs| {
                tabs.iter().any(|tab| {
                    tab.get("name").and_then(serde_json::Value::as_str) == Some("LCD settings")
                        && tab
                            .get("fields")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|declared| {
                                fields.iter().all(|(id, kind)| {
                                    declared.iter().any(|field| {
                                        field.get("qsid").and_then(serde_json::Value::as_u64)
                                            == Some(u64::from(*id))
                                            && field.get("type").and_then(serde_json::Value::as_str)
                                                == Some(*kind)
                                    })
                                })
                            })
                })
            })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostProtocol {
    // The selected connection already queried capabilities through its HID owner.
    Selected(bool),
    // Automatic bridges must discover through their own, otherwise unused owner.
    Discover,
}

impl HostProtocol {
    fn extended(self, device: &HostDataHid) -> bool {
        match (self, device) {
            // Permission belongs to the selected physical handle, not its path.
            (Self::Selected(supported), HostDataHid::Shared(_)) => supported,
            (Self::Selected(_), HostDataHid::Dedicated(_)) => false,
            (Self::Discover, HostDataHid::Dedicated(hid)) => {
                #[cfg(target_os = "macos")]
                let _lock = hid.macos_hid_operation_lock();
                hid.query_qmk_settings()
                    .is_ok_and(|settings| supports_extended_host_protocol(&settings))
            }
            (Self::Discover, HostDataHid::Shared(_)) => false,
        }
    }

    fn connection_lost(&mut self) {
        if matches!(self, Self::Selected(_)) {
            // A replacement physical device needs a new selected connection query.
            *self = Self::Selected(false);
        }
    }
}

// One ordering domain per physical HID owner, shared by every output clone.
// Claiming a generation is lock-free: only workers serialize actual HID writes.
#[derive(Default)]
pub(crate) struct HostOutputOwner {
    generation: std::sync::atomic::AtomicU64,
    active: Mutex<Option<HostOutputFootprint>>,
}

#[derive(Clone, Copy)]
struct HostOutputFootprint {
    generation: u64,
    mode: HostDataMode,
    extended: bool,
}

#[derive(Clone)]
pub(crate) struct HostOutputLease {
    owner: Arc<HostOutputOwner>,
    footprint: HostOutputFootprint,
}

impl HostOutputOwner {
    pub(crate) fn claim(self: &Arc<Self>, mode: HostDataMode, extended: bool) -> HostOutputLease {
        let generation = self
            .generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        HostOutputLease {
            owner: self.clone(),
            footprint: HostOutputFootprint {
                generation,
                mode,
                extended,
            },
        }
    }
}

impl HostOutputLease {
    pub(crate) fn is_current(&self) -> bool {
        self.owner.generation.load(Ordering::SeqCst) == self.footprint.generation
    }

    pub(crate) fn write(
        &self,
        payload: &[u8],
        mut send: impl FnMut(&[u8]) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut active = self
            .owner
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Check inside the output ordering lock, not before waiting for I/O.
        anyhow::ensure!(self.is_current(), "Host bridge output session superseded");
        if active.as_ref().map(|old| old.generation) != Some(self.footprint.generation) {
            if let Some(old) = *active {
                let removed = HostDataMode {
                    time: old.mode.time && !self.footprint.mode.time,
                    media: old.mode.media && !self.footprint.mode.media,
                    ..Default::default()
                };
                for clear in shutdown_payloads(removed, old.extended) {
                    send(&clear)?;
                }
            }
            // Retain the last initialized footprint across stop/re-enable and
            // skipped generations, so dropped modes are cleared by the successor.
            *active = Some(self.footprint);
        }
        send(payload)
    }

    pub(crate) fn shutdown(
        &self,
        mut send: impl FnMut(&[u8]) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut active = self
            .owner
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !self.is_current() {
            return Ok(());
        }
        let mut mode = self.footprint.mode;
        let mut extended = self.footprint.extended;
        if let Some(old) = *active {
            mode.time |= old.mode.time;
            mode.media |= old.mode.media;
            extended |= old.extended;
        }
        // A final shutdown is a single ordered batch. A concurrent new claim
        // never blocks the UI and its worker writes only after this batch ends.
        for clear in shutdown_payloads(mode, extended) {
            send(&clear)?;
        }
        *active = None;
        Ok(())
    }
}

pub struct QmkHidHostBridge {
    device: crate::device::Device,
    mode: HostDataMode,
    protocol: HostProtocol,
    shared_output: Option<crate::hid::SharedHidOutput>,
    control: Arc<BridgeTransportControl>,
    thread: Option<JoinHandle<()>>,
    send_shutdown_on_drop: Arc<AtomicBool>,
    layout_snapshot: Arc<AtomicU8>,
}

impl QmkHidHostBridge {
    /// Registry-only seam: no worker, filesystem probe, desktop service or HID.
    #[cfg(test)]
    pub(crate) fn test_inert(
        device: crate::device::Device,
        mode: HostDataMode,
        shared_output: Option<crate::hid::SharedHidOutput>,
        protocol: HostProtocol,
    ) -> Self {
        Self {
            device,
            mode,
            protocol,
            shared_output,
            control: Arc::new(BridgeTransportControl::default()),
            thread: None,
            send_shutdown_on_drop: Arc::new(AtomicBool::new(true)),
            layout_snapshot: Arc::new(AtomicU8::new(u8::MAX)),
        }
    }

    pub fn start(
        device: crate::device::Device,
        mode: HostDataMode,
        shared_output: Option<crate::hid::SharedHidOutput>,
        protocol: HostProtocol,
    ) -> Self {
        Self::start_with_desktop_service(
            device,
            mode,
            shared_output,
            protocol,
            HOST_DATA_SERVICE
                .get_or_init(|| HostDataService::start(NativeDesktopSource::new))
                .clone(),
            open_host_data_hid,
        )
    }

    #[cfg(test)]
    fn start_with_media_source(
        device: crate::device::Device,
        mode: HostDataMode,
        shared_output: Option<crate::hid::SharedHidOutput>,
        protocol: HostProtocol,
        media_source: impl FnMut() -> Option<(String, String)> + Send + 'static,
    ) -> Self {
        Self::start_with_sources(
            device,
            mode,
            shared_output,
            protocol,
            media_source,
            open_host_data_hid,
        )
    }

    #[cfg(test)]
    fn start_with_sources(
        device: crate::device::Device,
        mode: HostDataMode,
        shared_output: Option<crate::hid::SharedHidOutput>,
        protocol: HostProtocol,
        media_source: impl FnMut() -> Option<(String, String)> + Send + 'static,
        open_hid: impl FnMut(
                &crate::device::Device,
                Option<&crate::hid::SharedHidOutput>,
            ) -> anyhow::Result<HostDataHid>
            + Send
            + 'static,
    ) -> Self {
        Self::start_with_desktop_service(
            device,
            mode,
            shared_output,
            protocol,
            HostDataService::start(move || TestMediaSource(media_source)),
            open_hid,
        )
    }

    fn start_with_desktop_service(
        device: crate::device::Device,
        mode: HostDataMode,
        shared_output: Option<crate::hid::SharedHidOutput>,
        protocol: HostProtocol,
        desktop: HostDataService,
        open_hid: impl FnMut(
                &crate::device::Device,
                Option<&crate::hid::SharedHidOutput>,
            ) -> anyhow::Result<HostDataHid>
            + Send
            + 'static,
    ) -> Self {
        let shared_output = shared_output.map(|output| {
            output.for_host_bridge(mode, matches!(protocol, HostProtocol::Selected(true)))
        });
        let control = Arc::new(BridgeTransportControl::default());
        let worker_control = control.clone();
        let worker_device = device.clone();
        let worker_output = shared_output.clone();
        let layout_snapshot = Arc::new(AtomicU8::new(u8::MAX));
        let worker_layout = layout_snapshot.clone();
        let send_shutdown_on_drop = Arc::new(AtomicBool::new(true));
        let worker_shutdown = send_shutdown_on_drop.clone();
        let thread = thread::spawn(move || {
            run_bridge(
                worker_device,
                mode,
                worker_output,
                worker_control,
                worker_layout,
                protocol,
                worker_shutdown,
                desktop,
                open_hid,
            )
        });
        Self {
            device,
            mode,
            protocol,
            shared_output,
            control,
            thread: Some(thread),
            send_shutdown_on_drop,
            layout_snapshot,
        }
    }

    pub fn layout_label(&self) -> Option<&'static str> {
        layout_snapshot_label(&self.layout_snapshot)
    }

    pub(crate) fn protocol(&self) -> HostProtocol {
        self.protocol
    }

    /// Original enumeration bound to this bridge, not a stable device cache.
    /// Registry reconciliation must check it against the current enumeration
    /// with `Device::permits_hid_target` before preserving mode or ownership.
    pub(crate) fn device(&self) -> &crate::device::Device {
        &self.device
    }

    pub fn mode(&self) -> HostDataMode {
        self.mode
    }

    pub fn uses_shared_output(&self) -> bool {
        self.shared_output.is_some()
    }

    pub(crate) fn matches_shared_output(
        &self,
        output: Option<&crate::hid::SharedHidOutput>,
    ) -> bool {
        match (self.shared_output.as_ref(), output) {
            (Some(current), Some(next)) => current.shares_owner_with(next),
            (None, None) => true,
            _ => false,
        }
    }

    /// Move the selected connection's exact HID owner into this background
    /// bridge. This avoids dropping and reopening the macropad during a switch
    /// to a different physical keyboard, so its host-status lease never gaps.
    pub(crate) fn adopt_selected_hid(
        &mut self,
        hid: crate::hid::HidDevice,
    ) -> Result<(), crate::hid::HidDevice> {
        let extended = matches!(self.protocol, HostProtocol::Selected(true));
        if self.shared_output.is_none() {
            return Err(hid);
        }
        self.control.handoff_selected(hid, extended)?;
        self.shared_output = None;
        // The transferred handle keeps its selected capability below. Any
        // later reopen must discover capabilities from the replacement owner.
        self.protocol = HostProtocol::Discover;
        Ok(())
    }

    /// Keep the display contents intact while replacing this bridge with a
    /// different HID owner for the same, still-connected device.
    pub fn suppress_shutdown(&mut self) {
        self.send_shutdown_on_drop.store(false, Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        // Atomic revocation is independent of an unresponsive desktop query.
        // Never open/write HID, join, or spawn a join-waiter from UI/Drop.
        self.control.retire();
        self.layout_snapshot.store(u8::MAX, Ordering::Relaxed);
        self.thread.take();
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
    mut shared_output: Option<crate::hid::SharedHidOutput>,
    control: Arc<BridgeTransportControl>,
    layout_snapshot: Arc<AtomicU8>,
    mut protocol: HostProtocol,
    send_shutdown: Arc<AtomicBool>,
    desktop: HostDataService,
    mut open_hid: impl FnMut(
        &crate::device::Device,
        Option<&crate::hid::SharedHidOutput>,
    ) -> anyhow::Result<HostDataHid>,
) {
    let stop = &control.stop;
    let mut extended_protocol = false;
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
    let mut last_layout_full_send = Instant::now();
    let mut desktop_subscription = None;

    while !stop.load(Ordering::Relaxed) {
        if let Some((hid, selected_extended)) = control.take_selected_handoff() {
            // This worker now owns a dedicated transport. Never fall back to
            // the old selected connection's weak shared output after a later
            // write failure; reopen a fresh dedicated owner for this target.
            shared_output = None;
            let retirement = hid.retirement_handle();
            if control.publish(retirement).is_err() {
                break;
            }
            device = Some(HostDataHid::Dedicated(hid));
            extended_protocol = mode.time && selected_extended;
            protocol = HostProtocol::Discover;
            last_time = None;
            last_volume = None;
            last_layout = None;
            last_artist.clear();
            last_title.clear();
            last_time_poll = Instant::now() - Duration::from_secs(60);
            last_volume_poll = Instant::now() - Duration::from_secs(60);
            last_layout_poll = Instant::now() - Duration::from_secs(60);
            last_media_poll = Instant::now() - Duration::from_secs(60);
            last_media_full_send = Instant::now() - Duration::from_secs(60);
            reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);
            log::info!(
                "qmk-hid-host bridge adopted selected HID owner target={:?} extended={extended_protocol}",
                target.path,
            );
        }

        if device.is_none() && last_open_attempt.elapsed() >= Duration::from_secs(2) {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            last_open_attempt = Instant::now();
            device = open_hid(&target, shared_output.as_ref())
                .and_then(|device| {
                    let retirement = match &device {
                        HostDataHid::Dedicated(hid) => hid.retirement_handle(),
                        HostDataHid::Shared(_) => None,
                    };
                    control.publish(retirement)?;
                    Ok(device)
                })
                .map_err(|e| {
                    log::warn!(
                        "qmk-hid-host open failed: target={:?} error={e:#}",
                        target.path
                    )
                })
                .ok();
            if let Some(dev) = device.as_ref() {
                extended_protocol = mode.time && protocol.extended(dev);
                reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);
                log::info!(
                    "qmk-hid-host bridge started ({}) target={:?} protocol={protocol:?} extended={extended_protocol}",
                    if device.as_ref().is_some_and(HostDataHid::uses_shared_output) {
                        "shared HID owner"
                    } else {
                        "dedicated HID owner"
                    },
                    target.path,
                );
            }
        }

        let Some(dev) = device.as_ref() else {
            desktop_subscription = None;
            thread::sleep(Duration::from_millis(250));
            continue;
        };
        if stop.load(Ordering::Relaxed) {
            break;
        }

        #[cfg(target_os = "linux")]
        if !target.uses_bluez_gatt_transport() && !std::path::Path::new(&target.path).exists() {
            log::warn!("qmk-hid-host device path disappeared; reconnecting");
            layout_snapshot.store(u8::MAX, Ordering::Relaxed);
            device = None;
            extended_protocol = false;
            protocol.connection_lost();
            last_time = None;
            reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);
            thread::sleep(Duration::from_millis(250));
            continue;
        }

        let mut write_failed = false;

        if mode.time && last_time_poll.elapsed() >= Duration::from_secs(1) {
            last_time_poll = Instant::now();
            if extended_protocol {
                let started = Instant::now();
                let heartbeat = write_payload(dev, &[DATA_HOST_STATUS, 1]);
                log::debug!(
                    "qmk-hid-host heartbeat: target={:?} online=1 elapsed_ms={} result={heartbeat:?}",
                    target.path,
                    started.elapsed().as_millis(),
                );
                write_failed |= heartbeat.is_err();
            }
            let now = current_time_payload();
            if last_time != Some(now) {
                last_time = Some(now);
                write_failed |= write_payload(dev, &[DATA_TIME, now.0, now.1]).is_err();
                pause_between_packets();
                if extended_protocol {
                    use chrono::Datelike;
                    let today = chrono::Local::now();
                    let year = today.year() as u16;
                    write_failed |= write_payload(
                        dev,
                        &[
                            DATA_DATE,
                            today.day() as u8,
                            today.month() as u8,
                            year as u8,
                            (year >> 8) as u8,
                        ],
                    )
                    .is_err();
                    pause_between_packets();
                }
            }
        }

        // HID deadlines consume only cached desktop data. Subscribing cannot
        // run an OS query, and the sampler never receives transport ownership.
        desktop_subscription.get_or_insert_with(|| desktop.subscribe(mode));
        let snapshot = desktop.snapshot();

        if mode.volume && last_volume_poll.elapsed() >= VOLUME_POLL_INTERVAL {
            last_volume_poll = Instant::now();
            if let Some(volume) = snapshot.volume {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if last_volume != Some(volume) {
                    last_volume = Some(volume);
                    write_failed |= write_payload(dev, &[DATA_VOLUME, volume]).is_err();
                    pause_between_packets();
                }
            }
        }

        if mode.layout && last_layout_poll.elapsed() >= Duration::from_millis(100) {
            last_layout_poll = Instant::now();
            if let Some(layout) = snapshot.layout {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if layout_needs_send(last_layout, last_layout_full_send.elapsed(), layout) {
                    last_layout = Some(layout);
                    let layout_write = write_layout_snapshot(dev, layout, &layout_snapshot);
                    write_failed |= layout_write.is_err();
                    if layout_write.is_ok() {
                        last_layout_full_send = Instant::now();
                    }
                    pause_between_packets();
                }
            }
        }

        if mode.media
            && snapshot
                .media_sampled_at
                .is_some_and(|at| at > last_media_poll)
        {
            last_media_poll = snapshot.media_sampled_at.unwrap();
            let (artist, title) = snapshot.media.unwrap_or_default();
            log::debug!(
                "qmk-hid-host media query: target={:?} elapsed_ms={}",
                target.path,
                snapshot.media_query_ms
            );
            if stop.load(Ordering::Acquire) {
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
            log::warn!(
                "qmk-hid-host bridge write failed; reconnecting target={:?}",
                target.path
            );
            layout_snapshot.store(u8::MAX, Ordering::Relaxed);
            device = None;
            extended_protocol = false;
            protocol.connection_lost();
            last_time = None;
            last_volume = None;
            reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);
            last_artist.clear();
            last_title.clear();
            last_media_full_send = Instant::now() - Duration::from_secs(60);
        }

        thread::sleep(Duration::from_millis(20));
    }

    if send_shutdown.load(Ordering::Relaxed) {
        if let Some(device) = device.as_ref() {
            send_shutdown_payloads(device, mode, extended_protocol);
        } else if let Some(output) = shared_output.as_ref() {
            // A successor stopped before opening must also finish any retained
            // mode footprint; this uses the existing owner, never a reopened path.
            if let Err(error) = output.write_host_shutdown(&[]) {
                log::warn!("qmk-hid-host shutdown write failed: {error}");
            }
        }
    }
    layout_snapshot.store(u8::MAX, Ordering::Relaxed);
    // Dropping this subscription clears only demand owned by this bridge.
    // In-flight desktop results cannot write HID or revive a retired lease.
    drop(desktop_subscription);
    log::info!(
        "qmk-hid-host bridge stopped target={:?} shutdown_requested={}",
        target.path,
        send_shutdown.load(Ordering::Relaxed)
    );
}

// Preview state belongs to this device bridge and is published only after
// the same language packet has successfully been written to its HID channel.
fn layout_snapshot_label(snapshot: &AtomicU8) -> Option<&'static str> {
    match snapshot.load(Ordering::Relaxed) {
        0 => Some("EN"),
        1 => Some("RU"),
        _ => None,
    }
}

fn write_layout_snapshot(dev: &HostDataHid, layout: u8, snapshot: &AtomicU8) -> anyhow::Result<()> {
    let result = write_payload(dev, &[DATA_LAYOUT, layout]);
    snapshot.store(
        if result.is_ok() { layout } else { u8::MAX },
        Ordering::Relaxed,
    );
    result
}

fn reset_layout_sync_state(last_layout: &mut Option<u8>, last_layout_full_send: &mut Instant) {
    *last_layout = None;
    *last_layout_full_send = Instant::now();
}

fn layout_needs_send(
    last_layout: Option<u8>,
    elapsed_since_last_send: Duration,
    layout: u8,
) -> bool {
    last_layout != Some(layout) || elapsed_since_last_send >= LAYOUT_RESEND_INTERVAL
}

fn send_shutdown_payloads(device: &HostDataHid, mode: HostDataMode, extended_protocol: bool) {
    let payloads = shutdown_payloads(mode, extended_protocol);
    if let HostDataHid::Shared(output) = device {
        if let Err(error) = output.write_host_shutdown(&payloads) {
            log::warn!("qmk-hid-host shutdown write failed: {error}");
        }
        return;
    }
    for payload in payloads {
        if let Err(e) = write_payload(device, &payload) {
            log::warn!("qmk-hid-host shutdown write failed: {e}");
            break;
        }
        pause_between_packets();
    }
}

fn shutdown_payloads(mode: HostDataMode, extended_protocol: bool) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    // Never reopen a device or send BA to firmware that has not advertised it.
    if mode.time && extended_protocol {
        payloads.push(vec![DATA_HOST_STATUS, 0]);
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
    if let Some(output) = shared_output {
        // Never apply selected-connection capabilities to a freshly reopened
        // device at a recycled path. Let the connection owner reconnect first.
        anyhow::ensure!(
            output.is_available(),
            "Shared HID output owner is no longer available"
        );
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
fn linux_volume_command(program: &str, args: &[&str]) -> Option<String> {
    // These short queries run sequentially in the desktop sampler, never concurrently.
    // Do not add the generic 25 ms wait to every fast wpctl/pactl response, or
    // let a stalled audio server block display updates for ten seconds.
    command_stdout_timeout_with_poll(
        program,
        args,
        Duration::from_millis(250),
        Duration::from_millis(2),
    )
}

#[cfg(target_os = "linux")]
fn current_volume_percent() -> Option<u8> {
    linux_volume_command("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .and_then(|out| {
            out.split_whitespace()
                .find_map(|part| part.parse::<f32>().ok())
                .map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u8)
        })
        .or_else(|| {
            linux_volume_command("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).and_then(|out| {
                out.split_whitespace()
                    .find(|part| part.ends_with('%'))
                    .and_then(|part| part.trim_end_matches('%').parse::<u8>().ok())
            })
        })
}

#[cfg(target_os = "macos")]
fn current_volume_percent() -> Option<u8> {
    macos_volume::volume_percent()
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
fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    command_stdout_timeout(program, args, Duration::from_secs(10))
}

#[cfg(target_os = "macos")]
fn macos_automation_stdout(args: &[&str]) -> Option<String> {
    command_stdout_timeout("osascript", args, MACOS_AUTOMATION_COMMAND_TIMEOUT)
}

#[cfg(not(target_os = "windows"))]
fn command_stdout_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    command_stdout_timeout_with_poll(program, args, timeout, COMMAND_POLL_INTERVAL)
}

#[cfg(not(target_os = "windows"))]
fn command_stdout_timeout_with_poll(
    program: &str,
    args: &[&str],
    timeout: Duration,
    poll_interval: Duration,
) -> Option<String> {
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

        thread::sleep(poll_interval);
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
            instance_token: String::new(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        };
        let host_data_hid = open_host_data_hid(&target, Some(&output)).unwrap();

        assert!(host_data_hid.uses_shared_output());
        let snapshot = AtomicU8::new(u8::MAX);
        let other_device = AtomicU8::new(1);
        assert_eq!(layout_snapshot_label(&snapshot), None);
        for (index, label) in [(0, "EN"), (1, "RU"), (0, "EN")] {
            write_layout_snapshot(&host_data_hid, index, &snapshot).unwrap();
            assert_eq!(layout_snapshot_label(&snapshot), Some(label));
            assert_eq!(layout_snapshot_label(&other_device), Some("RU"));
        }
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests
                .iter()
                .map(|request| request[1])
                .collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
        assert!(requests.iter().all(|request| request[0] == DATA_LAYOUT));
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
    fn layout_resend_is_due_when_the_layout_does_not_change() {
        let layout = 1;

        assert!(!layout_needs_send(Some(layout), Duration::ZERO, layout));
        assert!(layout_needs_send(
            Some(layout),
            LAYOUT_RESEND_INTERVAL,
            layout
        ));
    }

    #[test]
    fn reset_layout_sync_state_forces_the_next_layout_write() {
        let layout = 1;
        let mut last_layout = Some(layout);
        let mut last_layout_full_send = Instant::now();

        reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);

        assert!(layout_needs_send(
            last_layout,
            last_layout_full_send.elapsed(),
            layout
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_layout_index_maps_configured_layout_order() {
        let layouts = vec!["us".to_owned(), "ru".to_owned()];
        assert_eq!(kde_layout_code_index(0, &layouts), Some(0));
        assert_eq!(kde_layout_code_index(1, &layouts), Some(1));
        assert_eq!(kde_layout_code_index(2, &layouts), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_desktop_names_are_recognized_without_matching_other_desktops() {
        assert!(desktop_list_has_gnome("GNOME"));
        assert!(desktop_list_has_gnome("ubuntu:GNOME"));
        assert!(desktop_list_has_gnome("GNOME-Classic:GNOME"));
        assert!(!desktop_list_has_gnome("KDE"));
        assert!(!desktop_list_has_gnome("NotGnome"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn ibus_engine_descriptor_maps_its_layout_field() {
        let engine: zbus::zvariant::OwnedValue = zbus::zvariant::StructureBuilder::new()
            .add_field("IBusEngineDesc")
            .add_field("")
            .add_field("xkb:ru::rus")
            .add_field("Russian")
            .add_field("Russian")
            .add_field("ru")
            .add_field("GPL")
            .add_field("IBus")
            .add_field("ibus-keyboard")
            .add_field("ru")
            .build()
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(
            ibus_engine_state(&engine),
            Some(IbusEngineState {
                entropy: false,
                layout: Some(1),
            })
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn entropy_ibus_engine_name_recovers_its_base_layout() {
        let engine: zbus::zvariant::OwnedValue = zbus::zvariant::StructureBuilder::new()
            .add_field("IBusEngineDesc")
            .add_field("")
            .add_field("entropy-universal-symbols-ru")
            .add_field("Entropy Text Expander RU")
            .add_field("Text expansion")
            .add_field("ru")
            .add_field("GPL")
            .add_field("Ergohaven")
            .add_field("input-keyboard")
            .add_field("default")
            .build()
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(
            ibus_engine_state(&engine),
            Some(IbusEngineState {
                entropy: true,
                layout: Some(1),
            })
        );
    }

    #[test]
    fn command_stdout_timeout_returns_successful_output() {
        let output =
            command_stdout_timeout("/bin/sh", &["-c", "printf entropy"], Duration::from_secs(1));

        assert_eq!(output.as_deref(), Some("entropy"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_volume_queries_return_output_and_bound_stalled_audio_commands() {
        assert_eq!(
            linux_volume_command("/bin/sh", &["-c", "printf 'Volume: 0.42'"]).as_deref(),
            Some("Volume: 0.42")
        );
        assert!(linux_volume_command("/bin/sh", &["-c", "exit 1"]).is_none());
        let started_at = Instant::now();
        assert!(linux_volume_command("/bin/sh", &["-c", "exec sleep 2"]).is_none());
        assert!(started_at.elapsed() < Duration::from_secs(1));
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
        let mode = HostDataMode {
            time: true,
            volume: true,
            layout: true,
            media: true,
        };
        // Preserve the original final-shutdown regression using the supported
        // BA0 contract, never the malformed legacy AA FF FF packet.
        assert_eq!(
            shutdown_payloads(mode, true),
            vec![
                vec![DATA_HOST_STATUS, 0],
                vec![DATA_MEDIA_ARTIST, 0],
                vec![DATA_MEDIA_TITLE, 0]
            ]
        );
        assert_eq!(
            shutdown_payloads(mode, false),
            vec![vec![DATA_MEDIA_ARTIST, 0], vec![DATA_MEDIA_TITLE, 0]]
        );
    }

    #[test]
    fn shutdown_payloads_clear_media_without_sending_invalid_time() {
        let payloads = shutdown_payloads(
            HostDataMode {
                time: true,
                volume: true,
                layout: true,
                media: true,
            },
            true,
        );

        assert_eq!(
            payloads,
            vec![
                vec![DATA_HOST_STATUS, 0],
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
enum NativeLayoutTracker {
    Kde(KdeLayoutTracker),
    Ibus,
    X11(X11LayoutTracker),
}

#[cfg(target_os = "linux")]
struct LayoutTracker {
    // Entropy Text Expander is an IBus input source even on desktops whose
    // normal layout tracker is KDE or X11. Keep this connection alongside the
    // native tracker so switching to the Entropy RU/EN source is noticed while
    // the bridge is already running.
    ibus: Option<IbusLayoutTracker>,
    native: Option<NativeLayoutTracker>,
}

#[cfg(target_os = "linux")]
impl LayoutTracker {
    fn new() -> Option<Self> {
        let mut ibus = IbusLayoutTracker::new();
        let native = if let Some(tracker) = KdeLayoutTracker::new() {
            Some(NativeLayoutTracker::Kde(tracker))
        } else if gnome_desktop_session()
            && ibus
                .as_mut()
                .and_then(IbusLayoutTracker::current_engine_state)
                .and_then(|state| state.layout)
                .is_some()
        {
            Some(NativeLayoutTracker::Ibus)
        } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            None
        } else {
            X11LayoutTracker::new().map(NativeLayoutTracker::X11)
        };

        (ibus.is_some() || native.is_some()).then_some(Self { ibus, native })
    }

    fn current_layout_index(&mut self) -> Option<u8> {
        let ibus_state = self
            .ibus
            .as_mut()
            .and_then(IbusLayoutTracker::current_engine_state);
        if let Some(layout) = ibus_state
            .filter(|state| state.entropy)
            .and_then(|state| state.layout)
        {
            return Some(layout);
        }

        match self.native.as_mut()? {
            NativeLayoutTracker::Kde(tracker) => tracker.current_layout_index(),
            NativeLayoutTracker::Ibus => ibus_state.and_then(|state| state.layout),
            NativeLayoutTracker::X11(tracker) => tracker.current_layout_index(),
        }
    }
}

#[cfg(target_os = "linux")]
struct KdeLayoutTracker {
    connection: zbus::blocking::Connection,
    layout_codes: Vec<String>,
}

#[cfg(target_os = "linux")]
impl KdeLayoutTracker {
    fn new() -> Option<Self> {
        let connection = zbus::blocking::Connection::session().ok()?;
        let layout_codes = {
            let proxy = zbus::blocking::Proxy::new(
                &connection,
                KDE_LAYOUT_DESTINATION,
                KDE_LAYOUT_PATH,
                KDE_LAYOUT_INTERFACE,
            )
            .ok()?;
            proxy
                .call::<_, _, Vec<(String, String, String)>>("getLayoutsList", &())
                .ok()?
                .into_iter()
                .map(|(short_name, _, _)| short_name)
                .collect::<Vec<_>>()
        };
        if layout_codes.is_empty() {
            return None;
        }
        Some(Self {
            connection,
            layout_codes,
        })
    }

    fn current_layout_index(&mut self) -> Option<u8> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            KDE_LAYOUT_DESTINATION,
            KDE_LAYOUT_PATH,
            KDE_LAYOUT_INTERFACE,
        )
        .ok()?;
        let layout = proxy.call::<_, _, u32>("getLayout", &()).ok()?;
        kde_layout_code_index(layout, &self.layout_codes)
    }
}

#[cfg(target_os = "linux")]
fn kde_layout_code_index(layout: u32, layout_codes: &[String]) -> Option<u8> {
    let raw = layout_codes.get(usize::try_from(layout).ok()?)?;
    layout_code_index(raw)
}

#[cfg(target_os = "linux")]
struct IbusLayoutTracker {
    connection: zbus::blocking::Connection,
}

#[cfg(target_os = "linux")]
impl IbusLayoutTracker {
    fn new() -> Option<Self> {
        let connection = zbus::blocking::connection::Builder::ibus()
            .ok()?
            .build()
            .ok()?;
        Some(Self { connection })
    }

    fn current_engine_state(&mut self) -> Option<IbusEngineState> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            IBUS_DESTINATION,
            IBUS_PATH,
            IBUS_INTERFACE,
        )
        .ok()?;
        let engine = proxy
            .call::<_, _, zbus::zvariant::OwnedValue>("GetGlobalEngine", &())
            .ok()?;
        ibus_engine_state(&engine)
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IbusEngineState {
    entropy: bool,
    layout: Option<u8>,
}

#[cfg(target_os = "linux")]
fn ibus_engine_field(engine: &zbus::zvariant::OwnedValue, index: usize) -> Option<&str> {
    let descriptor: &zbus::zvariant::Structure<'_> = engine.try_into().ok()?;
    descriptor.fields().get(index)?.try_into().ok()
}

#[cfg(target_os = "linux")]
fn ibus_engine_state(engine: &zbus::zvariant::OwnedValue) -> Option<IbusEngineState> {
    const ENTROPY_ENGINE_PREFIX: &str = "entropy-universal-symbols";

    let name = ibus_engine_field(engine, 2)?;
    let entropy = name == ENTROPY_ENGINE_PREFIX
        || name
            .strip_prefix(ENTROPY_ENGINE_PREFIX)
            .is_some_and(|suffix| suffix.starts_with('-'));
    let descriptor_layout = ibus_engine_field(engine, 9).and_then(layout_code_index);
    let name_layout = entropy
        .then(|| name.rsplit_once('-').map(|(_, suffix)| suffix))
        .flatten()
        .and_then(layout_code_index);

    Some(IbusEngineState {
        entropy,
        layout: descriptor_layout.or(name_layout),
    })
}

#[cfg(target_os = "linux")]
fn gnome_desktop_session() -> bool {
    ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter_map(|value| value.into_string().ok())
        .any(|value| desktop_list_has_gnome(&value))
}

#[cfg(target_os = "linux")]
fn desktop_list_has_gnome(value: &str) -> bool {
    value.split(':').any(|desktop| {
        desktop.eq_ignore_ascii_case("gnome")
            || desktop
                .get(..6)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("gnome-"))
    })
}

#[cfg(target_os = "linux")]
struct X11LayoutTracker {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    keyboard: x11_dl::xlib::XkbDescPtr,
    symbols: Vec<String>,
}

#[cfg(target_os = "linux")]
impl X11LayoutTracker {
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
impl Drop for X11LayoutTracker {
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
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
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
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = (|| {
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
            })();
            if initialized {
                CoUninitialize();
            }
            result
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

#[cfg(test)]
mod host_protocol_tests {
    use super::*;

    fn capability_reply(settings: &[u16]) -> [u8; 32] {
        let mut response = [0xFF; 32];
        for (slot, id) in settings.iter().enumerate() {
            response[slot * 2..slot * 2 + 2].copy_from_slice(&id.to_le_bytes());
        }
        response
    }

    #[test]
    fn automatic_bridge_negotiates_old_and_extended_firmware_through_its_own_hid() {
        for settings in [vec![333, 334, 356], (357..=371).collect()] {
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            recorder.respond_with([capability_reply(&settings)]);
            let device = HostDataHid::Dedicated(hid);
            let extended = HostProtocol::Discover.extended(&device);
            assert_eq!(extended, settings.contains(&357));
            assert_eq!(recorder.requests().len(), 1);
            assert_eq!(&recorder.requests()[0][..4], &[0xFE, 0x09, 0, 0]);
            let mode = HostDataMode {
                time: true,
                ..Default::default()
            };
            assert_eq!(
                shutdown_payloads(mode, extended),
                if extended {
                    vec![vec![DATA_HOST_STATUS, 0]]
                } else {
                    vec![]
                }
            );
        }
    }

    #[test]
    fn failed_or_echoed_capability_probe_never_authorizes_new_host_commands() {
        for response in [[0; 32], {
            let mut echo = [0; 32];
            echo[..2].copy_from_slice(&[0xFE, 0x09]);
            echo
        }] {
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            recorder.respond_with([response]);
            assert!(!HostProtocol::Discover.extended(&HostDataHid::Dedicated(hid)));
            assert_eq!(recorder.requests().len(), 1);
        }
        let (hid, recorder) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            0,
            crate::hid::TestHidFault::Timeout,
        )));
        assert!(!HostProtocol::Discover.extended(&HostDataHid::Dedicated(hid)));
        assert_eq!(recorder.requests().len(), 1);
    }

    #[test]
    fn selected_bridge_uses_connection_metadata_without_a_second_query() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let shared = HostDataHid::Shared(hid.shared_output().unwrap());
        let dedicated = HostDataHid::Dedicated(hid);
        for selected in [false, true] {
            let mut protocol = HostProtocol::Selected(selected);
            assert_eq!(protocol.extended(&shared), selected);
            assert!(
                !protocol.extended(&dedicated),
                "a reopened handle cannot inherit selected-connection permission"
            );
            protocol.connection_lost();
            assert!(!protocol.extended(&shared));
        }
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn expired_selected_owner_never_falls_back_to_reopening_a_recycled_path() {
        let output = crate::hid::SharedHidOutput::test_expired_proxy_owner();
        let target = crate::device::Device {
            name: "test".into(),
            vendor_id: 0,
            product_id: 0,
            manufacturer: String::new(),
            serial_number: String::new(),
            bus_type: "USB".into(),
            path: "must-not-be-opened".into(),
            instance_token: "host-test-instance".into(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        };
        assert!(!output.is_available());
        assert!(output.write_output_report(&[DATA_TIME, 12, 34]).is_err());
        let error = open_host_data_hid(&target, Some(&output)).err().unwrap();
        assert!(error.to_string().contains("owner is no longer available"));
    }

    #[test]
    fn automatic_bridge_rechecks_capabilities_after_physical_reconnection() {
        let (old, old_recorder) = crate::hid::HidDevice::test_device();
        old_recorder.respond_with([capability_reply(&(357..=371).collect::<Vec<_>>())]);
        let (replacement, replacement_recorder) = crate::hid::HidDevice::test_device();
        replacement_recorder.respond_with([capability_reply(&[333, 356])]);
        let mut protocol = HostProtocol::Discover;
        assert!(protocol.extended(&HostDataHid::Dedicated(old)));
        protocol.connection_lost();
        assert!(!protocol.extended(&HostDataHid::Dedicated(replacement)));
        assert_eq!(replacement_recorder.requests().len(), 1);
    }

    #[test]
    fn bridge_ticks_and_shutdown_gate_ba_af_but_keep_legacy_aa() {
        for extended in [false, true] {
            let path = tempfile::NamedTempFile::new().unwrap();
            let target = crate::device::Device {
                name: "test display".into(),
                vendor_id: 0,
                product_id: 0,
                manufacturer: String::new(),
                serial_number: String::new(),
                bus_type: "USB".into(),
                path: path.path().to_string_lossy().into_owned(),
                instance_token: "host-test-instance".into(),
                firmware: crate::firmware::FirmwareProtocol::Vial,
            };
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let output = hid.shared_output().map(|output| {
                output.for_host_bridge(
                    HostDataMode {
                        time: true,
                        ..Default::default()
                    },
                    extended,
                )
            });
            let control = Arc::new(BridgeTransportControl::default());
            let worker_control = control.clone();
            let worker = thread::spawn(move || {
                run_bridge(
                    target,
                    HostDataMode {
                        time: true,
                        ..Default::default()
                    },
                    output,
                    worker_control,
                    Arc::new(AtomicU8::new(u8::MAX)),
                    HostProtocol::Selected(extended),
                    Arc::new(AtomicBool::new(true)),
                    HostDataService::start(|| {
                        TestMediaSource(|| panic!("time-only bridge must not query desktop media"))
                    }),
                    open_host_data_hid,
                )
            });
            let deadline = Instant::now() + Duration::from_secs(3);
            while !recorder
                .requests()
                .iter()
                .any(|packet| packet[0] == if extended { DATA_DATE } else { DATA_TIME })
            {
                assert!(
                    Instant::now() < deadline,
                    "bridge did not emit the first clock update"
                );
                thread::sleep(Duration::from_millis(5));
            }
            control.retire();
            worker.join().unwrap();
            let requests = recorder.requests();
            let commands: Vec<_> = requests.iter().map(|packet| packet[0]).collect();
            println!(
                "HOST_PROTOCOL_TRACE extended={extended} packets={}",
                serde_json::to_string(&requests).unwrap()
            );
            if extended {
                assert_eq!(
                    commands,
                    vec![DATA_HOST_STATUS, DATA_TIME, DATA_DATE, DATA_HOST_STATUS]
                );
                assert_eq!(requests.last().unwrap()[1], 0);
            } else {
                assert_eq!(commands, vec![DATA_TIME]);
            }
        }
    }
}

#[cfg(test)]
mod qa_followup_shared_output {
    use super::*;
    use std::sync::mpsc;

    struct Finished(mpsc::Sender<()>);
    impl Drop for Finished {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    fn target() -> crate::device::Device {
        // Existing source path satisfies the Linux presence check. All output
        // uses the scripted owner; no file, real HID or desktop process is opened.
        crate::device::Device {
            name: "host ordering fixture".into(),
            vendor_id: 0,
            product_id: 0,
            manufacturer: String::new(),
            serial_number: String::new(),
            bus_type: "USB".into(),
            path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/qmk_hid_host.rs")
                .to_string_lossy()
                .into_owned(),
            instance_token: "host-test-instance".into(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        }
    }

    fn wait_for(recorder: &crate::hid::TestHidRecorder, from: usize, command: u8) -> Vec<[u8; 32]> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let reports = recorder.requests();
            if reports[from..].iter().any(|r| r[0] == command) {
                return reports;
            }
            assert!(Instant::now() < deadline, "missing report {command:02X}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn blocked_media_bridge(
        output: crate::hid::SharedHidOutput,
    ) -> (
        QmkHidHostBridge,
        mpsc::Receiver<()>,
        mpsc::Sender<()>,
        mpsc::Receiver<()>,
    ) {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let finished = Finished(finished_tx);
        let bridge = QmkHidHostBridge::start_with_media_source(
            target(),
            HostDataMode {
                time: true,
                media: true,
                ..Default::default()
            },
            Some(output),
            HostProtocol::Selected(true),
            move || {
                let _keep_until_worker_exit = &finished;
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Some(("retired artist".into(), "retired title".into()))
            },
        );
        (bridge, entered_rx, release_tx, finished_rx)
    }

    #[test]
    fn retiring_shared_bridge_cannot_shutdown_after_replacement_date() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let output = hid.shared_output().unwrap();
        let (mut old, entered, release, finished) = blocked_media_bridge(output.clone());
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let before = recorder.requests().len();
        let stopped = Instant::now();
        old.stop();
        assert!(
            stopped.elapsed() < Duration::from_secs(1),
            "shared stop waited for desktop I/O"
        );
        let mut replacement = QmkHidHostBridge::start_with_media_source(
            target(),
            HostDataMode {
                time: true,
                ..Default::default()
            },
            Some(output),
            HostProtocol::Selected(true),
            || panic!("time-only bridge must not poll media"),
        );
        let reports = wait_for(&recorder, before, DATA_DATE);
        assert_eq!(
            reports[before..].iter().map(|r| r[0]).collect::<Vec<_>>(),
            vec![
                DATA_MEDIA_ARTIST,
                DATA_MEDIA_TITLE,
                DATA_HOST_STATUS,
                DATA_TIME,
                DATA_DATE
            ]
        );
        assert_eq!(&reports[before][..2], &[DATA_MEDIA_ARTIST, 0]);
        assert_eq!(&reports[before + 1][..2], &[DATA_MEDIA_TITLE, 0]);
        let after_date = reports.len();
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            recorder.requests().len(),
            after_date,
            "retiring worker wrote after replacement AF"
        );
        replacement.suppress_shutdown();
        replacement.stop();
    }

    #[test]
    fn stop_then_later_reenable_and_skipped_generation_keep_ordering() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let output = hid.shared_output().unwrap();
        let (mut old, entered, release, finished) = blocked_media_bridge(output.clone());
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        old.stop();
        // No replacement was available to old.stop(). An intermediate claim is
        // never initialized; the next generation must still clear the old modes.
        let unused = output.for_host_bridge(HostDataMode::default(), false);
        drop(unused);
        let before = recorder.requests().len();
        let mut next = QmkHidHostBridge::start_with_media_source(
            target(),
            HostDataMode {
                time: true,
                ..Default::default()
            },
            Some(output),
            HostProtocol::Selected(true),
            || None,
        );
        let reports = wait_for(&recorder, before, DATA_DATE);
        assert_eq!(&reports[before][..2], &[DATA_MEDIA_ARTIST, 0]);
        assert_eq!(&reports[before + 1][..2], &[DATA_MEDIA_TITLE, 0]);
        let after_date = reports.len();
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(recorder.requests().len(), after_date);
        next.suppress_shutdown();
        next.stop();
    }

    #[test]
    fn final_shared_bridge_shutdown_clears_clock_and_media_once() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut bridge = QmkHidHostBridge::start_with_media_source(
            target(),
            HostDataMode {
                time: true,
                media: true,
                ..Default::default()
            },
            hid.shared_output(),
            HostProtocol::Selected(true),
            || Some(("artist".into(), "title".into())),
        );
        wait_for(&recorder, 0, DATA_MEDIA_TITLE);
        let before = recorder.requests().len();
        bridge.stop();
        let reports = wait_for(&recorder, before, DATA_MEDIA_TITLE);
        assert_eq!(
            reports[before..]
                .iter()
                .map(|r| [r[0], r[1]])
                .collect::<Vec<_>>(),
            vec![
                [DATA_HOST_STATUS, 0],
                [DATA_MEDIA_ARTIST, 0],
                [DATA_MEDIA_TITLE, 0]
            ]
        );
        bridge.stop();
        assert_eq!(recorder.requests().len(), reports.len());
    }

    #[test]
    fn blocked_final_bridge_still_shuts_down_when_no_successor_claims_the_owner() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let (mut old, entered, release, finished) =
            blocked_media_bridge(hid.shared_output().unwrap());
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let before = recorder.requests().len();
        old.stop();
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            recorder.requests()[before..]
                .iter()
                .map(|r| [r[0], r[1]])
                .collect::<Vec<_>>(),
            vec![
                [DATA_HOST_STATUS, 0],
                [DATA_MEDIA_ARTIST, 0],
                [DATA_MEDIA_TITLE, 0]
            ]
        );
    }

    #[test]
    fn removing_clock_clears_it_before_new_media_and_does_not_revoke_keyboard_owner() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let output = hid.shared_output().unwrap();
        let old = output.for_host_bridge(
            HostDataMode {
                time: true,
                media: true,
                ..Default::default()
            },
            true,
        );
        old.write_output_report(&[DATA_HOST_STATUS, 1]).unwrap();
        let next = hid.shared_output().unwrap().for_host_bridge(
            HostDataMode {
                media: true,
                ..Default::default()
            },
            false,
        );
        next.write_output_report(&[DATA_MEDIA_ARTIST, 1, b'N'])
            .unwrap();
        assert_eq!(&recorder.requests()[1][..2], &[DATA_HOST_STATUS, 0]);
        assert_eq!(&recorder.requests()[2][..3], &[DATA_MEDIA_ARTIST, 1, b'N']);
        old.write_host_shutdown(&[]).unwrap();
        assert!(old.write_output_report(&[DATA_MEDIA_ARTIST, 0]).is_err());
        // Only the stale host consumer loses permission, never the selected HID owner.
        hid.write_output_report(&[0xC6, 0, 0]).unwrap();
        output.write_output_report(&[DATA_VOLUME, 42]).unwrap();
        let before = recorder.requests().len();
        next.write_host_shutdown(&[]).unwrap();
        assert_eq!(
            recorder.requests()[before..]
                .iter()
                .map(|r| [r[0], r[1]])
                .collect::<Vec<_>>(),
            vec![[DATA_MEDIA_ARTIST, 0], [DATA_MEDIA_TITLE, 0]]
        );
    }

    #[test]
    fn host_output_generations_are_per_physical_owner() {
        let (a, ar) = crate::hid::HidDevice::test_device();
        let (b, br) = crate::hid::HidDevice::test_device();
        let mode = HostDataMode {
            time: true,
            ..Default::default()
        };
        let aw = a.shared_output().unwrap().for_host_bridge(mode, true);
        aw.write_output_report(&[DATA_HOST_STATUS, 1]).unwrap();
        let bw = b.shared_output().unwrap().for_host_bridge(mode, true);
        bw.write_output_report(&[DATA_HOST_STATUS, 1]).unwrap();
        aw.write_host_shutdown(&[]).unwrap();
        assert_eq!(&ar.requests().last().unwrap()[..2], &[DATA_HOST_STATUS, 0]);
        assert_eq!(br.requests().len(), 1);
        assert!(bw.host_session_is_current());
    }

    #[test]
    fn new_claim_never_waits_for_old_io_and_new_updates_follow_its_atomic_shutdown_batch() {
        let owner = Arc::new(HostOutputOwner::default());
        let mode = HostDataMode {
            time: true,
            media: true,
            ..Default::default()
        };
        let old = owner.claim(mode, true);
        old.write(&[DATA_HOST_STATUS, 1], |_| Ok(())).unwrap();
        let reports = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let old_reports = reports.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let old_worker = thread::spawn(move || {
            old.shutdown(|payload| {
                if payload[0] == DATA_HOST_STATUS {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }
                old_reports.lock().unwrap().push(payload.to_vec());
                Ok(())
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = Instant::now();
        let next = owner.claim(mode, true);
        assert!(start.elapsed() < Duration::from_secs(1));
        let next_reports = reports.clone();
        let next_worker = thread::spawn(move || {
            next.write(&[DATA_HOST_STATUS, 1], |payload| {
                next_reports.lock().unwrap().push(payload.to_vec());
                Ok(())
            })
        });
        release_tx.send(()).unwrap();
        old_worker.join().unwrap().unwrap();
        next_worker.join().unwrap().unwrap();
        assert_eq!(
            *reports.lock().unwrap(),
            vec![
                vec![DATA_HOST_STATUS, 0],
                vec![DATA_MEDIA_ARTIST, 0],
                vec![DATA_MEDIA_TITLE, 0],
                vec![DATA_HOST_STATUS, 1]
            ]
        );
    }
}

// Test-only stalled-desktop-query seam: exercises the real bridge stop/control
// with a real proxy while no OS desktop service or physical HID is accessed.
#[cfg(test)]
pub(crate) fn test_bridge_holding_transport(
    hid: crate::hid::HidDevice,
) -> (
    QmkHidHostBridge,
    std::sync::mpsc::Sender<()>,
    Arc<AtomicBool>,
) {
    let control = Arc::new(BridgeTransportControl::default());
    control.publish(hid.retirement_handle()).unwrap();
    let (release, blocked_query) = std::sync::mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let worker_finished = finished.clone();
    let thread = thread::spawn(move || {
        let _ = blocked_query.recv_timeout(Duration::from_secs(10));
        drop(hid);
        worker_finished.store(true, Ordering::Release);
    });
    (
        QmkHidHostBridge {
            device: crate::device::Device {
                name: "stalled transport fixture".into(),
                vendor_id: 0,
                product_id: 0,
                manufacturer: String::new(),
                serial_number: String::new(),
                bus_type: "USB".into(),
                path: String::new(),
                instance_token: String::new(),
                firmware: crate::firmware::FirmwareProtocol::Vial,
            },
            mode: HostDataMode::default(),
            protocol: HostProtocol::Discover,
            send_shutdown_on_drop: Arc::new(AtomicBool::new(true)),
            layout_snapshot: Arc::new(AtomicU8::new(u8::MAX)),
            shared_output: None,
            control,
            thread: Some(thread),
        },
        release,
        finished,
    )
}

// Only the endpoint acquisition is injected. Tests exercise the same bridge
// worker, discovery, retirement publication and shared-output path as production.
#[cfg(test)]
pub(crate) fn test_start_bridge(
    target: crate::device::Device,
    mode: HostDataMode,
    shared: Option<crate::hid::SharedHidOutput>,
    mut dedicated: Option<crate::hid::HidDevice>,
    protocol: HostProtocol,
    media: impl FnMut() -> Option<(String, String)> + Send + 'static,
) -> QmkHidHostBridge {
    let is_dedicated = dedicated.is_some();
    QmkHidHostBridge::start_with_sources(
        target,
        mode,
        shared,
        protocol,
        media,
        move |target, shared| {
            if is_dedicated {
                dedicated.take().map(HostDataHid::Dedicated).ok_or_else(|| {
                    anyhow::anyhow!("test dedicated owner exhausted; no hardware fallback")
                })
            } else {
                anyhow::ensure!(
                    shared.is_some(),
                    "test shared owner required; no hardware fallback"
                );
                open_host_data_hid(target, shared)
            }
        },
    )
}

#[cfg(test)]
pub(crate) fn test_open_selected_owner(
    target: &crate::device::Device,
    output: &crate::hid::SharedHidOutput,
) -> anyhow::Result<()> {
    open_host_data_hid(target, Some(output)).map(|_| ())
}
