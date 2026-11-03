//! The layout lane samples when the desktop announces a layout change and
//! polls rarely while it does. No session bus, IBus daemon or physical HID.
use super::*;

const LAYOUT: HostDataMode = HostDataMode {
    time: false,
    volume: false,
    layout: true,
    media: false,
};

/// Records what the layout lane asked for, and keeps the events handle the
/// lane gave it so the test can play the desktop.
#[derive(Clone, Default)]
struct FollowedDesktop {
    events: Arc<Mutex<Option<LayoutEvents>>>,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl FollowedDesktop {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }

    fn events(&self) -> LayoutEvents {
        self.events.lock().unwrap().clone().unwrap()
    }

    fn await_calls(&self, count: usize) -> Vec<&'static str> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let calls = self.calls();
            if calls.len() >= count {
                return calls;
            }
            assert!(Instant::now() < deadline, "lane made only {calls:?}");
            thread::sleep(Duration::from_millis(2));
        }
    }
}

impl DesktopSource for FollowedDesktop {
    fn volume(&mut self) -> Option<u8> {
        None
    }
    fn layout(&mut self) -> Option<u8> {
        self.calls.lock().unwrap().push("layout");
        Some(0)
    }
    fn media(&mut self) -> Option<MediaInfo> {
        None
    }
    fn follow_layout(&mut self, events: LayoutEvents) {
        *self.events.lock().unwrap() = Some(events);
    }
    fn layout_list_changed(&mut self) {
        self.calls.lock().unwrap().push("relist");
    }
}

#[test]
fn a_followed_layout_is_sampled_when_announced_and_polled_otherwise() {
    let desktop = FollowedDesktop::default();
    let service = HostDataService::start({
        let desktop = desktop.clone();
        move || desktop.clone()
    });
    let bridge = Arc::new(BridgeWake::default());
    let _subscription = service.subscribe(LAYOUT, &bridge);
    desktop.await_calls(1);
    let events = desktop.events();

    assert!(events.following(true));
    let switched = desktop.calls().len();
    // Polled every 100 ms this would have sampled about five times. One poll
    // may already have been under way when following began.
    thread::sleep(Duration::from_millis(500));
    let followed = desktop.calls().len();
    assert!(
        followed - switched <= 1,
        "a followed layout was polled {} times",
        followed - switched
    );

    // Nothing else is due for ten seconds: within five, only the
    // announcements can have caused these samples.
    assert!(events.changed(false));
    assert_eq!(desktop.await_calls(followed + 1)[followed..], ["layout"]);
    assert!(events.changed(true));
    assert_eq!(
        desktop.await_calls(followed + 3)[followed + 1..],
        ["relist", "layout"]
    );

    // Signals gone: the lane polls again without being told to sample.
    assert!(events.following(false));
    desktop.await_calls(followed + 5);
}

#[test]
fn layout_events_end_with_the_service() {
    let desktop = FollowedDesktop::default();
    let service = HostDataService::start({
        let desktop = desktop.clone();
        move || desktop.clone()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while desktop.events.lock().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "the lane never handed out its events"
        );
        thread::sleep(Duration::from_millis(2));
    }
    let events = desktop.events();
    assert!(events.changed(false));
    drop(service);
    assert!(
        !events.changed(false),
        "a stopped service still took events"
    );
    assert!(!events.following(true));
}

fn p2p_pair() -> (zbus::blocking::Connection, zbus::blocking::Connection) {
    let (client, server) = std::os::unix::net::UnixStream::pair().expect("socket pair");
    let guid = zbus::Guid::generate();
    let server = thread::spawn(move || {
        zbus::blocking::connection::Builder::async_io_unix_stream(server)
            .server(guid)
            .expect("server guid")
            .p2p()
            .build()
            .expect("server connection")
    });
    let client = zbus::blocking::connection::Builder::async_io_unix_stream(client)
        .p2p()
        .build()
        .expect("client connection");
    (client, server.join().expect("server thread"))
}

fn await_state(shared: &DesktopShared, what: &str, done: impl Fn(&DesktopState) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(&shared.state.lock().unwrap()) {
        assert!(Instant::now() < deadline, "never saw {what}");
        thread::sleep(Duration::from_millis(2));
    }
}

fn clear_announcements(shared: &DesktopShared) {
    let mut state = shared.state.lock().unwrap();
    state.layout_due = false;
    state.layout_list_stale = false;
}

