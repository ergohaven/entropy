//! The bridge and the desktop lanes sleep until something is due or a wake
//! arrives, instead of passing every 20 ms. No physical HID or desktop query.
use super::*;

fn target(instance: &str) -> crate::device::Device {
    crate::device::Device {
        name: "wake recorder".into(),
        vendor_id: 0,
        product_id: 0,
        manufacturer: String::new(),
        serial_number: instance.into(),
        bus_type: "USB".into(),
        // Only satisfies the Linux disappearance check, never opened.
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/qmk_hid_host.rs")
            .to_string_lossy()
            .into_owned(),
        instance_token: instance.into(),
        firmware: crate::firmware::FirmwareProtocol::Vial,
    }
}

/// Layout from a cell the test sets; no volume, no media, and the names of
/// the threads that asked.
#[derive(Clone, Default)]
struct CellDesktop {
    layout: Arc<AtomicU8>,
    threads: Arc<Mutex<Vec<String>>>,
}

impl CellDesktop {
    fn record_thread(&self) {
        let name = thread::current().name().unwrap_or_default().to_owned();
        self.threads.lock().unwrap().push(name);
    }
}

impl DesktopSource for CellDesktop {
    fn volume(&mut self) -> Option<u8> {
        self.record_thread();
        None
    }
    fn layout(&mut self) -> LayoutObservation {
        self.record_thread();
        Some(self.layout.load(Ordering::Acquire)).filter(|layout| *layout != u8::MAX).into()
    }
    fn media(&mut self) -> Option<MediaInfo> {
        self.record_thread();
        None
    }
}

struct Started {
    bridge: QmkHidHostBridge,
    recorder: crate::hid::TestHidRecorder,
    // The bridge writes through this owner's shared output; keep it alive.
    _owner: crate::hid::HidDevice,
    opened_on: Arc<Mutex<Option<String>>>,
}

fn start_bridge(instance: &str, mode: HostDataMode, desktop: CellDesktop) -> Started {
    let (owner, recorder) = crate::hid::HidDevice::test_device();
    let opened_on = Arc::new(Mutex::new(None));
    let worker_opened_on = opened_on.clone();
    let bridge = QmkHidHostBridge::start_with_desktop_service(
        target(instance),
        mode,
        owner.shared_output(),
        HostProtocol::Selected(false),
        HostDataService::start(move || desktop.clone()),
        move |target, shared| {
            *worker_opened_on.lock().unwrap() = thread::current().name().map(str::to_owned);
            open_host_data_hid(target, shared)
        },
    );
    Started {
        bridge,
        recorder,
        _owner: owner,
        opened_on,
    }
}

