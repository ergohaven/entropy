//! Host data for Rynk boards: the time, the keyboard layout and the media on
//! the screen, posted as `SetHostData` on the board's link. A value goes when
//! it changes, and all of them when the bridge starts; the firmware keeps them
//! until it restarts, which ends the link and with it the bridge. The bridge
//! shares the desktop service and its wakes with the Vial bridges.

use std::io;
use std::sync::mpsc;

use rmk_types_rynk::protocol::rynk::ergohaven::{
    caps, HostDataUpdate, SetHostData, HOST_DATA_TEXT_MAX_LEN,
};

use super::*;
use crate::rynk::client::RynkClientError;
use crate::rynk::link::{PhysicalId, RynkLink};

/// One post slot per kind: only the latest value of a kind waits to be sent.
const SLOT_TIME: u16 = 0;
const SLOT_LAYOUT: u16 = 1;
const SLOT_ARTIST: u16 = 2;
const SLOT_TITLE: u16 = 3;

pub(crate) struct RynkHostBridge {
    mode: HostDataMode,
    link: Arc<RynkLink>,
    wake: Arc<BridgeWake>,
    stop: Arc<AtomicBool>,
    clear_on_stop: Arc<AtomicBool>,
    layout_snapshot: Arc<AtomicU8>,
    thread: Option<JoinHandle<()>>,
}

impl RynkHostBridge {
    /// The bridge for the board behind `link`, sending what `mode` asks for
    /// and the board takes.
    pub(crate) fn start(link: Arc<RynkLink>, mode: HostDataMode) -> Self {
        let desktop = HOST_DATA_SERVICE
            .get_or_init(|| HostDataService::start(NativeDesktopSource::new))
            .clone();
        Self::start_with(link, mode, desktop, current_time_payload)
    }

    fn start_with(
        link: Arc<RynkLink>,
        mode: HostDataMode,
        desktop: HostDataService,
        clock: fn() -> (u8, u8),
    ) -> Self {
        let worker = RynkBridgeWorker {
            link,
            mode,
            desktop,
            clock,
            wake: Arc::new(BridgeWake::default()),
            stop: Arc::new(AtomicBool::new(false)),
            clear_on_stop: Arc::new(AtomicBool::new(true)),
            layout_snapshot: Arc::new(AtomicU8::new(u8::MAX)),
        };
        let mut bridge = Self {
            mode,
            link: worker.link.clone(),
            wake: worker.wake.clone(),
            stop: worker.stop.clone(),
            clear_on_stop: worker.clear_on_stop.clone(),
            layout_snapshot: worker.layout_snapshot.clone(),
            thread: None,
        };
        bridge.thread = thread::Builder::new()
            .name("rynk-host-data".into())
            .spawn(move || worker.run())
            .map_err(|error| log::warn!("Rynk host-data bridge did not start: {error}"))
            .ok();
        bridge
    }

    pub(crate) fn mode(&self) -> HostDataMode {
        self.mode
    }

    pub(crate) fn link(&self) -> &Arc<RynkLink> {
        &self.link
    }

    /// Whether the bridge still runs: false once its board went away.
    pub(crate) fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }

    pub(crate) fn layout_label(&self) -> Option<&'static str> {
        layout_snapshot_label(&self.layout_snapshot)
    }

    /// Leaves the media on the screen when this bridge stops: a successor on
    /// the same link sends its own.
    pub(crate) fn suppress_shutdown(&self) {
        self.clear_on_stop.store(false, Ordering::Relaxed);
    }
}

impl Drop for RynkHostBridge {
    fn drop(&mut self) {
        // The worker posts its last values on its own; the UI never waits.
        self.stop.store(true, Ordering::Relaxed);
        self.wake.wake();
        self.thread.take();
    }
}

/// A board's link on its way, opened off the UI thread.
pub(crate) type LinkOpening = mpsc::Receiver<Result<Arc<RynkLink>, RynkClientError>>;

