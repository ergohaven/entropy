use super::*;
use crate::application_layouts::{DetectedApplication, DeviceApplicationLayouts};
#[test]
fn resident100_library_publication_admits_all_100_and_syncs_without_select() {
    let s = settings(100);
    let library: Vec<_> = s
        .layouts
        .values()
        .filter(|p| p.id != "default")
        .map(|p| s.snapshot_for_layout(p))
        .collect();
    let readbacks: std::collections::BTreeMap<_, _> = library.iter().map(|snapshot|
        (snapshot.resident_slot.unwrap(), super::resident_regressions::full_raw(&ApplicationLayoutTransmission.packets(snapshot)))).collect();
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let stored = Arc::new(Mutex::new(std::collections::BTreeMap::<u8, [u8; 24]>::new()));
    let c = stored.clone();
    let validator = Arc::new(Mutex::new(super::resident_regressions::Peer::default()));
    let v = validator.clone();
    rec.on_output(move |p| v.lock().unwrap().output(p));
    rec.respond_by(move |p| {
        if p[0] == 0xeb {
            return Some(Ok(capability(p)));
        }
        if p[..2] == [0xef, 9] {
            let mut r = *p;
            r[2] |= 128;
            let key = p[8..32].try_into().unwrap();
            let mut slots = c.lock().unwrap();
            r[3] = match p[2] {
                3 => {
                    r[8..32].fill(0);
                    if let Some(k) = slots.get(&p[3]) {
                        r[8..32].copy_from_slice(k);
                        0
                    } else {
                        1
                    }
                }
                0 => {
                    if slots.get(&p[3]) == Some(&key) {
                        0
                    } else {
                        1
                    }
                }
                1 => {
                    assert!(
                        validator.lock().unwrap().ready_matches(p),
                        "STORE must bind actual production-validated stage and token"
                    );
                    slots.insert(p[3], key);
                    0
                }
                5 => {
                    assert!(validator.lock().unwrap().ready_matches(p));
                    0
                }
                6 => {
                    let raw = &readbacks[&p[3]];
                    let offset = usize::from(u16::from_le_bytes([p[8], p[9]]));
                    let n = raw.len().saturating_sub(offset).min(20);
                    r[3] = p[3]; r[10] = n as u8; r[11] = 0; r[12..].fill(0);
                    r[12..12+n].copy_from_slice(&raw[offset..offset+n]);
                    return Some(Ok(r));
                }
                2 => panic!("background must never SELECT"),
                _ => panic!("unexpected op"),
            };
            return Some(Ok(r));
        }
        None
    });
    let mut target = crate::device::test_usb_device("resident100-library", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |t, s| {
            if let Some(g) = gate.take() {
                g.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "no hardware");
            open_host_data_hid(t, s)
        },
    );
    bridge.bind_selected_application_layout_device("resident100".into());
    bridge.set_selected_application_layout_support(true);
    let generation = bridge.application_layout_publication_generation().unwrap();
    let ids: Vec<_> = library
        .iter()
        .map(|p| p.profile_id.clone().unwrap())
        .collect();
    assert!(
        bridge.publish_resident_library_for_generation(library, generation),
        "immutable library publication must be accepted by verified owner"
    );
    tx.send(()).unwrap();
    super::resident_regressions::wait_for(&rec, |ps| ps.iter().any(|p| p[..3] == [0xef, 9, 1]));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let settings = thread::spawn(move || {
        let _pause = hid.pause_application_event_polling();
        hid.with_host_output_paused_for_settings(|| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let paused = rec.requests().len();
    thread::sleep(Duration::from_millis(40));
    assert!(
        !rec.requests()[paused..]
            .iter()
            .any(|p| matches!(p[0], 0xec..=0xef)),
        "settings/unlock lease must fence every background operation"
    );
    release_tx.send(()).unwrap();
    settings.join().unwrap();
    // Each of 100 STOREs now includes bounded FULL READs before the next job.
    let deadline = Instant::now() + Duration::from_secs(150);
    while stored.lock().unwrap().len() < 100
        || ids
            .iter()
            .any(|id| bridge.resident_sync_status(id) != Some(resident100::SyncStatus::Synced))
    {
        assert!(
            Instant::now() < deadline,
            "stored {} of 100",
            stored.lock().unwrap().len()
        );
        thread::sleep(Duration::from_millis(5));
    }
    bridge.stop();
    let ps = rec.requests();
    assert!(
        ps.iter().any(|p| p[0] == 0xe5),
        "runtime keepalive must continue during 100-record background sync"
    );
    assert_eq!(
        stored.lock().unwrap().keys().copied().collect::<Vec<_>>(),
        (0..100).collect::<Vec<_>>()
    );
    assert!(!ps
        .iter()
        .any(|p| p[..3] == [0xec, 8, 1] || p[..3] == [0xef, 9, 2]));
}

pub(super) fn capability(p: &[u8; 32]) -> [u8; 32] {
    let mut r = *p;
    r[2] = 0x5a;
    if p[3] == 2 {
        r[8..15].copy_from_slice(&[2, 100, 0xd0, 0x11, 16, 62, 22]);
        r[16..20].copy_from_slice(&4194304u32.to_le_bytes());
        r[20..24].copy_from_slice(&15u32.to_le_bytes());
        r[24] = 2;
    } else {
        r[8] = 3;
        r[9] = 0;
        r[10..12].copy_from_slice(&4560u16.to_le_bytes());
    }
    r
}
#[test]
fn resident100_background_delete_slot0_confirms_absence_without_select() {
    let s = settings(1);
    let mut snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let legacy = ApplicationLayoutTransmission.packets(&snap);
    let raw=super::resident_regressions::full_raw(&legacy);
    let crc=compact::crc32(&raw).to_le_bytes();
    let read_ticks=raw.len().div_ceil(20)+112;
    let id: [u8; 20] = legacy.last().unwrap()[11..31].try_into().unwrap();
    let mut key=[0;24];key[..20].copy_from_slice(&id);key[20..].copy_from_slice(&crc);validate_recovered_full(&raw,&key).unwrap();
    snap.active = false;
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let exists = Arc::new(AtomicBool::new(true));
    let e = exists.clone();
    rec.respond_by(move |p| {
        if p[..2] != [0xef, 9] {
            return None;
        }
        let mut r = *p;
        r[2] |= 128;
        r[3] = 0;
        match p[2] {
            6 => {
                let offset=usize::from(u16::from_le_bytes([p[8],p[9]]));let n=raw.len().saturating_sub(offset).min(20);
                r[3]=p[3];r[10]=n as u8;r[11]=0;r[12..].fill(0);if n>0 {r[12..12+n].copy_from_slice(&raw[offset..offset+n]);}
            }
            3 => {
                r[8..32].fill(0);
                if p[3] == 0 && e.load(Ordering::Acquire) {
                    r[8..28].copy_from_slice(&id);
                    r[28..32].copy_from_slice(&crc);
                } else {
                    r[3] = 1;
                }
            }
            4 => {
                assert_eq!(p[3], 0);
                assert_eq!(p[8..28], id);
                assert_eq!(p[28..32], crc);
                e.store(false, Ordering::Release);
            }
            _ => panic!("delete must ENUM/DELETE/ENUM only"),
        };
        Some(Ok(r))
    });
    let fg = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState {
        library: Arc::new(vec![snap.clone()]),
        snapshot: fg.clone(),
        ..Default::default()
    });
    let mut bg = resident100::Background::default();
    let dev = HostDataHid::Shared(hid.shared_output().unwrap());
    let mut t = 0;
    for _ in 0..read_ticks {
        bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    }
    assert!(
        !exists.load(Ordering::Acquire),
        "persisted deletion intent must commit a slot0 tombstone"
    );
    assert_eq!(
        owner
            .lock()
            .unwrap()
            .resident_status
            .get(snap.profile_id.as_ref().unwrap()),
        Some(&resident100::SyncStatus::Synced)
    );
}

#[test]
fn resident100_enumeration_bad_echo_is_bounded_visible_error() {
    let s = settings(1);
    let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let id = snap.profile_id.clone().unwrap();
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(|p| Some(Ok(*p)));
    let fg = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState {
        library: Arc::new(vec![snap]),
        snapshot: fg.clone(),
        ..Default::default()
    });
    let mut bg = resident100::Background::default();
    let dev = HostDataHid::Shared(hid.shared_output().unwrap());
    let mut t = 0;
    for _ in 0..20 {
        bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    }
    assert!(
        matches!(
            owner.lock().unwrap().resident_status.get(&id),
            Some(resident100::SyncStatus::Error(_))
        ),
        "invalid ENUM must terminate visibly, not stay pending forever"
    );
    assert!(rec.requests().len() <= 3);
}
#[test]
fn resident100_background_uncertain_store_ack_queries_before_any_retry() {
    let s = settings(1);
    let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let id = snap.profile_id.clone().unwrap();
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let cached = Arc::new(Mutex::new(None::<[u8; 24]>));
    let c = cached.clone();
    rec.respond_by(move |p| {
        let mut r = *p;
        r[2] |= 128;
        r[3] = 0;
        let key = p[8..32].try_into().unwrap();
        match p[2] {
            3 => {
                r[8..].fill(0);
                r[3] = 1;
            }
            0 => {
                if *c.lock().unwrap() != Some(key) {
                    r[3] = 1;
                }
            }
            5 => {}
            1 => {
                *c.lock().unwrap() = Some(key);
                return Some(Err("uncertain delivered STORE ACK".into()));
            }
            6 => return Some(Err("readback unavailable after uncertain STORE".into())),
            _ => panic!("unexpected operation"),
        };
        Some(Ok(r))
    });
    let fg = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState {
        library: Arc::new(vec![snap]),
        snapshot: fg.clone(),
        ..Default::default()
    });
    let mut bg = resident100::Background::default();
    let dev = HostDataHid::Shared(hid.shared_output().unwrap());
    let mut t = 0;
    for _ in 0..120 {
        bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    }
    assert_eq!(
        owner.lock().unwrap().resident_status.get(&id),
        Some(&resident100::SyncStatus::Synced),
        "lost durable STORE ACK must reconcile QUERY"
    );
    let ps = rec.requests();
    assert_eq!(ps.iter().filter(|p| p[..3] == [0xef, 9, 1]).count(), 1);
    let store = ps.iter().position(|p| p[..3] == [0xef, 9, 1]).unwrap();
    assert_eq!(ps[store + 1][..3], [0xef, 9, 0]);
}

