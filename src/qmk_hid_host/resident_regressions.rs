use super::*;
use crate::application_layouts::{ApplicationLayoutSnapshot, DeviceApplicationLayouts};

#[test]
fn host_archive_readback_rejects_stale_owner_library_corrupt_and_truncated_full() {
    for fault in ["owner", "library", "corrupt", "truncated"] {
        let foreground = ApplicationLayoutSnapshot::inactive();
        let peer = seeded_peer(&[(0, fixture_raw(&profile()))]); let before = flash(&peer);
        let owner = Arc::new(Mutex::new(ApplicationLayoutOwnerState {
            snapshot: foreground.clone(), library: Arc::new(vec![profile()]), ..Default::default()
        }));
        let (hid, rec) = crate::hid::HidDevice::test_device();
        let p = peer.clone(); let state = owner.clone(); let tripped = Arc::new(AtomicBool::new(false)); let trip = tripped.clone();
        rec.respond_by(move |packet| {
            let mut reply = archive_reply(&p, packet);
            if packet[..3] == [0xef, 9, 6] {
                if !trip.swap(true, Ordering::AcqRel) {
                    if fault == "owner" { state.lock().unwrap().generation += 1; }
                    if fault == "library" { state.lock().unwrap().library_generation += 1; }
                }
                if let Some(Ok(r)) = &mut reply {
                    // Alter a valid keycode byte, not the FULL header: this is
                    // specifically a CRC mismatch with otherwise valid framing.
                    if fault == "corrupt" && packet[8..10] == 60u16.to_le_bytes() { r[16] ^= 1; }
                    if fault == "truncated" && u16::from_le_bytes(packet[8..10].try_into().unwrap()) >= 40 { r[10] = 0; r[12..].fill(0); }
                }
            }
            reply
        });
        let dev = HostDataHid::Shared(hid.shared_output().unwrap()); let mut bg = resident100::Background::default();
        for _ in 0..800 {
            bg.tick(&dev, &owner, &foreground, 0, &AtomicBool::new(false), &mut 0);
            if tripped.load(Ordering::Acquire) && matches!(fault, "owner" | "library") { break; }
        }
        assert!(tripped.load(Ordering::Acquire), "retained safety seam never reached: {fault}");
        let state = owner.lock().unwrap(); assert!(!state.resident_inventory_complete, "invalid archive certified: {fault}");
        if matches!(fault, "corrupt" | "truncated") {
            let record = &state.resident_recoveries[&0];
            assert!(record.validation.is_some()); assert!(validate_recovered_full(&record.raw, &record.identity_crc).is_err());
        } else { assert!(state.resident_recoveries.is_empty(), "stale READ published proof: {fault}"); }
        assert_eq!(flash(&peer), before); assert_host_wire(&rec.requests());
    }
}


#[test]
fn host_ram_library_republication_empty_archive_does_not_cancel_commit() {
    cancelled_ram_preserves_archive(false, "library");
}

#[test]
fn host_ram_foreground_preemption_empty_archive_commits_only_successor() {
    cancelled_ram_preserves_archive(false, "foreground");
}

#[test]
fn host_ram_library_republication_preserves_existing_archive_crc() {
    cancelled_ram_preserves_archive(true, "library");
}

#[test]
fn host_ram_foreground_preemption_preserves_existing_archive_crc() {
    cancelled_ram_preserves_archive(true, "foreground");
}

// Only physical wire I/O is modeled. All admission, streaming, CRC validation,
// inventory fencing and scheduling below use the accepted production owners.
pub(super) fn fixture_raw(snapshot: &ApplicationLayoutSnapshot) -> Vec<u8> {
    compact::full(&ApplicationLayoutTransmission.packets(snapshot)).unwrap()
}

pub(super) fn seeded_peer(records: &[(u8, Vec<u8>)]) -> Arc<Mutex<Peer>> {
    let mut peer = Peer::default();
    peer.slots.resize(100, None);
    peer.raws.resize(100, None);
    for (slot, raw) in records {
        let record = Peer::with_record(*slot, raw.clone());
        assert!(peer.slots[usize::from(*slot)].is_none(), "duplicate fixture slot");
        peer.slots[usize::from(*slot)] = record.slots[usize::from(*slot)];
        peer.raws[usize::from(*slot)] = Some(raw.clone());
    }
    Arc::new(Mutex::new(peer))
}

pub(super) fn ram_matches(peer: &Arc<Mutex<Peer>>, raw: &[u8]) -> bool {
    peer.lock().unwrap().last_raw.as_deref() == Some(raw)
}