/// Opens the link of the board `id` names, for a bridge nobody selected. A
/// Bluetooth board that is away is not paged: the open fails NotConnected.
pub(crate) fn open_link(id: PhysicalId) -> LinkOpening {
    let (sender, opening) = mpsc::channel();
    let spawned = thread::Builder::new()
        .name("rynk-host-open".into())
        .spawn(move || {
            let opened = match crate::rynk::transport::connected(&id) {
                Ok(true) => RynkLink::open(&id),
                Ok(false) => Err(io::Error::from(io::ErrorKind::NotConnected).into()),
                Err(error) => Err(error.into()),
            };
            let _ = sender.send(opened);
        });
    if let Err(error) = spawned {
        log::warn!("Rynk host-data link was not opened: {error}");
    }
    opening
}

/// How often a board that is away is looked for: asking opens nothing.
const AWAY_RECHECK: Duration = Duration::from_secs(5);
const OPEN_RETRY_FIRST: Duration = Duration::from_secs(2);
const OPEN_RETRY_MAX: Duration = Duration::from_secs(60);

/// The link of a remembered board that has no bridge yet: an open on its way,
/// or when to try again after the last one failed.
pub(crate) struct RynkHostOpen {
    opening: Option<LinkOpening>,
    retry_at: Instant,
    failures: u32,
}

impl RynkHostOpen {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            opening: None,
            retry_at: now,
            failures: 0,
        }
    }

    /// Starts an open unless one is on its way or the last failure still
    /// holds the next one back.
    pub(crate) fn start_if_due(&mut self, now: Instant, open: impl FnOnce() -> LinkOpening) {
        if self.opening.is_none() && now >= self.retry_at {
            self.opening = Some(open());
        }
    }

    /// The link once its open finished; a failed open sets when to retry.
    pub(crate) fn poll(&mut self, now: Instant) -> Option<Arc<RynkLink>> {
        let opened = match self.opening.as_ref()?.try_recv() {
            Ok(opened) => opened,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(io::Error::other("the open stopped").into())
            }
        };
        self.opening = None;
        let error = match opened {
            Ok(link) => return Some(link),
            Err(error) => error,
        };
        let away = matches!(&error, RynkClientError::Transport(error)
            if error.kind() == io::ErrorKind::NotConnected);
        self.retry_at = now
            + if away {
                AWAY_RECHECK
            } else {
                log::warn!("Rynk host-data link was not opened: {error}");
                self.failures += 1;
                OPEN_RETRY_FIRST
                    .saturating_mul(1 << (self.failures - 1).min(5))
                    .min(OPEN_RETRY_MAX)
            };
        None
    }
}

struct RynkBridgeWorker {
    link: Arc<RynkLink>,
    mode: HostDataMode,
    desktop: HostDataService,
    clock: fn() -> (u8, u8),
    wake: Arc<BridgeWake>,
    stop: Arc<AtomicBool>,
    clear_on_stop: Arc<AtomicBool>,
    layout_snapshot: Arc<AtomicU8>,
}

impl RynkBridgeWorker {
    fn run(self) {
        let mode = board_mode(self.mode, self.link.board().info.caps);
        if mode.is_empty() {
            return;
        }
        let _subscription = self.desktop.subscribe(mode, &self.wake);
        let mut sent = Sent::default();
        while !self.stop.load(Ordering::Relaxed) {
            let snapshot = self.desktop.snapshot();
            for (slot, update) in sent.updates(mode, (self.clock)(), &snapshot) {
                if let HostDataUpdate::KeyboardLayout(layout) = update {
                    self.layout_snapshot.store(layout, Ordering::Relaxed);
                }
                if self.link.post::<SetHostData>(slot, update).is_err() {
                    return;
                }
            }
            if !self.link.is_open() {
                return;
            }
            // Layout and media news wake the bridge; the minute is checked
            // once a second, as the Vial bridges do.
            self.wake.wait(BRIDGE_IDLE_RECHECK);
        }
        if mode.media && self.clear_on_stop.load(Ordering::Relaxed) {
            // Nothing plays once Entropy stops telling the board, as over Vial.
            for (slot, update) in [
                (SLOT_ARTIST, HostDataUpdate::MediaArtist(Default::default())),
                (SLOT_TITLE, HostDataUpdate::MediaTitle(Default::default())),
            ] {
                let _ = self.link.post::<SetHostData>(slot, update);
            }
        }
    }
}