#[test]
fn kde_and_ibus_signals_become_layout_events_until_a_bus_goes_away() {
    let shared = Arc::new(DesktopShared::default());
    let events = LayoutEvents {
        shared: Arc::downgrade(&shared),
    };
    let (kde_client, kde) = p2p_pair();
    let (ibus_client, ibus) = p2p_pair();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        futures_lite::future::block_on(async {
            let kde =
                layout_signal_stream(kde_client.inner(), KDE_LAYOUT_INTERFACE, KDE_LAYOUT_PATH)
                    .await;
            let ibus = layout_signal_stream(ibus_client.inner(), IBUS_INTERFACE, IBUS_PATH).await;
            assert!(kde.is_some() && ibus.is_some());
            let streams = LayoutSignalStreams {
                kde,
                ibus,
                ibus_started: None,
            };
            watch_layout_signals(streams, &events).await;
        });
        finished_tx.send(()).unwrap();
    });

    // Following starts with a full re-read.
    await_state(&shared, "following", |state| {
        state.layout_following && state.layout_due && state.layout_list_stale
    });
    clear_announcements(&shared);

    kde.emit_signal(
        None::<&str>,
        KDE_LAYOUT_PATH,
        KDE_LAYOUT_INTERFACE,
        "layoutChanged",
        &(1u32,),
    )
    .unwrap();
    await_state(&shared, "a KDE switch", |state| {
        state.layout_due && !state.layout_list_stale
    });
    clear_announcements(&shared);

    kde.emit_signal(
        None::<&str>,
        KDE_LAYOUT_PATH,
        KDE_LAYOUT_INTERFACE,
        "layoutListChanged",
        &(),
    )
    .unwrap();
    await_state(&shared, "a KDE list edit", |state| {
        state.layout_due && state.layout_list_stale
    });
    clear_announcements(&shared);

    ibus.emit_signal(
        None::<&str>,
        IBUS_PATH,
        IBUS_INTERFACE,
        "GlobalEngineChanged",
        &("xkb:us::eng",),
    )
    .unwrap();
    await_state(&shared, "an IBus engine switch", |state| {
        state.layout_due && !state.layout_list_stale
    });
    clear_announcements(&shared);

    // KDE went away: the watcher ends so the lane can go back to polling.
    drop(kde);
    finished_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the watcher outlived its bus");
}

#[test]
fn an_ibus_that_runs_unfollowed_keeps_the_lane_polling() {
    let (kde_client, _kde) = p2p_pair();
    let (ibus_client, _ibus) = p2p_pair();
    let (bus_client, _bus) = p2p_pair();
    futures_lite::future::block_on(async {
        let kde =
            || layout_signal_stream(kde_client.inner(), KDE_LAYOUT_INTERFACE, KDE_LAYOUT_PATH);
        let ibus = || async {
            layout_signal_stream(ibus_client.inner(), IBUS_INTERFACE, IBUS_PATH)
                .await
                .unwrap()
        };
        let absent = || async {
            IbusSignal::Absent {
                started: layout_signal_stream(bus_client.inner(), "org.freedesktop.DBus", "/")
                    .await
                    .unwrap(),
            }
        };

        // IBus runs but its engine signal is not subscribed to: its switches
        // would go unannounced, so KDE alone is not followed.
        assert!(followed_layout_signals(kde().await, IbusSignal::Unreached, false).is_none());
        // No IBus runs: KDE announces every switch, and IBus starting is watched.
        let followed = followed_layout_signals(kde().await, absent().await, false)
            .expect("KDE alone announces every switch");
        assert!(followed.kde.is_some() && followed.ibus.is_none());
        assert!(followed.ibus_started.is_some());
        // GNOME announces through IBus only.
        let gnome = followed_layout_signals(None, IbusSignal::Subscribed(ibus().await), true)
            .expect("IBus announces GNOME's switches");
        assert!(gnome.ibus.is_some() && gnome.ibus_started.is_none());
        assert!(followed_layout_signals(None, absent().await, true).is_none());
        assert!(
            followed_layout_signals(None, IbusSignal::Subscribed(ibus().await), false).is_none()
        );
    });
}

/// Plays the session bus daemon, which knows whether a name has an owner.
/// About any other name its answer is an error.
struct NameOwners(&'static [(&'static str, bool)]);

#[zbus::interface(name = "org.freedesktop.DBus")]
impl NameOwners {
    fn name_has_owner(&self, name: &str) -> zbus::fdo::Result<bool> {
        self.0
            .iter()
            .find(|(owned, _)| *owned == name)
            .map(|(_, has_owner)| *has_owner)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("no owner known for {name}")))
    }
}