#[test]
fn resident100_wire_allowlists_are_exact_readonly_and_reject_bad_padding() {
    for op in 0..=6 {
        let mut p = resident100::request(op, 99, None, &[0; 20], &[0; 4]);
        if op == 6 {
            p[10] = 20;
        }
        assert_eq!(
            crate::hid::hid_protocol::is_read_request(&p),
            matches!(op, 0 | 3 | 5 | 6),
            "EF09 op {op}"
        );
        assert!(crate::hid::is_resident09_request(&p));
        let mut bad = p;
        bad[3] = 100;
        assert!(!crate::hid::is_resident09_request(&bad));
        if matches!(op, 3 | 6) {
            bad = p;
            bad[31] = 1;
            assert!(!crate::hid::hid_protocol::is_read_request(&bad));
            assert!(!crate::hid::is_resident09_request(&bad));
        }
    }
    for version in [1, 2] {
        let mut p = compact::query(42);
        p[3] = version;
        assert!(crate::hid::hid_protocol::is_read_request(&p));
        p[31] = 1;
        assert!(!crate::hid::hid_protocol::is_read_request(&p));
    }
    let p = resident100::capability_query(43);
    let reply = capability(&p);
    assert!(resident100::supported(&p, &reply));
    for i in 0..32 {
        let mut bad = reply;
        bad[i] ^= 1;
        assert!(!resident100::supported(&p, &bad), "cap byte {i}");
    }
}

