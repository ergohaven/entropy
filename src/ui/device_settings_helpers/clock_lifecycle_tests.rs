//! Production registry/cleanup methods and real host workers; only bridge
//! endpoint creation is injected. Inert app bootstrap never reads user data.
use super::*;
use crate::hid::{HidDevice, SharedHidOutput, TestHidRecorder};
use crate::qmk_hid_host::{test_start_bridge, HostDataMode, HostProtocol, QmkHidHostBridge};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
#[test]
fn generic_usb_serial_handoff_keeps_distinct_parents_and_excludes_composite_alias() {
    let a = crate::device::test_usb_device("3-3", 5, 0x42);
    let b = crate::device::test_usb_device("3-2", 9, 0xA1);
    let mut app = EntropyApp::new_inert_for_test();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone()]);
    let mut starts = 0;
    let mut factory = |device, mode, shared, protocol| {
        starts += 1;
        QmkHidHostBridge::test_inert(device, mode, shared, protocol)
    };
    attach(&mut app, 0, true);
    app.sync_qmk_hid_host_bridges_with(&mut factory);
    assert!(app.qmk_hid_hosts[&a.path].uses_shared_output());
    // Exact production automatic mode, without calling OS media/volume APIs.
    let mode = app.qmk_hid_hosts[&a.path].mode();
    assert!(mode.time && mode.volume && mode.media);
    loading(&mut app, 1);
    app.clear_qmk_hid_host_bridges_for_reconnect_with(&mut factory);
    assert!(
        app.qmk_hid_hosts.contains_key(&a.path),
        "old clock owner was discarded as a serial alias"
    );
    assert!(!app.qmk_hid_hosts[&a.path].uses_shared_output());
    assert_eq!(
        app.qmk_hid_hosts[&a.path].protocol(),
        HostProtocol::Discover
    );
    assert_eq!(app.qmk_hid_hosts[&a.path].mode(), mode);
    for _ in 0..3 {
        app.sync_qmk_hid_host_bridges_with(&mut factory);
        assert_eq!(app.qmk_hid_hosts.len(), 1);
    }
    app.device_manager
        .replace_devices(vec![b.clone(), a.clone()]);
    app.selected_device = Some(0);
    app.sync_qmk_hid_host_bridges_with(&mut factory);
    attach(&mut app, 0, true);
    app.sync_qmk_hid_host_bridges_with(&mut factory);
    assert_eq!(app.qmk_hid_hosts.len(), 2);
    assert!(!app.qmk_hid_hosts[&a.path].uses_shared_output());
    assert!(app.qmk_hid_hosts[&b.path].uses_shared_output());

    let mut alias = crate::device::test_usb_device("3-3", 6, 0x42);
    alias.instance_token = alias.instance_token.replace(":1.1/", ":1.2/");
    app.device_manager.replace_devices(vec![a.clone(), alias]);
    loading(&mut app, 1);
    app.clear_qmk_hid_host_bridges_for_reconnect_with(&mut factory);
    app.sync_qmk_hid_host_bridges_with(&mut factory);
    assert!(
        app.qmk_hid_hosts.is_empty(),
        "same composite device must remain exclusively owned by the connector"
    );
    assert_eq!(
        starts, 3,
        "one shared A, one dedicated A and one shared B only"
    );
}

fn device(name: &str, source: &str) -> Device {
    Device {
        name: name.into(),
        vendor_id: 0xffff,
        product_id: 0xfffe,
        manufacturer: "fixture".into(),
        serial_number: name.into(),
        bus_type: "USB".into(),
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(source)
            .to_string_lossy()
            .into_owned(),
        instance_token: format!("{name}-enumeration"),
        firmware: FirmwareProtocol::Vial,
    }
}

fn time_mode() -> HostDataMode {
    HostDataMode {
        time: true,
        ..Default::default()
    }
}
fn layout(time: bool) -> KeyboardLayout {
    let mut layout = KeyboardLayout::from_vial_json(&serde_json::json!({
        "name": "clock fixture", "matrix": {"rows": 1, "cols": 1},
        "layouts": {"keymap": [["0,0"]]}
    }))
    .unwrap();
    layout.live_features.time = time;
    layout.live_features.extended_host_protocol = time;
    layout
}