pub(super) fn flash(peer: &Arc<Mutex<Peer>>) -> Vec<(u8, [u8; 24], Vec<u8>)> {
    let p = peer.lock().unwrap();
    assert_eq!((p.stores, p.selects), (0, 0), "host-managed path mutated legacy storage");
    p.slots.iter().enumerate().filter_map(|(slot, key)| key.map(|key|
        (slot as u8, key, p.raws[slot].clone().expect("occupied flash lacks FULL")))).collect()
}

pub(super) fn archive_reply(peer: &Arc<Mutex<Peer>>, p: &[u8; 32]) -> Option<Result<[u8; 32], String>> {
    if p[0] == 0xef {
        assert_eq!(p[1], 9);
        assert!(matches!(p[2], 3 | 6 | 7), "archive reader issued non-archive operation (legacy mutations are forbidden): {}", p[2]);
    }
    peer.lock().unwrap().reply(p)
}

pub(super) fn assert_host_wire(packets: &[[u8; 32]]) {
    assert!(packets.iter().all(|p| p[0] != 0xef || (p[1] == 9 && matches!(p[2], 3 | 6 | 7))),
        "host-managed work issued a legacy mutator");
    assert!(packets.iter().all(|p| p[0] != 0xec || p[2] == 1), "RAM must never use persistent stage mode2");
}

// Validate every completed real RAM transaction, not merely the appearance of EE.
// Incomplete preempted BEGINs are retained in the recorder and checked by callers.
pub(super) fn ram_transfers(packets: &[[u8; 32]]) -> Vec<Vec<u8>> {
    assert_host_wire(packets);
    let mut current = Vec::new();
    let mut complete = Vec::new();
    for p in packets {
        match p[0] {
            0xec => { current.clear(); current.push(*p); assert_ne!(p[4..8], [0; 4]); },
            0xed => { assert!(!current.is_empty()); current.push(*p); },
            0xee => {
                assert!(!current.is_empty());
                let begin = current[0];
                let count = u16::from_le_bytes(begin[12..14].try_into().unwrap()) as usize;
                assert_eq!(current.len() - 1, count);
                for (seq, data) in current[1..].iter().enumerate() {
                    assert_eq!(data[2..6], begin[4..8]);
                    assert_eq!(u16::from_le_bytes(data[6..8].try_into().unwrap()) as usize, seq);
                }
                assert_eq!(p[2..6], begin[4..8], "COMMIT transaction identity");
                assert_eq!(p[6..8], begin[12..14]);
                assert_eq!(p[8..12], begin[20..24], "COMMIT encoded CRC");
                let size = u16::from_le_bytes(begin[8..10].try_into().unwrap()) as usize;
                let encoded: Vec<u8> = current[1..].iter().flat_map(|p| p[8..].iter().copied()).take(size).collect();
                assert_eq!(compact::crc32(&encoded).to_le_bytes(), begin[20..24]);
                current.push(*p);
                let raw = super::resident100_tests::raw_from_transfer(&current);
                assert_eq!(raw.len(), u16::from_le_bytes(begin[10..12].try_into().unwrap()) as usize);
                assert_eq!(compact::crc32(&raw).to_le_bytes(), begin[16..20]);
                let mut key = [0; 24]; key[..20].copy_from_slice(&raw[43..63]); key[20..].copy_from_slice(&begin[16..20]);
                validate_recovered_full(&raw, &key).unwrap();
                complete.push(raw);
                current.clear();
            },
            _ => {},
        }
    }
    complete
}

pub(super) fn drive_archive(bg: &mut resident100::Background, dev: &HostDataHid,
    owner: &Mutex<ApplicationLayoutOwnerState>, foreground: &ApplicationLayoutSnapshot, generation: u64, budget: usize) {
    let stop = AtomicBool::new(false);
    let mut transaction = 0;
    for _ in 0..budget {
        bg.tick(dev, owner, foreground, generation, &stop, &mut transaction);
        if owner.lock().unwrap().resident_inventory_complete { break; }
    }
}