fn raw_from_transfer(packets: &[[u8; 32]]) -> Vec<u8> {
    let b = packets[0];
    let len = u16::from_le_bytes(b[8..10].try_into().unwrap()) as usize;
    let mut data: Vec<_> = packets[1..packets.len() - 1]
        .iter()
        .flat_map(|p| p[8..].iter().copied())
        .take(len)
        .collect();
    if b[3] == 0 {
        return data;
    }
    let mut raw = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        i += 1;
        if tag < 128 {
            let n = tag as usize + 1;
            raw.extend_from_slice(&data[i..i + n]);
            i += n;
        } else {
            let n = (tag & 127) as usize + 3;
            let d = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            i += 2;
            for _ in 0..n {
                raw.push(raw[raw.len() - d]);
            }
        }
    }
    data.clear();
    raw
}
#[test]
fn resident100_unknown_record_read_recovery_preserves_payload_and_never_evicts() {
    let s = settings(1);
    let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let legacy = ApplicationLayoutTransmission.packets(&snap);
    let mut raw = raw_from_transfer(&compact::encode(&legacy, 123).unwrap());
    raw[43..63].fill(7);
    let mut unknown = [7; 24];
    unknown[20..].copy_from_slice(&compact::crc32(&raw).to_le_bytes());
    let expected = raw.clone();
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(move |p| {
        assert_eq!(p[..2], [0xef, 9]);
        let mut r = *p;
        r[2] |= 128;
        match p[2] {
            3 => {
                r[3] = if p[3] == 0 { 0 } else { 1 };
                r[8..32].fill(0);
                if p[3] == 0 {
                    r[8..32].copy_from_slice(&unknown);
                }
            }
            6 => {
                let off = u16::from_le_bytes(p[8..10].try_into().unwrap()) as usize;
                assert!(off <= raw.len());
                let n = (raw.len() - off).min(p[10] as usize);
                r[10] = n as u8;
                r[11] = 0;
                r[12..].fill(0);
                r[12..12 + n].copy_from_slice(&raw[off..off + n]);
            }
            _ => panic!("unknown record must never be overwritten or deleted"),
        };
        Some(Ok(r))
    });
    let fg = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState {
        library: Arc::new(vec![snap]),
        snapshot: fg.clone(),
        ..Default::default()
    });
    let mut bg = resident100::Background::default();
    let dev = HostDataHid::Shared(hid.shared_output().unwrap());
    let mut t = 0;
    for _ in 0..400 {
        bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    }
    assert!(
        rec.requests().iter().any(|p| p[..3] == [0xef, 9, 6]),
        "unknown identities must be READ for recoverable raw preservation"
    );
    assert!(!expected.is_empty());
    let state = owner.lock().unwrap();
    let recovered = state
        .resident_recoveries
        .get(&0)
        .expect("recovered metadata");
    assert_eq!(recovered.raw, expected);
    assert_eq!(
        recovered.validation, None,
        "raw FULL must validate identity, geometry and both CRCs"
    );
}