#[derive(Default)]
struct Factory {
    starts: Vec<(String, bool, HostProtocol)>,
    dedicated: Vec<(String, TestHidRecorder)>,
}
impl Factory {
    fn start(
        &mut self,
        target: Device,
        mode: HostDataMode,
        shared: Option<SharedHidOutput>,
        protocol: HostProtocol,
    ) -> QmkHidHostBridge {
        assert!(
            mode.time && !mode.volume && !mode.layout && !mode.media,
            "fixture must not invoke OS desktop services"
        );
        self.starts
            .push((target.path.clone(), shared.is_some(), protocol));
        let dedicated = if shared.is_none() {
            let (hid, recorder) = HidDevice::test_device();
            let mut reply = [0xff; 32];
            for (slot, qsid) in (357u16..=366).enumerate() {
                reply[slot * 2..slot * 2 + 2].copy_from_slice(&qsid.to_le_bytes());
            }
            recorder.respond_with([reply]);
            self.dedicated.push((target.path.clone(), recorder));
            Some(hid)
        } else {
            None
        };
        test_start_bridge(target, mode, shared, dedicated, protocol, || None)
    }
    fn sync(&mut self, app: &mut EntropyApp) {
        app.sync_qmk_hid_host_bridges_with(&mut |d, m, s, p| self.start(d, m, s, p));
    }
    fn handoff(&mut self, app: &mut EntropyApp) {
        app.clear_qmk_hid_host_bridges_for_reconnect_with(&mut |d, m, s, p| self.start(d, m, s, p));
    }
    fn recorder(&self, device: &Device) -> TestHidRecorder {
        self.dedicated
            .iter()
            .rev()
            .find(|(path, _)| path == &device.path)
            .unwrap()
            .1
            .clone()
    }
}
fn wait(recorder: &TestHidRecorder, from: usize, opcode: u8, value: Option<u8>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !recorder
        .requests()
        .iter()
        .skip(from)
        .any(|r| r[0] == opcode && value.is_none_or(|v| r[1] == v))
    {
        assert!(
            Instant::now() < deadline,
            "missing {opcode:02X} after request {from}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn no_clear(recorder: &TestHidRecorder) {
    assert!(recorder
        .requests()
        .iter()
        .all(|r| !r.starts_with(&[0xba, 0])));
}
fn attach(app: &mut EntropyApp, index: usize, time: bool) -> TestHidRecorder {
    let (hid, recorder) = HidDevice::test_device();
    app.selected_device = Some(index);
    app.layout = Some(layout(time));
    app.shared_hid_output = hid.shared_output();
    app.hid_device = Some(hid);
    app.connect_state = ConnectState::Idle;
    recorder
}
fn loading(app: &mut EntropyApp, index: usize) {
    app.selected_device = Some(index);
    app.layout = None;
    app.hid_device = None;
    app.shared_hid_output = None;
    let (_tx, rx) = std::sync::mpsc::channel();
    let now = Instant::now();
    app.connect_state = ConnectState::Loading {
        device: app.device_manager.devices()[index].clone(),
        rx,
        started_at: now,
        last_progress_at: now,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        reconnect: None,
    };
}

#[test]
fn selected_clock_bridge_adopts_exact_hid_owner_without_a_lease_gap() {
    let target = device("A", "src/qmk_hid_host.rs");
    let (hid, recorder) = HidDevice::test_device();
    let shared = hid.shared_output().unwrap();
    let mut bridge = test_start_bridge(
        target,
        time_mode(),
        Some(shared),
        None,
        HostProtocol::Selected(true),
        || None,
    );
    wait(&recorder, 0, 0xba, Some(1));

    let before = recorder.requests().len();
    bridge
        .adopt_selected_hid(hid)
        .unwrap_or_else(|_| panic!("selected HID owner was not adopted"));

    assert!(!bridge.uses_shared_output());
    assert_eq!(bridge.protocol(), HostProtocol::Discover);
    wait(&recorder, before, 0xba, Some(1));
    no_clear(&recorder);
}

#[cfg(target_os = "linux")]
#[test]
fn production_selection_moves_old_usb_owner_into_existing_background_bridge() {
    let a = crate::device::test_usb_device("3-3", 5, 0x42);
    let b = crate::device::test_usb_device("3-2", 9, 0xA1);
    let mut app = EntropyApp::new_inert_for_test();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone()]);
    attach(&mut app, 0, true);
    app.sync_qmk_hid_host_bridges_with(&mut |device, mode, shared, protocol| {
        QmkHidHostBridge::test_inert(device, mode, shared, protocol)
    });
    assert!(app.qmk_hid_hosts[&a.path].uses_shared_output());

    let (launch, requests) = std::sync::mpsc::channel();
    app.test_connect_requests = Some(launch);
    app.start_connect(1);
    let (requested, _events) = requests.recv_timeout(Duration::from_secs(1)).unwrap();

    assert_eq!(requested.path, b.path);
    assert!(app.hid_device.is_none());
    assert!(app.shared_hid_output.is_none());
    assert!(app.qmk_hid_hosts.contains_key(&a.path));
    assert!(!app.qmk_hid_hosts[&a.path].uses_shared_output());
    assert_eq!(
        app.qmk_hid_hosts[&a.path].protocol(),
        HostProtocol::Discover
    );
}

