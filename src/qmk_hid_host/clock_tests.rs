//! Source-derived clock regression fixtures, with no app-data or physical HID.
use super::*;

fn legacy_definition() -> serde_json::Value {
    // cb48b917 keyboards/ergohaven/src/json/settings_lcd.json, included by
    // both K03Pro 4.0.5 definitions. No liveFeatures or clock layout preset.
    serde_json::json!({"settings": [{
        "name": "LCD settings",
        "fields": [
            {"type": "integer", "title": "LCD brightness", "qsid": 318,
             "min": 0, "max": 10, "width": 1},
            {"type": "select", "title": "LCD timeout", "qsid": 319,
             "variants": ["Never", "1 min", "2 min", "5 min", "10 min",
                          "15 min", "30 min", "60 min"]}
        ]
    }]})
}

#[test]
fn legacy_lcd_clock_requires_definition_and_live_settings_not_version_or_model() {
    let definition = legacy_definition();
    assert!(supports_legacy_lcd_clock(&definition, &[318, 319]));
    assert!(!supports_extended_host_protocol(&[318, 319]));
    for settings in [&[][..], &[318], &[319], &[333, 334, 356]] {
        assert!(!supports_legacy_lcd_clock(&definition, settings));
    }
    // 318/319 are unconditional in the original firmware's settings list.
    // Their presence alone says nothing about the board's display/clock.
    for definition in [
        serde_json::json!({}),
        serde_json::json!({"name": "K03Pro", "firmwareVersion": "4.0.5"}),
        serde_json::json!({"name": "M4CR0Pad v3", "firmwareVersion": "4.1.0"}),
    ] {
        assert!(!supports_legacy_lcd_clock(&definition, &[318, 319]));
    }
    for field in [0, 1] {
        let mut missing = definition.clone();
        missing["settings"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .remove(field);
        assert!(!supports_legacy_lcd_clock(&missing, &[318, 319]));
        let mut unrelated = definition.clone();
        unrelated["settings"][0]["fields"][field]["type"] = "boolean".into();
        assert!(!supports_legacy_lcd_clock(&unrelated, &[318, 319]));
    }
}

fn target(instance: &str) -> crate::device::Device {
    crate::device::Device {
        name: "clock recorder".into(),
        vendor_id: 0,
        product_id: 0,
        manufacturer: String::new(),
        serial_number: instance.into(),
        bus_type: "USB".into(),
        // Only satisfies the existing Linux disappearance check, never opened.
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/qmk_hid_host.rs")
            .to_string_lossy()
            .into_owned(),
        instance_token: instance.into(),
        firmware: crate::firmware::FirmwareProtocol::Vial,
    }
}

fn await_report(recorder: &crate::hid::TestHidRecorder, from: usize, opcode: u8) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !recorder.requests()[from..].iter().any(|r| r[0] == opcode) {
        assert!(Instant::now() < deadline, "missing opcode {opcode:02X}");
        thread::sleep(Duration::from_millis(2));
    }
}

fn stop_and_join(bridge: &mut QmkHidHostBridge) {
    // Joining is a test-only completion barrier, never production UI behavior.
    let worker = bridge.thread.take().unwrap();
    bridge.stop();
    worker.join().unwrap();
}

#[test]
fn background_clock_a_survives_start_and_stop_of_legacy_clock_b() {
    let (hid_a, a) = crate::hid::HidDevice::test_device();
    let (hid_b, b) = crate::hid::HidDevice::test_device();
    let mut bridge_a = test_start_bridge(
        target("A"),
        HostDataMode {
            time: true,
            ..Default::default()
        },
        hid_a.shared_output(),
        None,
        HostProtocol::Selected(true),
        || None,
    );
    assert!(bridge_a.device().permits_hid_target(&target("A")));
    assert!(!bridge_a.device().permits_hid_target(&target("B")));
    let mut replacement = target("A");
    replacement.instance_token = "A-replugged".into();
    assert!(!bridge_a.device().permits_hid_target(&replacement));
    await_report(&a, 0, DATA_DATE);
    let mut bridge_b = test_start_bridge(
        target("B"),
        HostDataMode {
            time: supports_legacy_lcd_clock(&legacy_definition(), &[318, 319]),
            ..Default::default()
        },
        hid_b.shared_output(),
        None,
        HostProtocol::Selected(false),
        || None,
    );
    await_report(&b, 0, DATA_TIME);
    let after_b_started = a.requests().len();
    await_report(&a, after_b_started, DATA_HOST_STATUS);
    stop_and_join(&mut bridge_b);
    let after_b_stopped = a.requests().len();
    await_report(&a, after_b_stopped, DATA_HOST_STATUS);
    assert!(a
        .requests()
        .iter()
        .filter(|r| r[0] == DATA_HOST_STATUS)
        .all(|r| r[1] == 1));
    assert!(b.requests().iter().all(|r| r[0] == DATA_TIME));
    assert!(b.requests().iter().all(|r| r[1] < 24 && r[2] < 60));
    stop_and_join(&mut bridge_a);
    assert_eq!(a.requests().last().unwrap()[..2], [DATA_HOST_STATUS, 0]);
    println!(
        "CLOCK_RECORDING_A={}",
        serde_json::to_string(&a.requests()).unwrap()
    );
    println!(
        "CLOCK_RECORDING_B_LEGACY={}",
        serde_json::to_string(&b.requests()).unwrap()
    );
}