/// A session bus that knows the owners in `names`. The bus object is served
/// as the connection is built: one added after it could miss a first call.
fn session_bus(
    names: &'static [(&'static str, bool)],
) -> (zbus::blocking::Connection, zbus::blocking::Connection) {
    let (session, bus) = std::os::unix::net::UnixStream::pair().expect("socket pair");
    let guid = zbus::Guid::generate();
    let bus = thread::spawn(move || {
        zbus::blocking::connection::Builder::async_io_unix_stream(bus)
            .server(guid)
            .expect("server guid")
            .p2p()
            .serve_at("/org/freedesktop/DBus", NameOwners(names))
            .expect("bus object")
            .build()
            .expect("bus connection")
    });
    let session = zbus::blocking::connection::Builder::async_io_unix_stream(session)
        .p2p()
        .build()
        .expect("session connection");
    (session, bus.join().expect("bus thread"))
}

#[test]
fn an_ibus_address_that_fails_is_no_proof_that_ibus_is_absent() {
    let address_failed = || async {
        Err::<zbus::Connection, _>(zbus::Error::Address("ibus terminated with code: 1".into()))
    };
    futures_lite::future::block_on(async {
        // IBus owns its name, so it runs and only its address is missing.
        // It announces no start, and KDE alone would be followed for good.
        let (session, _bus) =
            session_bus(&[(KDE_LAYOUT_DESTINATION, true), (IBUS_DESTINATION, true)]);
        assert!(
            layout_signal_streams_on(session.inner(), address_failed(), false)
                .await
                .is_none()
        );
        // Whether IBus owns its name cannot be told either.
        let (session, _bus) = session_bus(&[(KDE_LAYOUT_DESTINATION, true)]);
        assert!(
            layout_signal_streams_on(session.inner(), address_failed(), false)
                .await
                .is_none()
        );
        // Its name has no owner: no IBus runs, and its start is watched.
        let (session, _bus) =
            session_bus(&[(KDE_LAYOUT_DESTINATION, true), (IBUS_DESTINATION, false)]);
        let followed = layout_signal_streams_on(session.inner(), address_failed(), false)
            .await
            .expect("KDE alone announces every switch");
        assert!(followed.kde.is_some() && followed.ibus.is_none());
        assert!(followed.ibus_started.is_some());
        // A refused connection counts as an IBus that runs, whatever the bus
        // says.
        let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let refused =
            async { Err::<zbus::Connection, _>(zbus::Error::InputOutput(Arc::new(refused))) };
        assert!(layout_signal_streams_on(session.inner(), refused, false)
            .await
            .is_none());
        // The address is back: IBus is followed along with KDE.
        let (session, _bus) =
            session_bus(&[(KDE_LAYOUT_DESTINATION, true), (IBUS_DESTINATION, true)]);
        let (ibus, _daemon) = p2p_pair();
        let reached = async { Ok::<_, zbus::Error>(ibus.inner().clone()) };
        let followed = layout_signal_streams_on(session.inner(), reached, false)
            .await
            .expect("KDE and IBus announce every switch");
        assert!(followed.kde.is_some() && followed.ibus.is_some());
        assert!(followed.ibus_started.is_none());
    });
}

#[test]
fn ibus_starting_ends_a_kde_only_watch_while_kde_stays() {
    let shared = Arc::new(DesktopShared::default());
    let events = LayoutEvents {
        shared: Arc::downgrade(&shared),
    };
    let (kde_client, kde) = p2p_pair();
    let (bus_client, bus) = p2p_pair();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        futures_lite::future::block_on(async {
            let kde =
                layout_signal_stream(kde_client.inner(), KDE_LAYOUT_INTERFACE, KDE_LAYOUT_PATH)
                    .await;
            // A peer plays the bus: its signal stands for IBus taking its name.
            let started =
                layout_signal_stream(bus_client.inner(), "org.freedesktop.DBus", "/").await;
            let streams = LayoutSignalStreams {
                kde,
                ibus: None,
                ibus_started: started,
            };
            watch_layout_signals(streams, &events).await;
        });
        finished_tx.send(()).unwrap();
    });

    await_state(&shared, "following", |state| state.layout_following);
    clear_announcements(&shared);
    kde.emit_signal(
        None::<&str>,
        KDE_LAYOUT_PATH,
        KDE_LAYOUT_INTERFACE,
        "layoutChanged",
        &(1u32,),
    )
    .unwrap();
    await_state(&shared, "a KDE switch", |state| state.layout_due);

    // IBus starts: the watch ends with KDE still there, so that the next
    // setup follows IBus's engine signal too.
    bus.emit_signal(
        None::<&str>,
        "/",
        "org.freedesktop.DBus",
        "NameOwnerChanged",
        &(IBUS_DESTINATION, "", ":1.7"),
    )
    .unwrap();
    finished_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("IBus starting did not end the watch");
    drop(kde);
}