#[test]
fn production_selection_to_bluetooth_keeps_macropad_owner_in_background() {
    let mut macropad = device("M4CR0Pad v3", "src/qmk_hid_host.rs");
    macropad.vendor_id = 0xE126;
    macropad.product_id = 0x0042;
    macropad.manufacturer = "Ergohaven".into();
    macropad.serial_number = "vial:f64c2b3c".into();

    let mut bluetooth_keyboard = device("K:03 Pro", "src/hid.rs");
    bluetooth_keyboard.vendor_id = 0xE126;
    bluetooth_keyboard.product_id = 0x00A1;
    bluetooth_keyboard.manufacturer = "Ergohaven".into();
    bluetooth_keyboard.serial_number = "AA:BB:CC:DD:EE:FF".into();
    bluetooth_keyboard.bus_type = "Bluetooth".into();

    let mut app = EntropyApp::new_inert_for_test();
    app.device_manager
        .replace_devices(vec![macropad.clone(), bluetooth_keyboard.clone()]);
    attach(&mut app, 0, true);
    app.sync_qmk_hid_host_bridges_with(&mut |device, mode, shared, protocol| {
        QmkHidHostBridge::test_inert(device, mode, shared, protocol)
    });
    assert!(app.qmk_hid_hosts[&macropad.path].uses_shared_output());

    let (launch, requests) = std::sync::mpsc::channel();
    app.test_connect_requests = Some(launch);
    app.start_connect(1);
    let (requested, _events) = requests.recv_timeout(Duration::from_secs(1)).unwrap();

    assert_eq!(requested.path, bluetooth_keyboard.path);
    assert!(app.hid_device.is_none());
    assert!(app.shared_hid_output.is_none());
    assert!(app.qmk_hid_hosts.contains_key(&macropad.path));
    assert!(!app.qmk_hid_hosts[&macropad.path].uses_shared_output());
    assert_eq!(
        app.qmk_hid_hosts[&macropad.path].protocol(),
        HostProtocol::Discover
    );
}

#[test]
fn pending_new_selection_cannot_open_cancelled_loading_endpoint_in_background() {
    let mut a = device("A", "src/qmk_hid_host.rs");
    a.name = "M4CR0Pad v3".into();
    let b = device("B", "src/hid.rs");
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager.replace_devices(vec![a, b.clone()]);
    app.qmk_hid_hosts.insert(
        b.path.clone(),
        factory.start(b.clone(), time_mode(), None, HostProtocol::Discover),
    );
    let recording = factory.recorder(&b);
    wait(&recording, 0, 0xaf, None);
    loading(&mut app, 0);
    // start_connect's queued-selection path updates the index while the old
    // cancelled connector retains its endpoint until retirement completes.
    app.selected_device = Some(1);
    factory.sync(&mut app);
    assert_eq!(factory.starts.len(), 1, "do not open A or restart queued B");
    assert_eq!(app.qmk_hid_hosts.len(), 1);
    assert!(app.qmk_hid_hosts.contains_key(&b.path));
    // Timeout cleanup must use the actual loading A, not the queued B index.
    app.clear_connected_keyboard_state("Connect timeout — scripted A");
    assert!(app.qmk_hid_hosts.contains_key(&b.path));
    let before = recording.requests().len();
    wait(&recording, before, 0xba, Some(1));
    no_clear(&recording);
}

#[test]
fn bluetooth_reconnect_of_b_does_not_stop_distinct_background_a() {
    let mut a = device("A", "src/qmk_hid_host.rs");
    let mut b = device("B", "src/hid.rs");
    a.bus_type = "Bluetooth".into();
    a.serial_number = "AA:BB:CC:DD:EE:01".into();
    b.bus_type = "Bluetooth".into();
    b.serial_number = "AA:BB:CC:DD:EE:02".into();
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone()]);
    app.qmk_hid_hosts.insert(
        a.path.clone(),
        factory.start(a.clone(), time_mode(), None, HostProtocol::Discover),
    );
    let background_a = factory.recorder(&a);
    let selected_b = attach(&mut app, 1, true);
    factory.sync(&mut app);
    wait(&background_a, 0, 0xaf, None);
    wait(&selected_b, 0, 0xaf, None);
    assert!(app.begin_bluetooth_reconnect("scripted timeout"));
    assert!(app.bluetooth_reconnect_active());
    assert_eq!(app.qmk_hid_hosts.len(), 1);
    assert!(app.qmk_hid_hosts.contains_key(&a.path));
    let before = background_a.requests().len();
    wait(&background_a, before, 0xba, Some(1));
    no_clear(&background_a);
    assert_eq!(factory.starts.len(), 2);
}

