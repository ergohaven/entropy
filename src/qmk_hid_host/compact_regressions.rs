use super::*;
use crate::application_layouts::{ApplicationLayoutSnapshot, EncoderStackAction};

#[test]
fn resident_three_capability_preserves_compact_v1_and_strict_fields() {
    let request = compact::query(0x12345678);
    let mut reply = capability(&request, 0).unwrap();
    assert!(compact::supported(&request, &reply));
    reply[9] = 3;
    assert!(compact::supported(&request, &reply), "TEST23 resident capability rejected");
    for i in 0..32 {
        reply[i] ^= 0x40;
        assert!(!compact::supported(&request, &reply), "field {i}");
        reply[i] ^= 0x40;
    }
}

fn profile() -> ApplicationLayoutSnapshot {
    let mut layout =
        crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
    layout.id = "compact-semantic".into();
    layout.layers = vec![vec![0x22; 62]; 16];
    layout.revision = 42;
    ApplicationLayoutSnapshot::from_layout(&layout)
}
fn capability(p: &[u8; 32], mode: u8) -> anyhow::Result<[u8; 32]> {
    assert_ne!(mode, 4, "compact query escaped USB SM62 scope");
    if mode == 1 {
        return Ok(*p);
    } // old firmware request echo is NOT support
    if mode == 2 {
        let mut r = *p;
        r[0] = 0xff;
        return Ok(r);
    }
    if mode == 3 {
        anyhow::bail!("modeled lost capability reply");
    }
    if let Ok(exe) = std::env::var("COMPACT_RECEIVER") {
        let request = p.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let result = std::process::Command::new(exe)
            .args(["--query", &request])
            .output()
            .unwrap();
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        return Ok(std::array::from_fn(|i| {
            u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap()
        }));
    }
    // This modeled peer implements compact v1, not resident schema2. Reject
    // the newer query correlatively before advertising its actual v1 fields.
    if p[3] == 2 { return Ok(*p); }
    let mut r = *p;
    r[2] = 0x5a;
    r[8] = 3;
    r[10..12].copy_from_slice(&4560u16.to_le_bytes());
    Ok(r)
}
fn worker(snapshot: ApplicationLayoutSnapshot, mode: u8) -> Vec<[u8; 32]> {
    worker_with_device(snapshot, mode, None)
}
fn worker_with_device(
    snapshot: ApplicationLayoutSnapshot,
    mode: u8,
    device: Option<crate::device::Device>,
) -> Vec<[u8; 32]> {
    let mut target = crate::device::test_usb_device("compact-test-no-hardware", 1, 1);
    target.vendor_id = 0xe126;
    target.product_id = 0x00b2;
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let target = device.unwrap_or(target);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(move |p| {
        if p[0] == 0xeb {
            Some(capability(p, mode).map_err(|e| e.to_string()))
        } else if p[0] == 0xe6 {
            let mut r = [0; 32];
            r[..3].copy_from_slice(&[0xe6, 8, 0x5a]);
            Some(Ok(r))
        } else {
            None
        }
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
                rx.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "never open hardware");
            open_host_data_hid(t, s)
        },
    );
    bridge.bind_selected_application_layout_device("compact-model".into());
    bridge.set_selected_application_layout_support(true);
    let synthetic_stack_fixture = snapshot.encoder_stacks.iter().any(|s| !s.is_empty());
    let generation = bridge.application_layout_publication_generation().unwrap();
    let admitted =
        bridge.publish_application_layout_snapshot_for_generation(snapshot.clone(), generation);
    if synthetic_stack_fixture {
        // SM62 editor policy intentionally refuses encoder stacks. Inject only
        // this test fixture at the worker-owned snapshot seam to qualify wire
        // capacity, without broadening ordinary profile publication policy.
        assert!(!admitted);
        let mut state = bridge.application_layout_snapshot.lock().unwrap();
        state.snapshot = snapshot;
        state.snapshot_generation = generation;
        state.published_at = Instant::now();
    } else {
        assert!(admitted);
    }
    tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ps = recorder.requests();
        if ps.iter().any(|p| matches!(p[0], 0xee | 0xe3)) {
            break;
        }
        assert!(Instant::now() < deadline, "no complete transfer");
        thread::sleep(Duration::from_millis(5));
    }
    bridge.stop();
    let all = recorder.requests();
    let start = all
        .iter()
        .position(|p| matches!(p[0], 0xec | 0xe1))
        .unwrap();
    let end = start
        + all[start..]
            .iter()
            .position(|p| matches!(p[0], 0xee | 0xe3))
            .unwrap();
    all[start..=end].to_vec()
}
fn save(
    out: &std::path::Path,
    label: &str,
    snapshot: &ApplicationLayoutSnapshot,
    wire: &[[u8; 32]],
) {
    std::fs::write(
        out.join(format!("{label}.wire")),
        wire.iter().flatten().copied().collect::<Vec<_>>(),
    )
    .unwrap();
    let mut transmission = ApplicationLayoutTransmission::default();
    let mut s = snapshot.clone();
    if !s.active {
        for l in &mut s.layers {
            l.resize(62, 0xffff);
        }
        for v in &mut s.visuals {
            v.resize(62, 0);
        }
    }
    let legacy = transmission.packets(&s);
    assert_eq!(legacy.last().unwrap()[9..11], [0x57, 2]);
    assert_eq!(
        legacy.last().unwrap()[11..31],
        wake_identity::digest(s.profile_id.as_ref().unwrap())
    );
    std::fs::write(
        out.join(format!("{label}.legacy")),
        legacy.iter().flatten().copied().collect::<Vec<_>>(),
    )
    .unwrap();
}
#[test]
fn production_worker_reduces_full_profile_after_explicit_capability() {
    let snapshot = profile();
    let wire = worker(snapshot.clone(), 0);
    if let Ok(out) = std::env::var("COMPACT_OUT") {
        save(std::path::Path::new(&out), "worker", &snapshot, &wire);
    }
    assert!(
        wire.len() < 210,
        "production worker still sends {} reports",
        wire.len()
    );
    assert_eq!(wire[0][0], 0xec);
    assert_eq!(wire.last().unwrap()[0], 0xee);
}
#[test]
fn production_worker_echo_and_explicit_unsupported_fall_back_byte_identically() {
    for mode in [1, 2] {
        let snapshot = profile();
        let expected = ApplicationLayoutTransmission::default().packets(&snapshot);
        assert_eq!(worker(snapshot, mode), expected);
    }
    let q = compact::query(42);
    let mut r = capability(&q, 0).unwrap();
    assert!(compact::supported(&q, &r));
    assert!(!compact::supported(&q, &q));
    for i in 0..32 {
        r[i] ^= 1;
        assert!(!compact::supported(&q, &r));
        r[i] ^= 1;
    }
}
#[test]
fn compact_stream_rechecks_profile_and_owner_at_every_report() {
    for revoke in [false, true] {
        let mut target = crate::device::test_usb_device("compact-owner-test", 1, 1);
        target.vendor_id = 0xe126;
        target.product_id = 0x00b2;
        target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|p| {
            if p[0] == 0xeb {
                Some(capability(p, 0).map_err(|e| e.to_string()))
            } else {
                None
            }
        });
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut held = false;
        recorder.on_output(move |p| {
            if p[0] == 0xed && !held {
                held = true;
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
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
                anyhow::ensure!(s.is_some(), "no hardware");
                open_host_data_hid(t, s)
            },
        );
        bridge.bind_selected_application_layout_device("compact-model".into());
        bridge.set_selected_application_layout_support(true);
        bridge.set_application_layout_snapshot(profile());
        tx.send(()).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if revoke {
            bridge.clear_application_layout_binding();
            bridge.bind_selected_application_layout_device("compact-model".into());
        }
        let mut successor = profile();
        successor.profile_id = Some("successor".into());
        successor.revision = 43;
        successor.layers[0][0] = 0x23;
        bridge.set_application_layout_snapshot(successor.clone());
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if recorder.requests().iter().any(|p| p[0] == 0xee) {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        bridge.stop();
        let all = recorder.requests();
        let begins: Vec<_> = all
            .iter()
            .enumerate()
            .filter(|(_, p)| p[0] == 0xec)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(begins.len(), 2);
        assert!(
            !all[begins[0]..begins[1]].iter().any(|p| p[0] == 0xee),
            "stale COMMIT reached device"
        );
        let end = begins[1] + all[begins[1]..].iter().position(|p| p[0] == 0xee).unwrap();
        if let Ok(root) = std::env::var("COMPACT_OUT") {
            let label = if revoke {
                "owner-successor"
            } else {
                "profile-successor"
            };
            save(
                std::path::Path::new(&root),
                label,
                &successor,
                &all[begins[1]..=end],
            );
        }
    }
}
#[test]
fn compact_unknown_lost_reply_sends_no_optimistic_envelope() {
    let q = compact::query(99);
    assert!(capability(&q, 3).is_err());
    let owner = Mutex::new(ApplicationLayoutOwnerState::default());
    let snapshot = profile();
    {
        let mut state = owner.lock().unwrap();
        state.snapshot = snapshot.clone();
    }
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let mut attempts = 0;
    recorder.respond_by(move |p| {
        if p[0] != 0xeb {
            return None;
        }
        attempts += 1;
        if attempts == 1 {
            Some(Err("lost reply".into()))
        } else {
            Some(capability(p, 0).map_err(|e| e.to_string()))
        }
    });
    let device = HostDataHid::Shared(hid.shared_output().unwrap());
    let mut reply = None;
    let result = write_application_layout_payload(&owner, &snapshot, 0, &q, |p| {
        if let HostDataHid::Shared(o) = &device {
            reply = o.poll_application_layout_event(p)?;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert!(reply.is_none());
    assert!(recorder.requests().iter().all(|p| p[0] == 0xeb));
    assert!(
        write_application_layout_payload(&owner, &snapshot, 0, &q, |p| {
            if let HostDataHid::Shared(o) = &device {
                reply = o.poll_application_layout_event(p)?;
            }
            Ok(())
        })
        .unwrap()
    );
    assert!(compact::supported(&q, &reply.unwrap()));
}
#[test]
fn dedicated_owner_negotiates_on_its_verified_handle() {
    let mut target = crate::device::test_usb_device("compact-dedicated", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|p| {
        if p[0] == 0xeb {
            Some(capability(p, 0).map_err(|e| e.to_string()))
        } else if p[..2] == [0xfe, 0] {
            let mut r = [0; 32];
            r[0] = 6;
            r[4..12].copy_from_slice(&0x42u64.to_le_bytes());
            Some(Ok(r))
        } else if p[0] == 0xe6 {
            let mut r = [0; 32];
            r[..3].copy_from_slice(&[0xe6, 8, 0x5a]);
            Some(Ok(r))
        } else {
            None
        }
    });
    let mut hid = Some(hid);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        None,
        HostProtocol::Selected(false),
        || None,
        move |_, _| {
            Ok(HostDataHid::Dedicated(
                hid.take().expect("one synthetic owner"),
            ))
        },
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !recorder.requests().iter().any(|p| p[..2] == [0xea,8]) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(bridge.supports_application_layouts());
    let key = bridge.discovered_application_layout_key().unwrap();
    bridge.bind_selected_application_layout_device(key);
    bridge.set_application_layout_snapshot(profile());
    while recorder.requests().iter().filter(|p| p[0] == 0xee).count() < 1 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    bridge.stop();
    let all = recorder.requests();
    assert_eq!(
        all.iter().filter(|p| p[0] == 0xeb).map(|p|p[3]).collect::<Vec<_>>(),
        [2,1],
        "connection-local capability was needlessly reprobed"
    );
    let start = all.iter().rposition(|p| p[0] == 0xec).unwrap();
    let end = start + all[start..].iter().position(|p| p[0] == 0xee).unwrap();
    if let Ok(out) = std::env::var("COMPACT_OUT") {
        save(
            std::path::Path::new(&out),
            "dedicated-worker",
            &profile(),
            &all[start..=end],
        );
    }
}
#[test]
fn compact_does_not_change_macropad_or_non_usb_sm62_wire() {
    for macropad in [false, true] {
        let mut target = crate::device::test_usb_device(
            "compact-scope",
            1,
            if macropad { 0x0042 } else { 0x00b2 },
        );
        target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        if !macropad {
            target.bus_type = "Bluetooth".into();
        }
        let mut snapshot = profile();
        if macropad {
            for l in &mut snapshot.layers {
                l.resize(15, 0x22);
            }
            for v in &mut snapshot.visuals {
                v.resize(15, 0);
            }
        }
        let expected = ApplicationLayoutTransmission::default().packets(&snapshot);
        assert_eq!(worker_with_device(snapshot, 4, Some(target)), expected);
    }
}
fn from_packets(raw: &[u8], id: String) -> ApplicationLayoutSnapshot {
    let ps: Vec<[u8; 32]> = raw
        .chunks_exact(32)
        .map(|p| p.try_into().unwrap())
        .collect();
    assert_eq!(ps.len(), 210);
    let mut s = ApplicationLayoutSnapshot::inactive();
    s.profile_id = Some(id);
    s.active = ps[0][2] != 0;
    s.revision = u32::from_le_bytes(ps[0][3..7].try_into().unwrap());
    s.name = String::from_utf8(ps[0][10..10 + usize::from(ps[0][9])].to_vec()).unwrap();
    s.layers = std::array::from_fn(|_| vec![0; 62]);
    s.visuals = std::array::from_fn(|_| vec![0; 62]);
    for p in &ps[1..209] {
        let l = usize::from(p[2]);
        let start = usize::from(p[3]);
        match p[0] {
            0xe2 => {
                for k in 0..usize::from(p[4]) {
                    s.layers[l][start + k] = u16::from_le_bytes([p[5 + 2 * k], p[6 + 2 * k]]);
                }
            }
            0xe7 => s.visuals[l][start..start + usize::from(p[4])]
                .copy_from_slice(&p[5..5 + usize::from(p[4])]),
            0xe9 => {
                if p[3] < p[4] {
                    s.encoder_stacks[l].push(EncoderStackAction {
                        name: String::from_utf8(p[10..10 + usize::from(p[5])].to_vec()).unwrap(),
                        counter_clockwise: u16::from_le_bytes([p[6], p[7]]),
                        clockwise: u16::from_le_bytes([p[8], p[9]]),
                    });
                }
            }
            0xe4 => {
                s.layer_names[l] = String::from_utf8(p[4..4 + usize::from(p[3])].to_vec()).unwrap()
            }
            _ => panic!("unexpected fixture"),
        }
    }
    assert_eq!(
        s.packets(),
        ps,
        "fixture reconstruction changes production bytes"
    );
    s
}
#[test]
#[ignore = "requires the pinned external spike fixtures and compiled production C receiver"]
fn all_spike_fixtures_real_worker_to_production_receiver() {
    let root = std::path::PathBuf::from(std::env::var("COMPACT_FIXTURES").unwrap());
    let out = std::path::PathBuf::from(std::env::var("COMPACT_OUT").unwrap());
    let exe = std::env::var("COMPACT_RECEIVER").unwrap();
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "packets"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 18);
    for p in paths {
        let label = p.file_stem().unwrap().to_str().unwrap();
        let id = std::fs::read_to_string(p.with_extension("identity")).unwrap();
        let s = from_packets(&std::fs::read(&p).unwrap(), id);
        let wire = worker(s.clone(), 0);
        save(&out, label, &s, &wire);
        let result = std::process::Command::new(&exe)
            .arg(out.join(format!("{label}.legacy")))
            .arg(out.join(format!("{label}.wire")))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if label == "synthetic-incompressible-fields" {
            assert_eq!(wire[0][3], 0, "must use compact RAW if LZ is bigger");
        }
        println!("{label}: {}", String::from_utf8_lossy(&result.stdout));
    }
    let mut maximum = profile();
    maximum.name = "🙂".repeat(5) + "Ж";
    maximum.layer_names = std::array::from_fn(|_| maximum.name.clone());
    maximum.encoder_stacks = std::array::from_fn(|_| {
        (0..4)
            .map(|_| EncoderStackAction {
                name: "🙂".repeat(3),
                counter_clockwise: 0x22,
                clockwise: 0x23,
            })
            .collect()
    });
    let wire = worker(maximum.clone(), 0);
    assert_eq!(
        u16::from_le_bytes(wire[0][10..12].try_into().unwrap()),
        4560
    );
    save(&out, "maximum-utf8", &maximum, &wire);
    let result = std::process::Command::new(&exe)
        .arg(out.join("maximum-utf8.legacy"))
        .arg(out.join("maximum-utf8.wire"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    println!("maximum-utf8: {}", String::from_utf8_lossy(&result.stdout));
    let s = ApplicationLayoutSnapshot::firmware_default();
    let wire = worker(s.clone(), 0);
    save(&out, "inactive-default", &s, &wire);
    let result = std::process::Command::new(&exe)
        .arg(out.join("inactive-default.legacy"))
        .arg(out.join("inactive-default.wire"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    println!(
        "inactive-default: {}",
        String::from_utf8_lossy(&result.stdout)
    );
}