fn await_report(
    recorder: &crate::hid::TestHidRecorder,
    from: usize,
    within: Duration,
    matches: impl Fn(&[u8]) -> bool,
) -> Instant {
    let deadline = Instant::now() + within;
    loop {
        if recorder.requests()[from..].iter().any(|r| matches(r)) {
            return Instant::now();
        }
        assert!(
            Instant::now() < deadline,
            "no matching report within {within:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn stop_and_join(bridge: &mut QmkHidHostBridge) -> Duration {
    let worker = bridge.thread.take().unwrap();
    let started = Instant::now();
    bridge.stop();
    worker.join().unwrap();
    started.elapsed()
}

const TIME: HostDataMode = HostDataMode {
    time: true,
    volume: false,
    layout: false,
    media: false,
};

const LAYOUT: HostDataMode = HostDataMode {
    time: false,
    volume: false,
    layout: true,
    media: false,
};

#[test]
fn desktop_lanes_sleep_until_their_next_sample() {
    let service = HostDataService::start(CellDesktop::default);
    let bridge = Arc::new(BridgeWake::default());
    let subscription = service.subscribe(
        HostDataMode {
            layout: true,
            media: true,
            ..Default::default()
        },
        &bridge,
    );
    thread::sleep(Duration::from_millis(350));
    let passes = &service.owner.shared.lane_passes;
    let layout = passes[1].load(Ordering::Relaxed);
    let media = passes[2].load(Ordering::Relaxed);
    // A 20 ms pass would have come round ~17 times in either lane.
    assert!(layout <= 6, "layout lane passed {layout} times in 350 ms");
    assert!(media <= 2, "media lane passed {media} times in 350 ms");
    assert_eq!(passes[0].load(Ordering::Relaxed), 0, "unused lane woke");
    drop(subscription);
}

#[test]
fn a_desktop_sample_wakes_only_bridges_it_has_news_for() {
    let desktop = CellDesktop::default();
    let cell = desktop.layout.clone();
    let service = HostDataService::start(move || desktop.clone());
    let layout_bridge = Arc::new(BridgeWake::default());
    let other_bridge = Arc::new(BridgeWake::default());
    let _layout = service.subscribe(LAYOUT, &layout_bridge);
    let _other = service.subscribe(TIME, &other_bridge);

    assert!(
        layout_bridge.wait(Duration::from_secs(5)),
        "first sample is news"
    );
    // Three more samples of the same layout are not.
    assert!(!layout_bridge.wait(Duration::from_millis(350)));
    cell.store(1, Ordering::Release);
    assert!(
        layout_bridge.wait(Duration::from_secs(5)),
        "a changed layout is news"
    );
    assert!(
        !other_bridge.wait(Duration::from_millis(50)),
        "a bridge without the layout lane was woken"
    );
}

#[test]
fn an_idle_bridge_waits_for_its_next_deadline() {
    let Started {
        mut bridge,
        recorder,
        _owner,
        ..
    } = start_bridge("idle", TIME, CellDesktop::default());
    await_report(&recorder, 0, Duration::from_secs(5), |r| r[0] == DATA_TIME);
    let waits = || bridge.control.wake.waits.load(Ordering::Relaxed);
    let before = waits();
    thread::sleep(Duration::from_millis(500));
    let passes = waits() - before;
    // The clock is checked once a second; a 20 ms pass would be ~25.
    assert!(passes <= 2, "idle bridge passed {passes} times in 500 ms");
    stop_and_join(&mut bridge);
}

// A layout-only bridge has nothing of its own due for ten seconds after it
// sends (the periodic resend; the idle recheck is a minute in tests). What
// reaches it within five seconds below can only have come through its wake.
fn start_idle_layout_bridge(instance: &str, desktop: CellDesktop) -> Started {
    let started = start_bridge(instance, LAYOUT, desktop);
    await_report(&started.recorder, 0, Duration::from_secs(5), |r| {
        r[..2] == [DATA_LAYOUT, 0]
    });
    started
}

#[test]
fn a_sampled_layout_reaches_the_keyboard_through_the_wake() {
    let desktop = CellDesktop::default();
    let cell = desktop.layout.clone();
    let Started {
        mut bridge,
        recorder,
        _owner,
        ..
    } = start_idle_layout_bridge("layout", desktop);
    let before = recorder.requests().len();
    cell.store(1, Ordering::Release);
    await_report(&recorder, before, Duration::from_secs(5), |r| {
        r[..2] == [DATA_LAYOUT, 1]
    });
    stop_and_join(&mut bridge);
}

#[test]
fn stop_wakes_an_idle_bridge() {
    let Started {
        mut bridge, _owner, ..
    } = start_idle_layout_bridge("stop", CellDesktop::default());
    let took = stop_and_join(&mut bridge);
    assert!(took < Duration::from_secs(5), "stop took {took:?}");
}

#[test]
fn a_handed_over_owner_wakes_an_idle_bridge() {
    let Started {
        mut bridge, _owner, ..
    } = start_idle_layout_bridge("handoff", CellDesktop::default());
    let (dedicated, dedicated_reports) = crate::hid::HidDevice::test_device();
    assert!(bridge.adopt_selected_hid(dedicated).is_ok());
    // The adopted owner gets the layout again on the bridge's next pass.
    await_report(&dedicated_reports, 0, Duration::from_secs(5), |r| {
        r[..2] == [DATA_LAYOUT, 0]
    });
    stop_and_join(&mut bridge);
}

#[test]
fn bridge_and_desktop_lanes_run_on_named_threads() {
    let desktop = CellDesktop::default();
    let threads = desktop.threads.clone();
    let Started {
        mut bridge,
        recorder,
        opened_on,
        _owner,
    } = start_bridge(
        "names",
        HostDataMode {
            volume: true,
            layout: true,
            media: true,
            ..Default::default()
        },
        desktop,
    );
    await_report(&recorder, 0, Duration::from_secs(5), |r| {
        r[0] == DATA_LAYOUT
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while threads
        .lock()
        .unwrap()
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        < 3
    {
        assert!(Instant::now() < deadline, "a lane never sampled");
        thread::sleep(Duration::from_millis(2));
    }
    let names: std::collections::BTreeSet<_> = threads.lock().unwrap().iter().cloned().collect();
    assert_eq!(
        names.into_iter().collect::<Vec<_>>(),
        ["desktop-layout", "desktop-media", "desktop-volume"]
    );
    assert_eq!(opened_on.lock().unwrap().as_deref(), Some("host-bridge"));
    stop_and_join(&mut bridge);
}

#[test]
fn bridge_wait_follows_its_nearest_deadline() {
    let now = Instant::now();
    let ago = |millis| now - Duration::from_millis(millis);
    let wait = |mode, time_poll, volume, layout| {
        bridge_idle_wait(now, mode, false, time_poll, volume, layout)
    };
    let no_volume = (None, None, ago(0));
    let no_layout = (None, None, ago(0), ago(0));

    // Application layouts change from the UI without a wake: keep the old pace.
    assert_eq!(
        bridge_idle_wait(now, TIME, true, ago(0), no_volume, no_layout),
        BRIDGE_APPLICATION_LAYOUT_TICK
    );
    // Nothing of its own due: the recheck cap.
    assert_eq!(
        wait(HostDataMode::default(), ago(0), no_volume, no_layout),
        BRIDGE_IDLE_RECHECK
    );
    // The clock is checked a second after the last check.
    assert_eq!(
        wait(TIME, ago(300), no_volume, no_layout),
        Duration::from_millis(700)
    );

    // Sampled inside the poll interval and still unsent.
    let unsent = (Some(1), Some(0), ago(30), ago(30));
    assert_eq!(
        wait(LAYOUT, ago(0), no_volume, unsent),
        LAYOUT_POLL_INTERVAL - Duration::from_millis(30)
    );
    // Sent: the resend is due ten seconds after the last full send.
    let resend_soon = (Some(1), Some(1), ago(30), ago(9_700));
    assert_eq!(
        wait(LAYOUT, ago(0), no_volume, resend_soon),
        Duration::from_millis(300)
    );
    let sent = (Some(1), Some(1), ago(30), ago(30));
    assert_eq!(
        wait(LAYOUT, ago(0), no_volume, sent),
        (LAYOUT_RESEND_INTERVAL - Duration::from_millis(30)).min(BRIDGE_IDLE_RECHECK)
    );

    let volume = HostDataMode {
        volume: true,
        ..Default::default()
    };
    assert_eq!(
        wait(volume, ago(0), (Some(5), Some(4), ago(10)), no_layout),
        VOLUME_POLL_INTERVAL - Duration::from_millis(10)
    );
    assert_eq!(
        wait(volume, ago(0), (Some(5), Some(5), ago(10)), no_layout),
        BRIDGE_IDLE_RECHECK
    );
}
