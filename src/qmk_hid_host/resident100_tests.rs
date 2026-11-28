use super::*;
use crate::application_layouts::{DetectedApplication, DeviceApplicationLayouts};
use super::resident_regressions::{archive_reply, assert_host_wire, drive_archive, finish_bridge, fixture_raw, flash, gated_ram_rig, ram_matches, ram_transfers, seeded_peer, wait_for};
#[test]
fn host_archive_all_100_profiles_complete_without_uploading_host_library() {
    let s = settings(100);
    let library: Vec<_> = s.layouts.values().filter(|p| p.id != "default").map(|p| s.snapshot_for_layout(p)).collect();
    assert_eq!(library.len(), 100);
    let records: Vec<_> = library.iter().map(|p| (p.resident_slot.unwrap(), fixture_raw(p))).collect();
    let expected: std::collections::BTreeMap<_, _> = records.iter().cloned().collect();
    assert_eq!(expected.keys().copied().collect::<Vec<_>>(), (0..100).collect::<Vec<_>>());
    let peer = seeded_peer(&records); let before = flash(&peer);
    let (mut bridge, hid, rec, start) = gated_ram_rig(crate::application_layouts::ApplicationLayoutSnapshot::inactive(), peer.clone());
    let generation = bridge.application_layout_publication_generation().unwrap();
    assert!(bridge.publish_resident_library_for_generation(library.clone(), generation)); start.send(()).unwrap();
    wait_for(&rec, |ps| ps.iter().any(|p| p[..3] == [0xef, 9, 6]));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel(); let (release_tx, release_rx) = std::sync::mpsc::channel();
    let settings = thread::spawn(move || {
        let _pause = hid.pause_application_event_polling();
        hid.with_host_output_paused_for_settings(|| { entered_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap(); });
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(); let paused = rec.requests().len();
    thread::sleep(Duration::from_millis(40));
    assert!(!rec.requests()[paused..].iter().any(|p| matches!(p[0], 0xec..=0xef)), "settings/unlock lease leaked archive I/O");
    release_tx.send(()).unwrap(); settings.join().unwrap();
    // These are completion guards, not product latency promises. Keep the full
    // hundred-FULL workload, original total budget and no-progress watchdog.
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut progress_deadline = Instant::now() + Duration::from_secs(30); let mut previous = 0;
    loop {
        if bridge.completed_legacy_archive().is_some() { break; }
        let progress = bridge.resident_recoveries().len();
        if progress != previous { progress_deadline = Instant::now() + Duration::from_secs(30); previous = progress; }
        assert!(Instant::now() < deadline && Instant::now() < progress_deadline, "complete archive stalled at {progress}/100");
        thread::sleep(Duration::from_millis(5));
    }
    let archive = bridge.completed_legacy_archive().unwrap(); assert_eq!(archive.len(), 100);
    for record in &archive { assert_eq!(record.raw, expected[&record.slot]); assert!(record.validation.is_none()); validate_recovered_full(&record.raw, &record.identity_crc).unwrap(); }
    assert!(!bridge.resident_export_inventory_confirmed(&DeviceApplicationLayouts::default()), "physical read is not a durable archive receipt");
    for snapshot in &library { assert_ne!(bridge.resident_sync_status(snapshot.profile_id.as_ref().unwrap()), Some(resident100::SyncStatus::Synced)); }
    finish_bridge(&mut bridge); let packets = rec.requests(); assert_host_wire(&packets);
    assert!(!packets.iter().any(|p| matches!(p[0], 0xec..=0xee)), "published host library was bulk uploaded");
    assert!(packets.iter().any(|p| p[0] == 0xe5), "runtime keepalive starved during hundred-profile archive");
    assert_eq!(flash(&peer), before);
}

pub(super) fn capability(p: &[u8; 32]) -> [u8; 32] {
    if p[3] == 3 { return *p; } // legacy schema2 peer
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
fn host_archive_inactive_slot0_intent_preserves_legacy_record_without_delete() {
    let s = settings(1); let mut snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let raw = fixture_raw(&snapshot); snapshot.active = false;
    let peer = seeded_peer(&[(0, raw.clone())]); let before = flash(&peer);
    let (hid, rec) = crate::hid::HidDevice::test_device(); let p = peer.clone(); rec.respond_by(move |packet| archive_reply(&p, packet));
    let foreground = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState { library: Arc::new(vec![snapshot.clone()]), snapshot: foreground.clone(), ..Default::default() });
    let dev = HostDataHid::Shared(hid.shared_output().unwrap()); let mut bg = resident100::Background::default();
    drive_archive(&mut bg, &dev, &owner, &foreground, 0, 800);
    let state = owner.lock().unwrap(); assert!(state.resident_inventory_complete);
    let record = &state.resident_recoveries[&0]; assert_eq!(record.raw, raw); assert!(record.validation.is_none());
    validate_recovered_full(&record.raw, &record.identity_crc).unwrap();
    assert_ne!(state.resident_status.get(snapshot.profile_id.as_ref().unwrap()), Some(&resident100::SyncStatus::Synced), "inactive host intent falsely confirmed a flash tombstone");
    assert_host_wire(&rec.requests()); assert_eq!(flash(&peer), before);
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
fn host_archive_uncertain_read_retries_same_offset_without_flash_mutation() {
    let s = settings(1); let snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let raw = fixture_raw(&snapshot); let peer = seeded_peer(&[(0, raw.clone())]); let before = flash(&peer);
    let (hid, rec) = crate::hid::HidDevice::test_device(); let p = peer.clone();
    let lost = Arc::new(AtomicBool::new(false)); let tripped = lost.clone();
    rec.respond_by(move |packet| {
        if packet[..3] == [0xef, 9, 6]
            && u16::from_le_bytes(packet[8..10].try_into().unwrap()) >= 40
            && !tripped.swap(true, Ordering::AcqRel) {
            return Some(Err("modeled missing READ reply after partial FULL delivery".into()));
        }
        archive_reply(&p, packet)
    });
    let foreground = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState { library: Arc::new(vec![snapshot]), snapshot: foreground.clone(), ..Default::default() });
    let dev = HostDataHid::Shared(hid.shared_output().unwrap()); let mut bg = resident100::Background::default();
    drive_archive(&mut bg, &dev, &owner, &foreground, 0, 800);
    assert!(lost.load(Ordering::Acquire)); let state = owner.lock().unwrap(); assert!(state.resident_inventory_complete);
    assert_eq!(state.resident_recoveries[&0].raw, raw); assert!(state.resident_recoveries[&0].validation.is_none());
    let packets = rec.requests(); let reads: Vec<_> = packets.iter().filter(|p| p[..3] == [0xef, 9, 6]).collect();
    assert_eq!(reads.iter().take(5).map(|p| u16::from_le_bytes(p[8..10].try_into().unwrap())).collect::<Vec<_>>(),
        [0, 20, 40, 40, 60], "late uncertain chunk discarded the prefix or skipped bytes");
    assert_ne!(reads[2][4..8], reads[3][4..8]); assert_host_wire(&packets); assert_eq!(flash(&peer), before);
}

#[test]
fn resident100_wire_allowlists_are_exact_readonly_and_reject_bad_padding() {
    for op in 0..=7 {
        let mut p = resident100::request(op, if op == 7 { 0 } else { 99 }, None, &[0; 20], &[0; 4]);
        if op == 6 {
            p[10] = 20;
        }
        assert_eq!(
            crate::hid::hid_protocol::is_read_request(&p),
            matches!(op, 0 | 3 | 6 | 7),
            "EF09 op {op}"
        );
        assert_eq!(crate::hid::is_resident09_request(&p), matches!(op, 0 | 3 | 6 | 7),
            "host-managed transport must deny legacy mutation/staging op {op}");
        let mut bad = p;
        bad[3] = 100;
        assert!(!crate::hid::is_resident09_request(&bad));
        if matches!(op, 3 | 6 | 7) {
            bad = p;
            bad[31] = 1;
            assert!(!crate::hid::hid_protocol::is_read_request(&bad));
            assert!(!crate::hid::is_resident09_request(&bad));
        }
    }
    let mut unknown = resident_catalog::query();
    unknown[2] = 8;
    assert!(!crate::hid::is_resident09_request(&unknown));
    assert!(!crate::hid::hid_protocol::is_read_request(&unknown));
    let mut bad_catalog = resident_catalog::query();
    bad_catalog[3] = 1;
    assert!(!crate::hid::is_resident09_request(&bad_catalog));
    assert!(!crate::hid::hid_protocol::is_read_request(&bad_catalog));
    for version in [1, 2, 3] {
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

pub(super) fn raw_from_transfer(packets: &[[u8; 32]]) -> Vec<u8> {
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
fn host_ram_old_firmware_reports_legacy_capability_not_resident_synced() {
    let s = settings(1); let snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let raw = fixture_raw(&snapshot); let id = snapshot.profile_id.clone().unwrap(); let peer = seeded_peer(&[]); let before = flash(&peer);
    let (mut bridge, _hid, rec, start) = gated_ram_rig(snapshot.clone(), peer.clone());
    rec.respond_by(|p| {
        if p[0] == 0xeb {
            let mut r = *p;
            if p[3] == 1 { r[2] = 0x5a; r[8] = 3; r[9] = 3; r[10..12].copy_from_slice(&4560u16.to_le_bytes()); }
            return Some(Ok(r));
        }
        if p[0] == 0xe6 { let mut r = [0; 32]; r[..3].copy_from_slice(&[0xe6, 8, 0x5a]); return Some(Ok(r)); }
        assert_ne!(p[0], 0xef, "old firmware received a resident operation"); None
    });
    assert!(bridge.publish_resident_library_for_generation(vec![snapshot], bridge.application_layout_publication_generation().unwrap()));
    start.send(()).unwrap(); wait_for(&rec, |_| ram_matches(&peer, &raw));
    thread::sleep(Duration::from_millis(50));
    assert_eq!(bridge.application_layout_snapshot.lock().unwrap().resident_export_capability, Some(resident_catalog::Capability::Legacy));
    assert_ne!(bridge.resident_sync_status(&id), Some(resident100::SyncStatus::Synced), "RAM success became fake legacy resident Synced");
    assert_eq!(bridge.completed_legacy_archive().unwrap().len(), 0, "legacy firmware cannot provide a resident archive");
    finish_bridge(&mut bridge); let packets = rec.requests(); assert_eq!(ram_transfers(&packets), vec![raw]);
    assert_eq!(packets.iter().filter(|p| p[0] == 0xeb).map(|p| p[3]).collect::<Vec<_>>(), [3, 2, 1]);
    assert!(!packets.iter().any(|p| p[0] == 0xef)); assert_eq!(flash(&peer), before);
}

#[test]
fn host_archive_cold_conflict_recovers_before_eligible_ram_playback() {
    let s = settings(1); let snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let mut foreign = snapshot.clone(); foreign.profile_id = Some("foreign-legacy-slot0".into()); foreign.layers[15][61] = 0x31;
    let raw = fixture_raw(&foreign); let peer = seeded_peer(&[(0, raw.clone())]); let before = flash(&peer);
    // Cold-owner migration starts read-first/inactive. Only after a complete
    // archive is available does an explicit host publication become eligible.
    // This does not claim a RAM-first worker can replace the UI migration gate.
    let (mut bridge, _hid, rec, start) = gated_ram_rig(crate::application_layouts::ApplicationLayoutSnapshot::inactive(), peer.clone());
    let generation = bridge.application_layout_publication_generation().unwrap();
    assert!(bridge.publish_resident_library_for_generation(vec![snapshot.clone()], generation)); start.send(()).unwrap();
    wait_for(&rec, |_| bridge.completed_legacy_archive().is_some());
    let archive = bridge.completed_legacy_archive().unwrap(); assert_eq!(archive.len(), 1); assert_eq!(archive[0].raw, raw);
    validate_recovered_full(&archive[0].raw, &archive[0].identity_crc).unwrap();
    assert!(!rec.requests().iter().any(|p| matches!(p[0], 0xec..=0xee)), "cold host draft played before archive recovery");
    assert!(matches!(bridge.resident_sync_status(snapshot.profile_id.as_ref().unwrap()), Some(resident100::SyncStatus::Error(_))), "identity conflict was hidden");
    let read_fence = rec.requests().len(); let expected = fixture_raw(&snapshot);
    assert!(bridge.publish_application_layout_snapshot_for_generation(snapshot.clone(), generation));
    wait_for(&rec, |_| ram_matches(&peer, &expected));
    wait_for(&rec, |ps| ps[read_fence..].iter().any(|p| p[0] == 0xe6));
    let cut = rec.requests().len(); assert!(bridge.publish_application_layout_snapshot_for_generation(snapshot, generation));
    wait_for(&rec, |ps| ps[cut..].iter().any(|p| p[0] == 0xe6));
    assert!(!rec.requests()[cut..].iter().any(|p| matches!(p[0], 0xec..=0xee)), "same eligible draft repeatedly uploaded");
    finish_bridge(&mut bridge); let packets = rec.requests(); assert_eq!(ram_transfers(&packets), vec![expected]);
    assert!(packets.iter().position(|p| p[0] == 0xec).unwrap() >= read_fence);
    assert_eq!(flash(&peer), before, "eligible RAM playback replaced the conflicting legacy record");
}

#[test]
fn host_archive_read_cursor_yields_across_unlock_idle_gaps() {
    let s = settings(1); let snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
    let peer = seeded_peer(&[(0, fixture_raw(&snapshot))]); let before_flash = flash(&peer);
    let (hid, rec) = crate::hid::HidDevice::test_device(); let p = peer.clone(); rec.respond_by(move |packet| archive_reply(&p, packet));
    let foreground = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
    let owner = Mutex::new(ApplicationLayoutOwnerState { library: Arc::new(vec![snapshot]), snapshot: foreground.clone(), ..Default::default() });
    let dev = HostDataHid::Shared(hid.shared_output().unwrap().for_host_bridge(Default::default(), false)); let mut bg = resident100::Background::default();
    for _ in 0..3 { bg.tick(&dev, &owner, &foreground, 0, &AtomicBool::new(false), &mut 0); }
    let initial = rec.requests(); let last = initial.iter().rev().find(|p| p[..3] == [0xef, 9, 6]).unwrap();
    let next_offset = u16::from_le_bytes(last[8..10].try_into().unwrap()) + 20;
    let pause = hid.pause_application_event_polling(); let before = rec.requests().len();
    for _ in 0..4 { bg.tick(&dev, &owner, &foreground, 0, &AtomicBool::new(false), &mut 0); }
    assert_eq!(rec.requests().len(), before, "whole unlock pause must suppress archive requests during settings idle gaps");
    drop(pause); bg.tick(&dev, &owner, &foreground, 0, &AtomicBool::new(false), &mut 0);
    let packets = rec.requests(); assert_eq!(packets[before][..3], [0xef, 9, 6]);
    assert_eq!(packets[before][8..10], next_offset.to_le_bytes(), "unlock lost the read cursor");
    assert!(packets.len() - before <= 8, "bounded complete reports per pass");
    drive_archive(&mut bg, &dev, &owner, &foreground, 0, 800); assert!(owner.lock().unwrap().resident_inventory_complete);
    assert_host_wire(&rec.requests()); assert_eq!(flash(&peer), before_flash);
}

#[test]
fn host_archive_late_confirmation_is_fenced_by_library_owner_and_stop() {
    for boundary in 0..3 {
        let s = settings(1); let snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| p.id != "default").unwrap());
        let raw = fixture_raw(&snapshot); let peer = seeded_peer(&[(0, raw.clone())]); let before = flash(&peer);
        let (hid, rec) = crate::hid::HidDevice::test_device(); let p = peer.clone();
        let terminal_read = Arc::new(AtomicBool::new(false)); let terminal = terminal_read.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel(); let (release_tx, release_rx) = std::sync::mpsc::channel(); let mut held = false;
        rec.respond_by(move |packet| {
            if packet[..3] == [0xef, 9, 3] && terminal.load(Ordering::Acquire) && !held {
                held = true; entered_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            let reply = archive_reply(&p, packet);
            if packet[..3] == [0xef, 9, 6] && reply.as_ref().is_some_and(|r| r.as_ref().is_ok_and(|r| r[10] < 20)) { terminal.store(true, Ordering::Release); }
            reply
        });
        let foreground = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
        let owner = Arc::new(Mutex::new(ApplicationLayoutOwnerState { library: Arc::new(vec![snapshot]), snapshot: foreground.clone(), ..Default::default() }));
        let stop = Arc::new(AtomicBool::new(false)); let dev = HostDataHid::Shared(hid.shared_output().unwrap()); let mut bg = resident100::Background::default();
        for _ in 0..800 { bg.tick(&dev, &owner, &foreground, 0, &stop, &mut 0); if terminal_read.load(Ordering::Acquire) { break; } }
        assert!(terminal_read.load(Ordering::Acquire), "terminal physical READ not reached");
        let state = owner.clone(); let stopping = stop.clone(); let fg = foreground.clone();
        let worker = thread::spawn(move || { bg.tick(&dev, &state, &fg, 0, &stopping, &mut 0); (bg, dev) });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match boundary { 0 => owner.lock().unwrap().library_generation += 1, 1 => owner.lock().unwrap().revoke(), _ => stop.store(true, Ordering::Release) };
        release_tx.send(()).unwrap(); let (mut bg, dev) = worker.join().unwrap();
        assert!(owner.lock().unwrap().resident_recoveries.is_empty(), "late confirmation published stale proof: {boundary}");
        assert!(!owner.lock().unwrap().resident_inventory_complete);
        if boundary == 0 {
            drive_archive(&mut bg, &dev, &owner, &foreground, 0, 800);
            let state = owner.lock().unwrap(); assert!(state.resident_inventory_complete); assert_eq!(state.resident_recoveries[&0].raw, raw);
            assert_eq!(rec.requests().iter().filter(|p| p[..3] == [0xef, 9, 6] && p[8..10] == [0, 0]).count(), 1, "library-only retry discarded physical READ cursor");
        } else {
            let cut = rec.requests().len(); for _ in 0..3 { bg.tick(&dev, &owner, &foreground, 0, &stop, &mut 0); }
            assert_eq!(rec.requests().len(), cut, "revoked/stopped archive owner resumed I/O");
        }
        assert_host_wire(&rec.requests()); assert_eq!(flash(&peer), before);
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
fn host_ram_real_owner_slot99_commits_exact_full_and_forced_replay() {
    let s = settings(100); let snapshot = s.snapshot_for_layout(s.layouts.values().find(|p| s.snapshot_for_layout(p).resident_slot == Some(99)).unwrap());
    assert!(snapshot.active); assert_eq!(snapshot.resident_slot, Some(99));
    let expected = fixture_raw(&snapshot); let peer = seeded_peer(&[]); let before = flash(&peer);
    let (mut bridge, _hid, rec, start) = gated_ram_rig(snapshot.clone(), peer.clone()); start.send(()).unwrap();
    wait_for(&rec, |_| ram_matches(&peer, &expected));
    wait_for(&rec, |_| bridge.completed_legacy_archive().is_some());
    assert!(bridge.completed_legacy_archive().unwrap().is_empty(), "active RAM profile appeared in persistent archive");
    bridge.force_application_layout_resend(); wait_for(&rec, |ps| ps.iter().filter(|p| p[0] == 0xee).count() == 2);
    finish_bridge(&mut bridge); let packets = rec.requests(); assert_eq!(ram_transfers(&packets), vec![expected.clone(), expected]);
    assert_eq!(packets.iter().filter(|p| p[0] == 0xec).count(), 2, "replay skipped complete RAM transaction");
    assert_eq!(flash(&peer), before); assert_host_wire(&packets);
}