// Gate only endpoint acquisition so callers can install deterministic wire faults
// before the real worker starts. No native enumeration/open fallback is possible.
pub(super) fn gated_ram_rig(snapshot: ApplicationLayoutSnapshot, peer: Arc<Mutex<Peer>>) ->
    (QmkHidHostBridge, crate::hid::HidDevice, crate::hid::TestHidRecorder, std::sync::mpsc::Sender<()>) {
    let mut target = crate::device::test_usb_device("host-ram-regression", 1, 0x00b2);
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let p = peer.clone(); rec.on_output(move |bytes| p.lock().unwrap().output(bytes));
    rec.respond_by(move |p| archive_reply(&peer, p));
    let (tx, rx) = std::sync::mpsc::channel(); let mut gate = Some(rx);
    let bridge = QmkHidHostBridge::start_with_sources(target, Default::default(), hid.shared_output(),
        HostProtocol::Selected(false), || None, move |t, shared| {
            if let Some(rx) = gate.take() { rx.recv_timeout(Duration::from_secs(5)).unwrap(); }
            anyhow::ensure!(shared.is_some(), "native open forbidden"); open_host_data_hid(t, shared)
        });
    let mut bridge = bridge;
    bridge.bind_selected_application_layout_device("host-ram-model".into());
    bridge.set_selected_application_layout_support(true);
    bridge.set_application_layout_snapshot(snapshot);
    (bridge, hid, rec, tx)
}

pub(super) fn finish_bridge(bridge: &mut QmkHidHostBridge) {
    let worker = bridge.thread.take(); bridge.stop();
    if let Some(worker) = worker { worker.join().unwrap(); }
}

fn cancelled_ram_preserves_archive(existing: bool, cancellation: &str) {
    let original = profile();
    let old_raw = fixture_raw(&original);
    let records = if existing { vec![(0, old_raw)] } else { Vec::new() };
    let peer = seeded_peer(&records); let before = flash(&peer);
    let (mut bridge, _hid, rec, start) = gated_ram_rig(original.clone(), peer.clone());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let p = peer.clone(); let mut held = false;
    rec.on_output(move |packet| {
        p.lock().unwrap().output(packet);
        if packet[0] == 0xed && !held { held = true; entered_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap(); }
    });
    start.send(()).unwrap(); entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut successor = original.clone(); successor.layers[15][61] = 0x1234; successor.revision += 1;
    let generation = bridge.application_layout_publication_generation().unwrap();
    if cancellation == "library" {
        assert!(bridge.publish_resident_library_for_generation(vec![successor.clone()], generation));
    } else {
        assert!(bridge.publish_application_layout_snapshot_for_generation(successor.clone(), generation));
    }
    release_tx.send(()).unwrap();
    let expected = fixture_raw(if cancellation == "library" { &original } else { &successor });
    wait_for(&rec, |_| peer.lock().unwrap().last_raw.as_ref() == Some(&expected));
    wait_for(&rec, |_| bridge.completed_legacy_archive().is_some());
    let archive = bridge.completed_legacy_archive().unwrap();
    assert_eq!(archive.len(), if existing { 1 } else { 0 });
    if existing { assert_eq!(archive[0].raw, records[0].1); validate_recovered_full(&archive[0].raw, &archive[0].identity_crc).unwrap(); }
    assert_eq!(bridge.application_layout_publication_generation(), Some(generation));
    finish_bridge(&mut bridge);
    assert_eq!(flash(&peer), before, "RAM preemption changed physical legacy flash");
    let ps = rec.requests(); assert_eq!(ram_transfers(&ps), vec![expected]);
    let begins = ps.iter().filter(|p| p[0] == 0xec).count();
    assert_eq!(begins, if cancellation == "library" { 1 } else { 2 },
        "library-only publication must not cancel RAM; foreground replacement must cancel before COMMIT");
    assert!(!ps.iter().any(|p| p[0] == 0xef && matches!(p[2], 1 | 2 | 4 | 5)));
}

fn profile() -> ApplicationLayoutSnapshot {
    let mut layout = DeviceApplicationLayouts::default().layouts["default"].clone();
    layout.layers = vec![vec![0x22; 62]; 16];
    layout.id="resident-application".into();
    let mut snapshot=ApplicationLayoutSnapshot::from_layout(&layout);snapshot.resident_slot=Some(0);snapshot
}

