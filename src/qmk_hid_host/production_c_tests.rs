//! Opt-in producer/owner interop. Only the OS transport boundary is synthetic.
use super::*;
use std::io::{BufRead, Write};

struct CPeer {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    output: std::io::BufReader<std::process::ChildStdout>,
    executable: std::path::PathBuf,
    image: std::path::PathBuf,
}
impl CPeer {
    fn new(executable: std::path::PathBuf, image: std::path::PathBuf) -> Self {
        let mut child = std::process::Command::new(&executable)
            .arg(&image)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("production C backend");
        let input = child.stdin.take();
        let output = std::io::BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            executable,
            image,
        }
    }
    fn command(&mut self, command: &str) -> serde_json::Value {
        writeln!(self.input.as_mut().unwrap(), "{command}").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
        let mut line = String::new();
        assert!(
            self.output.read_line(&mut line).unwrap() > 0,
            "C backend terminated on {command}"
        );
        serde_json::from_str(&line).expect("real C response JSON")
    }
    fn packets(&mut self, payload: &[u8]) -> Vec<[u8; 32]> {
        assert!(payload.len() <= 32);
        // Match the real OS write boundary's report ID; C strips only exact 00.
        let mut report = [0u8; 33];
        report[1..1 + payload.len()].copy_from_slice(payload);
        let text = report
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        self.command(&text)["responses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let s = value.as_str().unwrap();
                assert_eq!(s.len(), 64);
                std::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            })
            .collect()
    }
    fn restart(&mut self) {
        drop(self.input.take());
        assert!(self.child.wait().unwrap().success());
        let next = Self::new(self.executable.clone(), self.image.clone());
        // The old child has exited, so replacing it cannot leave two writers.
        *self = next;
    }
}
impl Drop for CPeer {
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "requires production C firmware fixture; run explicitly with --ignored --exact"]
fn production_c_owner_syncs_100_profiles_and_restarts_with_exact_full_readback() {
    let executable = std::path::PathBuf::from(
        std::env::var_os("SM62_C_FIRMWARE").expect("explicit C fixture path required"),
    );
    let directory = tempfile::tempdir().unwrap();
    let peer = Arc::new(Mutex::new(CPeer::new(
        executable,
        directory.path().join("flash.bin"),
    )));
    let info = peer.lock().unwrap().command("INFO");
    assert_eq!(info["macro_count"], 109);
    assert_eq!(info["resident_slots"], 100);
    assert_eq!(info["cols"], 7);
    assert_eq!(info["controls"], 62);
    let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
    settings.layouts.get_mut("default").unwrap().layers = vec![vec![4; 62]; 16];
    for slot in 0..100u16 {
        let id =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: format!("interop-{slot:03}"),
                display_name: format!("Application {slot:03}"),
                ..Default::default()
            });
        let profile = settings.layouts.get_mut(&id).unwrap();
        for layer in 0..16 {
            profile.layers[layer] = (0..62)
                .map(|key| 4 + slot + layer as u16 * 62 + key)
                .collect();
            profile.layer_names[layer] = format!("L{layer:02} profile {slot:03}");
        }
    }
    let library: Vec<_> = settings
        .layouts
        .values()
        .filter(|p| p.id != "default")
        .map(|p| settings.snapshot_for_layout(p))
        .collect();
    assert_eq!(library.len(), 100);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let output = peer.clone();
    recorder.on_output(move |payload| {
        assert!(output.lock().unwrap().packets(payload).is_empty(),"write-only traffic unexpectedly produced real input reports; do not drop or synthesize echoes");
    });
    let input = peer.clone();
    recorder.respond_by(move |request| {
        let packets = input.lock().unwrap().packets(request);
        Some(if packets.len() == 1 {
            Ok(packets[0])
        } else {
            Err(format!(
                "actual C returned {} reports for request {request:02x?}",
                packets.len()
            ))
        })
    });
    // The shipping secure firmware starts locked. Authorize macro writes via
    // the public unlock dispatcher and actual matrix/timer task, not a flag.
    {
        let mut peer = peer.lock().unwrap();
        assert_eq!(peer.packets(&[0xfe, 6]).len(), 1);
        peer.command("MATRIX 1 48");
        peer.command("TICK 2999");
        let pending = peer.packets(&[0xfe, 7]);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0][0], 0);
        peer.command("TICK 1");
        let unlocked = peer.packets(&[0xfe, 7]);
        assert_eq!(unlocked.len(), 1);
        assert_eq!(&unlocked[0][..2], &[1, 0]);
        peer.command("MATRIX 1 0");
    }
    // Actual host encoders/readers against public production VIA/Vial storage.
    let size = hid.get_macro_buffer_size().unwrap();
    assert_eq!(u64::from(size), info["macro_size"].as_u64().unwrap());
    let mut macros = vec![Vec::new(); 109];
    macros[108] = vec![b'a'; usize::from(size) - 109];
    let encoded_macros = crate::hid::HidDevice::encode_macros(&macros, size).unwrap();
    hid.set_macro_buffer(&encoded_macros).unwrap();
    let read = hid.get_macro_buffer(size, 109).unwrap();
    assert_eq!(crate::hid::HidDevice::parse_macros(&read, 109), macros);
    assert_eq!(read.last(), Some(&0));
    let mut overflow = macros.clone();
    overflow[108].push(b'b');
    assert!(crate::hid::HidDevice::encode_macros(&overflow, size).is_err());
    let td = info["td"].as_u64().unwrap() as u8;
    for slot in 0..32u8 {
        hid.set_combo(slot, [4, 5, 0, 0], 6).unwrap();
        assert_eq!(hid.get_combo(slot).unwrap(), ([4, 5, 0, 0], 6));
        hid.set_key_override(slot, 4, 5, 65535, 1, 0, 1, 0).unwrap();
        assert_eq!(
            hid.get_key_override(slot).unwrap(),
            (4, 5, 65535, 1, 0, 1, 0)
        );
        hid.set_alt_repeat_key(slot, 4, 5, 0, 0).unwrap();
        assert_eq!(hid.get_alt_repeat_key(slot).unwrap(), (4, 5, 0, 0));
        if slot < td {
            hid.set_tap_dance(slot, 4, 5, 6, 7, 150).unwrap();
            assert_eq!(hid.get_tap_dance(slot).unwrap(), (4, 5, 6, 7, 150));
        }
    }
    for layer in 0..16u8 {
        for (control, &(row, col)) in crate::application_layouts::SM62_APPLICATION_POSITIONS
            .iter()
            .enumerate()
        {
            let code = 4 + u16::from(layer) * 62 + control as u16;
            hid.set_keycode(layer, row, col, code).unwrap();
            assert_eq!(hid.get_keycode(layer, row, col).unwrap(), code);
        }
    }
    let foreground = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState {
        library: Arc::new(library.clone()),
        snapshot: foreground.clone(),
        ..Default::default()
    });
    let dev = HostDataHid::Shared(hid.shared_output().unwrap());
    let mut background = resident100::Background::default();
    let mut transaction = 0;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        background.tick(
            &dev,
            &owner,
            &foreground,
            0,
            &AtomicBool::new(false),
            &mut transaction,
        );
        let state = owner.lock().unwrap();
        assert!(
            !state
                .resident_status
                .values()
                .any(|s| matches!(s, resident100::SyncStatus::Error(_))),
            "{:?}",
            state.resident_status
        );
        if library.iter().all(|s| {
            state.resident_status.get(s.profile_id.as_ref().unwrap())
                == Some(&resident100::SyncStatus::Synced)
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owner did not finish all100: {:?}",
            state.resident_status
        );
    }
    peer.lock().unwrap().restart();
    let before = peer.lock().unwrap().command("INFO");
    assert_eq!(
        crate::hid::HidDevice::parse_macros(&hid.get_macro_buffer(size, 109).unwrap(), 109),
        macros
    );
    assert_eq!(
        peer.lock().unwrap().command("PLAY 108")["output_chars"].as_u64(),
        Some((usize::from(size) - 109) as u64)
    );
    for slot in 0..32u8 {
        assert_eq!(hid.get_combo(slot).unwrap(), ([4, 5, 0, 0], 6));
        assert_eq!(
            hid.get_key_override(slot).unwrap(),
            (4, 5, 65535, 1, 0, 1, 0)
        );
        assert_eq!(hid.get_alt_repeat_key(slot).unwrap(), (4, 5, 0, 0));
        if slot < td {
            assert_eq!(hid.get_tap_dance(slot).unwrap(), (4, 5, 6, 7, 150));
        }
    }
    for layer in 0..16u8 {
        for (control, &(row, col)) in crate::application_layouts::SM62_APPLICATION_POSITIONS
            .iter()
            .enumerate()
        {
            assert_eq!(
                hid.get_keycode(layer, row, col).unwrap(),
                4 + u16::from(layer) * 62 + control as u16
            );
        }
    }
    for snapshot in &library {
        let legacy = ApplicationLayoutTransmission.packets(snapshot);
        let encoded = compact::encode(&legacy, 123).unwrap();
        let identity: [u8; 20] = legacy.last().unwrap()[11..31].try_into().unwrap();
        let crc: [u8; 4] = encoded[0][16..20].try_into().unwrap();
        let slot = snapshot.resident_slot.unwrap();
        for op in [0, 3] {
            let q = if op == 3 {
                resident100::request(op, slot, None, &[0; 20], &[0; 4])
            } else {
                resident100::request(op, slot, None, &identity, &crc)
            };
            let r = hid.usb_send(&q).unwrap();
            assert_eq!(r[3], 0);
            assert_eq!(r[2], op | 128);
        }
        let mut got = Vec::new();
        let mut offset = 0u16;
        loop {
            let mut q = [0u8; 32];
            q[..4].copy_from_slice(&[0xef, 9, 6, slot]);
            q[4..8].copy_from_slice(&456u32.to_le_bytes());
            q[8..10].copy_from_slice(&offset.to_le_bytes());
            q[10] = 20;
            let r = hid.usb_send(&q).unwrap();
            assert_eq!(r[11], 0);
            let count = usize::from(r[10]);
            assert!(count <= 20);
            got.extend_from_slice(&r[12..12 + count]);
            offset += count as u16;
            if count < 20 || got.len() == compact::LIMIT {
                break;
            }
        }
        let mut key = [0u8; 24];
        key[..20].copy_from_slice(&identity);
        key[20..].copy_from_slice(&crc);
        resident_full::validate_recovered_full(&got, &key).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&legacy[0]);
        expected.extend_from_slice(legacy.last().unwrap());
        for packet in &legacy[1..legacy.len() - 1] {
            match packet[0] {
                0xe2 => expected.extend_from_slice(&packet[5..5 + usize::from(packet[4]) * 2]),
                0xe7 => expected.extend_from_slice(&packet[5..5 + usize::from(packet[4])]),
                0xe9 => expected.extend_from_slice(&packet[4..10 + usize::from(packet[5])]),
                0xe4 => expected.extend_from_slice(&packet[3..4 + usize::from(packet[3])]),
                _ => panic!("unexpected actual production packet"),
            }
        }
        assert_eq!(
            got, expected,
            "slot{slot}: complete names/visuals/stacks must remain byte-exact"
        );
        let q = resident100::request(2, slot, None, &identity, &crc);
        assert_eq!(hid.usb_send(&q).unwrap()[3], 0);
        for layer in 0..16 {
            for control in 0..62 {
                let actual = peer
                    .lock()
                    .unwrap()
                    .command(&format!("KEY {layer} {control}"));
                assert_eq!(
                    actual["key"].as_u64().unwrap() as u16,
                    snapshot.layers[layer][control]
                );
            }
        }
    }
    let after = peer.lock().unwrap().command("INFO");
    assert_eq!(before["nor_writes"], 0);
    assert_eq!(after["nor_writes"], 0);
    assert_eq!(after["eeprom_writes"], 0);
}