/// What of `mode` a board with `board_caps` takes. The firmware has nothing
/// to show the volume on.
fn board_mode(mode: HostDataMode, board_caps: u32) -> HostDataMode {
    let takes = |cap| board_caps & cap != 0;
    HostDataMode {
        time: mode.time && takes(caps::HOST_TIME),
        volume: false,
        layout: mode.layout && takes(caps::HOST_KEYBOARD_LAYOUT),
        media: mode.media && takes(caps::HOST_MEDIA),
    }
}

/// What the board was last sent, slot by slot.
#[derive(Default)]
struct Sent([Option<HostDataUpdate>; 4]);

impl Sent {
    /// The posts that bring the board to `time` and `snapshot`: the kinds
    /// `mode` sends whose value changed since the last ones.
    fn updates(
        &mut self,
        mode: HostDataMode,
        (hour, minute): (u8, u8),
        snapshot: &DesktopSnapshot,
    ) -> Vec<(u16, HostDataUpdate)> {
        let mut wanted = Vec::new();
        if mode.time {
            wanted.push((SLOT_TIME, HostDataUpdate::Time { hour, minute }));
        }
        if let Some(layout) = snapshot.layout.filter(|_| mode.layout) {
            wanted.push((SLOT_LAYOUT, HostDataUpdate::KeyboardLayout(layout)));
        }
        // No sample yet sends nothing; a stopped player sends empty texts.
        if let Some(media) = snapshot.media.as_ref().filter(|_| mode.media) {
            let text = |text: &str| {
                host_text(text)
                    .try_into()
                    .expect("cut to the screen's length")
            };
            wanted.push((
                SLOT_ARTIST,
                HostDataUpdate::MediaArtist(text(&media.artist)),
            ));
            wanted.push((SLOT_TITLE, HostDataUpdate::MediaTitle(text(&media.title))));
        }
        wanted
            .into_iter()
            .filter(|(slot, update)| {
                let last = &mut self.0[*slot as usize];
                let changed = last.as_ref() != Some(update);
                if changed {
                    *last = Some(update.clone());
                }
                changed
            })
            .collect()
    }
}

/// `text` cut to what the screen shows, on a character boundary so that the
/// last character shown is whole.
fn host_text(text: &str) -> &[u8] {
    let mut end = text.len().min(HOST_DATA_TEXT_MAX_LEN);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text.as_bytes()[..end]
}

#[cfg(test)]
pub(crate) mod testing {
    //! A board that takes host data, and bridges on a desktop the test drives.

    use rmk_types_rynk::protocol::rynk::ergohaven::{
        ErgohavenInfo, SetSettingRequest, SettingValue, ERGOHAVEN_RYNK_REVISION,
    };
    use rmk_types_rynk::protocol::rynk::{Cmd, RynkError, RynkHeader};

    use super::*;
    use crate::rynk::client::testing::{err, handshake, info, ok, quick, FakeBoard};
    use crate::rynk::link::{BoxedTransport, PhysicalId};

    pub(crate) const ALL_HOST_DATA: u32 =
        caps::HOST_TIME | caps::HOST_KEYBOARD_LAYOUT | caps::HOST_MEDIA;

    /// What a board was sent, update after update.
    pub(crate) type Received = Arc<Mutex<Vec<HostDataUpdate>>>;