#[test]
fn resident100_publication_rejects_malformed_incomplete_and_duplicate_slots() {
    let mut target = crate::device::test_usb_device("publication", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let mut b = QmkHidHostBridge::test_inert(
        target,
        Default::default(),
        None,
        HostProtocol::Selected(false),
    );
    b.bind_selected_application_layout_device("test".into());
    let g = b.application_layout_publication_generation().unwrap();
    let s = settings(2);
    let good: Vec<_> = s
        .layouts
        .values()
        .filter(|p| p.id != "default")
        .map(|p| s.snapshot_for_layout(p))
        .collect();
    for mode in 0..4 {
        let mut invalid = good.clone();
        match mode {
            0 => invalid[0].layers[15].pop(),
            1 => {
                invalid[0].layers[15][61] = 0xffff;
                None
            }
            2 => {
                invalid[1].resident_slot = invalid[0].resident_slot;
                None
            }
            _ => {
                invalid[0].profile_id = Some("default".into());
                None
            }
        };
        assert!(
            !b.publish_resident_library_for_generation(invalid, g),
            "malformed library mode {mode} must fail closed"
        );
    }
    assert!(b.publish_resident_library_for_generation(good, g));
    b.clear_application_layout_binding();
    assert!(!b.publish_resident_library_for_generation(vec![], g));
}

#[test]
fn resident100_old_firmware_streams_and_reports_not_resident_synced() {
    let s = settings(1);
    let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let id = snap.profile_id.clone().unwrap();
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(|p| {
        if p[0] == 0xeb {
            let mut r = *p;
            if p[3] == 1 {
                r[2] = 0x5a;
                r[8] = 3;
                r[9] = 3;
                r[10..12].copy_from_slice(&4560u16.to_le_bytes());
            }
            return Some(Ok(r));
        }
        None
    });
    let mut target = crate::device::test_usb_device("old-fw", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut b = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |t, s| {
            if let Some(g) = gate.take() {
                g.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "no hardware");
            open_host_data_hid(t, s)
        },
    );
    b.bind_selected_application_layout_device("old".into());
    b.set_selected_application_layout_support(true);
    b.set_application_layout_snapshot(snap.clone());
    assert!(b.publish_resident_library_for_generation(
        vec![snap],
        b.application_layout_publication_generation().unwrap()
    ));
    tx.send(()).unwrap();
    super::resident_regressions::wait_for(&rec, |ps| ps.iter().any(|p| p[0] == 0xee));
    thread::sleep(Duration::from_millis(50));
    assert!(
        matches!(
            b.resident_sync_status(&id),
            Some(resident100::SyncStatus::Error(_))
        ),
        "old firmware cannot confirm resident library; visible streaming-only state required"
    );
    b.stop();
    let ps = rec.requests();
    assert!(!ps.iter().any(|p| p[0] == 0xef));
    assert!(ps.iter().any(|p| p[..3] == [0xec, 8, 1]));
}

#[test]
fn resident100_active_identity_conflict_yields_to_background_without_repeated_bulk() {
    let s = settings(1);
    let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let id = snap.profile_id.clone().unwrap();
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(|p| {
        if p[0] == 0xeb {
            return Some(Ok(capability(p)));
        }
        if p[..2] == [0xef, 9] {
            let mut r = *p;
            r[2] |= 128;
            r[3] = match p[2] {
                0 => 1,
                5 => 0,
                1 => 6,
                3 => {
                    r[8..].fill(0);
                    if p[3] == 0 {
                        r[8..28].fill(9);
                        0
                    } else {
                        1
                    }
                }
                6 => {
                    r[10] = 0;
                    r[11] = 2;
                    2
                }
                _ => panic!("conflict must not SELECT"),
            };
            return Some(Ok(r));
        }
        None
    });
    let mut target = crate::device::test_usb_device("conflict-fw", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut b = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |t, s| {
            if let Some(g) = gate.take() {
                g.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "no hardware");
            open_host_data_hid(t, s)
        },
    );
    b.bind_selected_application_layout_device("conflict".into());
    b.set_selected_application_layout_support(true);
    b.set_application_layout_snapshot(snap.clone());
    b.publish_resident_library_for_generation(
        vec![snap],
        b.application_layout_publication_generation().unwrap(),
    );
    tx.send(()).unwrap();
    super::resident_regressions::wait_for(&rec, |ps| ps.iter().any(|p| p[..3] == [0xef, 9, 3]));
    thread::sleep(Duration::from_millis(50));
    assert!(matches!(
        b.resident_sync_status(&id),
        Some(resident100::SyncStatus::Error(_))
    ));
    b.stop();
    assert_eq!(
        rec.requests().iter().filter(|p| p[0] == 0xec).count(),
        0,
        "a cold identity conflict must be refused before bulk transfer"
    );
    assert!(!rec.requests().iter().any(|p|p[..2]==[0xef,9]&&matches!(p[2],1|2|4)),"cold readback conflict must not STORE, SELECT or DELETE");
}

#[test]
fn resident100_background_stage_yields_across_unlock_idle_gaps() {
    let s = settings(1);
    let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(|p| {
        let mut r = *p;
        r[2] |= 128;
        r[3] = if matches!(p[2], 0 | 3) { 1 } else { 0 };
        if p[2] == 3 {
            r[8..].fill(0);
        }
        Some(Ok(r))
    });
    let fg = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState {
        library: Arc::new(vec![snap]),
        snapshot: fg.clone(),
        ..Default::default()
    });
    let mut bg = resident100::Background::default();
    let dev = HostDataHid::Shared(
        hid.shared_output()
            .unwrap()
            .for_host_bridge(Default::default(), false),
    );
    let mut t = 0;
    for _ in 0..101 {
        bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    }
    let pause = hid.pause_application_event_polling();
    let before = rec.requests().len();
    for _ in 0..4 {
        bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    }
    assert_eq!(
        rec.requests().len(),
        before,
        "whole unlock pause, not only settings worker mutex, must suppress staging"
    );
    drop(pause);
    bg.tick(&dev, &owner, &fg, 0, &AtomicBool::new(false), &mut t);
    assert!(rec.requests()[before..]
        .iter()
        .any(|p| p[..3] == [0xec, 8, 2]));
    assert!(
        rec.requests().len() - before <= 8,
        "bounded8 complete stage reports per pass"
    );
}