pub(super) fn wait_for(recorder: &crate::hid::TestHidRecorder, predicate: impl Fn(&[[u8; 32]]) -> bool) {
    // A complete read-only archive requires hundreds of correlated chunks.
    // This is a completion/deadlock guard, not a product latency requirement.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if predicate(&recorder.requests()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "resident transaction missing: {:?}",
            recorder
                .requests()
                .iter()
                .map(|p| (p[0], p[2]))
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn host_ram_same_profile_republication_is_quiet_and_forced_replay_is_complete() {
    let snapshot = profile(); let raw = fixture_raw(&snapshot); let peer = seeded_peer(&[]);
    let before = flash(&peer); let (mut bridge, _hid, rec, start) = gated_ram_rig(snapshot.clone(), peer.clone());
    start.send(()).unwrap(); wait_for(&rec, |_| peer.lock().unwrap().last_raw.as_ref() == Some(&raw));
    wait_for(&rec, |ps| ps.iter().any(|p| p[0] == 0xe6));
    let cut = rec.requests().len(); let generation = bridge.application_layout_publication_generation().unwrap();
    for _ in 0..3 { assert!(bridge.publish_application_layout_snapshot_for_generation(snapshot.clone(), generation)); }
    wait_for(&rec, |ps| ps[cut..].iter().any(|p| p[0] == 0xe6));
    assert!(!rec.requests()[cut..].iter().any(|p| matches!(p[0], 0xec..=0xee)), "identical repaint bypassed RAM cache");
    bridge.force_application_layout_resend();
    wait_for(&rec, |ps| ps.iter().filter(|p| p[0] == 0xee).count() == 2);
    finish_bridge(&mut bridge); let ps = rec.requests();
    assert_eq!(ram_transfers(&ps), vec![raw.clone(), raw]);
    let tokens: Vec<_> = ps.iter().filter(|p| p[0] == 0xec).map(|p| p[4..8].to_vec()).collect();
    assert_eq!(tokens.len(), 2); assert_ne!(tokens[0], tokens[1]); assert_eq!(flash(&peer), before);
}

#[derive(Clone, Copy)]
enum Fault {
    Lost,
    Echo,
    Rejected,
    WrongNonce,
}

#[cfg(test)]
pub(super) fn full_raw(packets:&[[u8;32]])->Vec<u8> {
    assert_eq!(packets.len(),210);let mut raw=Vec::from(packets[0]);raw.extend_from_slice(packets.last().unwrap());
    for p in &packets[1..209] {match p[0] {
        0xe2=>raw.extend_from_slice(&p[5..5+usize::from(p[4])*2]),
        0xe7=>raw.extend_from_slice(&p[5..5+usize::from(p[4])]),
        0xe9=>raw.extend_from_slice(&p[4..10+usize::from(p[5])]),
        0xe4=>raw.extend_from_slice(&p[3..4+usize::from(p[3])]),
        _=>panic!("unexpected FULL fixture packet"),
    }}raw
}

#[derive(Default)]
pub(super) struct Peer {
    slots: Vec<Option<[u8; 24]>>,
    ready_token: Option<u32>,
    begin: Option<[u8; 32]>,
    data: Vec<u8>,
    last: Option<[u8; 24]>,
    last_raw: Option<Vec<u8>>,
    raws: Vec<Option<Vec<u8>>>,
    stores: usize,
    selects: usize,
    fault: Option<(u8, Fault)>,
}

impl Peer {
    // Model only the wire boundary. Production encoder, bridge, ownership,
    // retry/deadline and scheduling remain real Rust, with no device access.
    pub(super) fn output(&mut self, p: &[u8]) {
        match p[0] {
            0xec => {
                self.begin = Some(p.try_into().unwrap());
                self.data.clear();
                self.last = None;
                self.last_raw = None;
            }
            0xed => {
                let b = self.begin.unwrap();
                assert_eq!(p[2..6], b[4..8]);
                assert_eq!(
                    u16::from_le_bytes(p[6..8].try_into().unwrap()) as usize,
                    self.data.len() / 24
                );
                self.data.extend_from_slice(&p[8..32]);
            }
            0xee => {
                let b = self.begin.take().unwrap();
                let size = u16::from_le_bytes(b[8..10].try_into().unwrap()) as usize;
                self.data.truncate(size);
                assert_eq!(p[8..12], compact::crc32(&self.data).to_le_bytes());
                let raw = if b[3] == 0 {
                    self.data.clone()
                } else {
                    let mut raw = Vec::new();
                    let mut i = 0;
                    while i < self.data.len() {
                        let tag = self.data[i];
                        i += 1;
                        if tag < 128 {
                            let n = usize::from(tag) + 1;
                            raw.extend_from_slice(&self.data[i..i + n]);
                            i += n;
                        } else {
                            let n = usize::from(tag & 127) + 3;
                            let offset = u16::from_le_bytes(self.data[i..i + 2].try_into().unwrap())
                                as usize;
                            i += 2;
                            for _ in 0..n {
                                raw.push(raw[raw.len() - offset]);
                            }
                        }
                    }
                    raw
                };
                assert_eq!(
                    raw.len(),
                    u16::from_le_bytes(b[10..12].try_into().unwrap()) as usize
                );
                let crc = compact::crc32(&raw).to_le_bytes();
                assert_eq!(crc, b[16..20]);
                let mut key = [0; 24];
                key[..20].copy_from_slice(&raw[43..63]);
                key[20..].copy_from_slice(&crc);
                validate_recovered_full(&raw,&key).expect("production FULL validation");
                self.last = Some(key);
                self.last_raw = Some(raw);
                self.ready_token=Some(u32::from_le_bytes(b[4..8].try_into().unwrap()));
            }
            _ => {}
        }
    }

    pub(super) fn ready_matches(&self,p:&[u8;32])->bool {self.last.as_ref().is_some_and(|key|key[..]==p[8..32]) && self.ready_token==Some(u32::from_le_bytes(p[4..8].try_into().unwrap()))}

    pub(super) fn with_record(slot: u8, raw: Vec<u8>) -> Self {
        assert!(slot < 100);
        let mut key = [0; 24];
        key[..20].copy_from_slice(&raw[43..63]);
        key[20..].copy_from_slice(&compact::crc32(&raw).to_le_bytes());
        validate_recovered_full(&raw, &key).unwrap();
        let mut peer = Self::default();
        peer.slots.resize(100, None);
        peer.raws.resize(100, None);
        peer.slots[usize::from(slot)] = Some(key);
        peer.raws[usize::from(slot)] = Some(raw);
        peer
    }

    pub(super) fn reply(&mut self, p: &[u8; 32]) -> Option<Result<[u8; 32], String>> {
        let mut reply = *p;
        match p[0] {
            0xeb => {
                reply=super::resident100_tests::capability(p);
            }
            0xef => {
                assert_eq!(p[1],9);
                if p[2] == 7 { reply[2] = 0x87; reply[3] = 2; reply[8..].fill(0); return Some(Ok(reply)); }
                self.slots.resize(100,None);self.raws.resize(100,None);
                if p[2]==3 {
                    reply[2]=0x83;reply[8..].fill(0);
                    reply[3]=if let Some(key)=self.slots[usize::from(p[3])] {reply[8..].copy_from_slice(&key);0}else{1};
                    return Some(Ok(reply));
                }
                if p[2]==6 {
                    let Some(raw)=self.raws[usize::from(p[3])].as_ref() else{return Some(Err("fixture has no stored FULL payload".into()));};
                    let offset=usize::from(u16::from_le_bytes([p[8],p[9]]));
                    let n=raw.len().saturating_sub(offset).min(20);reply[2]=0x86;reply[10]=n as u8;reply[11]=0;reply[12..].fill(0);
                    if n>0 {reply[12..12+n].copy_from_slice(&raw[offset..offset+n]);}
                    return Some(Ok(reply));
                }
                let slot = usize::from(p[3]);
                let key: [u8; 24] = p[8..32].try_into().unwrap();
                let fault = self
                    .fault
                    .filter(|(op, _)| *op == p[2])
                    .map(|(_, fault)| fault);
                if fault.is_some() {
                    self.fault = None;
                }
                let accepted = match p[2] {
                    0 => self.slots[slot] == Some(key),
                    5 => self.last==Some(key) && self.ready_token==Some(u32::from_le_bytes(p[4..8].try_into().unwrap())),
                    1 => {
                        assert_eq!(Some(u32::from_le_bytes(p[4..8].try_into().unwrap())),self.ready_token);
                        assert_eq!(
                            self.last,
                            Some(key),
                            "STORE must refer to delivered decoded EE stream"
                        );
                        if !matches!(fault, Some(Fault::Rejected | Fault::Echo)) {
                            self.slots[slot] = Some(key);
                            self.raws[slot] = Some(self.last_raw.clone().expect("STORE requires actual decoded FULL"));
                            self.stores += 1;
                        }
                        true
                    }
                    2 => {
                        let hit = self.slots[slot] == Some(key);
                        if hit {
                            self.selects += 1;
                        }
                        hit
                    }
                    _ => panic!("unknown EF operation"),
                };
                reply[2] |= 0x80;
                reply[3] = if accepted {0}else{1};
                match fault {
                    Some(Fault::Lost) => {
                        return Some(Err("modeled missing ACK after delivery".into()))
                    }
                    Some(Fault::Echo) => return Some(Ok(*p)),
                    Some(Fault::Rejected) => reply[3] = 3,
                    Some(Fault::WrongNonce) => reply[4] ^= 1,
                    None => {}
                }
            }
            0xe6 => {
                reply = [0; 32];
                reply[..3].copy_from_slice(&[0xe6, 8, 0x5a]);
            }
            _ => return None,
        }
        Some(Ok(reply))
    }
}

fn rig(
    snapshot: ApplicationLayoutSnapshot,
    peer: Arc<Mutex<Peer>>,
) -> (
    QmkHidHostBridge,
    crate::hid::HidDevice,
    crate::hid::TestHidRecorder,
) {
    let target = crate::device::Device {
        path: format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR")),
        ..crate::device::test_usb_device("resident-stateful", 1, 0x00b2)
    };
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let output = peer.clone();
    recorder.on_output(move |p| output.lock().unwrap().output(p));
    recorder.respond_by(move |p| peer.lock().unwrap().reply(p));
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
            anyhow::ensure!(s.is_some(), "no native open");
            open_host_data_hid(t, s)
        },
    );
    bridge.bind_selected_application_layout_device("resident-stateful-model".into());
    bridge.set_selected_application_layout_support(true);
    bridge.set_application_layout_snapshot(snapshot);
    tx.send(()).unwrap();
    (bridge, hid, recorder)
}