#[test]
fn production_connect_and_loading_scan_keep_existing_background_owner() {
    let a = device("A", "src/qmk_hid_host.rs");
    let b = device("B", "src/hid.rs");
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone()]);
    app.qmk_hid_hosts.insert(
        a.path.clone(),
        factory.start(a.clone(), time_mode(), None, HostProtocol::Discover),
    );
    let background_a = factory.recorder(&a);
    wait(&background_a, 0, 0xaf, None);
    // Existing connector seam intercepts worker launch before any real HID or
    // definition/cache access. Production pre-open and scan callers still run.
    let (launch, requests) = std::sync::mpsc::channel();
    app.test_connect_requests = Some(launch);
    app.start_connect(1);
    let (requested, _events) = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(requested.path, b.path);
    assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
    assert_eq!(app.qmk_hid_hosts.len(), 1);
    app.apply_device_scan_result(vec![b, a.clone()]);
    assert_eq!(app.selected_device, Some(0));
    assert!(app.qmk_hid_hosts.contains_key(&a.path));
    app.clear_connected_keyboard_state("Open failed: scripted B");
    assert!(app.qmk_hid_hosts.contains_key(&a.path));
    let before = background_a.requests().len();
    wait(&background_a, before, 0xba, Some(1));
    no_clear(&background_a);
    assert_eq!(factory.starts.len(), 1);
}

#[test]
fn loading_b_preserves_a_clock_and_unrelated_c_worker_then_no_host_keyboard_b() {
    let a = device("A", "src/qmk_hid_host.rs");
    let b = device("B", "src/hid.rs");
    let c = device("C", "src/device.rs");
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone(), c.clone()]);
    let original_a = attach(&mut app, 0, true);
    app.qmk_hid_hosts.insert(
        c.path.clone(),
        factory.start(c.clone(), time_mode(), None, HostProtocol::Discover),
    );
    factory.sync(&mut app);
    let c_recording = factory.recorder(&c);
    wait(&original_a, 0, 0xaf, None);
    wait(&c_recording, 0, 0xaf, None);
    loading(&mut app, 1);
    factory.handoff(&mut app);
    let background_a = factory.recorder(&a);
    wait(&background_a, 0, 0xaf, None);
    for _ in 0..3 {
        factory.sync(&mut app);
    }
    assert_eq!(
        factory.starts.len(),
        3,
        "A shared, C dedicated, A dedicated only"
    );
    assert!(!app.qmk_hid_hosts.contains_key(&b.path));
    assert_eq!(
        app.qmk_hid_hosts[&a.path].protocol(),
        HostProtocol::Discover
    );
    let before_a = background_a.requests().len();
    let before_c = c_recording.requests().len();
    wait(&background_a, before_a, 0xba, Some(1));
    wait(&c_recording, before_c, 0xba, Some(1));
    // Vial/QMK/RMK keyboards without host metadata must not inherit A's mode.
    let b_recording = attach(&mut app, 1, false);
    factory.sync(&mut app);
    assert_eq!(factory.starts.len(), 3);
    assert!(b_recording.requests().is_empty());
    let before = background_a.requests().len();
    wait(&background_a, before, 0xba, Some(1));
    for recorder in [&original_a, &background_a, &c_recording] {
        no_clear(recorder);
    }
    assert_eq!(
        background_a
            .requests()
            .iter()
            .filter(|r| r.starts_with(&[0xfe, 0x09]))
            .count(),
        1
    );
    println!("LIFECYCLE_A_ORIGINAL={:?}", original_a.requests());
    println!("LIFECYCLE_A_BACKGROUND={:?}", background_a.requests());
    println!("LIFECYCLE_C_UNCHANGED={:?}", c_recording.requests());
}

