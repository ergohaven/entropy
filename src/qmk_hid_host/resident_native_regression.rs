//! Explicit opt-in software interoperability: actual Rust owner + production C peer.
//! OS HID, MCU time and flash are substituted; this is not hardware acceptance.
use super::*;
use crate::application_layouts::{
    ApplicationLayoutSnapshot, DetectedApplication, DeviceApplicationLayouts,
};
use std::io::{BufRead, Write};

struct NativePeer {
    child: std::process::Child,
    selects: usize,
    input: std::process::ChildStdin,
    output: std::io::BufReader<std::process::ChildStdout>,
}
impl NativePeer {
    fn line(&mut self, line: &str) -> String {
        writeln!(self.input, "{line}").unwrap();
        self.input.flush().unwrap();
        let mut reply = String::new();
        assert!(
            self.output.read_line(&mut reply).unwrap() > 0,
            "native peer exited"
        );
        reply.trim().to_owned()
    }
    fn packet(&mut self, packet: &[u8]) -> Option<[u8; 32]> {
        let mut full = [0; 32];
        full[..packet.len()].copy_from_slice(packet);
        let request = full.iter().map(|v| format!("{v:02x}")).collect::<String>();
        let reply = self.line(&request);
        if reply == "-" {
            None
        } else {
            assert_eq!(reply.len(), 64);
            let reply =
                std::array::from_fn(|i| u8::from_str_radix(&reply[i * 2..i * 2 + 2], 16).unwrap());
            if full[..3] == [0xef, 8, 2] && reply[2..4] == [0x82, 1] {
                self.selects += 1;
            }
            Some(reply)
        }
    }
    fn state(&mut self) -> Vec<u32> {
        let reply = self.line("STATE");
        assert!(reply.starts_with("STATE "));
        reply
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse().unwrap())
            .collect()
    }
}
impl Drop for NativePeer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn rig(
    snapshot: ApplicationLayoutSnapshot,
    peer: Arc<Mutex<NativePeer>>,
) -> (
    QmkHidHostBridge,
    crate::hid::HidDevice,
    crate::hid::TestHidRecorder,
) {
    let mut target = crate::device::test_usb_device("native-peer-no-hardware", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let output = peer.clone();
    recorder.on_output(move |p| {
        assert!(
            output.lock().unwrap().packet(p).is_none(),
            "unexpected duplicate VIA reply"
        );
    });
    recorder.respond_by(move |p| {
        Some(
            peer.lock()
                .unwrap()
                .packet(p)
                .ok_or_else(|| "no native response".to_owned()),
        )
    });
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |t, s| {
            if let Some(rx) = gate.take() {
                rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "real HID forbidden");
            open_host_data_hid(t, s)
        },
    );
    bridge.bind_selected_application_layout_device("native-peer-model".into());
    bridge.set_selected_application_layout_support(true);
    bridge.set_application_layout_snapshot(snapshot);
    tx.send(()).unwrap();
    (bridge, hid, recorder)
}
fn wait_select(peer: &Arc<Mutex<NativePeer>>, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while peer.lock().unwrap().selects < n {
        assert!(
            Instant::now() < deadline,
            "actual C firmware did not accept selection"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
#[ignore = "requires explicitly built production native peer via SM62_NATIVE_PEER"]
fn resident_three_actual_rust_owner_and_c_firmware_roundtrip() {
    let exe = std::path::PathBuf::from(
        std::env::var_os("SM62_NATIVE_PEER").expect("build paired native peer first"),
    );
    let disk = exe
        .parent()
        .unwrap()
        .join(format!("interop-{}.flash", std::process::id()));
    assert!(!disk.exists(), "use a fresh evidence output root");
    let mut child = std::process::Command::new(&exe)
        .arg(&disk)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap();
    let peer = Arc::new(Mutex::new(NativePeer {
        selects: 0,
        input: child.stdin.take().unwrap(),
        output: std::io::BufReader::new(child.stdout.take().unwrap()),
        child,
    }));
    let mut settings = DeviceApplicationLayouts::default();
    settings.layouts.get_mut("default").unwrap().layers = vec![vec![0x22; 62]; 16];
    settings.provision_builtin_presets(None);
    let firefox = settings
        .layouts
        .values()
        .find(|p| p.executable == "firefox")
        .unwrap()
        .id
        .clone();
    let user = settings.create_for_application(&DetectedApplication {
        executable: "native-arbitrary-app".into(),
        ..Default::default()
    });
    settings
        .layouts
        .get_mut(&user)
        .unwrap()
        .set_keycode(0, 0, 0x25);
    let snapshots: Vec<_> = ["default", &firefox, &user]
        .iter()
        .map(|id| settings.snapshot_for_layout(&settings.layouts[*id]))
        .collect();
    let (mut bridge, hid, recorder) = rig(snapshots[0].clone(), peer.clone());
    for (i, snapshot) in snapshots.iter().enumerate() {
        if i > 0 {
            bridge.set_application_layout_snapshot(snapshot.clone());
        }
        wait_select(&peer, i + 1);
        let state = peer.lock().unwrap().state();
        assert_eq!(state[1], 1);
        assert_eq!(state[2], if i == 2 { 0x25 } else { 0x22 });
    }
    let writes = peer.lock().unwrap().state()[0];
    assert!(writes > 0);
    for (i, snapshot) in snapshots.iter().enumerate() {
        bridge.set_application_layout_snapshot(snapshot.clone());
        wait_select(&peer, i + 4);
        assert_eq!(
            peer.lock().unwrap().state()[0],
            writes,
            "SELECT wrote flash"
        );
    }
    assert_eq!(
        recorder.requests().iter().filter(|p| p[0] == 0xec).count(),
        3
    );
    assert_eq!(
        recorder
            .requests()
            .iter()
            .filter(|p| p[..3] == [0xef, 8, 1])
            .count(),
        3
    );
    bridge.stop();
    drop(bridge);
    drop(hid);
    drop(recorder);
    assert_eq!(peer.lock().unwrap().line("REBOOT"), "REBOOT");
    let (mut bridge, hid, recorder) = rig(snapshots[2].clone(), peer.clone());
    wait_select(&peer, 7);
    let state = peer.lock().unwrap().state();
    assert_eq!(&state[..3], &[0, 1, 0x25]);
    assert!(
        !recorder
            .requests()
            .iter()
            .any(|p| p[0] == 0xec || p[..3] == [0xef, 8, 1]),
        "reboot cache hit uploaded/stored"
    );
    bridge.stop();
    drop(bridge);
    drop(hid);
    drop(recorder);
    println!("PASS actual Rust owner + production C VIA/parser/journal: all3 slots, positive STORE/SELECT, no-write hits, fresh C process RAM after reboot with same modeled flash");
}