#[test]
fn host_ram_switches_and_revisits_all_profiles_without_resident_selection() {
    let snapshots: Vec<_> = (0..3).map(|slot| {
        let mut s = profile(); s.profile_id = Some(format!("host-switch-{slot}")); s.resident_slot = Some(slot);
        s.layers[0][0] = 0x20 + u16::from(slot); s
    }).collect();
    let peer = seeded_peer(&[]); let before = flash(&peer);
    let (mut bridge, _hid, rec, start) = gated_ram_rig(snapshots[0].clone(), peer.clone()); start.send(()).unwrap();
    let mut expected = Vec::new();
    for (i, snapshot) in snapshots.iter().chain(snapshots.iter()).enumerate() {
        if i > 0 { bridge.set_application_layout_snapshot(snapshot.clone()); }
        let raw = fixture_raw(snapshot); expected.push(raw.clone());
        wait_for(&rec, |ps| ps.iter().filter(|p| p[0] == 0xee).count() >= i + 1 && peer.lock().unwrap().last_raw.as_ref() == Some(&raw));
    }
    let mut edited = snapshots[1].clone(); edited.layers[15][61] = 0x1234; edited.revision += 1;
    bridge.set_application_layout_snapshot(edited.clone()); expected.push(fixture_raw(&edited));
    wait_for(&rec, |ps| ps.iter().filter(|p| p[0] == 0xee).count() >= 7);
    let mut unslotted = snapshots[2].clone(); unslotted.resident_slot = None;
    bridge.set_application_layout_snapshot(unslotted.clone()); expected.push(fixture_raw(&unslotted));
    wait_for(&rec, |ps| ps.iter().filter(|p| p[0] == 0xee).count() >= 8);
    finish_bridge(&mut bridge); assert_eq!(ram_transfers(&rec.requests()), expected); assert_eq!(flash(&peer), before);
}