    /// A board with `board_caps` that records host data and keeps settings.
    pub(crate) fn board(board_caps: u32) -> (FakeBoard, Received) {
        let received = Received::default();
        let log = received.clone();
        let mut settings = std::collections::BTreeMap::<u16, SettingValue>::new();
        let board = FakeBoard::new(move |header: RynkHeader, payload: &[u8]| {
            let reply = match header.cmd {
                Cmd::GetErgohavenInfo => ok(
                    header,
                    ErgohavenInfo {
                        caps: board_caps,
                        ..info(ERGOHAVEN_RYNK_REVISION)
                    },
                ),
                Cmd::SetHostData => {
                    log.lock()
                        .unwrap()
                        .push(postcard::from_bytes(payload).unwrap());
                    ok(header, ())
                }
                Cmd::SetSetting => {
                    let set: SetSettingRequest = postcard::from_bytes(payload).unwrap();
                    settings.insert(set.id, set.value);
                    ok(header, ())
                }
                Cmd::GetSetting => match settings.get(&postcard::from_bytes(payload).unwrap()) {
                    Some(value) => ok(header, value.clone()),
                    None => err(header, RynkError::Invalid),
                },
                cmd => handshake(header, 512)
                    .unwrap_or_else(|| panic!("the test board does not serve {cmd:?}")),
            };
            vec![reply]
        });
        (board, received)
    }

    pub(crate) fn link(id: &str, board: &FakeBoard) -> Arc<RynkLink> {
        let board = board.clone();
        RynkLink::acquire(PhysicalId(id.to_owned()), quick(), move || {
            Ok(Box::new(board.transport()) as BoxedTransport)
        })
        .unwrap()
    }

    /// The desktop's layout and media, as the test sets them.
    #[derive(Clone, Default)]
    pub(crate) struct ScriptDesktop(Arc<Mutex<(Option<u8>, Option<MediaInfo>)>>);

    impl DesktopSource for ScriptDesktop {
        fn volume(&mut self) -> Option<u8> {
            None
        }
        fn layout(&mut self) -> Option<u8> {
            self.0.lock().unwrap().0
        }
        fn media(&mut self) -> Option<MediaInfo> {
            self.0.lock().unwrap().1.clone()
        }
    }

    pub(crate) struct Desktop {
        script: ScriptDesktop,
        service: HostDataService,
    }

    impl Desktop {
        pub(crate) fn new() -> Self {
            let script = ScriptDesktop::default();
            let source = script.clone();
            Self {
                script,
                service: HostDataService::start(move || source.clone()),
            }
        }

        /// Shows `layout` and `media` and tells the subscribed bridges at once.
        pub(crate) fn show(&self, layout: Option<u8>, media: Option<(&str, &str)>) {
            let media = media.map(|(artist, title)| MediaInfo {
                artist: artist.into(),
                title: title.into(),
                ..Default::default()
            });
            *self.script.0.lock().unwrap() = (layout, media.clone());
            self.service.owner.shared.store_for_test(|snapshot| {
                snapshot.layout = layout;
                snapshot.media = Some(media.unwrap_or_default());
            });
        }

        /// A bridge on this desktop with the clock at noon.
        pub(crate) fn bridge(&self, link: Arc<RynkLink>, mode: HostDataMode) -> RynkHostBridge {
            RynkHostBridge::start_with(link, mode, self.service.clone(), || (12, 0))
        }
    }

    /// Waits until `received` holds `count` updates, and returns them.
    pub(crate) fn await_updates(received: &Received, count: usize) -> Vec<HostDataUpdate> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let updates = received.lock().unwrap().clone();
            if updates.len() >= count {
                return updates;
            }
            assert!(Instant::now() < deadline, "only {updates:?} arrived");
            thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use rmk_types_rynk::protocol::rynk::ergohaven::{SetSetting, SetSettingRequest, SettingValue};

    use super::testing::{await_updates, board, link, Desktop, ALL_HOST_DATA};
    use super::*;

    const EVERYTHING: HostDataMode = HostDataMode {
        time: true,
        volume: true,
        layout: true,
        media: true,
    };