#[test]
fn legacy_clock_capability_never_grants_extended_frames_after_owner_replacement() {
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let output = hid.shared_output().unwrap();
    let mode = HostDataMode {
        time: true,
        ..Default::default()
    };
    let old = output.for_host_bridge(mode, true);
    old.write_output_report(&[DATA_HOST_STATUS, 1]).unwrap();
    let legacy = output.for_host_bridge(mode, false);
    let before = recorder.requests().len();
    assert!(old
        .write_output_report(&[DATA_DATE, 15, 9, 234, 7])
        .is_err());
    old.write_host_shutdown(&[vec![DATA_HOST_STATUS, 0]])
        .unwrap();
    legacy.write_output_report(&[DATA_TIME, 11, 22]).unwrap();
    legacy.write_host_shutdown(&[]).unwrap();
    let reports = recorder.requests();
    assert_eq!(reports[before..].len(), 1);
    assert_eq!(&reports[before][..3], &[DATA_TIME, 11, 22]);
}

#[test]
fn real_bridge_renews_five_second_lease_while_media_query_is_blocked() {
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut bridge = test_start_bridge(
        target("stalled-media-liveness"),
        HostDataMode {
            time: true,
            media: true,
            ..Default::default()
        },
        hid.shared_output(),
        None,
        HostProtocol::Selected(true),
        move || {
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            Some(("delayed artist".into(), "delayed title".into()))
        },
    );
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Actual wall-clock > firmware hid.c's unchanged 5000 ms BA lease.
    // Record arrival times without changing the production one-second cadence.
    let started = Instant::now();
    let mut seen = recorder.requests().len();
    let mut heartbeats = Vec::new();
    while started.elapsed() < Duration::from_millis(6200) {
        let reports = recorder.requests();
        for report in &reports[seen..] {
            if report[..2] == [DATA_HOST_STATUS, 1] {
                heartbeats.push(started.elapsed());
            }
        }
        seen = reports.len();
        thread::sleep(Duration::from_millis(5));
    }
    // Always release/join before asserting, including the red baseline.
    bridge.suppress_shutdown();
    bridge.control.retire();
    release_tx.send(()).unwrap();
    stop_and_join(&mut bridge);
    println!("BA01 arrival offsets during blocked media: {heartbeats:?}");
    assert!(
        heartbeats.len() >= 5,
        "BA01 renewal stopped during media query: {heartbeats:?}"
    );
    assert!(heartbeats[0] < Duration::from_secs(2));
    assert!(heartbeats
        .windows(2)
        .all(|pair| pair[1] - pair[0] < Duration::from_secs(2)));
    assert!(started.elapsed() - *heartbeats.last().unwrap() < Duration::from_secs(2));
}

struct QueryFinished(std::sync::mpsc::Sender<()>);

