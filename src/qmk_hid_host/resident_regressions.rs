use super::*;
use crate::application_layouts::{ApplicationLayoutSnapshot, DeviceApplicationLayouts};

#[test]
fn p2_post_store_readback_rejects_stale_owner_library_and_corruption() {
    for fault in ["owner", "library", "corrupt"] {
        let foreground=ApplicationLayoutSnapshot::inactive();
        let owner=Arc::new(Mutex::new(ApplicationLayoutOwnerState {
            snapshot:foreground.clone(), library:Arc::new(vec![profile()]), ..Default::default()
        }));
        let peer=Arc::new(Mutex::new(Peer::default()));
        let (hid,rec)=crate::hid::HidDevice::test_device();
        let p=peer.clone();rec.on_output(move|bytes|p.lock().unwrap().output(bytes));
        let p=peer.clone();let state=owner.clone();let tripped=Arc::new(AtomicBool::new(false));let trip=tripped.clone();
        rec.respond_by(move|packet| {
            let mut reply=p.lock().unwrap().reply(packet);
            if packet[..3]==[0xef,9,6] {
                trip.store(true,Ordering::Release);
                if fault=="corrupt" {
                    if let Some(Ok(r))=&mut reply {if r[10]>0 {r[12]^=1;}}
                } else {
                    let mut s=state.lock().unwrap();
                    if fault=="owner" {s.generation+=1;} else {s.library_generation+=1;}
                }
            }
            reply
        });
        let dev=HostDataHid::Shared(hid.shared_output().unwrap());
        let mut background=resident100::Background::default();let mut transaction=0;
        for _ in 0..800 {
            background.tick(&dev,&owner,&foreground,0,&AtomicBool::new(false),&mut transaction);
            if tripped.load(Ordering::Acquire) && fault!="corrupt" {break;}
        }
        assert!(tripped.load(Ordering::Acquire),"post-STORE verified READ was never scheduled: {fault}");
        let state=owner.lock().unwrap();
        assert!(!state.resident_inventory_complete,"invalid READ certified export: {fault}");
        if fault=="corrupt" {
            assert!(state.resident_recoveries.values().any(|r|r.validation.is_some()),"corrupt FULL must retain an explicit validation error");
        } else {assert!(state.resident_recoveries.is_empty(),"stale completion published recovered proof: {fault}");}
        assert_eq!(peer.lock().unwrap().stores,1,"readback must not automatically retry STORE");
    }
}


fn profile() -> ApplicationLayoutSnapshot {
    let mut layout = DeviceApplicationLayouts::default().layouts["default"].clone();
    layout.layers = vec![vec![0x22; 62]; 16];
    layout.id="resident-application".into();
    let mut snapshot=ApplicationLayoutSnapshot::from_layout(&layout);snapshot.resident_slot=Some(0);snapshot
}