#[test]
fn host_archive_lost_or_invalid_enum_read_replies_retry_before_certifying() {
    // Retain the eleven former ACK fault scenarios on operations that still
    // exist: initial ENUM, chunk READ and post-READ identity confirmation.
    for (phase, fault) in [("enum", "lost"), ("enum", "echo"), ("enum", "nonce"),
        ("read", "lost"), ("read", "echo"), ("read", "status"), ("read", "nonce"),
        ("confirm", "lost"), ("confirm", "echo"), ("confirm", "status"), ("confirm", "nonce")] {
        let raw = fixture_raw(&profile()); let peer = seeded_peer(&[(0, raw.clone())]); let before = flash(&peer);
        let (hid, rec) = crate::hid::HidDevice::test_device(); let p = peer.clone();
        let injected = Arc::new(AtomicBool::new(false)); let trip = injected.clone(); let mut read_started = false;
        rec.respond_by(move |packet| {
            let mut reply = archive_reply(&p, packet);
            if packet[..3] == [0xef, 9, 6] { read_started = true; }
            let target = match phase { "enum" => packet[2] == 3 && !read_started,
                "read" => packet[2] == 6, _ => packet[2] == 3 && read_started };
            if target && !trip.swap(true, Ordering::AcqRel) {
                if fault == "lost" { return Some(Err("modeled lost read-only reply".into())); }
                let r = reply.as_mut().unwrap().as_mut().unwrap();
                match fault { "echo" => *r = *packet, "nonce" => r[4] ^= 1,
                    _ if phase == "read" => r[11] = 2, _ => r[3] = 3 }
            }
            reply
        });
        let foreground = ApplicationLayoutSnapshot::inactive(); let owner = Mutex::new(ApplicationLayoutOwnerState::default());
        let dev = HostDataHid::Shared(hid.shared_output().unwrap()); let mut bg = resident100::Background::default();
        drive_archive(&mut bg, &dev, &owner, &foreground, 0, 800);
        assert!(injected.load(Ordering::Acquire), "fault seam not reached: {phase}/{fault}");
        let state = owner.lock().unwrap(); assert!(state.resident_inventory_complete, "retry did not complete: {phase}/{fault}");
        assert_eq!(state.resident_recoveries[&0].raw, raw); assert!(state.resident_recoveries[&0].validation.is_none());
        let packets = rec.requests(); assert_host_wire(&packets); assert_eq!(flash(&peer), before);
        if phase == "read" {
            let reads: Vec<_> = packets.iter().filter(|p| p[2] == 6).collect();
            assert_eq!(reads[0][8..10], reads[1][8..10], "uncertain chunk advanced cursor");
            assert_ne!(reads[0][4..8], reads[1][4..8], "retry reused correlation nonce");
        }
        assert!(!packets.iter().any(|p| matches!(p[0], 0xec..=0xee)), "read retry staged a host draft");
    }
}