impl Drop for QueryFinished {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

struct BlockedDesktopSource {
    blocked: usize,
    entered: std::sync::mpsc::Sender<usize>,
    release: std::sync::mpsc::Receiver<()>,
    calls: usize,
    _finished: QueryFinished,
}

impl BlockedDesktopSource {
    fn query(&mut self, index: usize) {
        if index == self.blocked {
            self.calls += 1;
            self.entered.send(self.calls).unwrap();
            self.release.recv().unwrap();
        }
    }
}

impl DesktopSource for BlockedDesktopSource {
    fn volume(&mut self) -> Option<u8> {
        self.query(0);
        Some(42)
    }
    fn layout(&mut self) -> Option<u8> {
        self.query(1);
        Some(1)
    }
    fn media(&mut self) -> Option<(String, String)> {
        self.query(2);
        Some((format!("artist {}", self.calls), "title".into()))
    }
}

fn blocked_service(
    index: usize,
) -> (
    HostDataService,
    std::sync::mpsc::Receiver<usize>,
    std::sync::mpsc::Sender<()>,
    std::sync::mpsc::Receiver<()>,
) {
    let (entered, received) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let (finished, done) = std::sync::mpsc::channel();
    let service = HostDataService::start(move || BlockedDesktopSource {
        blocked: index,
        entered,
        release: resume,
        calls: 0,
        _finished: QueryFinished(finished),
    });
    (service, received, release, done)
}

fn mode_for_source(index: usize) -> HostDataMode {
    HostDataMode {
        time: true,
        volume: index == 0,
        layout: index == 1,
        media: index == 2,
    }
}

#[test]
fn real_bridge_stop_and_heartbeat_do_not_wait_for_any_desktop_source() {
    for index in 0..3 {
        let (service, entered, release, done) = blocked_service(index);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut old = QmkHidHostBridge::start_with_desktop_service(
            target("blocked-desktop-stop"),
            mode_for_source(index),
            hid.shared_output(),
            HostProtocol::Selected(true),
            service,
            open_host_data_hid,
        );
        assert_eq!(entered.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        let before = recorder.requests().len();
        await_report(&recorder, before, DATA_HOST_STATUS);
        // Take a completion barrier before stop consumes the production handle.
        let worker = old.thread.take().unwrap();
        let stopped = Instant::now();
        old.stop();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let joiner = thread::spawn(move || finished_tx.send(worker.join()).unwrap());
        let exited = finished_rx.recv_timeout(Duration::from_secs(1));
        if exited.is_err() {
            // Unblock before failing so a regression cannot leak a fixture.
            release.send(()).unwrap();
            joiner.join().unwrap();
            panic!("HID worker stop waited for desktop source {index}");
        }
        exited.unwrap().unwrap();
        joiner.join().unwrap();
        assert!(stopped.elapsed() < Duration::from_secs(1));
        assert!(
            done.try_recv().is_err(),
            "query was meant to remain blocked"
        );

        let after_stop = recorder.requests().len();
        let mut successor = test_start_bridge(
            target("blocked-desktop-stop"),
            HostDataMode {
                time: true,
                ..Default::default()
            },
            hid.shared_output(),
            None,
            HostProtocol::Selected(true),
            || None,
        );
        await_report(&recorder, after_stop, DATA_DATE);
        let after_successor = recorder.requests().len();
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(5)).unwrap();
        await_report(&recorder, after_successor, DATA_HOST_STATUS);
        assert!(
            recorder.requests()[after_successor..].iter().all(|r| {
                r[..2] != [DATA_HOST_STATUS, 0]
                    && ![
                        DATA_VOLUME,
                        DATA_LAYOUT,
                        DATA_MEDIA_ARTIST,
                        DATA_MEDIA_TITLE,
                    ]
                    .contains(&r[0])
            }),
            "late source {index} wrote through a retired bridge"
        );
        stop_and_join(&mut successor);
    }
}

#[test]
fn one_desktop_sampler_survives_subscription_churn_and_discards_old_epoch() {
    let (service, entered, release, done) = blocked_service(2);
    let mode = mode_for_source(2);
    assert!(
        entered.recv_timeout(Duration::from_millis(60)).is_err(),
        "idle service polled"
    );
    let first = service.subscribe(mode);
    assert_eq!(entered.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    drop(first);
    // Reusing the same production service cannot spawn another query or worker
    // when repeated enable/reconnect arrives during an uninterruptible call.
    for _ in 0..100 {
        drop(service.subscribe(mode));
    }
    let survivor = service.subscribe(mode);
    let peer = service.subscribe(mode);
    assert_eq!(service.owner.shared.state.lock().unwrap().demand, [0, 0, 2]);
    drop(peer);
    assert_eq!(service.owner.shared.state.lock().unwrap().demand, [0, 0, 1]);
    assert!(
        entered.try_recv().is_err(),
        "parallel desktop query escaped bound"
    );
    assert!(service.snapshot().media.is_none());
    release.send(()).unwrap();
    assert_eq!(entered.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    assert!(
        service.snapshot().media.is_none(),
        "old epoch was published after re-enable"
    );
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while service.snapshot().media.is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(service.snapshot().media.unwrap().0, "artist 2");
    drop(survivor);
    assert!(service.snapshot().media.is_none());
    assert!(
        entered.recv_timeout(Duration::from_millis(60)).is_err(),
        "unsubscribed service polled"
    );
    drop(service);
    done.recv_timeout(Duration::from_secs(1)).unwrap();
}

#[test]
fn selected_to_dedicated_clock_handoff_keeps_heartbeat_during_same_stalled_query() {
    let (service, entered, release, done) = blocked_service(2);
    let mode = mode_for_source(2);
    let (selected, selected_reports) = crate::hid::HidDevice::test_device();
    let mut old = QmkHidHostBridge::start_with_desktop_service(
        target("selected-to-dedicated"),
        mode,
        selected.shared_output(),
        HostProtocol::Selected(true),
        service.clone(),
        open_host_data_hid,
    );
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    old.suppress_shutdown();
    stop_and_join(&mut old);
    assert!(selected_reports
        .requests()
        .iter()
        .all(|r| r[..2] != [DATA_HOST_STATUS, 0]));
    let (dedicated, reports) = crate::hid::HidDevice::test_device();
    let mut reply = [0xff; 32];
    for (slot, id) in (357u16..=371).enumerate() {
        reply[slot * 2..slot * 2 + 2].copy_from_slice(&id.to_le_bytes());
    }
    reports.respond_with([reply]);
    let mut dedicated = Some(dedicated);
    let mut next = QmkHidHostBridge::start_with_desktop_service(
        target("selected-to-dedicated"),
        mode,
        None,
        HostProtocol::Discover,
        service.clone(),
        move |_, shared| {
            assert!(shared.is_none());
            Ok(HostDataHid::Dedicated(
                dedicated.take().expect("no real HID fallback"),
            ))
        },
    );
    await_report(&reports, 0, DATA_DATE);
    let before = reports.requests().len();
    await_report(&reports, before, DATA_HOST_STATUS);
    assert_eq!(
        reports
            .requests()
            .iter()
            .filter(|r| r[..2] == [0xfe, 0x09])
            .count(),
        1
    );
    assert!(
        entered.try_recv().is_err(),
        "handoff spawned a second desktop query"
    );
    stop_and_join(&mut next);
    drop(service);
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
fn adopted_clock_bridge_reopens_dedicated_after_transport_loss() {
    let mode = HostDataMode {
        time: true,
        ..Default::default()
    };
    let (selected, selected_reports) = crate::hid::HidDevice::test_device();
    let (adopted, adopted_reports) = crate::hid::HidDevice::test_device();
    let (replacement, replacement_reports) = crate::hid::HidDevice::test_device();
    let mut capability = [0xff; 32];
    for (slot, id) in (357u16..=371).enumerate() {
        capability[slot * 2..slot * 2 + 2].copy_from_slice(&id.to_le_bytes());
    }
    replacement_reports.respond_with([capability]);

    let path = std::env::temp_dir().join(format!(
        "entropy-adopted-reopen-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, []).unwrap();
    let mut bridge_target = target("adopted-reopen");
    bridge_target.path = path.to_string_lossy().into_owned();

    let (opened_tx, opened_rx) = std::sync::mpsc::channel();
    let mut replacement = Some(replacement);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        bridge_target,
        mode,
        selected.shared_output(),
        HostProtocol::Selected(true),
        || None,
        move |_, shared| {
            opened_tx.send(shared.is_some()).unwrap();
            match shared {
                Some(output) => Ok(HostDataHid::Shared(output.clone())),
                None => Ok(HostDataHid::Dedicated(
                    replacement.take().expect("replacement owner exhausted"),
                )),
            }
        },
    );

    assert!(opened_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    await_report(&selected_reports, 0, DATA_DATE);
    assert!(bridge.adopt_selected_hid(adopted).is_ok());
    await_report(&adopted_reports, 0, DATA_HOST_STATUS);
    std::fs::remove_file(&path).unwrap();

    assert!(
        !opened_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "worker retried the expired selected shared owner after handoff"
    );
    stop_and_join(&mut bridge);
}
