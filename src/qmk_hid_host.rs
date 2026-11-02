//! Small built-in qmk-hid-host bridge for display presets that expect host data.
//! Sends the same Raw HID packet family as https://github.com/ergohaven/qmk-hid-host.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
    Arc, Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
#[cfg(any(target_os = "windows", test))]
use std::time::SystemTime;
use std::time::{Duration, Instant};

const RAW_HID_PACKET_LEN: usize = 32;
const DATA_TIME: u8 = 0xAA;
const DATA_VOLUME: u8 = 0xAB;
const DATA_LAYOUT: u8 = 0xAC;
const DATA_MEDIA_ARTIST: u8 = 0xAD;
const DATA_MEDIA_TITLE: u8 = 0xAE;
const DATA_DATE: u8 = 0xAF;
// SM62 firmware: elapsed seconds (LE32), then total seconds (LE32).
const DATA_SM62_MEDIA_PROGRESS: u8 = 0xB1;
const SM62_VENDOR_ID: u16 = 0xE126;
const SM62_PRODUCT_ID: u16 = 0x00B2;
const DEFAULT_LAYOUT_CODES: [&str; 2] = ["en", "ru"];
const LAYOUT_RESEND_INTERVAL: Duration = Duration::from_secs(10);
const LAYOUT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const LAYOUT_FOLLOWED_POLL_INTERVAL: Duration = LAYOUT_RESEND_INTERVAL;
// An idle bridge still wakes this often, which bounds how late the Linux
// device-path check notices an unplugged keyboard. Tests use a long recheck
// so every path that must reach a bridge early has to prove its own wake.
#[cfg(not(test))]
const BRIDGE_IDLE_RECHECK: Duration = Duration::from_secs(1);
#[cfg(test)]
const BRIDGE_IDLE_RECHECK: Duration = Duration::from_secs(60);
// Application-layout snapshots change from the UI without waking the bridge.
const BRIDGE_APPLICATION_LAYOUT_TICK: Duration = Duration::from_millis(20);
#[cfg(target_os = "windows")]
const MEDIA_POLL_INTERVAL: Duration = Duration::from_millis(500);
#[cfg(not(target_os = "windows"))]
const MEDIA_POLL_INTERVAL: Duration = Duration::from_secs(1);
// One process-wide desktop service with bounded independent polling lanes, not one query thread per
// bridge/reconnect. It never owns HID or a host-output lease. Demand and cached
// results share one short-held lock; no desktop call executes under that lock.
static HOST_DATA_SERVICE: OnceLock<HostDataService> = OnceLock::new();

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MediaInfo {
    artist: String,
    title: String,
    position_seconds: Option<u32>,
    duration_seconds: Option<u32>,
}

impl MediaInfo {
    fn progress_payload(&self) -> [u8; 9] {
        let duration = self.duration_seconds.unwrap_or(0);
        let position = self.position_seconds.unwrap_or(0).min(duration);
        let mut packet = [0; 9];
        packet[0] = DATA_SM62_MEDIA_PROGRESS;
        packet[1..5].copy_from_slice(&position.to_le_bytes());
        packet[5..9].copy_from_slice(&duration.to_le_bytes());
        packet
    }
}

// Some Windows media sessions publish a position only when the user seeks.
// Keep the displayed clock moving between samples, but re-anchor immediately
// when the session, track, position or playback state changes.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct MediaProgressClock {
    identity: String,
    raw_position: Option<Duration>,
    last_updated_ticks: Option<i64>,
    playing: bool,
    anchor: Option<Instant>,
}

#[cfg(any(target_os = "windows", test))]
impl MediaProgressClock {
    fn update(
        &mut self,
        identity: &str,
        raw_position: Option<Duration>,
        last_updated_ticks: Option<i64>,
        sample_age: Option<Duration>,
        playing: bool,
        duration: Option<u32>,
        now: Instant,
    ) -> Option<u32> {
        // A newer valid publication can be a seek back to the same raw
        // position. Missing/stale/regressing timeline identity is not a seek.
        let fresh_timeline = sample_age.is_some()
            && last_updated_ticks.is_some_and(|ticks| {
                ticks > 0 && self.last_updated_ticks.is_none_or(|previous| ticks > previous)
            });
        if self.anchor.is_none()
            || fresh_timeline
            || self.identity != identity
            || self.raw_position != raw_position
            || self.playing != playing
        {
            let resumed_without_new_timeline = !self.playing
                && playing
                && self.last_updated_ticks == last_updated_ticks
                && self.identity == identity;
            self.identity = identity.to_owned();
            self.raw_position = raw_position;
            self.last_updated_ticks = last_updated_ticks;
            self.playing = playing;
            self.anchor = Some(if playing && !resumed_without_new_timeline {
                now.checked_sub(sample_age.unwrap_or_default())
                    .unwrap_or(now)
            } else {
                now
            });
        }
        let position = raw_position?;
        let elapsed = if playing {
            now.duration_since(self.anchor?)
        } else {
            Duration::ZERO
        };
        Some(
            position
                .saturating_add(elapsed)
                .as_secs()
                .min(duration.unwrap_or(u32::MAX) as u64) as u32,
        )
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_timeline_sample_age(last_updated_ticks: i64, now: SystemTime) -> Option<Duration> {
    // WinRT DateTime and FILETIME both count 100 ns intervals since 1601-01-01.
    const UNIX_EPOCH_TICKS: i128 = 116_444_736_000_000_000;
    const TICKS_PER_SECOND: i128 = 10_000_000;
    let unix_age = now.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    let now_ticks = UNIX_EPOCH_TICKS
        + (unix_age.as_secs() as i128) * TICKS_PER_SECOND
        + (unix_age.subsec_nanos() as i128) / 100;
    let age_ticks = now_ticks - last_updated_ticks as i128;
    // Missing/invalid timestamps must not jump the clock far into a track.
    if last_updated_ticks <= 0 || !(0..=3_600 * TICKS_PER_SECOND).contains(&age_ticks) {
        return None;
    }
    Some(Duration::from_nanos((age_ticks * 100) as u64))
}

fn supports_sm62_media_progress(device: &crate::device::Device) -> bool {
    device.vendor_id == SM62_VENDOR_ID && device.product_id == SM62_PRODUCT_ID
}

#[cfg(any(target_os = "windows", test))]
fn windows_timeline_seconds(
    start_ticks: Option<i64>,
    end_ticks: Option<i64>,
    max_seek_ticks: Option<i64>,
    position_ticks: Option<i64>,
) -> (Option<u32>, Option<u32>) {
    const TICKS_PER_SECOND: i64 = 10_000_000;
    // Some media sessions publish EndTime/Position but not StartTime.
    let start = start_ticks.unwrap_or(0);
    let duration = end_ticks
        .filter(|end| *end > start)
        .or_else(|| max_seek_ticks.filter(|end| *end > start))
        .and_then(|end| u32::try_from((end - start) / TICKS_PER_SECOND).ok())
        .filter(|seconds| *seconds > 0);
    let position = position_ticks
        .and_then(|position| {
            u32::try_from(position.saturating_sub(start).max(0) / TICKS_PER_SECOND).ok()
        })
        .map(|position| duration.map_or(position, |duration| position.min(duration)));
    (position, duration)
}

pub fn media_snapshot() -> Option<(String, String)> {
    HOST_DATA_SERVICE
        .get()
        .and_then(|service| service.snapshot().media)
        .and_then(|media| {
            (!media.artist.is_empty() || !media.title.is_empty())
                .then_some((media.artist, media.title))
        })
}

#[derive(Clone, Default)]
struct DesktopSnapshot {
    volume: Option<u8>,
    layout: Option<u8>,
    // None means no completed sample, Some(empty) means playback stopped.
    media: Option<MediaInfo>,
    media_query_ms: u128,
    media_sampled_at: Option<Instant>,
}

#[derive(Default)]
struct DesktopState {
    demand: [usize; 3],
    epoch: [u64; 3],
    snapshot: DesktopSnapshot,
    stopped: bool,
    // Bridges to wake when a lane they subscribed to stores news for them.
    listeners: Vec<([bool; 3], std::sync::Weak<BridgeWake>)>,
    // The desktop announces every layout change (LayoutEvents): the layout
    // lane polls rarely, samples when `layout_due` is set, and has its source
    // drop what it read from the old list when `layout_list_stale` is.
    layout_following: bool,
    layout_due: bool,
    layout_list_stale: bool,
}

/// Lets a desktop source tell the layout lane what the desktop announced.
/// Holds the service weakly, so a stopped service ends whoever holds it.
#[derive(Clone)]
struct LayoutEvents {
    shared: std::sync::Weak<DesktopShared>,
}

impl LayoutEvents {
    /// The active layout may have changed; with `list_edited`, the list of
    /// layouts did too. Returns false once the service is gone.
    fn changed(&self, list_edited: bool) -> bool {
        self.update(|state| {
            state.layout_due = true;
            state.layout_list_stale |= list_edited;
        })
    }

    /// Whether every layout change is announced from now on, so that the
    /// lane can stop polling for them. Returns false once the service is gone.
    fn following(&self, following: bool) -> bool {
        self.update(|state| state.layout_following = following)
    }

    fn update(&self, change: impl FnOnce(&mut DesktopState)) -> bool {
        let Some(shared) = self.shared.upgrade() else {
            return false;
        };
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return false;
        }
        change(&mut state);
        shared.wake.notify_all();
        true
    }
}

#[derive(Default)]
struct DesktopShared {
    state: Mutex<DesktopState>,
    wake: std::sync::Condvar,
    #[cfg(test)]
    lane_passes: [AtomicU64; 3],
}

#[cfg(test)]
impl DesktopShared {
    /// Stores a sample as a lane does, waking the subscribed bridges, for
    /// tests that drive the snapshot without lane threads.
    fn store_for_test(&self, update: impl FnOnce(&mut DesktopSnapshot)) {
        let state = &mut *self.state.lock().unwrap();
        update(&mut state.snapshot);
        for (_, bridge) in &state.listeners {
            if let Some(bridge) = bridge.upgrade() {
                bridge.wake();
            }
        }
    }
}

/// Wakes a host bridge before its next deadline: a desktop sample it uses
/// changed, it was stopped, or it was handed a transport. A wake that lands
/// while the bridge is busy is kept for its next wait, so none is lost.
#[derive(Default)]
struct BridgeWake {
    pending: Mutex<bool>,
    signal: std::sync::Condvar,
    #[cfg(test)]
    waits: AtomicU64,
}

impl BridgeWake {
    fn wake(&self) {
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.signal.notify_one();
    }

    /// Waits up to `timeout`; returns whether a wake ended the wait.
    fn wait(&self, timeout: Duration) -> bool {
        #[cfg(test)]
        self.waits.fetch_add(1, Ordering::Relaxed);
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let (mut pending, _) = self
            .signal
            .wait_timeout_while(pending, timeout, |pending| !*pending)
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *pending)
    }
}

struct DesktopOwner {
    shared: Arc<DesktopShared>,
}

impl Drop for DesktopOwner {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        self.shared.wake.notify_all();
    }
}

#[derive(Clone)]
struct HostDataService {
    owner: Arc<DesktopOwner>,
}

trait DesktopSource {
    fn volume(&mut self) -> Option<u8>;
    fn layout(&mut self) -> Option<u8>;
    fn media(&mut self) -> Option<MediaInfo>;
    /// Starts following the layout changes the desktop announces. Called on
    /// the layout lane before its first sample; a source that cannot follow
    /// them is polled.
    fn follow_layout(&mut self, _events: LayoutEvents) {}
    /// The desktop's list of layouts changed: forget anything read from it.
    fn layout_list_changed(&mut self) {}
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

    fn media(&mut self) -> Option<MediaInfo> {
        current_media_info()
    }