#[test]
fn host_ram_settings_lease_expiry_and_worker_restart_replay_complete_full() {
    let snapshot = profile(); let raw = fixture_raw(&snapshot); let peer = seeded_peer(&[]); let before = flash(&peer);
    let (mut bridge, hid, rec, start) = gated_ram_rig(snapshot.clone(), peer.clone()); start.send(()).unwrap();
    wait_for(&rec, |ps| ps.iter().any(|p| p[0] == 0xee));
    let (held_tx, held_rx) = std::sync::mpsc::channel(); let (release_tx, release_rx) = std::sync::mpsc::channel();
    let settings = thread::spawn(move || {
        let _events = hid.pause_application_event_polling();
        hid.with_host_output_paused_for_settings(|| { held_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(6)).unwrap(); }); hid
    });
    held_rx.recv_timeout(Duration::from_secs(2)).unwrap(); let cut = rec.requests().len();
    thread::sleep(Duration::from_millis(2700));
    assert!(!rec.requests()[cut..].iter().any(|p| matches!(p[0], 0xec..=0xef)), "settings lease leaked profile output");
    release_tx.send(()).unwrap(); let _hid = settings.join().unwrap();
    wait_for(&rec, |ps| ps.iter().filter(|p| p[0] == 0xee).count() >= 2);
    finish_bridge(&mut bridge); let first = rec.requests(); assert_eq!(ram_transfers(&first), vec![raw.clone(), raw.clone()]);
    let (mut successor, _hid2, rec2, start2) = gated_ram_rig(snapshot, peer.clone()); start2.send(()).unwrap();
    wait_for(&rec2, |ps| ps.iter().any(|p| p[0] == 0xee)); finish_bridge(&mut successor);
    assert_eq!(ram_transfers(&rec2.requests()), vec![raw]); assert_eq!(flash(&peer), before);
}

