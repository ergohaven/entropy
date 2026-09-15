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