pub(super) fn wait_for(recorder: &crate::hid::TestHidRecorder, predicate: impl Fn(&[[u8; 32]]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
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
fn resident_three_worker_miss_store_select_then_cache_hit() {
    // Historical test name retained; protocol is now EF09, Default is external.
    let peer=Arc::new(Mutex::new(Peer::default()));
    let (mut bridge,_hid,recorder)=rig(profile(),peer.clone());
    wait_for(&recorder,|_|peer.lock().unwrap().selects==1);
    bridge.force_application_layout_resend();
    wait_for(&recorder,|_|peer.lock().unwrap().selects==2);bridge.stop();
    let ps=recorder.requests();assert_eq!(peer.lock().unwrap().stores,1);
    assert_eq!(ps.iter().filter(|p|p[0]==0xec).count(),1);
    assert_eq!(ps.iter().filter(|p|p[0]==0xef && !matches!(p[2],3|6)).map(|p|p[2]).collect::<Vec<_>>(),[0,5,1,0,2,0,2]);
    assert!(ps.iter().any(|p|p[..3]==[0xec,8,2]));
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

    fn reply(&mut self, p: &[u8; 32]) -> Option<Result<[u8; 32], String>> {
        let mut reply = *p;
        match p[0] {
            0xeb => {
                reply=super::resident100_tests::capability(p);
            }
            0xef => {
                assert_eq!(p[1],9);
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
fn resident_three_switches_all_slots_without_storing_and_nonresident_falls_back() {
    let mut settings = DeviceApplicationLayouts::default();
    settings.layouts.get_mut("default").unwrap().layers = vec![vec![0x22; 62]; 16];
    settings.provision_builtin_presets(None);
    let firefox = settings
        .layouts
        .values()
        .find(|layout| layout.executable == "firefox")
        .unwrap()
        .id
        .clone();
    let user = settings.create_for_application(&crate::application_layouts::DetectedApplication {
        executable: "arbitrary".into(),
        ..Default::default()
    });
    let extra = settings.create_for_application(&crate::application_layouts::DetectedApplication {
        executable: "extra".into(),
        ..Default::default()
    });
    let snapshots: Vec<_> = [firefox.as_str(), user.as_str(), extra.as_str()]
        .into_iter()
        .map(|id| settings.snapshot_for_layout(&settings.layouts[id]))
        .collect();
    let peer = Arc::new(Mutex::new(Peer::default()));
    let (mut bridge, _hid, recorder) = rig(snapshots[0].clone(), peer.clone());
    for (n, snapshot) in snapshots.iter().enumerate() {
        if n > 0 {
            bridge.set_application_layout_snapshot(snapshot.clone());
        }
        wait_for(&recorder, |_| peer.lock().unwrap().selects >= n + 1);
    }
    let before = recorder.requests().len();
    for (n, snapshot) in snapshots.iter().enumerate() {
        bridge.set_application_layout_snapshot(snapshot.clone());
        wait_for(&recorder, |_| peer.lock().unwrap().selects >= n + 4);
    }
    let all = recorder.requests();
    assert_eq!(peer.lock().unwrap().stores, 3);
    let operations: Vec<_> = all[before..]
        .iter()
        .filter(|p| p[0] == 0xef && !matches!(p[2],3|6))
        .map(|p| (p[2], p[3]))
        .collect();
    assert_eq!(operations, snapshots.iter().flat_map(|s|[(0,s.resident_slot.unwrap()),(2,s.resident_slot.unwrap())]).collect::<Vec<_>>());
    assert!(!all[before..].iter().any(|p| matches!(p[0], 0xec..=0xee)));
    // Only changed content causes another STORE.
    settings
        .layouts
        .get_mut(&user)
        .unwrap()
        .set_keycode(15, 61, 0x1234);
    bridge.set_application_layout_snapshot(settings.snapshot_for_layout(&settings.layouts[&user]));
    wait_for(&recorder, |_| {
        let peer = peer.lock().unwrap();
        peer.stores == 4 && peer.selects >= 7
    });
    let before = recorder.requests().len();
    let mut fallback=settings.snapshot_for_layout(&settings.layouts[&extra]);fallback.resident_slot=None;
    bridge.set_application_layout_snapshot(fallback);
    wait_for(&recorder, |ps| ps[before..].iter().any(|p| p[0] == 0xee));
    bridge.stop();
    assert!(!recorder.requests()[before..].iter().any(|p| p[0] == 0xef && !matches!(p[2],3|6)));
    assert_eq!(peer.lock().unwrap().stores, 4);
}

#[test]
fn resident_three_lost_or_invalid_store_select_ack_requires_query_recovery() {
    for (operation, fault) in [
        (0, Fault::Lost),
        (0, Fault::Echo),
        (0, Fault::WrongNonce),
        (1, Fault::Lost),
        (1, Fault::Echo),
        (1, Fault::Rejected),
        (1, Fault::WrongNonce),
        (2, Fault::Lost),
        (2, Fault::Echo),
        (2, Fault::Rejected),
        (2, Fault::WrongNonce),
    ] {
        let peer = Arc::new(Mutex::new(Peer {
            fault: Some((operation, fault)),
            ..Default::default()
        }));
        let (mut bridge, _hid, recorder) = rig(profile(), peer.clone());
        wait_for(&recorder, |ps| {
            ps.iter().filter(|p| p[..3] == [0xef, 9, 0]).count() >= 2
                && ps.iter().any(|p| p[..3] == [0xef, 9, 2])
        });
        // Wait for a healthy confirmed SELECT followed by a normal event poll.
        wait_for(&recorder, |ps| ps.iter().any(|p| p[0] == 0xe6));
        bridge.stop();
        let all = recorder.requests();
        let failed = all
            .iter()
            .position(|p| p[..3] == [0xef, 9, operation])
            .unwrap();
        let next = all[failed + 1..].iter().find(|p| p[0] == 0xef).unwrap();
        assert_eq!(next[2], 0, "uncertain ACK must not advance transaction");
        assert_ne!(next[4..8], all[failed][4..8], "fresh query nonce");
        assert_eq!(
            peer.lock().unwrap().stores,
            1,
            "lost STORE ACK must not rewrite flash"
        );
    }
}

#[test]
fn resident_three_settings_lease_recovery_and_worker_restart_use_query_select_only() {
    let peer = Arc::new(Mutex::new(Peer::default()));
    let (mut bridge, hid, recorder) = rig(profile(), peer.clone());
    wait_for(&recorder, |_| peer.lock().unwrap().selects == 1);
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let settings = thread::spawn(move || {
        let _events = hid.pause_application_event_polling();
        hid.with_host_output_paused_for_settings(|| {
            held_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(6)).unwrap();
        });
        hid
    });
    held_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let before = recorder.requests().len();
    thread::sleep(Duration::from_millis(2700));
    assert!(!recorder.requests()[before..].iter().any(|p| p[0] == 0xef && !matches!(p[2],3|6)));
    release_tx.send(()).unwrap();
    let _hid = settings.join().unwrap();
    wait_for(&recorder, |_| peer.lock().unwrap().selects >= 2);
    bridge.stop();
    assert_eq!(peer.lock().unwrap().stores, 1);
    let recovery: Vec<_> = recorder.requests()[before..]
        .iter()
        .filter(|p| p[0] == 0xef && !matches!(p[2],3|6))
        .map(|p| p[2])
        .collect();
    assert_eq!(recovery, [0, 2]);
    // New worker, same persistent device slots: capability and cache re-query,
    // not a second upload or an assumed host-side cache hit.
    let (mut successor, _hid2, recorder2) = rig(profile(), peer.clone());
    wait_for(&recorder2, |_| peer.lock().unwrap().selects >= 3);
    successor.stop();
    let ps = recorder2.requests();
    let old = recorder.requests();
    let old_nonce = &old.iter().find(|p| p[..3] == [0xef, 9, 0]).unwrap()[4..8];
    let new_nonce = &ps.iter().find(|p| p[..3] == [0xef, 9, 0]).unwrap()[4..8];
    assert_ne!(
        old_nonce, new_nonce,
        "worker replacement must not reuse an EF nonce for the same profile/CRC"
    );
    assert_eq!(
        ps.iter()
            .filter(|p| p[0] == 0xef && !matches!(p[2],3|6))
            .map(|p| p[2])
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert!(!ps.iter().any(|p| matches!(p[0], 0xec..=0xee)));
    assert_eq!(peer.lock().unwrap().stores, 1);
}

#[test]
fn resident_three_late_query_is_fenced_by_publication_stop_and_output_owner() {
    for boundary in 0..3 {
        let peer = Arc::new(Mutex::new(Peer::default()));
        let (mut bridge, hid, recorder) = rig(profile(), peer.clone());
        wait_for(&recorder, |_| peer.lock().unwrap().selects == 1);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let responding = peer.clone();
        let mut hold = true;
        recorder.respond_by(move |p| {
            if p[..3] == [0xef, 9, 0] && hold {
                hold = false;
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            responding.lock().unwrap().reply(p)
        });
        let before = recorder.requests().len();
        bridge.force_application_layout_resend();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let successor_lease;
        if boundary == 0 {
            let mut successor = profile();
            successor.layers[15][61] = 0x1234;
            successor.revision += 1;
            bridge.clear_application_layout_binding();
            bridge.bind_selected_application_layout_device("resident-stateful-model".into());
            bridge.set_application_layout_snapshot(successor);
        } else if boundary == 1 {
            bridge.stop();
        } else {
            // Claiming this same serialized output revokes the old lease.
            successor_lease = hid
                .shared_output()
                .unwrap()
                .for_host_bridge(Default::default(), true);
            assert!(successor_lease.host_session_is_current());
        }
        release_tx.send(()).unwrap();
        if boundary == 0 {
            // A renewed endpoint grant does not authorize the old host draft.
            // Recover the physical baseline through the actual owner first.
            wait_for(&recorder, |_| bridge.resident_recoveries().iter().any(|r|
                r.slot == 0 && r.validation.is_none()));
            let record = bridge.resident_recoveries().into_iter()
                .find(|r| r.slot == 0 && r.validation.is_none()).unwrap();
            {
                let physical = peer.lock().unwrap();
                assert_eq!(record.raw, *physical.raws[0].as_ref().unwrap());
                assert_eq!(record.identity_crc, physical.slots[0].unwrap());
                assert_eq!(physical.stores, 1);
                assert_eq!(physical.selects, 1);
            }
            assert!(!recorder.requests()[before..].iter().any(|p|
                matches!(p[0], 0xec..=0xee)
                    || (p[0] == 0xef && matches!(p[2], 1 | 2 | 4 | 5))),
                "revoked draft published before physical adoption and explicit edit");
            let mut edited = decode_device_profile(&record).unwrap();
            edited.id = profile().profile_id.unwrap();
            edited.layers[15][61] = 0x1234;
            edited.revision += 1;
            let mut successor = ApplicationLayoutSnapshot::from_layout(&edited);
            successor.resident_slot = Some(record.slot);
            attach_device_profile(&mut successor, &edited, &record.raw);
            assert_eq!(successor.device_baseline_crc,
                Some(record.identity_crc[20..].try_into().unwrap()));
            assert!(successor.device_raw.is_none(), "edited layout reused stale FULL");
            bridge.set_application_layout_snapshot(successor);
            wait_for(&recorder, |_| {
                let physical = peer.lock().unwrap();
                physical.stores == 2 && physical.selects == 2
            });
            assert_ne!(peer.lock().unwrap().raws[0].as_ref().unwrap(), &record.raw);
        } else {
            thread::sleep(Duration::from_millis(150));
        }
        bridge.stop();
        let ps = recorder.requests();
        let stale_query = ps[before..]
            .iter()
            .position(|p| p[..3] == [0xef, 9, 0])
            .unwrap()
            + before;
        let next = ps[stale_query + 1..].iter().find(|p| p[0] == 0xef);
        if boundary == 0 {
            assert_eq!(
                next.unwrap()[2],
                3,
                "revoked owner must resume with fresh read-first ENUM, not stale SELECT"
            );
        } else {
            assert!(next.is_none(), "revoked/stopped output sent EF");
        }
    }
}

#[test]
fn resident_three_expired_query_reply_requeries_without_store() {
    let peer = Arc::new(Mutex::new(Peer::default()));
    let (mut bridge, _hid, recorder) = rig(profile(), peer.clone());
    wait_for(&recorder, |_| peer.lock().unwrap().selects == 1);
    let responding = peer.clone();
    let mut hold = true;
    recorder.respond_by(move |p| {
        if p[..3] == [0xef, 9, 0] && hold {
            hold = false;
            thread::sleep(Duration::from_millis(2600));
        }
        responding.lock().unwrap().reply(p)
    });
    let before = recorder.requests().len();
    bridge.force_application_layout_resend();
    wait_for(&recorder, |_| peer.lock().unwrap().selects == 2);
    bridge.stop();
    let operations: Vec<_> = recorder.requests()[before..]
        .iter()
        .filter(|p| p[0] == 0xef && !matches!(p[2],3|6))
        .map(|p| p[2])
        .collect();
    assert_eq!(
        operations,
        [0, 0, 2],
        "over-deadline ACK was treated as applied"
    );
    assert_eq!(peer.lock().unwrap().stores, 1);
}

#[test]
fn resident100_stage_tokens_are_unique_across_worker_replacements() {
    let peer=Arc::new(Mutex::new(Peer::default()));let (mut first,_hid,rec)=rig(profile(),peer.clone());wait_for(&rec,|_|peer.lock().unwrap().selects==1);first.stop();
    peer.lock().unwrap().slots.clear();
    let (mut second,_hid,rec2)=rig(profile(),peer.clone());wait_for(&rec2,|_|peer.lock().unwrap().selects==2);second.stop();
    let p1=rec.requests().into_iter().find(|p|p[0]==0xec).unwrap();let p2=rec2.requests().into_iter().find(|p|p[0]==0xec).unwrap();assert_ne!(p1[4..8],p2[4..8],"stage-token namespace cannot restart with worker");
}

#[path = "resident_native_regression.rs"]
mod native;