#[test]
fn host_archive_late_read_is_fenced_by_publication_stop_and_output_owner() {
    for boundary in 0..3 {
        let peer = seeded_peer(&[(0, fixture_raw(&profile()))]); let before = flash(&peer);
        let foreground = ApplicationLayoutSnapshot::inactive();
        let owner = Arc::new(Mutex::new(ApplicationLayoutOwnerState::default()));
        let stop = Arc::new(AtomicBool::new(false)); let (hid, rec) = crate::hid::HidDevice::test_device();
        let output = hid.shared_output().unwrap().for_host_bridge(Default::default(), true);
        let dev = HostDataHid::Shared(output.clone()); let p = peer.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel(); let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut held = false;
        rec.respond_by(move |packet| {
            if packet[..3] == [0xef, 9, 6] && !held { held = true; entered_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap(); }
            archive_reply(&p, packet)
        });
        let mut bg = resident100::Background::default();
        bg.tick(&dev, &owner, &foreground, 0, &stop, &mut 0); // occupied ENUM
        let state = owner.clone(); let stopping = stop.clone(); let fg = foreground.clone();
        let worker = thread::spawn(move || {
            bg.tick(&dev, &state, &fg, 0, &stopping, &mut 0);
            (bg, dev)
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let successor_output = if boundary == 2 { Some(hid.shared_output().unwrap().for_host_bridge(Default::default(), true)) } else { None };
        if boundary == 0 { owner.lock().unwrap().snapshot.revision += 1; }
        if boundary == 1 { stop.store(true, Ordering::Release); }
        release_tx.send(()).unwrap(); let (mut bg, dev) = worker.join().unwrap();
        assert!(owner.lock().unwrap().resident_recoveries.is_empty(), "late READ published recovery");
        let cut = rec.requests().len();
        for _ in 0..3 { bg.tick(&dev, &owner, &foreground, 0, &stop, &mut 0); }
        assert_eq!(rec.requests().len(), cut, "revoked archive owner kept reading");
        let state = owner.lock().unwrap(); assert!(!state.resident_inventory_complete);
        // Three revoked-output errors may retain a diagnostic partial record,
        // never validated proof or bytes from the fenced late reply.
        assert!(state.resident_recoveries.values().all(|r| r.validation.is_some() && r.raw.is_empty())); drop(state);
        if let Some(output) = successor_output { assert!(output.host_session_is_current()); }
        assert_eq!(flash(&peer), before); assert_host_wire(&rec.requests());
    }
}

#[test]
fn host_ram_expired_midstream_restarts_before_commit_with_fresh_transaction() {
    // An archive READ is read-only and has no former STORE ACK deadline. The
    // retained 2600ms fault belongs to the actual 2500ms RAM staging lease.
    let snapshot = profile(); let raw = fixture_raw(&snapshot); let peer = seeded_peer(&[]); let before = flash(&peer);
    let (mut bridge, _hid, rec, start) = gated_ram_rig(snapshot, peer.clone());
    let p = peer.clone(); let mut held = false;
    rec.on_output(move |packet| {
        p.lock().unwrap().output(packet);
        if packet[0] == 0xed && !held { held = true; thread::sleep(Duration::from_millis(2600)); }
    });
    start.send(()).unwrap(); wait_for(&rec, |ps| ps.iter().any(|p| p[0] == 0xee)); finish_bridge(&mut bridge);
    let ps = rec.requests(); let begins: Vec<_> = ps.iter().enumerate().filter(|(_, p)| p[0] == 0xec).map(|(i, _)| i).collect();
    assert_eq!(begins.len(), 2, "expired RAM transaction was not restarted");
    assert!(!ps[begins[0]..begins[1]].iter().any(|p| p[0] == 0xee), "expired COMMIT reached firmware");
    assert_ne!(ps[begins[0]][4..8], ps[begins[1]][4..8]); assert_eq!(ram_transfers(&ps), vec![raw]); assert_eq!(flash(&peer), before);
}

#[test]
fn host_ram_transaction_tokens_are_unique_across_worker_replacements() {
    let snapshot = profile(); let raw = fixture_raw(&snapshot); let peer = seeded_peer(&[]); let before = flash(&peer);
    let (mut first, _hid, rec, start) = gated_ram_rig(snapshot.clone(), peer.clone()); start.send(()).unwrap();
    wait_for(&rec, |ps| ps.iter().any(|p| p[0] == 0xee)); finish_bridge(&mut first);
    let (mut second, _hid2, rec2, start2) = gated_ram_rig(snapshot, peer.clone()); start2.send(()).unwrap();
    wait_for(&rec2, |ps| ps.iter().any(|p| p[0] == 0xee)); finish_bridge(&mut second);
    let first = rec.requests(); let second = rec2.requests();
    assert_eq!(ram_transfers(&first), vec![raw.clone()]); assert_eq!(ram_transfers(&second), vec![raw]);
    let a = first.iter().find(|p| p[0] == 0xec).unwrap(); let b = second.iter().find(|p| p[0] == 0xec).unwrap();
    assert_ne!(a[4..8], b[4..8], "RAM transaction namespace restarted with worker");
    assert_eq!(a[16..20], b[16..20], "control must be identical content/CRC"); assert_eq!(flash(&peer), before);
}

#[path = "resident_native_regression.rs"]
mod native;