    fn artist(text: &str) -> HostDataUpdate {
        HostDataUpdate::MediaArtist(text.as_bytes().try_into().unwrap())
    }

    fn title(text: &str) -> HostDataUpdate {
        HostDataUpdate::MediaTitle(text.as_bytes().try_into().unwrap())
    }

    /// The firmware's Universal Symbols read 0 as English and 1 as Russian
    /// (`HostLayout::from_host_code`): the desktop's layout index goes as it
    /// is, as over Vial, so `us,ru` is EN then RU.
    #[test]
    fn the_layout_index_is_the_code_the_firmware_reads() {
        let desktop = Desktop::new();
        desktop.show(Some(0), None);
        let (board, received) = board(ALL_HOST_DATA);
        let link = link("rynk-bridge-layout-index", &board);
        let layout_only = HostDataMode {
            time: false,
            volume: false,
            layout: true,
            media: false,
        };
        let bridge = desktop.bridge(link, layout_only);
        assert_eq!(
            await_updates(&received, 1),
            [HostDataUpdate::KeyboardLayout(0)]
        );
        assert_eq!(bridge.layout_label(), Some("EN"));

        desktop.show(Some(1), None);
        assert_eq!(
            await_updates(&received, 2)[1..],
            [HostDataUpdate::KeyboardLayout(1)]
        );
        assert_eq!(bridge.layout_label(), Some("RU"));
    }

    /// K:04 Mini over Bluetooth and Phenom over USB at once: every layout
    /// change reaches both.
    #[test]
    fn two_boards_get_every_layout_change() {
        let desktop = Desktop::new();
        desktop.show(Some(0), None);
        let layout_only = HostDataMode {
            time: false,
            volume: false,
            layout: true,
            media: false,
        };
        let (bluetooth, bluetooth_received) = board(ALL_HOST_DATA);
        let (usb, usb_received) = board(ALL_HOST_DATA);
        let usb = usb.usb();
        let _bridges = [
            desktop.bridge(link("rynk-bridge-two-bt", &bluetooth), layout_only),
            desktop.bridge(link("rynk-bridge-two-usb", &usb), layout_only),
        ];

        let expected = [0, 1, 0, 1].map(HostDataUpdate::KeyboardLayout);
        for (count, layout) in [(2, 1), (3, 0), (4, 1)] {
            await_updates(&bluetooth_received, count - 1);
            await_updates(&usb_received, count - 1);
            desktop.show(Some(layout), None);
        }
        assert_eq!(await_updates(&bluetooth_received, 4), expected);
        assert_eq!(await_updates(&usb_received, 4), expected);
    }

    #[test]
    fn the_board_gets_everything_at_start_then_only_what_changes() {
        let desktop = Desktop::new();
        desktop.show(Some(1), Some(("Artist", "Title")));
        let (board, received) = board(ALL_HOST_DATA);
        // The session holds the link too, so posts made on the way out go.
        let link = link("rynk-bridge-start", &board);
        let bridge = desktop.bridge(link.clone(), EVERYTHING);

        let start = await_updates(&received, 4);
        assert_eq!(
            start,
            [
                HostDataUpdate::Time {
                    hour: 12,
                    minute: 0
                },
                HostDataUpdate::KeyboardLayout(1),
                artist("Artist"),
                title("Title"),
            ],
            "the volume is not sent"
        );
        assert_eq!(bridge.layout_label(), Some("RU"));

        // The same values, told again and again, send nothing.
        for _ in 0..50 {
            desktop.show(Some(1), Some(("Artist", "Title")));
        }
        desktop.show(Some(0), Some(("Artist", "Title")));
        assert_eq!(
            await_updates(&received, 5)[4..],
            [HostDataUpdate::KeyboardLayout(0)]
        );
        thread::sleep(Duration::from_millis(100));
        assert_eq!(received.lock().unwrap().len(), 5);

        // Stopping clears the media, as over Vial; the time stays.
        drop(bridge);
        assert_eq!(
            await_updates(&received, 7)[5..],
            [
                HostDataUpdate::MediaArtist(Default::default()),
                HostDataUpdate::MediaTitle(Default::default()),
            ]
        );
    }