    #[cfg(target_os = "linux")]
    fn follow_layout(&mut self, events: LayoutEvents) {
        if let Err(error) = thread::Builder::new()
            .name("layout-events".to_owned())
            .spawn(move || follow_layout_signals(events))
        {
            log::warn!("layout signals unavailable, polling the layout instead: {error}");
        }
    }

    fn layout_list_changed(&mut self) {
        // Rebuild on the next sample instead of mapping onto the old list.
        self.layout = None;
        self.last_layout_attempt = Instant::now() - Duration::from_secs(60);
    }
}

impl HostDataService {
    fn start<S: DesktopSource + 'static>(source: impl Fn() -> S + Send + Sync + 'static) -> Self {
        let shared = Arc::new(DesktopShared::default());
        let source = Arc::new(source);
        // One process-wide service, three bounded lanes. Construct platform
        // handles on their owning thread; no OS query or source owns HID.
        // A slow layout/media API must never freeze fresh volume feedback.
        for (index, name) in ["desktop-volume", "desktop-layout", "desktop-media"]
            .into_iter()
            .enumerate()
        {
            let worker = shared.clone();
            let source = source.clone();
            thread::Builder::new()
                .name(name.to_owned())
                .spawn(move || run_desktop_service(worker, source(), index))
                .expect("failed to spawn a desktop host-data lane");
        }
        Self {
            owner: Arc::new(DesktopOwner { shared }),
        }
    }

    fn subscribe(&self, mode: HostDataMode, bridge: &Arc<BridgeWake>) -> DesktopSubscription {
        let enabled = [mode.volume, mode.layout, mode.media];
        let bridge = Arc::downgrade(bridge);
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
        state.listeners.push((enabled, bridge.clone()));
        self.owner.shared.wake.notify_all();
        DesktopSubscription {
            service: self.clone(),
            enabled,
            bridge,
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
    bridge: std::sync::Weak<BridgeWake>,
}

impl Drop for DesktopSubscription {
    fn drop(&mut self) {
        let shared = &self.service.owner.shared;
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(listener) = state
            .listeners
            .iter()
            .position(|(_, bridge)| bridge.ptr_eq(&self.bridge))
        {
            state.listeners.swap_remove(listener);
        }
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
        shared.wake.notify_all();
    }
}

fn run_desktop_service(shared: Arc<DesktopShared>, mut source: impl DesktopSource, index: usize) {
    let polled = [VOLUME_POLL_INTERVAL, LAYOUT_POLL_INTERVAL, MEDIA_POLL_INTERVAL][index];
    // A followed layout is sampled when the desktop announces a change, and
    // otherwise only now and then, in case an announcement went missing.
    let interval = |state: &DesktopState| {
        if index == 1 && state.layout_following {
            LAYOUT_FOLLOWED_POLL_INTERVAL
        } else {
            polled
        }
    };
    if index == 1 {
        source.follow_layout(LayoutEvents {
            shared: Arc::downgrade(&shared),
        });
    }
    let mut last_poll = Instant::now() - Duration::from_secs(60);
    let mut last_epoch = 0;
    loop {
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        while !state.stopped && state.demand[index] == 0 {
            state = shared.wake.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        if state.stopped { return; }
        let epoch = state.epoch[index];
        let announced = index == 1 && std::mem::take(&mut state.layout_due);
        let relist = index == 1 && std::mem::take(&mut state.layout_list_stale);
        let due = announced || last_epoch != epoch || last_poll.elapsed() >= interval(&state);
        drop(state);
        if relist {
            source.layout_list_changed();
        }
        if due {
            last_epoch = epoch;
            last_poll = Instant::now();
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
            if state.stopped { return; }
            // Old in-flight queries cannot populate a re-enabled session.
            if state.demand[index] > 0 && state.epoch[index] == epoch {
                // Every media sample is news: bridges resend and pace progress by it.
                let news = match index {
                    0 => {
                        std::mem::replace(&mut state.snapshot.volume, sample.volume)
                            != sample.volume
                    }
                    1 => {
                        std::mem::replace(&mut state.snapshot.layout, sample.layout)
                            != sample.layout
                    }
                    _ => {
                        state.snapshot.media = sample.media;
                        state.snapshot.media_query_ms = sample.media_query_ms;
                        state.snapshot.media_sampled_at = sample.media_sampled_at;
                        true
                    }
                };
                if news {
                    for (lanes, bridge) in &state.listeners {
                        if let Some(bridge) = bridge.upgrade().filter(|_| lanes[index]) {
                            bridge.wake();
                        }
                    }
                }
            }
        }
        let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped { return; }
        // Sleep until the next sample is due, never closer than the old 20 ms
        // pass. A re-enable or an announcement that arrived during the query
        // is due at once; demand changes, announcements and stop notify the
        // condvar and end the wait early.
        let due_in = if state.epoch[index] != last_epoch || (index == 1 && state.layout_due) {
            Duration::ZERO
        } else {
            interval(&state).saturating_sub(last_poll.elapsed())
        }
        .max(Duration::from_millis(20));
        let _ = shared.wake.wait_timeout(state, due_in);
        #[cfg(test)]
        shared.lane_passes[index].fetch_add(1, Ordering::Relaxed);
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
    fn media(&mut self) -> Option<MediaInfo> {
        (self.0)().map(|(artist, title)| MediaInfo {
            artist,
            title,
            ..Default::default()
        })
    }
}

#[cfg(target_os = "linux")]
const KDE_LAYOUT_DESTINATION: &str = "org.kde.keyboard";
#[cfg(target_os = "linux")]
const KDE_LAYOUT_PATH: &str = "/Layouts";
#[cfg(target_os = "linux")]
const KDE_LAYOUT_INTERFACE: &str = "org.kde.KeyboardLayouts";
#[cfg(target_os = "linux")]
const KDE_LAYOUT_LIST_REFRESH: Duration = Duration::from_secs(1);
#[cfg(target_os = "linux")]
const LAYOUT_DBUS_CALL_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(target_os = "linux")]
pub(crate) const IBUS_DESTINATION: &str = "org.freedesktop.IBus";
#[cfg(target_os = "linux")]
pub(crate) const IBUS_PATH: &str = "/org/freedesktop/IBus";
#[cfg(target_os = "linux")]
pub(crate) const IBUS_INTERFACE: &str = "org.freedesktop.IBus";
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
mod bridge_wake_tests;
#[cfg(test)]
mod clock_tests;
#[cfg(all(test, target_os = "linux"))]
mod layout_signal_tests;
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
    wake: Arc<BridgeWake>,
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
        self.wake.wake();
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
        self.wake.wake();
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
    sm62: bool,
}

#[derive(Clone)]
pub(crate) struct HostOutputLease {
    owner: Arc<HostOutputOwner>,
    footprint: HostOutputFootprint,
}

impl HostOutputOwner {
    /// Keep unsolicited host-data packets off the selected HID stream while a
    /// setting SET and its verified GET are in flight. The bridge already
    /// serializes output through this lock; settings use the proxy directly.
    pub(crate) fn with_settings_write_paused<T>(&self, operation: impl FnOnce() -> T) -> T {
        let _output = self.active.lock().unwrap_or_else(|error| error.into_inner());
        operation()
    }

    pub(crate) fn claim(self: &Arc<Self>, mode: HostDataMode, extended: bool) -> HostOutputLease {
        self.claim_sm62(mode, extended, false)
    }

    pub(crate) fn claim_sm62(
        self: &Arc<Self>,
        mode: HostDataMode,
        extended: bool,
        sm62: bool,
    ) -> HostOutputLease {
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
                sm62,
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
                    layout: old.mode.layout && !self.footprint.mode.layout,
                    ..Default::default()
                };
                for clear in shutdown_payloads_for_device(removed, old.extended, old.sm62) {
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
        let mut sm62 = self.footprint.sm62;
        if let Some(old) = *active {
            mode.time |= old.mode.time;
            mode.media |= old.mode.media;
            mode.layout |= old.mode.layout;
            extended |= old.extended;
            sm62 |= old.sm62;
        }
        // A final shutdown is a single ordered batch. A concurrent new claim
        // never blocks the UI and its worker writes only after this batch ends.
        for clear in shutdown_payloads_for_device(mode, extended, sm62) {
            send(&clear)?;
        }
        *active = None;
        Ok(())
    }
}

// Identity, revocation and send admission share a short-held state lock.
// Only workers take output_order. An admitted write precedes a concurrent
// revocation; it cannot be cancelled inside the native transport. Successor
// writes wait behind it and revalidate after acquiring output_order, so an old
// queued packet can never follow a successor's deactivate/new-profile packet.
struct ApplicationLayoutOwnerState {
    output_order: Arc<Mutex<()>>,
    generation: u64,
    selected: bool,
    discovered_key: Option<String>,
    verified_key: Option<String>,
    snapshot_generation: u64,
    snapshot: crate::application_layouts::ApplicationLayoutSnapshot,
}

impl Default for ApplicationLayoutOwnerState {
    fn default() -> Self {
        Self { output_order: Arc::new(Mutex::new(())), generation: 0, selected: false, discovered_key: None, verified_key: None,
            snapshot_generation: 0,
            snapshot: crate::application_layouts::ApplicationLayoutSnapshot::inactive() }
    }
}

impl ApplicationLayoutOwnerState {
    fn revoke(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.selected = false;
        self.discovered_key = None;
        self.verified_key = None;
        self.snapshot_generation = self.generation;
        self.snapshot = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    }
}

mod wake_identity;

// Stateless across workers/reconnects: semantic identity, not a transition counter.
#[derive(Default)]
struct ApplicationLayoutTransmission;

impl ApplicationLayoutTransmission {
    fn packets(&mut self, snapshot: &crate::application_layouts::ApplicationLayoutSnapshot) -> Vec<[u8; 32]> {
        let mut packets = snapshot.packets();
        if let Some(id) = &snapshot.profile_id {
            let commit = packets.last_mut().expect("snapshot has a commit");
            // Optional v2 extension. v1 consumers ignore it rather than comparing
            // an identity against their obsolete worker-local epoch namespace.
            commit[9..11].copy_from_slice(&[0x57, 2]);
            commit[11..31].copy_from_slice(&wake_identity::digest(id));
        }
        packets
    }
}

fn write_application_layout_payload(
    owner: &Mutex<ApplicationLayoutOwnerState>,
    snapshot: &crate::application_layouts::ApplicationLayoutSnapshot,
    generation: u64,
    packet: &[u8],
    send: impl FnOnce(&[u8]) -> anyhow::Result<()>,
) -> anyhow::Result<bool> {
    let order = owner.lock().unwrap_or_else(|e| e.into_inner()).output_order.clone();
    let _output = order.lock().unwrap_or_else(|e| e.into_inner());
    let state = owner.lock().unwrap_or_else(|e| e.into_inner());
    if state.generation != generation || state.snapshot_generation != generation
        || &state.snapshot != snapshot {
        return Ok(false);
    }
    // Admission is linearized against revoke/publication under the state lock.
    // UI operations never wait on I/O; no other sender can pass this write.
    drop(state);
    send(packet)?;
    Ok(true)
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
    application_layout_snapshot: Arc<Mutex<ApplicationLayoutOwnerState>>,
    application_layout_resend_generation: Arc<AtomicU64>,
    application_layouts_enabled: Arc<AtomicBool>,
    application_layout_device_key: Option<String>,
    application_layout_binding_generation: Option<u64>,
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
            application_layout_snapshot: Arc::new(Mutex::new(
                ApplicationLayoutOwnerState::default(),
            )),
            application_layout_resend_generation: Arc::new(AtomicU64::new(0)),
            application_layout_device_key: None,
            application_layout_binding_generation: None,
            application_layouts_enabled: Arc::new(AtomicBool::new(false)),
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
            HostDataService::start({
                let source = Arc::new(Mutex::new(media_source));
                move || {
                    let source = source.clone();
                    TestMediaSource(move || (source.lock().unwrap_or_else(|e| e.into_inner()))())
                }
            }),
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
            output.for_host_bridge_with_sm62(
                mode,
                matches!(protocol, HostProtocol::Selected(true)),
                supports_sm62_media_progress(&device),
            )
        });
        let control = Arc::new(BridgeTransportControl::default());
        let worker_control = control.clone();
        let worker_device = device.clone();
        let worker_output = shared_output.clone();
        let layout_snapshot = Arc::new(AtomicU8::new(u8::MAX));
        let worker_layout = layout_snapshot.clone();
        let application_layout_snapshot = Arc::new(Mutex::new(
            ApplicationLayoutOwnerState::default(),
        ));
        let worker_application_layout = application_layout_snapshot.clone();
        let application_layout_resend_generation = Arc::new(AtomicU64::new(0));
        let worker_application_layout_resend = application_layout_resend_generation.clone();
        let application_layouts_enabled = Arc::new(AtomicBool::new(false));
        let worker_application_layouts_enabled = application_layouts_enabled.clone();
        let send_shutdown_on_drop = Arc::new(AtomicBool::new(true));
        let worker_shutdown = send_shutdown_on_drop.clone();
        let thread = thread::Builder::new()
            .name("host-bridge".to_owned())
            .spawn(move || {
                run_bridge(
                    worker_device,
                    mode,
                    worker_output,
                    worker_control,
                    worker_layout,
                    worker_application_layout,
                    worker_application_layout_resend,
                    worker_application_layouts_enabled,
                    protocol,
                    worker_shutdown,
                    desktop,
                    open_hid,
                )
            })
            .expect("failed to spawn a host-data bridge");
        Self {
            device,
            mode,
            protocol,
            shared_output,
            control,
            thread: Some(thread),
            send_shutdown_on_drop,
            layout_snapshot,
            application_layout_snapshot,
            application_layout_resend_generation,
            application_layouts_enabled,
            application_layout_device_key: None,
            application_layout_binding_generation: None,
        }
    }

    pub(crate) fn discovered_application_layout_key(&self) -> Option<String> {
        self.discovered_application_layout_identity().map(|(key, _)| key)
    }

    pub(crate) fn discovered_application_layout_identity(&self) -> Option<(String, u64)> {
        let state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        state.discovered_key.clone().map(|key| (key, state.generation))
    }

    pub(crate) fn application_layout_device_key(&self) -> Option<&str> {
        self.application_layout_device_key.as_deref()
    }

    pub(crate) fn clear_application_layout_binding(&mut self) {
        self.application_layout_device_key = None;
        self.application_layout_binding_generation = None;
        let mut state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        if state.verified_key.is_some() {
            // Regranting this same discovered owner must not revive validation
            // tokens captured before a registry/identity revocation.
            state.generation = state.generation.wrapping_add(1);
        }
        state.selected = false;
        state.verified_key = None;
        state.snapshot_generation = state.generation;
        state.snapshot = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    }

    /// Ephemeral evidence from the selected, identified owner, never settings.
    pub(crate) fn bind_selected_application_layout_device(&mut self, key: String) {
        if !self.device.is_ergohaven_display_macropad() { return; }
        let mut state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        state.discovered_key = Some(key.clone());
        state.verified_key = Some(key.clone());
        state.selected = true;
        self.application_layout_binding_generation = Some(state.generation);
        self.application_layout_device_key = Some(key);
    }

    pub(crate) fn has_selected_application_layout_binding(&self) -> bool {
        let state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        state.selected && self.application_layout_binding_generation == Some(state.generation)
            && self.application_layout_device_key.as_ref() == state.verified_key.as_ref()
    }

    pub(crate) fn bind_application_layout_device(&mut self, key: String) {
        if !self.device.is_ergohaven_display_macropad() { return; }
        if self.application_layout_device_key.as_ref() == Some(&key)
            && self.application_layout_publication_generation().is_some() { return; }
        // Registry refresh may carry an old UI routing label, but cannot grant
        // a cold owner merely because its model UID matches. Runtime must check
        // serial/scan identity and bind the captured discovery generation.
        self.clear_application_layout_binding();
        self.application_layout_device_key = Some(key);
    }

    pub(crate) fn bind_verified_application_layout_device(&mut self, key: String, generation: u64) {
        if !self.device.is_ergohaven_display_macropad() { return; }
        let mut state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != generation || state.discovered_key.as_ref() != Some(&key) { return; }
        state.verified_key = Some(key.clone());
        self.application_layout_binding_generation = Some(generation);
        self.application_layout_device_key = Some(key);
    }

    pub(crate) fn application_layout_publication_generation(&self) -> Option<u64> {
        let state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        (self.application_layout_binding_generation == Some(state.generation)
            && state.verified_key.is_some()
            && state.verified_key.as_ref() == self.application_layout_device_key.as_ref())
            .then_some(state.generation)
    }

    pub(crate) fn publish_application_layout_snapshot_for_generation(
        &self, snapshot: crate::application_layouts::ApplicationLayoutSnapshot, generation: u64,
    ) -> bool {
        let mut state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != generation || state.verified_key.is_none()
            || state.verified_key.as_ref() != self.application_layout_device_key.as_ref()
            || self.application_layout_binding_generation != Some(generation) { return false; }
        state.snapshot_generation = generation;
        state.snapshot = snapshot;
        true
    }

    pub(crate) fn set_application_layout_snapshot(
        &self, snapshot: crate::application_layouts::ApplicationLayoutSnapshot,
    ) {
        if let Some(generation) = self.application_layout_publication_generation() {
            self.publish_application_layout_snapshot_for_generation(snapshot, generation);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_application_layout_snapshot(&self) -> crate::application_layouts::ApplicationLayoutSnapshot {
        self.application_layout_snapshot.lock().unwrap().snapshot.clone()
    }

    pub(crate) fn supports_application_layouts(&self) -> bool {
        self.application_layouts_enabled.load(Ordering::Acquire)
    }

    pub(crate) fn set_selected_application_layout_support(&self, supported: bool) {
        self.application_layouts_enabled.store(
            supported && self.device.is_ergohaven_display_macropad(),
            Ordering::Release,
        );
    }

    /// Requests a complete transfer even if the logical profile is unchanged.
    /// Reattached firmware has lost volatile host state and needs all packets,
    /// not only the periodic keepalive.
    pub(crate) fn force_application_layout_resend(&self) {
        self.application_layout_resend_generation
            .fetch_add(1, Ordering::SeqCst);
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
    application_layout_snapshot: Arc<Mutex<ApplicationLayoutOwnerState>>,
    application_layout_resend_generation: Arc<AtomicU64>,
    application_layouts_enabled: Arc<AtomicBool>,
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
    let mut last_application_layout = None;
    let mut last_application_layout_resend_generation =
        application_layout_resend_generation.load(Ordering::SeqCst);
    let mut application_layout_transmission = ApplicationLayoutTransmission::default();
    let mut last_application_layout_send = Instant::now() - Duration::from_secs(60);
    let mut desktop_subscription = None;
    let mut consecutive_write_failures = 0u8;
    let mut first_write_failure_at: Option<Instant> = None;

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
            application_layouts_enabled.store(
                target.is_ergohaven_display_macropad()
                    && hid.supports_application_layout_protocol(),
                Ordering::Release,
            );
            device = Some(HostDataHid::Dedicated(hid));
            first_write_failure_at = None;
            consecutive_write_failures = 0;
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
            last_application_layout = None;
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
            // Revoke before opening/discovering any replacement physical owner.
            if shared_output.is_none() {
                application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner()).revoke();
            }
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
                first_write_failure_at = None;
                consecutive_write_failures = 0;
                if let HostDataHid::Dedicated(hid) = dev {
                    // Also discard any UI grant made while endpoint acquisition
                    // was in flight; it cannot identify this newly opened owner.
                    application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner()).revoke();
                    // Query through this worker's exclusive owner, never the UI's selected handle.
                    let key = target
                        .is_ergohaven_display_macropad()
                        .then(|| {
                            hid.get_keyboard_id().ok().and_then(|(version, uid)| {
                                (version <= 6 && uid != 0 && uid != u64::MAX).then(|| {
                                    let key = format!("vial-{uid:016x}");
                                    if target.is_m4cr0pad_v3() {
                                        key
                                    } else {
                                        format!("{key}-m4cr0pad-v2")
                                    }
                                })
                            })
                        })
                        .flatten();
                    application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner()).discovered_key = key;
                    application_layouts_enabled.store(
                        target.is_ergohaven_display_macropad()
                            && hid.supports_application_layout_protocol(),
                        Ordering::Release,
                    );
                }
                extended_protocol = mode.time && protocol.extended(dev);
                reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);
                last_application_layout = None;
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
            // Until the next open attempt; stop and a handed-over owner wake it.
            control
                .wake
                .wait(Duration::from_secs(2).saturating_sub(last_open_attempt.elapsed()));
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
            last_application_layout = None;
            application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner()).revoke();
            application_layouts_enabled.store(false, Ordering::Release);
            control.wake.wait(Duration::from_millis(250));
            continue;
        }

        let mut write_failed = false;
        let mut write_attempted = false;
        let sm62 = target.is_ergohaven_sm62();

        if application_layouts_enabled.load(Ordering::Acquire) {
            let resend_generation = application_layout_resend_generation.load(Ordering::SeqCst);
            if resend_generation != last_application_layout_resend_generation {
                last_application_layout = None;
                last_application_layout_resend_generation = resend_generation;
            }
            let (application_layout, generation) = {
                let state = application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
                (state.snapshot.clone(), state.snapshot_generation)
            };
            let snapshot_changed = last_application_layout.as_ref() != Some(&application_layout);
            if snapshot_changed {
                let mut sent = true;
                for packet in application_layout_transmission.packets(&application_layout) {
                    match write_application_layout_payload(&application_layout_snapshot,
                        &application_layout, generation, &packet, |p| write_payload_with_retry(dev, p, sm62)) {
                        Ok(true) => { write_attempted = true; },
                        Ok(false) => { sent = false; break; },
                        Err(_) => { sent = false; write_failed = true; break; },
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                if sent {
                    last_application_layout = Some(application_layout);
                    last_application_layout_send = Instant::now();
                }
            } else if last_application_layout_send.elapsed() >= Duration::from_secs(1) {
                match write_application_layout_payload(&application_layout_snapshot,
                    &application_layout, generation, &application_layout.keepalive_packet(),
                    |p| write_payload_with_retry(dev, p, sm62)) {
                    Ok(true) => { write_attempted = true; last_application_layout_send = Instant::now(); },
                    Ok(false) => continue,
                    Err(_) => write_failed = true,
                }
            }
        }

        if mode.time && last_time_poll.elapsed() >= Duration::from_secs(1) {
            last_time_poll = Instant::now();
            if extended_protocol {
                write_attempted = true;
                let started = Instant::now();
                let heartbeat = write_payload_with_retry(dev, &[DATA_HOST_STATUS, 1], sm62);
                log::debug!(
                    "qmk-hid-host heartbeat: target={:?} online=1 elapsed_ms={} result={heartbeat:?}",
                    target.path,
                    started.elapsed().as_millis(),
                );
                write_failed |= heartbeat.is_err();
            }
            let now = current_time_payload();
            if last_time != Some(now) {
                write_attempted = true;
                last_time = Some(now);
                write_failed |= write_payload_with_retry(dev, &[DATA_TIME, now.0, now.1], sm62).is_err();
                pause_between_packets();
                if extended_protocol || target.is_ergohaven_sm62() {
                    use chrono::Datelike;
                    let today = chrono::Local::now();
                    let year = today.year() as u16;
                    write_failed |= write_payload_with_retry(
                        dev,
                        &[
                            DATA_DATE,
                            today.day() as u8,
                            today.month() as u8,
                            year as u8,
                            (year >> 8) as u8,
                        ],
                        sm62,
                    )
                    .is_err();
                    pause_between_packets();
                }
            }
        }

        // HID deadlines consume only cached desktop data. Subscribing cannot
        // run an OS query, and the sampler never receives transport ownership.
        desktop_subscription.get_or_insert_with(|| desktop.subscribe(mode, &control.wake));
        let snapshot = desktop.snapshot();

        if mode.volume && last_volume_poll.elapsed() >= VOLUME_POLL_INTERVAL {
            last_volume_poll = Instant::now();
            if let Some(volume) = snapshot.volume {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if last_volume != Some(volume) {
                    write_attempted = true;
                    last_volume = Some(volume);
                    write_failed |= write_payload_with_retry(dev, &[DATA_VOLUME, volume], sm62).is_err();
                    pause_between_packets();
                }
            }
        }

        if mode.layout && last_layout_poll.elapsed() >= LAYOUT_POLL_INTERVAL {
            last_layout_poll = Instant::now();
            if let Some(layout) = snapshot.layout {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if layout_needs_send(last_layout, last_layout_full_send.elapsed(), layout) {
                    write_attempted = true;
                    last_layout = Some(layout);
                    let layout_write = write_layout_snapshot(dev, layout, &layout_snapshot, sm62);
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
            let media = snapshot.media.unwrap_or_default();
            let artist = &media.artist;
            let title = &media.title;
            log::debug!(
                "qmk-hid-host media query: target={:?} elapsed_ms={}",
                target.path,
                snapshot.media_query_ms
            );
            if stop.load(Ordering::Acquire) {
                break;
            }
            let full_resend = last_media_full_send.elapsed() >= Duration::from_secs(10);
            if full_resend || artist != &last_artist {
                write_attempted = true;
                last_artist = artist.clone();
                write_failed |= write_text_payload(dev, DATA_MEDIA_ARTIST, artist, sm62).is_err();
                pause_between_packets();
            }
            if full_resend || title != &last_title {
                write_attempted = true;
                last_title = title.clone();
                write_failed |= write_text_payload(dev, DATA_MEDIA_TITLE, title, sm62).is_err();
                pause_between_packets();
            }
            if supports_sm62_media_progress(&target) {
                write_attempted = true;
                write_failed |= write_payload_with_retry(dev, &media.progress_payload(), sm62).is_err();
                pause_between_packets();
            }
            if full_resend {
                last_media_full_send = Instant::now();
            }
        }

        if write_failed {
            consecutive_write_failures = consecutive_write_failures.saturating_add(1);
            let failure_now = Instant::now();
            #[cfg(test)]
            let failure_now = review_regressions::failure_now(failure_now);
            let failure_started = *first_write_failure_at.get_or_insert(failure_now);
            if sm62 && failure_now.duration_since(failure_started) < Duration::from_secs(5) {
                // A display transition can temporarily reject an output report
                // while the selected USB HID owner is still alive. Keep the
                // bridge/host status and retry cached data on the next pass.
                log::warn!(
                    "SM62 host output temporarily busy; retrying without disconnect target={:?} failure={}",
                    target.path, consecutive_write_failures
                );
                last_time = None;
                last_volume = None;
                reset_layout_sync_state(&mut last_layout, &mut last_layout_full_send);
                last_application_layout = None;
                last_artist.clear();
                last_title.clear();
                last_media_poll = Instant::now() - Duration::from_secs(60);
                last_media_full_send = Instant::now() - Duration::from_secs(60);
                thread::sleep(Duration::from_millis(100));
                continue;
            }
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
            last_application_layout = None;
            application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner()).revoke();
            application_layouts_enabled.store(false, Ordering::Release);
            last_artist.clear();
            last_title.clear();
            last_media_full_send = Instant::now() - Duration::from_secs(60);
        } else if write_attempted {
            consecutive_write_failures = 0;
            // Idle passes are not evidence of transport recovery.
            first_write_failure_at = None;
        }

        control.wake.wait(bridge_idle_wait(
            Instant::now(),
            mode,
            application_layouts_enabled.load(Ordering::Acquire),
            last_time_poll,
            (snapshot.volume, last_volume, last_volume_poll),
            (
                snapshot.layout,
                last_layout,
                last_layout_poll,
                last_layout_full_send,
            ),
        ));
    }

    if send_shutdown.load(Ordering::Relaxed) {
        if let Some(device) = device.as_ref() {
            if application_layouts_enabled.load(Ordering::Acquire) {
                let packet =
                    crate::application_layouts::ApplicationLayoutSnapshot::deactivate_packet();
                let _ = write_payload(device, &packet);
            }
            send_shutdown_payloads(
                device,
                mode,
                extended_protocol,
                supports_sm62_media_progress(&target),
            );
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

fn write_layout_snapshot(dev: &HostDataHid, layout: u8, snapshot: &AtomicU8, sm62: bool) -> anyhow::Result<()> {
    let result = write_payload_with_retry(dev, &[DATA_LAYOUT, layout], sm62);
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

/// How long a bridge pass may wait before one of its own deadlines falls due.
/// New desktop samples, stop and a handed-over owner wake it earlier.
///
/// `volume` is the sampled value, the last one sent and the last poll;
/// `layout` adds the last full send, which paces the periodic resend.
fn bridge_idle_wait(
    now: Instant,
    mode: HostDataMode,
    application_layouts: bool,
    last_time_poll: Instant,
    (volume, last_volume, last_volume_poll): (Option<u8>, Option<u8>, Instant),
    (layout, last_layout, last_layout_poll, last_layout_full_send): (
        Option<u8>,
        Option<u8>,
        Instant,
        Instant,
    ),
) -> Duration {
    if application_layouts {
        return BRIDGE_APPLICATION_LAYOUT_TICK;
    }
    let mut due = now + BRIDGE_IDLE_RECHECK;
    if mode.time {
        due = due.min(last_time_poll + Duration::from_secs(1));
    }
    // A sample that arrived inside its poll interval is still unsent: come
    // back when the interval opens instead of at the next recheck.
    if mode.volume && volume.is_some() && volume != last_volume {
        due = due.min(last_volume_poll + VOLUME_POLL_INTERVAL);
    }
    if let Some(layout) = layout.filter(|_| mode.layout) {
        let since_send = now.saturating_duration_since(last_layout_full_send);
        due = due.min(if layout_needs_send(last_layout, since_send, layout) {
            last_layout_poll + LAYOUT_POLL_INTERVAL
        } else {
            last_layout_full_send + LAYOUT_RESEND_INTERVAL
        });
    }
    due.saturating_duration_since(now)
}

fn send_shutdown_payloads(
    device: &HostDataHid,
    mode: HostDataMode,
    extended_protocol: bool,
    sm62: bool,
) {
    let payloads = shutdown_payloads_for_device(mode, extended_protocol, sm62);
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
    shutdown_payloads_for_device(mode, extended_protocol, false)
}

fn shutdown_payloads_for_device(
    mode: HostDataMode,
    extended_protocol: bool,
    sm62: bool,
) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    if sm62 && mode.layout {
        payloads.push(vec![DATA_LAYOUT, 255]);
    }
    // Never reopen a device or send BA to firmware that has not advertised it.
    if mode.time && extended_protocol {
        payloads.push(vec![DATA_HOST_STATUS, 0]);
    }
    if mode.media {
        payloads.push(vec![DATA_MEDIA_ARTIST, 0]);
        payloads.push(vec![DATA_MEDIA_TITLE, 0]);
        if sm62 {
            payloads.push(MediaInfo::default().progress_payload().to_vec());
        }
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
    #[cfg(test)]
    if let Some(result) = review_regressions::scripted_write(payload) {
        result?;
    }
    device.write_output_report(payload)
}

fn write_payload_with_retry(device: &HostDataHid, payload: &[u8], sm62: bool) -> anyhow::Result<()> {
    match write_payload(device, payload) {
        Err(error) if sm62 && !crate::hid::is_transport_disconnect_error(&error) => {
            log::warn!("SM62 host data transient HID write failure; retrying once: {error:#}");
            thread::sleep(Duration::from_millis(35));
            write_payload(device, payload)
        }
        result => result,
    }
}

fn sm62_title_payloads(value: &str) -> Vec<Vec<u8>> {
    // The display scrolls pixels; the wire budget is independent of its width.
    // Cap the protocol at 255 bytes without cutting a UTF-8 code point.
    let mut end = value.len().min(255);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = &value.as_bytes()[..end];
    if bytes.len() <= 30 {
        let mut payload = vec![DATA_MEDIA_TITLE, bytes.len() as u8];
        payload.extend_from_slice(bytes);
        return vec![payload];
    }
    bytes.chunks(29).enumerate().map(|(index, chunk)| {
        let mut payload = vec![0xBB, (index * 29) as u8, bytes.len() as u8];
        payload.extend_from_slice(chunk);
        payload
    }).collect()
}

fn write_text_payload(device: &HostDataHid, data_type: u8, value: &str, sm62: bool) -> anyhow::Result<()> {
    if sm62 && data_type == DATA_MEDIA_TITLE {
        let payloads = sm62_title_payloads(value);
        for (index, payload) in payloads.iter().enumerate() {
            if index != 0 {
                pause_between_packets();
            }
            write_payload_with_retry(device, payload, sm62)?;
        }
        return Ok(());
    }
    let mut payload = Vec::with_capacity(RAW_HID_PACKET_LEN);
    let mut bytes = value.as_bytes().to_vec();
    bytes.truncate(30);
    payload.push(data_type);
    payload.push(bytes.len() as u8);
    payload.extend(bytes);
    write_payload_with_retry(device, &payload, sm62)
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
fn current_media_info() -> Option<MediaInfo> {
    windows_platform::media_info()
}

#[cfg(target_os = "linux")]
fn current_media_info() -> Option<MediaInfo> {
    command_stdout(
        "playerctl",
        &[
            "-a",
            "metadata",
            "--format",
            "{{playerName}}\t{{status}}\t{{artist}}\t{{title}}\t{{mpris:length}}",
        ],
    )
    .and_then(|out| split_playerctl_all_metadata(&out))
    .or_else(|| {
        command_stdout(
            "playerctl",
            &[
                "metadata",
                "--format",
                "{{artist}}\t{{title}}\t{{mpris:length}}",
            ],
        )
        .and_then(|out| split_media_line(&out))
        .map(|mut media| {
            media.position_seconds = playerctl_position(None);
            media
        })
    })
    .or_else(mpris_media_info_via_gdbus)
}

#[cfg(target_os = "macos")]
fn current_media_info() -> Option<MediaInfo> {
    let script = r#"
set mediaArtist to ""
set mediaTitle to ""
set mediaPosition to ""
set mediaDuration to ""
tell application "System Events"
    if exists process "Spotify" then
        tell application "Spotify"
            if player state is not stopped then
                set mediaArtist to artist of current track
                set mediaTitle to name of current track
                try
                    set mediaPosition to player position
                    set mediaDuration to duration of current track / 1000
                end try
            end if
        end tell
    else if exists process "Music" then
        tell application "Music"
            if player state is not stopped then
                set mediaArtist to artist of current track
                set mediaTitle to name of current track
                try
                    set mediaPosition to player position
                    set mediaDuration to duration of current track
                end try
            end if
        end tell
    end if
end tell
return mediaArtist & tab & mediaTitle & tab & mediaPosition & tab & mediaDuration
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
fn current_media_info() -> Option<MediaInfo> {
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn split_media_line(line: &str) -> Option<MediaInfo> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(4, '\t');
    let artist = parts.next().unwrap_or_default().trim().to_string();
    let title = parts.next().unwrap_or_default().trim().to_string();
    #[cfg(target_os = "linux")]
    let (position_seconds, duration_seconds) = (None, parts.next().and_then(parse_microseconds));
    #[cfg(target_os = "macos")]
    let (position_seconds, duration_seconds) = (
        parts.next().and_then(parse_seconds),
        parts.next().and_then(parse_seconds),
    );
    (!artist.is_empty() || !title.is_empty()).then_some(MediaInfo {
        artist,
        title,
        position_seconds,
        duration_seconds,
    })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn parse_seconds(value: &str) -> Option<u32> {
    let seconds = value.trim().parse::<f64>().ok()?;
    (seconds.is_finite() && seconds >= 0.0 && seconds <= u32::MAX as f64)
        .then_some(seconds.floor() as u32)
}

#[cfg(target_os = "linux")]
fn parse_microseconds(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_div(1_000_000)?
        .try_into()
        .ok()
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires MACROPAD_WAKE_EXE production C/LVGL harness"]
    fn application_identity_survives_worker_replacement() {
        use crate::application_layouts::ApplicationLayoutSnapshot;
        use std::io::Write;
        for prior in ["", "B"] {
            for disconnect in ["", "U\n", "X\n", "unknown"] {
                for next in ["A", "B"] {
                    let snapshot = |id: &str| {
                        let mut s = ApplicationLayoutSnapshot::inactive();
                        s.active = true;
                        s.name = "worker replacement".into();
                        s.profile_id = Some(id.into());
                        s
                    };
                    let mut old = ApplicationLayoutTransmission::default();
                    let mut new = ApplicationLayoutTransmission::default();
                    let mut trace = String::new();
                    let mut emit = |packets: Vec<[u8;32]>| {
                        for packet in packets {
                            trace.push('P');
                            for byte in packet {trace.push_str(&format!(" {byte:02x}"));}
                            trace.push('\n');
                        }
                    };
                    if !prior.is_empty() {emit(old.packets(&snapshot(prior)));}
                    emit(old.packets(&snapshot("A")));
                    drop(emit);
                    trace.push_str("S\n");
                    if disconnect == "unknown" {
                        for packet in new.packets(&ApplicationLayoutSnapshot::inactive()) {
                            trace.push('P');for byte in packet {trace.push_str(&format!(" {byte:02x}"));}trace.push('\n');
                        }
                        trace.push_str("N\n");
                    } else {trace.push_str(disconnect);}
                    for packet in new.packets(&snapshot(next)) {
                        trace.push('P');for byte in packet {trace.push_str(&format!(" {byte:02x}"));}trace.push('\n');
                    }
                    trace.push_str(if next == "A" {"N\n"} else {"W\n"});
                    if let Ok(dir) = std::env::var("MACROPAD_WAKE_CASES") {
                        let path = std::path::Path::new(&dir).join(format!("worker-{}-{}-{next}.trace",if prior.is_empty(){"initial"}else{prior},match disconnect {""=>"reconnect","U\n"=>"deactivate","X\n"=>"expiry",_=>"unknown"}));
                        std::fs::write(path,&trace).unwrap();
                    }
                    let mut child = std::process::Command::new(std::env::var("MACROPAD_WAKE_EXE").unwrap())
                        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
                    child.stdin.take().unwrap().write_all(trace.as_bytes()).unwrap();
                    let out = child.wait_with_output().unwrap();
                    assert!(out.status.success(), "prior={prior:?} disconnect={disconnect:?} next={next}: {}{}",String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr));
                }
            }
        }
    }

    #[test]
    fn application_profile_wake_epoch_tracks_identity_not_snapshot_content() {
        use crate::application_layouts::ApplicationLayoutSnapshot;
        let mut wire = ApplicationLayoutTransmission::default();
        let mut a = ApplicationLayoutSnapshot::inactive();
        a.active = true;
        a.profile_id = Some("profile-a".into());
        let epoch = |packets: Vec<[u8; 32]>| {
            let commit = packets.last().unwrap();
            assert_eq!(&commit[9..11], &[0x57, 2]);
            <[u8;20]>::try_from(&commit[11..31]).unwrap()
        };
        let first = epoch(wire.packets(&a));
        assert_eq!(epoch(wire.packets(&a)), first, "forced resend");
        a.revision += 1;
        a.name = "renamed".into();
        a.layers[0][0] = 0x0010;
        a.visuals[0][0] = 1;
        assert_eq!(epoch(wire.packets(&a)), first, "edits and visuals are not identity");
        let mut b = a.clone();
        b.profile_id = Some("profile-b".into());
        assert_eq!(a.packets(), b.packets(), "identical legacy wire content");
        let second = epoch(wire.packets(&b));
        assert_ne!(second, first, "different profiles with identical content");
        assert_ne!(epoch(wire.packets(&a)), second, "return to earlier profile");
        let unknown = wire.packets(&ApplicationLayoutSnapshot::inactive());
        assert!(unknown.last().unwrap()[9..].iter().all(|b|*b==0));
    }

    #[test]
    fn forced_application_layout_resend_advances_transport_generation() {
        let bridge = QmkHidHostBridge::test_inert(
            crate::device::Device {
                name: "M4CR0Pad v3".to_owned(),
                vendor_id: 0xE126,
                product_id: 0x0042,
                manufacturer: "Ergohaven".to_owned(),
                serial_number: "resend-test".to_owned(),
                bus_type: "USB".to_owned(),
                path: "resend-test".to_owned(),
                instance_token: "resend-test".to_owned(),
                firmware: crate::firmware::FirmwareProtocol::Vial,
            },
            HostDataMode::default(),
            None,
            HostProtocol::Discover,
        );
        assert_eq!(
            bridge
                .application_layout_resend_generation
                .load(Ordering::SeqCst),
            0
        );

        bridge.force_application_layout_resend();

        assert_eq!(
            bridge
                .application_layout_resend_generation
                .load(Ordering::SeqCst),
            1
        );
    }

    #[test]
    fn unsupported_macropad_gets_clock_and_media_but_no_layout_writes() {
        let path = tempfile::NamedTempFile::new().unwrap();
        let target = crate::device::Device {
            name: "M4CR0Pad v3".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "old-firmware".to_owned(),
            bus_type: "USB".to_owned(),
            path: path.path().to_string_lossy().into_owned(),
            instance_token: "old-firmware".to_owned(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        };
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        // Old firmware does not acknowledge the read-only E6 capability query.
        recorder.respond_with([[0; 32]]);
        let mut bridge = test_start_bridge(
            target,
            HostDataMode {
                time: true,
                media: true,
                ..Default::default()
            },
            None,
            Some(hid),
            HostProtocol::Selected(false),
            || Some(("Artist".to_owned(), "Title".to_owned())),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let requests = recorder.requests();
            if requests.iter().any(|packet| packet[0] == DATA_TIME)
                && requests.iter().any(|packet| packet[0] == DATA_MEDIA_TITLE)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "clock/media host data did not arrive"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!bridge.supports_application_layouts());
        bridge.control.retire();
        bridge.thread.take().unwrap().join().unwrap();
        assert!(
            recorder
                .requests()
                .iter()
                .all(|packet| { !matches!(packet[0], 0xE1..=0xEA) || packet[0] == 0xE6 }),
            "unsupported firmware received an application-layout write"
        );
    }

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
            write_layout_snapshot(&host_data_hid, index, &snapshot, false).unwrap();
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
    fn sm62_shutdown_clears_layout_without_affecting_other_devices() {
        let mode = HostDataMode { time: false, volume: false, layout: true, media: false };
        assert_eq!(shutdown_payloads_for_device(mode, false, true), vec![vec![DATA_LAYOUT, 255]]);
        assert!(shutdown_payloads_for_device(mode, false, false).is_empty());
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
    fn kde_layout_list(codes: &[&str], read_at: Instant) -> KdeLayoutList {
        KdeLayoutList {
            codes: codes.iter().map(|code| (*code).to_owned()).collect(),
            read_at,
        }
    }

    #[cfg(target_os = "linux")]
    fn kde_layouts(codes: &[&str]) -> Option<Vec<String>> {
        Some(codes.iter().map(|code| (*code).to_owned()).collect())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_layout_list_follows_a_reorder_in_the_running_session() {
        let start = Instant::now();
        let mut list = kde_layout_list(&["us", "ru"], start);

        let fresh = start + KDE_LAYOUT_LIST_REFRESH / 2;
        let unread = list.code_index(0, fresh, || panic!("a fresh list is not read again"));
        assert_eq!(unread, Some(0));

        // Same length, so the index alone cannot tell the order changed.
        let due = start + KDE_LAYOUT_LIST_REFRESH;
        let reordered = list.code_index(0, due, || kde_layouts(&["ru", "us"]));
        assert_eq!(reordered, Some(1));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_layout_list_is_read_at_once_for_an_index_past_its_end() {
        let start = Instant::now();
        let mut list = kde_layout_list(&["us"], start);

        let added = list.code_index(1, start, || kde_layouts(&["us", "ru"]));
        assert_eq!(added, Some(1));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_layout_list_keeps_the_last_list_when_kde_does_not_answer() {
        let start = Instant::now();
        let mut list = kde_layout_list(&["us", "ru"], start);

        let due = start + KDE_LAYOUT_LIST_REFRESH;
        assert_eq!(list.code_index(1, due, || None), Some(1));
        assert_eq!(list.code_index(1, due, || Some(Vec::new())), Some(1));
        // The failed read counts as the refresh; the next one waits its turn.
        let soon = due + KDE_LAYOUT_LIST_REFRESH / 2;
        let unread = list.code_index(1, soon, || panic!("a failed read is not retried at once"));
        assert_eq!(unread, Some(1));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn layout_bus_calls_give_up_on_a_peer_that_never_answers() {
        let (client, server) = std::os::unix::net::UnixStream::pair().expect("socket pair");
        let guid = zbus::Guid::generate();
        // A peer with no object server reads the call and never replies.
        let server = thread::spawn(move || {
            zbus::blocking::connection::Builder::unix_stream(server)
                .server(guid)
                .expect("server guid")
                .p2p()
                .build()
        });
        let timeout = Duration::from_millis(200);
        let client = layout_bus(
            Ok(zbus::blocking::connection::Builder::unix_stream(client).p2p()),
            timeout,
        )
        .expect("client connection");
        let _server = server
            .join()
            .expect("server thread")
            .expect("server connection");

        let started = Instant::now();
        let reply = client.call_method(None::<&str>, "/", Some("org.example.Silent"), "Never", &());

        assert!(
            matches!(&reply, Err(zbus::Error::InputOutput(error)) if error.kind() == std::io::ErrorKind::TimedOut),
            "expected a timeout, got {reply:?}"
        );
        assert!(started.elapsed() < timeout * 10);
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

/// Builds a layout tracker connection whose method calls give up after
/// `timeout`. zbus applies no reply timeout of its own, so a wedged KWin or
/// IBus daemon would otherwise park the layout lane for good.
#[cfg(target_os = "linux")]
fn layout_bus(
    builder: zbus::Result<zbus::blocking::connection::Builder<'_>>,
    timeout: Duration,
) -> Option<zbus::blocking::Connection> {
    builder.ok()?.method_timeout(timeout).build().ok()
}

#[cfg(target_os = "linux")]
struct KdeLayoutTracker {
    connection: zbus::blocking::Connection,
    layout_codes: KdeLayoutList,
}

#[cfg(target_os = "linux")]
impl KdeLayoutTracker {
    fn new() -> Option<Self> {
        let connection = layout_bus(
            zbus::blocking::connection::Builder::session(),
            LAYOUT_DBUS_CALL_TIMEOUT,
        )?;
        let layout_codes = kde_layout_list(&connection).filter(|codes| !codes.is_empty())?;
        Some(Self {
            connection,
            layout_codes: KdeLayoutList {
                codes: layout_codes,
                read_at: Instant::now(),
            },
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
        let connection = &self.connection;
        self.layout_codes
            .code_index(layout, Instant::now(), || kde_layout_list(connection))
    }
}

#[cfg(target_os = "linux")]
fn kde_layout_list(connection: &zbus::blocking::Connection) -> Option<Vec<String>> {
    let proxy = zbus::blocking::Proxy::new(
        connection,
        KDE_LAYOUT_DESTINATION,
        KDE_LAYOUT_PATH,
        KDE_LAYOUT_INTERFACE,
    )
    .ok()?;
    let layouts = proxy
        .call::<_, _, Vec<(String, String, String)>>("getLayoutsList", &())
        .ok()?;
    Some(
        layouts
            .into_iter()
            .map(|(short_name, _, _)| short_name)
            .collect(),
    )
}

/// KDE's layout list as last read. `getLayout` answers with a position in it,
/// and a running session adds, removes and reorders layouts without telling
/// the tracker, so the list is read again rather than kept for good.
#[cfg(target_os = "linux")]
struct KdeLayoutList {
    codes: Vec<String>,
    read_at: Instant,
}

#[cfg(target_os = "linux")]
impl KdeLayoutList {
    fn code_index(
        &mut self,
        layout: u32,
        now: Instant,
        read: impl FnOnce() -> Option<Vec<String>>,
    ) -> Option<u8> {
        // An index past the end means the list grew: read it now rather than
        // after the refresh interval.
        let past_end = usize::try_from(layout).map_or(true, |index| index >= self.codes.len());
        if past_end || now.duration_since(self.read_at) >= KDE_LAYOUT_LIST_REFRESH {
            // Stamp the attempt: a list KDE cannot return right now keeps the
            // last one and is asked for again at the next refresh.
            self.read_at = now;
            if let Some(codes) = read().filter(|codes| !codes.is_empty()) {
                self.codes = codes;
            }
        }
        kde_layout_code_index(layout, &self.codes)
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
        let connection = layout_bus(
            zbus::blocking::connection::Builder::ibus(),
            LAYOUT_DBUS_CALL_TIMEOUT,
        )?;
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
fn kde_desktop_session() -> bool {
    ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter_map(|value| value.into_string().ok())
        .any(|value| {
            value
                .split(':')
                .any(|desktop| desktop.eq_ignore_ascii_case("kde"))
        })
}

#[cfg(target_os = "linux")]
const LAYOUT_SIGNALS_RETRY: Duration = Duration::from_secs(10);

/// Follows KDE's and IBus's layout signals for the layout lane. Whenever they
/// stop, for instance across a Plasma or IBus restart, the lane polls again
/// until they are back. Ends with the service, or at once on a desktop that
/// announces no layout changes.
#[cfg(target_os = "linux")]
fn follow_layout_signals(events: LayoutEvents) {
    while kde_desktop_session() || gnome_desktop_session() {
        futures_lite::future::block_on(async {
            if let Some(streams) = layout_signal_streams().await {
                watch_layout_signals(streams, &events).await;
            }
        });
        if !events.following(false) {
            return;
        }
        thread::sleep(LAYOUT_SIGNALS_RETRY);
    }
}

/// The signals the layout lane follows.
#[cfg(target_os = "linux")]
struct LayoutSignalStreams {
    kde: Option<zbus::MessageStream>,
    ibus: Option<zbus::MessageStream>,
    /// While no IBus runs: IBus taking its name on the session bus.
    ibus_started: Option<zbus::MessageStream>,
}

/// What setting up IBus's engine signal found.
#[cfg(target_os = "linux")]
enum IbusSignal {
    Subscribed(zbus::MessageStream),
    /// No IBus runs, so it switches no engine until `started` says it runs.
    Absent {
        started: zbus::MessageStream,
    },
    /// IBus runs, or may run, but its engine signal could not be subscribed
    /// to.
    Unreached,
}

/// KDE's layout signals while KDE serves layouts, and IBus's engine signal
/// while IBus runs; None unless together they announce every layout change.
#[cfg(target_os = "linux")]
async fn layout_signal_streams() -> Option<LayoutSignalStreams> {
    let session = zbus::Connection::session().await.ok()?;
    let ibus = async { zbus::connection::Builder::ibus()?.build().await };
    layout_signal_streams_on(&session, ibus, gnome_desktop_session()).await
}

/// [`layout_signal_streams`] on `session`, with `ibus` connecting to IBus.
#[cfg(target_os = "linux")]
async fn layout_signal_streams_on(
    session: &zbus::Connection,
    ibus: impl std::future::Future<Output = zbus::Result<zbus::Connection>>,
    gnome: bool,
) -> Option<LayoutSignalStreams> {
    let bus = zbus::fdo::DBusProxy::new(session).await.ok()?;
    let kde_serves = bus
        .name_has_owner(KDE_LAYOUT_DESTINATION.try_into().ok()?)
        .await
        .ok()?;
    let kde = if kde_serves {
        Some(layout_signal_stream(session, KDE_LAYOUT_INTERFACE, KDE_LAYOUT_PATH).await?)
    } else {
        None
    };
    // Watched before IBus is looked for, so that one starting in between is
    // not missed.
    let started = ibus_started_stream(session).await?;
    // With no IBus daemon there is no engine switch to miss. With one, its
    // switches count too: the Entropy engine takes precedence over KDE.
    let ibus = match ibus.await {
        Ok(ibus) => match layout_signal_stream(&ibus, IBUS_INTERFACE, IBUS_PATH).await {
            Some(stream) => IbusSignal::Subscribed(stream),
            None => IbusSignal::Unreached,
        },
        // `ibus address` also fails when the command cannot run or prints no
        // address, so that alone does not mean IBus is absent. IBus owns its
        // session bus name while it runs: one that already runs announces no
        // start, and following KDE alone would miss its switches for good.
        Err(zbus::Error::Address(_)) => {
            match bus.name_has_owner(IBUS_DESTINATION.try_into().ok()?).await {
                Ok(false) => IbusSignal::Absent { started },
                _ => IbusSignal::Unreached,
            }
        }
        Err(_) => IbusSignal::Unreached,
    };
    followed_layout_signals(kde, ibus, gnome)
}

/// The streams to follow, if together they announce every layout change.
/// KDE announces its own switches; elsewhere only IBus does (GNOME). An IBus
/// that runs unfollowed switches engines unannounced, so the lane must keep
/// polling for it.
#[cfg(target_os = "linux")]
fn followed_layout_signals(
    kde: Option<zbus::MessageStream>,
    ibus: IbusSignal,
    gnome: bool,
) -> Option<LayoutSignalStreams> {
    let (ibus, ibus_started) = match ibus {
        IbusSignal::Subscribed(stream) => (Some(stream), None),
        IbusSignal::Absent { started } => (None, Some(started)),
        IbusSignal::Unreached => return None,
    };
    (kde.is_some() || (ibus.is_some() && gnome)).then_some(LayoutSignalStreams {
        kde,
        ibus,
        ibus_started,
    })
}

/// IBus taking its name on the session bus, as it does when it starts.
#[cfg(target_os = "linux")]
async fn ibus_started_stream(session: &zbus::Connection) -> Option<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .ok()?
        .interface("org.freedesktop.DBus")
        .ok()?
        .member("NameOwnerChanged")
        .ok()?
        .arg(0, IBUS_DESTINATION)
        .ok()?
        .build();
    zbus::MessageStream::for_match_rule(rule, session, None)
        .await
        .ok()
}

#[cfg(target_os = "linux")]
async fn layout_signal_stream(
    connection: &zbus::Connection,
    interface: &'static str,
    path: &'static str,
) -> Option<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(interface)
        .ok()?
        .path(path)
        .ok()?
        .build();
    zbus::MessageStream::for_match_rule(rule, connection, None)
        .await
        .ok()
}

/// Turns layout signals into lane events until a stream ends or fails, or
/// IBus starts and its engine signal is to be followed too.
#[cfg(target_os = "linux")]
async fn watch_layout_signals(streams: LayoutSignalStreams, events: &LayoutEvents) {
    use futures_lite::StreamExt as _;

    let LayoutSignalStreams {
        mut kde,
        mut ibus,
        mut ibus_started,
    } = streams;
    // Whatever changed while nobody listened is read again first.
    if !events.following(true) || !events.changed(true) {
        return;
    }
    loop {
        let next = futures_lite::future::race(
            futures_lite::future::race(
                async {
                    match kde.as_mut() {
                        Some(kde) => kde.next().await.map(|message| (true, message)),
                        None => std::future::pending().await,
                    }
                },
                async {
                    match ibus.as_mut() {
                        Some(ibus) => ibus.next().await.map(|message| (false, message)),
                        None => std::future::pending().await,
                    }
                },
            ),
            async {
                match ibus_started.as_mut() {
                    // Ends the watch: the next setup subscribes to IBus too.
                    Some(started) => started.next().await.and(None),
                    None => std::future::pending().await,
                }
            },
        )
        .await;
        let Some((from_kde, Ok(message))) = next else {
            return;
        };
        let list_edited = from_kde
            && message
                .header()
                .member()
                .is_some_and(|member| member.as_str() == "layoutListChanged");
        if !events.changed(list_edited) {
            return;
        }
    }
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
fn playerctl_position(player: Option<&str>) -> Option<u32> {
    let args = match player {
        Some(name) => vec!["-p", name, "position"],
        None => vec!["position"],
    };
    command_stdout("playerctl", &args)
        .as_deref()
        .and_then(parse_seconds)
}

#[cfg(target_os = "linux")]
fn split_playerctl_all_metadata(output: &str) -> Option<MediaInfo> {
    let mut fallback = None;
    for line in output.lines() {
        let mut parts = line.splitn(5, '\t');
        let player = parts.next().unwrap_or_default().trim();
        let status = parts.next().unwrap_or_default().trim();
        let artist = parts.next().unwrap_or_default().trim().to_string();
        let title = parts.next().unwrap_or_default().trim().to_string();
        if artist.is_empty() && title.is_empty() {
            continue;
        }
        let media = MediaInfo {
            artist,
            title,
            position_seconds: None,
            duration_seconds: parts.next().and_then(parse_microseconds),
        };
        if status.eq_ignore_ascii_case("playing") {
            return Some(MediaInfo {
                position_seconds: playerctl_position(Some(player)),
                ..media
            });
        }
        fallback.get_or_insert((player.to_string(), media));
    }
    fallback.map(|(player, media)| MediaInfo {
        position_seconds: playerctl_position(Some(&player)),
        ..media
    })
}

#[cfg(target_os = "linux")]
fn mpris_media_info_via_gdbus() -> Option<MediaInfo> {
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
        let Some(mut media) = split_gdbus_mpris_metadata(&metadata) else {
            continue;
        };
        media.position_seconds = gdbus_get_mpris_property(&name, "Position")
            .as_deref()
            .and_then(gvariant_microseconds);
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
fn split_gdbus_mpris_metadata(metadata: &str) -> Option<MediaInfo> {
    let artist = gvariant_metadata_string(metadata, "xesam:artist").unwrap_or_default();
    let title = gvariant_metadata_string(metadata, "xesam:title").unwrap_or_default();
    (!artist.is_empty() || !title.is_empty()).then_some(MediaInfo {
        artist,
        title,
        position_seconds: None,
        duration_seconds: metadata
            .find("mpris:length")
            .and_then(|index| gvariant_microseconds(&metadata[index + "mpris:length".len()..])),
    })
}

#[cfg(target_os = "linux")]
fn gvariant_microseconds(value: &str) -> Option<u32> {
    let after_type = value
        .split_once("int64 ")
        .map(|(_, tail)| tail)
        .or_else(|| value.split_once('<').map(|(_, tail)| tail))?;
    let number = after_type.split(|c: char| !c.is_ascii_digit()).next()?;
    parse_microseconds(number)
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
    use std::cell::RefCell;
    use std::time::Duration;
    use windows::{
        Media::Control::{
            GlobalSystemMediaTransportControlsSession,
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        },
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

    type SessionSample = (
        super::MediaInfo,
        bool,
        String,
        Option<Duration>,
        Option<i64>,
        Option<Duration>,
    );
    thread_local! {
        static MEDIA_CLOCK: RefCell<super::MediaProgressClock> = RefCell::new(Default::default());
    }

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

    pub fn media_info() -> Option<super::MediaInfo> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .and_then(|request| request.get())
            .ok()?;
        let current = manager.GetCurrentSession().ok();
        let current_info = current.as_ref().and_then(read_session);
        if current_info
            .as_ref()
            .is_some_and(|(media, playing, ..)| *playing && media.duration_seconds.is_some())
        {
            return current_info.map(finish_sample);
        }
        // A metadata-only browser session may remain selected while another
        // playing session publishes a usable timeline. Never combine metadata
        // and timeline from different sessions.
        if let Ok(sessions) = manager.GetSessions() {
            for session in sessions {
                if session
                    .GetPlaybackInfo()
                    .and_then(|info| info.PlaybackStatus())
                    .ok()
                    != Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
                {
                    continue;
                }
                if let Some(media) =
                    read_session(&session).filter(|(media, ..)| media.duration_seconds.is_some())
                {
                    return Some(finish_sample(media));
                }
            }
        }
        current_info.map(finish_sample)
    }

    fn finish_sample(
        (mut media, playing, identity, raw_position, last_updated_ticks, sample_age): SessionSample,
    ) -> super::MediaInfo {
        media.position_seconds = MEDIA_CLOCK.with(|clock| {
            clock.borrow_mut().update(
                &identity,
                raw_position,
                last_updated_ticks,
                sample_age,
                playing,
                media.duration_seconds,
                std::time::Instant::now(),
            )
        });
        media
    }

    fn read_session(session: &GlobalSystemMediaTransportControlsSession) -> Option<SessionSample> {
        let props = session
            .TryGetMediaPropertiesAsync()
            .and_then(|request| request.get())
            .ok()?;
        let artist = props.Artist().unwrap_or_default().to_string();
        let title = props.Title().unwrap_or_default().to_string();
        if artist.is_empty() && title.is_empty() {
            return None;
        }
        let timeline_result = session.GetTimelineProperties();
        if let Err(error) = &timeline_result {
            log::debug!("Windows media timeline unavailable: {error}");
        }
        let timeline = timeline_result.ok();
        let sampled_at = std::time::SystemTime::now();
        let last_updated_ticks = timeline
            .as_ref()
            .and_then(|t| t.LastUpdatedTime().ok())
            .map(|t| t.UniversalTime);
        let sample_age = last_updated_ticks
            .and_then(|ticks| super::windows_timeline_sample_age(ticks, sampled_at));
        let raw_position = timeline
            .as_ref()
            .and_then(|t| t.Position().ok())
            .map(|t| t.Duration);
        let playback = session.GetPlaybackInfo().ok();
        let playing = playback
            .as_ref()
            .and_then(|info| info.PlaybackStatus().ok())
            == Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
        let (position_seconds, duration_seconds) = super::windows_timeline_seconds(
            timeline
                .as_ref()
                .and_then(|t| t.StartTime().ok())
                .map(|t| t.Duration),
            timeline
                .as_ref()
                .and_then(|t| t.EndTime().ok())
                .map(|t| t.Duration),
            timeline
                .as_ref()
                .and_then(|t| t.MaxSeekTime().ok())
                .map(|t| t.Duration),
            raw_position,
        );
        let start_ticks = timeline
            .as_ref()
            .and_then(|t| t.StartTime().ok())
            .map(|t| t.Duration)
            .unwrap_or(0);
        let precise_position = raw_position.map(|position| {
            Duration::from_nanos(
                (position.saturating_sub(start_ticks).max(0) as u64).saturating_mul(100),
            )
        });
        let source = session
            .SourceAppUserModelId()
            .unwrap_or_default()
            .to_string();
        log::debug!(
            "Windows media session {}: start={:?} end={:?} max_seek={:?} position={:?} updated={:?} age={:?} playing={playing}; parsed={:?}/{:?}",
            source,
            timeline.as_ref().and_then(|t| t.StartTime().ok()).map(|t| t.Duration),
            timeline.as_ref().and_then(|t| t.EndTime().ok()).map(|t| t.Duration),
            timeline.as_ref().and_then(|t| t.MaxSeekTime().ok()).map(|t| t.Duration),
            raw_position,
            last_updated_ticks,
            sample_age,
            position_seconds,
            duration_seconds
        );
        let identity = format!("{source}\0{artist}\0{title}\0{duration_seconds:?}");
        Some((
            super::MediaInfo {
                artist,
                title,
                position_seconds,
                duration_seconds,
            },
            playing,
            identity,
            precise_position,
            last_updated_ticks,
            sample_age,
        ))
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
                    Arc::new(Mutex::new(
                        ApplicationLayoutOwnerState::default(),
                    )),
                    Arc::new(AtomicU64::new(0)),
                    Arc::new(AtomicBool::new(false)),
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
        // The replacement claims the owner before the old bridge is stopped.
        // Stop wakes the old bridge at once, so the other order would let its
        // shutdown run first; the UI suppresses it in that case instead.
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
        let stopped = Instant::now();
        old.stop();
        assert!(
            stopped.elapsed() < Duration::from_secs(1),
            "shared stop waited for desktop I/O"
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
        // No replacement bridge was available to old.stop(), only a claim that
        // lands before it and is never initialized: the next generation must
        // still clear the old modes. A stopped bridge shuts down at once, so
        // the claim has to come first rather than race the old shutdown.
        let unused = output.for_host_bridge(HostDataMode::default(), false);
        old.stop();
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
    fn sm62_setting_write_excludes_shared_host_packets_until_readback() {
        let owner = Arc::new(HostOutputOwner::default());
        let lease = owner.claim_sm62(HostDataMode { media: true, ..Default::default() }, false, true);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let settings_owner = owner.clone();
        let settings = thread::spawn(move || {
            settings_owner.with_settings_write_paused(|| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (write_tx, write_rx) = std::sync::mpsc::channel();
        let host = thread::spawn(move || {
            lease.write(&[DATA_MEDIA_ARTIST, 0], |_| {
                write_tx.send(()).unwrap();
                Ok(())
            }).unwrap();
        });
        assert!(write_rx.recv_timeout(Duration::from_millis(50)).is_err());
        release_tx.send(()).unwrap();
        settings.join().unwrap();
        write_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        host.join().unwrap();
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
            application_layout_snapshot: Arc::new(Mutex::new(
                ApplicationLayoutOwnerState::default(),
            )),
            application_layout_resend_generation: Arc::new(AtomicU64::new(0)),
            application_layout_device_key: None,
            application_layout_binding_generation: None,
            application_layouts_enabled: Arc::new(AtomicBool::new(false)),
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
pub(crate) fn test_start_profile_handoff_bridge(
    target: crate::device::Device,
    shared: Option<crate::hid::SharedHidOutput>,
) -> QmkHidHostBridge {
    struct EmptyDesktop;
    impl DesktopSource for EmptyDesktop {
        fn volume(&mut self) -> Option<u8> { None }
        fn layout(&mut self) -> Option<u8> { None }
        fn media(&mut self) -> Option<MediaInfo> { None }
    }
    QmkHidHostBridge::start_with_desktop_service(
        target,
        HostDataMode { time: true, volume: true, media: true, ..Default::default() },
        shared,
        HostProtocol::Selected(true),
        HostDataService::start(|| EmptyDesktop),
        |target, shared| {
            anyhow::ensure!(shared.is_some(), "handoff fixture must not open real hardware");
            open_host_data_hid(target, shared)
        },
    )
}

#[cfg(test)]
#[test]
fn fresh_dedicated_owner_never_receives_previous_profile_snapshot() {
    let mut target = crate::device::test_usb_device("3-3", 5, 0x42);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (hid, packets) = crate::hid::HidDevice::test_device();
    packets.respond_by(|request| {
        if request[..2] == [0xfe, 0] {
            let mut response = [0; 32];
            response[..4].copy_from_slice(&6u32.to_le_bytes());
            response[4..12].copy_from_slice(&0x99u64.to_le_bytes());
            Some(Ok(response))
        } else if request[0] == 0xe6 {
            let mut response = [0; 32];
            response[..3].copy_from_slice(&[
                0xe6,
                crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                0x5a,
            ]);
            Some(Ok(response))
        } else {
            None
        }
    });
    let (release, wait) = std::sync::mpsc::channel();
    let mut hid = Some(hid);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        None,
        HostProtocol::Discover,
        || None,
        move |_, _| {
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(HostDataHid::Dedicated(hid.take().unwrap()))
        },
    );
    let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
    settings
        .layouts
        .get_mut(&settings.active_layout_id.clone())
        .unwrap()
        .set_keycode(0, 0, 5);
    let previous = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(
        settings.active_layout().unwrap(),
    );
    bridge.bind_selected_application_layout_device("vial-0000000000000042".into());
    assert!(bridge.has_selected_application_layout_binding());
    bridge.set_application_layout_snapshot(previous.clone());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !bridge.supports_application_layouts() || packets.requests().len() < 3 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(100));
    assert!(!bridge.has_selected_application_layout_binding(), "fresh open retained selected-owner trust");
    assert_eq!(
        bridge.test_application_layout_snapshot(),
        crate::application_layouts::ApplicationLayoutSnapshot::inactive(),
        "fresh endpoint inherited snapshot from a previous owner before identity validation"
    );
    let inactive_packets =
        crate::application_layouts::ApplicationLayoutSnapshot::inactive().packets();
    for old in previous
        .packets()
        .iter()
        .filter(|packet| !inactive_packets.contains(packet))
    {
        assert!(
            !packets
                .requests()
                .iter()
                .any(|request| request.starts_with(old)),
            "new physical owner received previous active-profile bytes"
        );
    }
}

#[cfg(test)]
#[test]
fn cleared_application_binding_never_revives_old_publication_token() {
    let target = crate::device::test_usb_device("3-3", 5, 0x42);
    let mut bridge = QmkHidHostBridge::test_inert(target, Default::default(), None, HostProtocol::Discover);
    let key = "vial-0000000000000042";
    bridge.bind_selected_application_layout_device(key.into());
    let old_generation = bridge.application_layout_publication_generation().unwrap();
    bridge.clear_application_layout_binding();
    bridge.bind_selected_application_layout_device(key.into());
    assert!(!bridge.publish_application_layout_snapshot_for_generation(
        crate::application_layouts::ApplicationLayoutSnapshot::inactive(), old_generation),
        "regrant resurrected a revoked publication token");
}

#[cfg(test)]
#[test]
fn application_output_revalidates_queued_packets_and_keepalives_after_owner_revocation() {
    use crate::application_layouts::{ApplicationLayoutSnapshot, DeviceApplicationLayouts};
    for keepalive in [false, true] {
        let mut settings = DeviceApplicationLayouts::default();
        settings.active_layout_id = settings.create_for_application(&Default::default());
        let snapshot = ApplicationLayoutSnapshot::from_layout(settings.active_layout().unwrap());
        let owner = Arc::new(Mutex::new(ApplicationLayoutOwnerState::default()));
        owner.lock().unwrap().snapshot = snapshot.clone();
        let packet = if keepalive { snapshot.keepalive_packet() } else { snapshot.packets()[0] };
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let sending_owner = owner.clone();
        let sending_snapshot = snapshot.clone();
        let admitted = thread::spawn(move || write_application_layout_payload(
            &sending_owner, &sending_snapshot, 0, &packet, |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            }));
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Revocation itself is short and never waits on the admitted transport.
        owner.lock().unwrap().revoke();
        let stale_owner = owner.clone();
        let stale = thread::spawn(move || write_application_layout_payload(
            &stale_owner, &snapshot, 0, &packet, |_| panic!("queued stale send reached HID")));
        let successor_owner = owner.clone();
        let (successor_tx, successor_rx) = std::sync::mpsc::channel();
        let successor = thread::spawn(move || {
            let inactive = ApplicationLayoutSnapshot::inactive();
            write_application_layout_payload(&successor_owner, &inactive, 1,
                &inactive.keepalive_packet(), |_| { successor_tx.send(()).unwrap(); Ok(()) })
        });
        assert!(successor_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "successor overtook an admitted write");
        release_tx.send(()).unwrap();
        assert!(admitted.join().unwrap().unwrap());
        assert!(!stale.join().unwrap().unwrap());
        assert!(successor.join().unwrap().unwrap(), "idle/revoked inactive control was rejected");
        successor_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }
}

#[cfg(test)]
fn assert_gated_application_output_ui_progress(keepalive: bool) {
    use crate::application_layouts::{ApplicationLayoutSnapshot, DeviceApplicationLayouts};
    let path = tempfile::NamedTempFile::new().unwrap();
    let mut target = crate::device::test_usb_device("3-3", 5, 0x42);
    target.path = path.path().to_string_lossy().into_owned();
    let (hid, packets) = crate::hid::HidDevice::test_device();
    let mut settings = DeviceApplicationLayouts::default();
    settings.active_layout_id = settings.create_for_application(&Default::default());
    settings.layouts.get_mut(&settings.active_layout_id).unwrap().set_keycode(0, 0, 5);
    let snapshot = ApplicationLayoutSnapshot::from_layout(settings.active_layout().unwrap());
    let gated_packet = if keepalive { snapshot.keepalive_packet() } else { snapshot.packets()[0] };
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut gated = false;
    packets.on_output(move |packet| {
        if !gated && packet == gated_packet {
            gated = true;
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    });
    // The worker acquires its endpoint only once the owner is bound, supported
    // and published. None of those setters wakes it, so a worker that ran
    // ahead of them would select the long test idle before sending anything.
    let (open_tx, open_rx) = std::sync::mpsc::channel::<()>();
    let mut open_gate = Some(open_rx);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |target, shared| {
            // A failed setup drops the sender, which releases the worker too.
            if let Some(gate) = open_gate.take() {
                let _ = gate.recv();
            }
            anyhow::ensure!(
                shared.is_some(),
                "test shared owner required; no hardware fallback"
            );
            open_host_data_hid(target, shared)
        },
    );
    bridge.bind_selected_application_layout_device("vial-0000000000000042".into());
    bridge.set_selected_application_layout_support(true);
    let generation = bridge.application_layout_publication_generation().unwrap();
    bridge.set_application_layout_snapshot(snapshot.clone());
    assert_eq!(
        bridge.control.wake.waits.load(Ordering::Relaxed),
        0,
        "worker waited before its fixture was set up"
    );
    open_tx.send(()).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (progress_tx, progress_rx) = std::sync::mpsc::channel();
    let ui = thread::spawn(move || {
        assert_eq!(bridge.application_layout_publication_generation(), Some(generation));
        assert!(bridge.discovered_application_layout_identity().is_some());
        bridge.clear_application_layout_binding();
        let rejected = !bridge.publish_application_layout_snapshot_for_generation(snapshot, generation);
        progress_tx.send(rejected).unwrap();
        bridge
    });
    // Release and join even on RED: the fixture must not leave a blocked worker.
    let progress = progress_rx.recv_timeout(Duration::from_millis(250));
    release_tx.send(()).unwrap();
    let mut bridge = ui.join().unwrap();
    let worker = bridge.thread.take().unwrap();
    thread::sleep(Duration::from_millis(100));
    bridge.stop();
    worker.join().unwrap();
    assert_eq!(progress.ok(), Some(true), "UI identity/generation readers stalled behind HID output");
    let requests = packets.requests();
    let admitted = requests.iter().position(|p| p == &gated_packet).unwrap();
    let mut inactive = ApplicationLayoutSnapshot::inactive().packets();
    inactive.push(ApplicationLayoutSnapshot::inactive().keepalive_packet());
    inactive.push(ApplicationLayoutSnapshot::deactivate_packet()); // worker shutdown
    assert!(requests[admitted + 1..].iter().all(|p| inactive.iter().any(|expected| {
        // Wake-extension bytes do not change the revocation/active flag, CRC
        // or revision contract checked by this ownership regression.
        if p[0] == 0xe3 { p[..9] == expected[..9] } else { p == expected }
    })),
        "revoked snapshot sent another packet/keepalive after the admitted write");
    assert_eq!(bridge.test_application_layout_snapshot(), ApplicationLayoutSnapshot::inactive());
}

#[cfg(test)]
#[test]
fn gated_application_packet_does_not_stall_ui_or_send_revoked_snapshot() {
    assert_gated_application_output_ui_progress(false);
}

#[cfg(test)]
#[test]
fn gated_application_keepalive_does_not_stall_ui_or_send_revoked_snapshot() {
    assert_gated_application_output_ui_progress(true);
}

#[cfg(test)]
#[test]
fn late_profile_publication_after_worker_reopen_is_refused() {
    let mut target = crate::device::test_usb_device("3-3", 5, 0x42);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (hid, packets) = crate::hid::HidDevice::test_device();
    let (opened_tx, opened_rx) = std::sync::mpsc::channel();
    let (discover_tx, discover_rx) = std::sync::mpsc::channel();
    let (tick_tx, tick_rx) = std::sync::mpsc::channel();
    let fresh_ready = Arc::new(AtomicBool::new(false));
    let discovery_ready = fresh_ready.clone();
    packets.respond_by(move |request| {
        if request[..2] == [0xfe, 0] {
            let mut response = [0; 32];
            response[..4].copy_from_slice(&6u32.to_le_bytes());
            response[4..12].copy_from_slice(&0x99u64.to_le_bytes());
            Some(Ok(response))
        } else if request[0] == 0xe6 {
            opened_tx.send(()).unwrap();
            discover_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            discovery_ready.store(true, Ordering::Release);
            let mut response = [0; 32];
            response[..3].copy_from_slice(&[
                0xe6, crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION, 0x5a,
            ]);
            Some(Ok(response))
        } else { None }
    });
    let (open_tx, open_rx) = std::sync::mpsc::channel();
    let mut hid = Some(hid);
    let (selected_hid, selected_packets) = crate::hid::HidDevice::test_device();
    let selected_output = selected_hid.shared_output().unwrap();
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target, HostDataMode { time: true, media: true, ..Default::default() }, Some(selected_output),
        HostProtocol::Selected(false),
        move || {
            if fresh_ready.load(Ordering::Acquire) { let _ = tick_tx.send(()); }
            None
        },
        move |target, shared| {
            if shared.is_some() { return open_host_data_hid(target, shared); }
            open_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(HostDataHid::Dedicated(hid.take().unwrap()))
        },
    );
    bridge.bind_selected_application_layout_device("vial-0000000000000042".into());
    // Exact-owner handoff retains the grant; subsequent transport loss must not.
    assert!(bridge.adopt_selected_hid(selected_hid).is_ok());
    // Runtime validates the selected binding before the worker replaces its owner.
    assert!(bridge.has_selected_application_layout_binding());
    let validated_generation = bridge.application_layout_publication_generation().unwrap();
    let settings = crate::application_layouts::DeviceApplicationLayouts::default();
    let previous = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(
        settings.active_layout().unwrap(),
    );
    selected_packets.disconnect_output();
    open_tx.send(()).unwrap();
    opened_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Discovery is paused AFTER real worker revocation, BEFORE its first profile write.
    let late_publication = bridge.publish_application_layout_snapshot_for_generation(
        previous.clone(), validated_generation,
    );
    bridge.set_application_layout_snapshot(previous.clone());
    discover_tx.send(()).unwrap();
    tick_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let worker = bridge.thread.take().unwrap();
    bridge.stop();
    worker.join().unwrap();
    assert!(!late_publication, "publication accepted a revoked validation token");
    assert_eq!(bridge.test_application_layout_snapshot(),
        crate::application_layouts::ApplicationLayoutSnapshot::inactive(),
        "late runtime publication revived the revoked owner snapshot");
    let inactive = crate::application_layouts::ApplicationLayoutSnapshot::inactive().packets();
    assert!(!previous.packets().iter().filter(|p| !inactive.contains(p))
        .any(|p| packets.requests().iter().any(|r| r.starts_with(p))),
        "fresh transport received old active profile packets");
    // A late cold-discovery validation cannot bind the new owner with an old
    // token either. Only its actually queried UID and current generation grant.
    let (key, current_generation) = bridge.discovered_application_layout_identity().unwrap();
    assert_eq!(key, "vial-0000000000000099");
    assert_ne!(current_generation, validated_generation);
    bridge.bind_application_layout_device(key.clone());
    assert!(bridge.application_layout_publication_generation().is_none(),
        "a registry routing label granted a cold discovered owner");
    bridge.bind_verified_application_layout_device(key.clone(), validated_generation);
    assert!(bridge.application_layout_publication_generation().is_none());
    bridge.bind_verified_application_layout_device("vial-0000000000000042".into(), current_generation);
    assert!(bridge.application_layout_publication_generation().is_none());
    bridge.bind_verified_application_layout_device(key, current_generation);
    assert_eq!(bridge.application_layout_publication_generation(), Some(current_generation));
    assert!(!bridge.has_selected_application_layout_binding());
    assert!(bridge.publish_application_layout_snapshot_for_generation(previous, current_generation));
}

#[cfg(test)]
pub(crate) fn test_open_selected_owner(
    target: &crate::device::Device,
    output: &crate::hid::SharedHidOutput,
) -> anyhow::Result<()> {
    open_host_data_hid(target, Some(output)).map(|_| ())
}

#[cfg(test)]
mod review_regressions;

#[cfg(test)]
mod sm62_timeline_tests {
    use super::*;

    #[test]
    fn progress_packet_uses_elapsed_and_total_seconds() {
        let info = MediaInfo {
            position_seconds: Some(75),
            duration_seconds: Some(230),
            ..Default::default()
        };
        assert_eq!(info.progress_payload(), [0xB1, 75, 0, 0, 0, 230, 0, 0, 0]);
        assert_eq!(
            MediaInfo::default().progress_payload(),
            [0xB1, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        let mode = HostDataMode {
            media: true,
            ..Default::default()
        };
        assert!(shutdown_payloads_for_device(mode, false, true)
            .contains(&vec![0xB1, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!shutdown_payloads_for_device(mode, false, false)
            .iter()
            .any(|packet| packet.first() == Some(&0xB1)));
    }

    #[test]
    fn playback_advances_between_samples_and_resets_on_seek_or_track_change() {
        let start = Instant::now();
        let mut clock = MediaProgressClock::default();
        assert_eq!(
            clock.update(
                "track A",
                Some(Duration::from_secs(12)),
                Some(1),
                None,
                true,
                Some(90),
                start
            ),
            Some(12)
        );
        assert_eq!(
            clock.update(
                "track A",
                Some(Duration::from_secs(12)),
                Some(1),
                None,
                true,
                Some(90),
                start + Duration::from_secs(2)
            ),
            Some(14)
        );
        assert_eq!(
            clock.update(
                "track A",
                Some(Duration::from_secs(40)),
                Some(2),
                None,
                true,
                Some(90),
                start + Duration::from_secs(3)
            ),
            Some(40)
        );
        assert_eq!(
            clock.update(
                "track A",
                Some(Duration::from_secs(40)),
                Some(2),
                None,
                false,
                Some(90),
                start + Duration::from_secs(5)
            ),
            Some(40)
        );
        assert_eq!(
            clock.update(
                "track B",
                Some(Duration::ZERO),
                Some(3),
                None,
                true,
                Some(60),
                start + Duration::from_secs(6)
            ),
            Some(0)
        );
    }

    #[test]
    fn missing_timeline_start_still_yields_duration_and_position() {
        let ticks = |seconds: i64| seconds * 10_000_000;
        assert_eq!(
            windows_timeline_seconds(None, Some(ticks(230)), None, Some(ticks(75))),
            (Some(75), Some(230))
        );
        assert_eq!(
            windows_timeline_seconds(None, Some(0), Some(ticks(230)), Some(ticks(75))),
            (Some(75), Some(230))
        );
    }
}