#[test]
fn resident100_late_background_confirmation_is_fenced_by_library_owner_and_stop() {
    for boundary in 0..3 {
        let s = settings(1);
        let snap = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
        let id = snap.profile_id.clone().unwrap();
        let (hid, rec) = crate::hid::HidDevice::test_device();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut cached = false;
        rec.respond_by(move |p| {
            let mut r = *p;
            r[2] |= 128;
            r[3] = 0;
            match p[2] {
                3 => {
                    r[3] = 1;
                    r[8..].fill(0);
                }
                0 => {
                    if cached {
                        entered_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    } else {
                        r[3] = 1;
                    }
                }
                1 => cached = true,
                5 => {}
                _ => panic!("bad op"),
            };
            Some(Ok(r))
        });
        let fg = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
        let owner = Arc::new(Mutex::new(ApplicationLayoutOwnerState {
            library: Arc::new(vec![snap]),
            snapshot: fg.clone(),
            ..Default::default()
        }));
        let mut bg = resident100::Background::default();
        let dev = HostDataHid::Shared(hid.shared_output().unwrap());
        let mut t = 0;
        let stop = Arc::new(AtomicBool::new(false));
        for _ in 0..105 {
            bg.tick(&dev, &owner, &fg, 0, &stop, &mut t);
        }
        let o = owner.clone();
        let stopping = stop.clone();
        let worker = thread::spawn(move || bg.tick(&dev, &o, &fg, 0, &stopping, &mut t));
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match boundary {
            0 => owner.lock().unwrap().library_generation += 1,
            1 => owner.lock().unwrap().revoke(),
            _ => stop.store(true, Ordering::Release),
        };
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert_ne!(
            owner.lock().unwrap().resident_status.get(&id),
            Some(&resident100::SyncStatus::Synced),
            "late background ACK boundary {boundary}"
        );
    }
}