#[test]
fn same_path_new_shared_owner_restarts_without_clear_but_disabling_clock_shuts_down() {
    let a = device("A", "src/qmk_hid_host.rs");
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager.replace_devices(vec![a.clone()]);
    let first = attach(&mut app, 0, true);
    factory.sync(&mut app);
    wait(&first, 0, 0xaf, None);
    let next = attach(&mut app, 0, true);
    factory.sync(&mut app);
    wait(&next, 0, 0xaf, None);
    assert_eq!(factory.starts.len(), 2);
    assert!(app.qmk_hid_hosts[&a.path].matches_shared_output(app.shared_hid_output.as_ref()));
    let before = next.requests().len();
    wait(&next, before, 0xba, Some(1));
    no_clear(&first);
    no_clear(&next);
    app.layout.as_mut().unwrap().live_features.time = false;
    factory.sync(&mut app);
    assert!(app.qmk_hid_hosts.is_empty());
    wait(&next, 0, 0xba, Some(0));
    assert_eq!(
        next.requests()
            .iter()
            .filter(|r| r.starts_with(&[0xba, 0]))
            .count(),
        1
    );
    assert_eq!(factory.starts.len(), 2);
}

#[test]
fn selection_releases_target_alias_and_does_not_reopen_it_while_loading() {
    let mut a = device("A", "src/qmk_hid_host.rs");
    let mut alias = device("alias", "src/hid.rs");
    alias.serial_number = a.serial_number.clone();
    // Automatic policy must not re-open an alias behind a loading connector.
    a.name = "M4CR0Pad v3".into();
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager
        .replace_devices(vec![a.clone(), alias.clone()]);
    app.qmk_hid_hosts.insert(
        a.path.clone(),
        factory.start(a.clone(), time_mode(), None, HostProtocol::Discover),
    );
    let recording = factory.recorder(&a);
    wait(&recording, 0, 0xaf, None);
    loading(&mut app, 1);
    factory.handoff(&mut app);
    assert!(app.qmk_hid_hosts.is_empty());
    factory.sync(&mut app);
    assert!(app.qmk_hid_hosts.is_empty());
    assert_eq!(factory.starts.len(), 1);
    no_clear(&recording);
}

#[test]
fn replacement_at_same_path_cannot_inherit_known_background_clock_mode() {
    let a = device("A", "src/qmk_hid_host.rs");
    let b = device("B", "src/hid.rs");
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone()]);
    app.qmk_hid_hosts.insert(
        a.path.clone(),
        factory.start(a.clone(), time_mode(), None, HostProtocol::Discover),
    );
    wait(&factory.recorder(&a), 0, 0xaf, None);
    let mut replaced = a.clone();
    replaced.instance_token = "new enumeration".into();
    app.device_manager.replace_devices(vec![replaced, b]);
    loading(&mut app, 1);
    factory.sync(&mut app);
    assert!(app.qmk_hid_hosts.is_empty());
    assert_eq!(factory.starts.len(), 1);
}

#[test]
fn failed_b_connection_and_picker_scan_keep_a_but_remove_disconnected_c() {
    let a = device("A", "src/qmk_hid_host.rs");
    let b = device("B", "src/hid.rs");
    let c = device("C", "src/device.rs");
    let mut app = EntropyApp::new_inert_for_test();
    let mut factory = Factory::default();
    app.device_manager
        .replace_devices(vec![a.clone(), b.clone(), c.clone()]);
    for d in [&a, &c] {
        app.qmk_hid_hosts.insert(
            d.path.clone(),
            factory.start(d.clone(), time_mode(), None, HostProtocol::Discover),
        );
    }
    let background_a = factory.recorder(&a);
    wait(&background_a, 0, 0xaf, None);
    let selected_b = attach(&mut app, 1, true);
    factory.sync(&mut app);
    wait(&selected_b, 0, 0xaf, None);
    app.clear_connected_keyboard_state("Open failed: B");
    assert_eq!(app.qmk_hid_hosts.len(), 2);
    assert!(!app.qmk_hid_hosts.contains_key(&b.path));
    wait(&selected_b, 0, 0xba, Some(0));
    app.selected_device = None;
    app.connect_state = ConnectState::SelectingDevice;
    app.apply_device_scan_result(vec![a.clone(), b]);
    assert_eq!(app.qmk_hid_hosts.len(), 1);
    assert!(app.qmk_hid_hosts.contains_key(&a.path));
    assert!(!app.qmk_hid_hosts.contains_key(&c.path));
    let before = background_a.requests().len();
    wait(&background_a, before, 0xba, Some(1));
    no_clear(&background_a);
    app.apply_device_scan_result(vec![]);
    assert!(app.qmk_hid_hosts.is_empty());
    assert_eq!(factory.starts.len(), 3);
}