    #[test]
    fn a_board_gets_only_the_kinds_it_takes_and_the_mode_asks_for() {
        assert_eq!(
            board_mode(EVERYTHING, caps::HOST_TIME | caps::HOST_MEDIA),
            HostDataMode {
                time: true,
                media: true,
                ..Default::default()
            }
        );
        assert!(board_mode(EVERYTHING, 0).is_empty());

        let desktop = Desktop::new();
        desktop.show(Some(1), Some(("Artist", "Title")));
        let (board, received) = board(caps::HOST_TIME);
        let bridge = desktop.bridge(link("rynk-bridge-caps", &board), EVERYTHING);
        await_updates(&received, 1);
        desktop.show(Some(0), Some(("Other", "Song")));
        thread::sleep(Duration::from_millis(150));
        drop(bridge);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            *received.lock().unwrap(),
            [HostDataUpdate::Time {
                hour: 12,
                minute: 0
            }]
        );
    }

    #[test]
    fn media_text_is_cut_on_a_character_boundary() {
        let max = HOST_DATA_TEXT_MAX_LEN;
        let ascii = "a".repeat(max + 10);
        assert_eq!(host_text(&ascii).len(), max);
        for wide in ["я", "€", "😀"] {
            // The character starts inside the limit and ends past it.
            let fill = max + 1 - wide.len();
            let text = format!("{}{wide}", "a".repeat(fill));
            let cut = host_text(&text);
            assert_eq!(cut.len(), fill, "{text}");
            assert!(std::str::from_utf8(cut).is_ok());
        }
        // A character that ends exactly at the limit stays.
        assert_eq!(host_text(&"я".repeat(max / 2)).len(), max);
        assert_eq!(host_text("Song"), b"Song");
    }

    #[test]
    fn a_cyrillic_title_of_32_letters_goes_whole() {
        // Ergohaven revision 2 carries 64 bytes of media text.
        let title = "Ж".repeat(32);
        assert_eq!(host_text(&title), title.as_bytes());
        assert_eq!(host_text(&format!("{title}Ж")).len(), 64);
    }

    #[test]
    fn a_title_changing_every_20_ms_sends_no_backlog() {
        let desktop = Desktop::new();
        desktop.show(None, Some(("Artist", "0")));
        let (board, received) = board(caps::HOST_MEDIA);
        let _bridge = desktop.bridge(link("rynk-bridge-20ms", &board), EVERYTHING);
        await_updates(&received, 2);

        for change in 1..=100 {
            desktop.show(None, Some(("Artist", &change.to_string())));
            thread::sleep(Duration::from_millis(20));
        }
        let last = title("100");
        let deadline = Instant::now() + Duration::from_millis(500);
        while received.lock().unwrap().last() != Some(&last) {
            assert!(Instant::now() < deadline, "the last title did not arrive");
            thread::sleep(Duration::from_millis(2));
        }
        thread::sleep(Duration::from_millis(200));
        let updates = received.lock().unwrap().clone();
        // Nothing left over: the last value is the last post, and no value
        // went twice.
        assert_eq!(updates.last(), Some(&last));
        assert!(updates.len() <= 2 + 100, "{}", updates.len());
        assert!(updates.windows(2).all(|pair| pair[0] != pair[1]));
    }

    /// A background board's bridge holds its link alone: the media it clears
    /// on the way out still reaches the board as the link closes.
    #[test]
    fn a_bridge_holding_the_last_link_still_clears_the_media() {
        let desktop = Desktop::new();
        desktop.show(None, Some(("Artist", "Title")));
        let (board, received) = board(caps::HOST_MEDIA);
        let bridge = desktop.bridge(link("rynk-bridge-last-holder", &board), EVERYTHING);
        await_updates(&received, 2);

        drop(bridge);

        assert_eq!(await_updates(&received, 4)[2..], [artist(""), title("")]);
    }

    #[test]
    fn a_new_link_gets_the_whole_snapshot_again() {
        let desktop = Desktop::new();
        desktop.show(Some(1), Some(("Artist", "Title")));
        for attempt in 0..2 {
            let (board, received) = board(ALL_HOST_DATA);
            let id = format!("rynk-bridge-replug-{attempt}");
            let bridge = desktop.bridge(link(&id, &board), EVERYTHING);
            assert_eq!(await_updates(&received, 4).len(), 4);
            drop(bridge);
        }
    }

    #[test]
    fn the_bridge_stops_when_its_board_goes_away() {
        let desktop = Desktop::new();
        desktop.show(Some(1), None);
        let (board, received) = board(ALL_HOST_DATA);
        let unplugged = Arc::new(AtomicBool::new(false));
        let link = {
            let (board, unplugged) = (board.clone(), unplugged.clone());
            RynkLink::acquire(
                crate::rynk::link::PhysicalId("rynk-bridge-unplug".into()),
                crate::rynk::client::testing::quick(),
                move || {
                    Ok(Box::new(Unpluggable {
                        inner: board.transport(),
                        unplugged: unplugged.clone(),
                    }) as crate::rynk::link::BoxedTransport)
                },
            )
            .unwrap()
        };
        let bridge = desktop.bridge(link, EVERYTHING);
        await_updates(&received, 2);

        unplugged.store(true, Ordering::SeqCst);
        desktop.show(Some(0), None);
        let deadline = Instant::now() + Duration::from_secs(3);
        while bridge.is_running() {
            assert!(Instant::now() < deadline, "the bridge outlived its board");
            thread::sleep(Duration::from_millis(5));
        }
    }

    struct Unpluggable {
        inner: crate::rynk::client::testing::FakeTransport,
        unplugged: Arc<AtomicBool>,
    }

    impl crate::rynk::client::RynkTransport for Unpluggable {
        fn send(&mut self, frame: &[u8]) -> std::io::Result<()> {
            if self.unplugged.load(Ordering::SeqCst) {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            self.inner.send(frame)
        }

        fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> std::io::Result<usize> {
            if self.unplugged.load(Ordering::SeqCst) {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            self.inner.recv(buf, timeout)
        }
    }

    /// Settings writes share the link with host data: each reply reaches its
    /// own request, so Rynk needs no pause around them, unlike Vial.
    #[test]
    fn settings_writes_and_host_data_posts_share_a_link() {
        let (board, received) = board(ALL_HOST_DATA);
        let link = link("rynk-bridge-settings", &board);
        let poster = {
            let link = link.clone();
            thread::spawn(move || {
                for minute in 0..200u8 {
                    let update = HostDataUpdate::Time { hour: 1, minute };
                    link.post::<SetHostData>(SLOT_TIME, update).unwrap();
                    thread::sleep(Duration::from_micros(200));
                }
            })
        };
        for value in 0..50u8 {
            let id = 300 + u16::from(value % 5);
            let setting = SettingValue(
                [value, value.wrapping_mul(3)]
                    .as_slice()
                    .try_into()
                    .unwrap(),
            );
            link.request::<SetSetting>(SetSettingRequest {
                id,
                value: setting.clone(),
            })
            .unwrap();
            let read = link
                .request::<rmk_types_rynk::protocol::rynk::ergohaven::GetSetting>(id)
                .unwrap();
            assert_eq!(read, setting, "setting {id}, write {value}");
        }
        poster.join().unwrap();
        // The last time posted arrives; earlier ones may have been replaced.
        let last = HostDataUpdate::Time {
            hour: 1,
            minute: 199,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while received.lock().unwrap().last() != Some(&last) {
            assert!(Instant::now() < deadline, "the last post did not arrive");
            thread::sleep(Duration::from_millis(2));
        }
    }
}