fn settings(n: usize) -> DeviceApplicationLayouts {
    let mut s = DeviceApplicationLayouts::default();
    s.layouts.get_mut("default").unwrap().layers = vec![vec![42; 62]; 16];
    for i in 0..n {
        s.create_for_application(&DetectedApplication {
            executable: format!("app-{i:03}"),
            ..Default::default()
        });
    }
    s.normalize();
    s
}
#[test]
fn resident100_real_owner_active_stage_store_query_select_and_hit_no_bulk() {
    let s = settings(100);
    let mut snap = s.snapshot_for_layout(
        s.layouts
            .values()
            .find(|p| s.snapshot_for_layout(p).resident_slot == Some(99))
            .unwrap(),
    );
    snap.active = true;
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let cached = Arc::new(Mutex::new(None::<[u8; 24]>));
    let c = cached.clone();
    rec.respond_by(move |p| {
        if p[0] == 0xeb {
            return Some(Ok(capability(p)));
        }
        if p[..2] == [0xef, 9] {
            let mut r = *p;
            r[2] |= 128;
            let key = p[8..32].try_into().unwrap();
            r[3] = match p[2] {
                0 | 2 => {
                    if *c.lock().unwrap() == Some(key) {
                        0
                    } else {
                        1
                    }
                }
                1 => {
                    *c.lock().unwrap() = Some(key);
                    0
                }
                5 => 0,
                3 => {
                    r[8..32].fill(0);
                    1
                }
                _ => panic!("bad op"),
            };
            return Some(Ok(r));
        }
        None
    });
    let mut target = crate::device::test_usb_device("resident100", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |t, s| {
            if let Some(g) = gate.take() {
                g.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "no hardware");
            open_host_data_hid(t, s)
        },
    );
    bridge.bind_selected_application_layout_device("resident100".into());
    bridge.set_selected_application_layout_support(true);
    bridge.set_application_layout_snapshot(snap);
    tx.send(()).unwrap();
    super::resident_regressions::wait_for(&rec, |ps| ps.iter().any(|p| p[..3] == [0xef, 9, 2]));
    let first = rec.requests();
    assert!(
        first.iter().any(|p| p[..3] == [0xec, 8, 2]),
        "stage only, never activate before stored"
    );
    let ops: Vec<_> = first
        .iter()
        .filter(|p| p[..2] == [0xef, 9] && !matches!(p[2],3|6))
        .map(|p| p[2])
        .collect();
    assert_eq!(ops, [0, 5, 1, 0, 2]);
    let before = first.iter().filter(|p| p[0] == 0xec).count();
    bridge.force_application_layout_resend();
    super::resident_regressions::wait_for(&rec, |ps| {
        ps.iter().filter(|p| p[..3] == [0xef, 9, 2]).count() == 2
    });
    bridge.stop();
    assert_eq!(
        rec.requests().iter().filter(|p| p[0] == 0xec).count(),
        before
    );
    assert_eq!(cached.lock().unwrap().is_some(), true);
}
