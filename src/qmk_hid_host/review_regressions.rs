use super::*;
use std::cell::RefCell;

// Per-worker OS/HID boundary scripts; no global clock changes or copied
// recovery logic. Unscripted tests and production use the native boundaries.
thread_local! {
    static FAILURE_TIME: RefCell<Option<Box<dyn Fn() -> Instant>>> = RefCell::new(None);
    static OUTPUT_SCRIPT: RefCell<Option<Box<dyn FnMut(&[u8]) -> anyhow::Result<()>>>> = RefCell::new(None);
}

pub(super) fn failure_now(native: Instant) -> Instant {
    FAILURE_TIME.with(|clock| clock.borrow().as_ref().map_or(native, |clock| clock()))
}

pub(super) fn scripted_write(payload: &[u8]) -> Option<anyhow::Result<()>> {
    OUTPUT_SCRIPT.with(|script| script.borrow_mut().as_mut().map(|script| script(payload)))
}

#[test]
fn stale_replay_event_is_acked_without_feedback_and_next_event_drains() {
    for (version, controls, shared) in [(8,62,true),(8,62,false),(7,15,true)] {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut profile = crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
        profile.layers = vec![vec![4;controls];16];profile.revision = 42;
        let snapshot = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&profile);
        let owner = Mutex::new(ApplicationLayoutOwnerState {snapshot:snapshot.clone(),..Default::default()});
        let mut events = std::collections::VecDeque::from([(100u16,41u32),(101,42)]);
        recorder.respond_by(move |request| {
            if request[0] != 0xe6 {return None;}
            let ack = u16::from_le_bytes([request[3],request[4]]);
            if events.front().is_some_and(|(sequence,_)| *sequence == ack) {events.pop_front();}
            let mut response = [0;32];response[..3].copy_from_slice(&[0xe6,version,0x5a]);
            if let Some((sequence,revision)) = events.front() {
                response[3]=1;response[4..6].copy_from_slice(&sequence.to_le_bytes());
                response[6]=15;response[7]=(controls-1) as u8;
                response[8..12].copy_from_slice(&revision.to_le_bytes());
            }
            Some(Ok(response))
        });
        let device = if shared {
            HostDataHid::Shared(hid.shared_output().unwrap().for_host_bridge(Default::default(),true))
        } else {HostDataHid::Dedicated(hid)};
        let mut poll = ApplicationLayoutEventPoll::default();
        poll.poll(&device,&owner,&snapshot,0,version).unwrap();
        assert_eq!(poll.acknowledged,100,"well-formed stale head was not discarded");
        assert!(!recorder.requests().iter().any(|p|p[0]==0xe8),"stale replay generated feedback");
        poll.poll(&device,&owner,&snapshot,0,version).unwrap();
        assert_eq!(poll.acknowledged,101);
        poll.poll(&device,&owner,&snapshot,0,version).unwrap();
        let requests = recorder.requests();
        let acks: Vec<_> = requests.iter().filter(|p|p[0]==0xe6).map(|p|u16::from_le_bytes([p[3],p[4]])).collect();
        assert_eq!(acks,vec![0,100,101]);
        let states: Vec<_> = requests.iter().filter(|p|p[0]==0xe8).collect();
        assert_eq!(states.len(),1);
        assert_eq!(&states[0][2..6],&42u32.to_le_bytes());
        assert_eq!(states[0][8],0);
    }
}

#[test]
fn stale_replay_reply_after_owner_revocation_is_not_acked() {
    let (hid,recorder)=crate::hid::HidDevice::test_device();
    let mut profile=crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
    profile.layers=vec![vec![4;62];16];profile.revision=42;
    let snapshot=crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&profile);
    let owner=Arc::new(Mutex::new(ApplicationLayoutOwnerState {snapshot:snapshot.clone(),..Default::default()}));
    let retired=owner.clone();
    recorder.respond_by(move |request| {
        if request[0]!=0xe6 {return None;}
        retired.lock().unwrap().revoke();
        let mut response=[0;32];response[..8].copy_from_slice(&[0xe6,8,0x5a,1,100,0,15,61]);
        response[8..12].copy_from_slice(&41u32.to_le_bytes());Some(Ok(response))
    });
    let device=HostDataHid::Shared(hid.shared_output().unwrap().for_host_bridge(Default::default(),true));
    let mut poll=ApplicationLayoutEventPoll::default();
    poll.poll(&device,&owner,&snapshot,0,8).unwrap();
    assert_eq!(poll.acknowledged,0);
    assert!(!recorder.requests().iter().any(|p|p[0]==0xe8));
}

#[test]
fn sm62_poll_rejects_wrong_version_revision_geometry_and_borrowed_owner() {
    let (hid,recorder)=crate::hid::HidDevice::test_device();
    let other=crate::hid::HidDevice::test_device().1;
    let mut profile=crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
    profile.layers=vec![vec![4;62];16];profile.revision=42;
    let snapshot=crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&profile);
    let owner=Mutex::new(ApplicationLayoutOwnerState { snapshot:snapshot.clone(),..Default::default() });
    let output=hid.shared_output().unwrap().for_host_bridge(Default::default(),true);
    let device=HostDataHid::Shared(output);
    let mut poll=ApplicationLayoutEventPoll::default();
    for (version,revision,layer,control,sequence) in [(7,42,15,61,1),(9,42,15,61,1),(8,41,15,61,1),(8,42,16,61,1),(8,42,15,62,1),(8,42,15,61,0)] {
        let mut poll=ApplicationLayoutEventPoll::default();
        recorder.respond_by(move |_| {
            let mut reply=[0;32];reply[..8].copy_from_slice(&[0xe6,version,0x5a,1,sequence,0,layer,control]);
            reply[8..12].copy_from_slice(&(revision as u32).to_le_bytes());Some(Ok(reply))
        });
        poll.poll(&device,&owner,&snapshot,0,8).unwrap();
        let stale_but_valid = version==8 && revision!=42 && layer<16 && control<62 && sequence!=0;
        assert_eq!(poll.acknowledged,if stale_but_valid {u16::from(sequence)} else {0});
    }
    assert!(!recorder.requests().iter().any(|r|r[0]==0xe8));
    let before=recorder.requests().len();
    hid.with_host_output_paused_for_settings(|| {
        poll.poll(&device,&owner,&snapshot,0,8).unwrap();
    });
    assert_eq!(recorder.requests().len(),before,"borrowed serialized HID owner was polled");
    owner.lock().unwrap().revoke();
    poll.poll(&device,&owner,&snapshot,0,8).unwrap();
    assert_eq!(recorder.requests().len(),before,"revoked owner was polled");
    assert!(other.requests().is_empty(),"event path crossed device ownership");
}

#[test]
fn sm62_poll_late_reply_is_discarded_after_selection_change_or_stop() {
    for stop in [false,true] {
        let mut target=incident_target();target.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
        let (hid,recorder)=crate::hid::HidDevice::test_device();
        let (entered_tx,entered_rx)=std::sync::mpsc::channel();
        let (release_tx,release_rx)=std::sync::mpsc::channel();
        let (done_tx,done_rx)=std::sync::mpsc::channel();
        let mut held=false;
        recorder.respond_by(move |r| {
            if r[0] == 0xeb { return Some(Ok(*r)); }
            if r[0]!=0xe6 {return None;}
            if !held { held=true;entered_tx.send(()).unwrap();release_rx.recv_timeout(Duration::from_secs(4)).unwrap();done_tx.send(()).unwrap(); }
            let mut reply=[0;32];reply[..8].copy_from_slice(&[0xe6,8,0x5a,1,1,0,15,61]);
            reply[8..12].copy_from_slice(&42u32.to_le_bytes());Some(Ok(reply))
        });
        let mut bridge=test_start_bridge(target,Default::default(),hid.shared_output(),None,HostProtocol::Selected(true),||None);
        bridge.set_selected_application_layout_support(true);bridge.bind_selected_application_layout_device("old".into());
        let mut profile=crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
        profile.layers=vec![vec![4;62];16];profile.revision=42;
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&profile));
        entered_rx.recv_timeout(Duration::from_secs(4)).unwrap();
        if stop {bridge.suppress_shutdown();bridge.stop();}
        else { bridge.bind_selected_application_layout_device("new".into()); }
        release_tx.send(()).unwrap();done_rx.recv_timeout(Duration::from_secs(4)).unwrap();
        // Wait for the worker to cross the post-response fence, not merely return from HID.
        thread::sleep(Duration::from_millis(150));
        assert!(!recorder.requests().iter().any(|r|r[0]==0xe8),"late response mutated execution state after stop={stop}");
        bridge.suppress_shutdown();bridge.stop();
    }
}

#[test]
fn sm62_and_macropad_bridge_poll_native_events_across_every_control() {
    for (version, controls, dedicated) in [(7,15,false),(8,62,false),(7,15,true),(8,62,true)] {
        let mut target=incident_target();
        target.product_id=if version==8 {0x00b2} else {0x0042};
        target.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
        let (hid,recorder)=crate::hid::HidDevice::test_device();
        let (other,other_recorder)=crate::hid::HidDevice::test_device();
        let mut next=0usize;
        recorder.respond_by(move |r| {
            if r[..2]==[0xfe,0] {
                let mut reply=[0;32];reply[..4].copy_from_slice(&6u32.to_le_bytes());
                reply[4..12].copy_from_slice(&42u64.to_le_bytes());return Some(Ok(reply));
            }
            if r[0] == 0xeb {
                // Model an explicit legacy rejection; zero/unhandled synthetic
                // replies are capability UNKNOWN and cannot authorize a map.
                return Some(Ok(*r));
            }
            if r[0]!=0xe6 { return None; }
            assert_eq!(r[1],version);
            let ack=u16::from_le_bytes([r[3],r[4]]);
            if ack!=0 { assert_eq!(ack as usize,next+1); next+=1; }
            let mut reply=[0;32];reply[..3].copy_from_slice(&[0xe6,version,0x5a]);
            if next<controls { reply[3]=1;reply[4..6].copy_from_slice(&((next+1) as u16).to_le_bytes());
                reply[6]=15;reply[7]=next as u8;reply[8..12].copy_from_slice(&42u32.to_le_bytes()); }
            Some(Ok(reply))
        });
        let (shared,owned)=if dedicated {(None,Some(hid))} else {(hid.shared_output(),None)};
        let mut bridge=test_start_bridge(target,Default::default(),shared,owned,HostProtocol::Selected(true),||None);
        if dedicated {
            let deadline=Instant::now()+Duration::from_secs(4);
            while bridge.discovered_application_layout_key().is_none() {
                assert!(Instant::now()<deadline);thread::sleep(Duration::from_millis(5));
            }
            // Discovery is published before the capability response; wait until
            // the worker has established support before the selected grant.
            while !bridge.supports_application_layouts() {
                assert!(Instant::now()<deadline);thread::sleep(Duration::from_millis(5));
            }
        }
        bridge.set_selected_application_layout_support(true);
        bridge.bind_selected_application_layout_device("poll-target".into());
        let mut layout=crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
        layout.layers=vec![vec![4;controls];16];layout.revision=42;
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&layout));
        let deadline=Instant::now()+Duration::from_secs(12);
        while recorder.requests().iter().filter(|r|r[0]==0xe8).count()<controls {
            assert!(Instant::now()<deadline,"native event polling never completed version {version}");
            thread::sleep(Duration::from_millis(10));
        }
        let packets=recorder.requests();
        let states:Vec<_>=packets.iter().filter(|r|r[0]==0xe8).collect();
        for control in 0..controls { assert!(states.iter().any(|r|r[1]==version && r[2..6]==42u32.to_le_bytes() && r[6]==15 && r[7]==control as u8)); }
        assert!(other.shared_output().unwrap().is_available());
        assert!(other_recorder.requests().is_empty());
        assert!(states.iter().all(|r| r[8]==0),"native dispatch must not invent OS completion");
        bridge.suppress_shutdown();bridge.stop();
    }
}

fn incident_target() -> crate::device::Device {
    crate::device::Device {
        name: "SM62 incident fixture".into(),
        vendor_id: SM62_VENDOR_ID,
        product_id: SM62_PRODUCT_ID,
        manufacturer: String::new(),
        serial_number: "incident fixture".into(),
        bus_type: "USB".into(),
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/qmk_hid_host.rs")
            .to_string_lossy()
            .into_owned(),
        instance_token: "incident-owner".into(),
        firmware: crate::firmware::FirmwareProtocol::Vial,
    }
}

// SM62 owners now perform the same read-only identity/capability discovery as
// Macropad owners. Keep the original output assertions exact after proving the
// complete discovery prefix; do not hide arbitrary transport requests.
fn after_sm62_discovery(recorder: &crate::hid::TestHidRecorder, cold: bool) -> Vec<[u8;32]> {
    let requests=recorder.requests();
    let mut probe=[0;32];probe[..5].copy_from_slice(&[0xe6,8,0xa5,0,0]);
    let mut expected=Vec::new();
    if cold { let mut uid=[0;32];uid[..2].copy_from_slice(&[0xfe,0]); expected.push(uid); }
    expected.push(probe);
    assert!(requests.len()>=expected.len(),"missing SM62 discovery: {requests:?}");
    assert_eq!(requests[..expected.len()],expected);
    requests[expected.len()..].to_vec()
}

#[test]
fn real_worker_grants_second_transient_incident_its_own_grace() {
    let desktop = HostDataService {
        owner: Arc::new(DesktopOwner {
            shared: Arc::new(DesktopShared::default()),
        }),
    };
    desktop.owner.shared.state.lock().unwrap().snapshot.volume = Some(50);
    let cache = desktop.clone();
    let t = Instant::now();
    let logical_seconds = Arc::new(AtomicU64::new(0));
    let clock_seconds = logical_seconds.clone();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let script_events = event_tx.clone();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let mut initial_hid = Some(hid);
    let mut bridge = QmkHidHostBridge::start_with_desktop_service(
        incident_target(),
        HostDataMode {
            volume: true,
            ..Default::default()
        },
        None,
        HostProtocol::Selected(false),
        desktop,
        move |_, _| {
            let Some(hid) = initial_hid.take() else {
                event_tx.send("reopened").unwrap();
                anyhow::bail!("scripted reconnect refused");
            };
            let clock_seconds = clock_seconds.clone();
            FAILURE_TIME.with(|clock| {
                *clock.borrow_mut() = Some(Box::new(move || {
                    t + Duration::from_secs(clock_seconds.load(Ordering::SeqCst))
                }))
            });
            let logical_seconds = logical_seconds.clone();
            let cache = cache.clone();
            let script_events = script_events.clone();
            let mut attempts = [0; 2];
            OUTPUT_SCRIPT.with(|script| {
                *script.borrow_mut() = Some(Box::new(move |packet| {
                    assert_eq!(packet[0], DATA_VOLUME);
                    let index = usize::from(packet[1] == 51);
                    attempts[index] += 1;
                    if index == 1 {
                        logical_seconds.store(6, Ordering::SeqCst);
                    }
                    if attempts[index] <= 2 {
                        anyhow::bail!("scripted transient output busy");
                    }
                    if index == 0 {
                        cache
                            .owner
                            .shared
                            .store_for_test(|snapshot| snapshot.volume = Some(51));
                    } else {
                        script_events.send("second recovered").unwrap();
                    }
                    Ok(())
                }))
            });
            Ok(HostDataHid::Dedicated(hid))
        },
    );
    let outcome = event_rx.recv_timeout(Duration::from_secs(5));
    bridge.suppress_shutdown();
    let worker = bridge.thread.take().unwrap();
    bridge.stop();
    worker.join().unwrap();
    assert_eq!(
        outcome.unwrap(),
        "second recovered",
        "second incident inherited the first grace timestamp"
    );
    assert_eq!(
        after_sm62_discovery(&recorder,true)
            .iter()
            .map(|p| p[..2].to_vec())
            .collect::<Vec<_>>(),
        vec![vec![DATA_VOLUME, 50], vec![DATA_VOLUME, 51]]
    );
}

#[test]
fn real_worker_owner_handoff_starts_a_new_failure_grace() {
    let desktop = HostDataService {
        owner: Arc::new(DesktopOwner {
            shared: Arc::new(DesktopShared::default()),
        }),
    };
    desktop.owner.shared.state.lock().unwrap().snapshot.volume = Some(50);
    let t = Instant::now();
    let seconds = Arc::new(AtomicU64::new(0));
    let (tx, rx) = std::sync::mpsc::channel();
    let (old_hid, old_recorder) = crate::hid::HidDevice::test_device();
    let (new_hid, new_recorder) = crate::hid::HidDevice::test_device();
    let mut hid = Some(old_hid);
    let mut bridge = QmkHidHostBridge::start_with_desktop_service(
        incident_target(),
        HostDataMode {
            volume: true,
            ..Default::default()
        },
        None,
        HostProtocol::Selected(false),
        desktop,
        move |_, _| {
            let Some(hid) = hid.take() else {
                tx.send("reopened").unwrap();
                anyhow::bail!("unexpected reconnect");
            };
            let clock = seconds.clone();
            FAILURE_TIME.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    t + Duration::from_secs(clock.load(Ordering::SeqCst))
                }))
            });
            let seconds = seconds.clone();
            let tx = tx.clone();
            let mut attempts = 0;
            OUTPUT_SCRIPT.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move |_| {
                    attempts += 1;
                    match attempts {
                        1 => anyhow::bail!("temporary busy"),
                        2 => {
                            tx.send("first incident").unwrap();
                            anyhow::bail!("temporary busy");
                        }
                        3 | 4 => {
                            seconds.store(6, Ordering::SeqCst);
                            anyhow::bail!("new owner temporarily busy");
                        }
                        _ => {
                            tx.send("replacement recovered").unwrap();
                            Ok(())
                        }
                    }
                }))
            });
            Ok(HostDataHid::Dedicated(hid))
        },
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "first incident"
    );
    assert!(bridge.control.handoff_selected(new_hid, false).is_ok());
    let outcome = rx.recv_timeout(Duration::from_secs(5));
    bridge.suppress_shutdown();
    let worker = bridge.thread.take().unwrap();
    bridge.stop();
    worker.join().unwrap();
    assert_eq!(
        outcome.unwrap(),
        "replacement recovered",
        "new owner retained an old incident timestamp"
    );
    assert!(after_sm62_discovery(&old_recorder,true).is_empty());
    let output=after_sm62_discovery(&new_recorder,false);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0][..2], [DATA_VOLUME, 50]);
}

#[test]
fn real_worker_reconnects_on_persistent_failure_and_resets_replacement_grace() {
    let desktop = HostDataService {
        owner: Arc::new(DesktopOwner {
            shared: Arc::new(DesktopShared::default()),
        }),
    };
    desktop.owner.shared.state.lock().unwrap().snapshot.volume = Some(50);
    let t = Instant::now();
    let seconds = Arc::new(AtomicU64::new(0));
    let (tx, rx) = std::sync::mpsc::channel();
    let (old_hid, old_recorder) = crate::hid::HidDevice::test_device();
    let (new_hid, new_recorder) = crate::hid::HidDevice::test_device();
    let mut owners = vec![new_hid, old_hid];
    let cache = desktop.clone();
    let mut opens = 0;
    let mut bridge = QmkHidHostBridge::start_with_desktop_service(
        incident_target(),
        HostDataMode {
            volume: true,
            ..Default::default()
        },
        None,
        HostProtocol::Selected(false),
        desktop,
        move |_, _| {
            opens += 1;
            // The real service clears cache when reconnect drops demand; inject
            // the replacement OS sample rather than keeping stale demand alive.
            cache.owner.shared.state.lock().unwrap().snapshot.volume = Some(50);
            let Some(hid) = owners.pop() else {
                tx.send("extra reconnect").unwrap();
                anyhow::bail!("unexpected extra reconnect");
            };
            let clock = seconds.clone();
            FAILURE_TIME.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    t + Duration::from_secs(clock.load(Ordering::SeqCst))
                }))
            });
            let seconds = seconds.clone();
            let tx = tx.clone();
            let owner = opens;
            let mut attempts = 0;
            OUTPUT_SCRIPT.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move |_| {
                    attempts += 1;
                    if owner == 1 {
                        if attempts > 2 {
                            seconds.store(6, Ordering::SeqCst);
                        }
                        anyhow::bail!("persistent busy");
                    }
                    seconds.store(7, Ordering::SeqCst);
                    if attempts <= 2 {
                        anyhow::bail!("replacement transient busy");
                    }
                    tx.send("reconnected owner recovered").unwrap();
                    Ok(())
                }))
            });
            Ok(HostDataHid::Dedicated(hid))
        },
    );
    let outcome = rx.recv_timeout(Duration::from_secs(5));
    bridge.suppress_shutdown();
    let worker = bridge.thread.take().unwrap();
    bridge.stop();
    worker.join().unwrap();
    assert_eq!(outcome.unwrap(), "reconnected owner recovered");
    assert!(after_sm62_discovery(&old_recorder,true).is_empty());
    let output=after_sm62_discovery(&new_recorder,true);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0][..2], [DATA_VOLUME, 50]);
}

#[test]
fn fresh_timeline_seek_to_same_raw_position_reanchors_real_clock() {
    let t = Instant::now();
    let mut clock = MediaProgressClock::default();
    let mut update = |ticks, age, seconds| {
        clock.update(
            "track A",
            Some(Duration::ZERO),
            ticks,
            age,
            true,
            Some(90),
            t + Duration::from_secs(seconds),
        )
    };
    assert_eq!(update(Some(100), Some(Duration::ZERO), 0), Some(0));
    assert_eq!(
        update(Some(100), Some(Duration::from_secs(10)), 10),
        Some(10)
    );
    assert_eq!(
        update(Some(200), Some(Duration::ZERO), 11),
        Some(0),
        "new timeline at same raw position must reset the anchor"
    );
    assert_eq!(update(Some(200), Some(Duration::from_secs(2)), 13), Some(2));
}

#[test]
fn timeline_clock_preserves_stale_invalid_pause_and_unknown_controls() {
    let t = Instant::now();
    let mut c = MediaProgressClock::default();
    let zero = Some(Duration::ZERO);
    // Initial old sample extrapolates from its original publication time.
    assert_eq!(
        c.update(
            "A",
            zero,
            Some(100),
            Some(Duration::from_secs(5)),
            true,
            Some(90),
            t
        ),
        Some(5)
    );
    assert_eq!(
        c.update(
            "A",
            zero,
            Some(100),
            Some(Duration::from_secs(15)),
            true,
            Some(90),
            t + Duration::from_secs(10)
        ),
        Some(15)
    );
    // Missing, invalid or regressing timestamp alone cannot restart progress.
    for (ticks, age, second) in [
        (None, None, 11),
        (Some(0), Some(Duration::ZERO), 12),
        (Some(99), Some(Duration::ZERO), 13),
        (Some(101), None, 14),
    ] {
        assert_eq!(
            c.update(
                "A",
                zero,
                ticks,
                age,
                true,
                Some(90),
                t + Duration::from_secs(second)
            ),
            Some(5 + second as u32)
        );
    }
    assert_eq!(
        c.update(
            "A",
            zero,
            Some(100),
            Some(Duration::from_secs(20)),
            false,
            Some(90),
            t + Duration::from_secs(15)
        ),
        Some(0)
    );
    assert_eq!(
        c.update(
            "A",
            zero,
            Some(100),
            Some(Duration::from_secs(25)),
            true,
            Some(90),
            t + Duration::from_secs(20)
        ),
        Some(0)
    );
    assert_eq!(
        c.update(
            "A",
            Some(Duration::from_secs(30)),
            Some(100),
            None,
            true,
            Some(90),
            t + Duration::from_secs(21)
        ),
        Some(30)
    );
    assert_eq!(
        c.update(
            "B",
            zero,
            None,
            None,
            true,
            None,
            t + Duration::from_secs(22)
        ),
        Some(0)
    );
    assert_eq!(
        c.update(
            "B",
            zero,
            None,
            None,
            true,
            None,
            t + Duration::from_secs(24)
        ),
        Some(2)
    );
    assert_eq!(
        c.update(
            "unknown",
            None,
            None,
            None,
            true,
            None,
            t + Duration::from_secs(25)
        ),
        None
    );
}

#[test]
fn sm62_uninitialized_successor_shutdown_clears_retained_layout() {
    let owner = Arc::new(HostOutputOwner::default());
    let old = owner.claim_sm62(
        HostDataMode {
            volume: true,
            layout: true,
            ..Default::default()
        },
        false,
        true,
    );
    old.write(&[DATA_LAYOUT, 1], |_| Ok(())).unwrap();
    let next = owner.claim_sm62(
        HostDataMode {
            volume: true,
            ..Default::default()
        },
        false,
        true,
    );
    old.shutdown(|_| panic!("retired lease wrote")).unwrap();
    let mut packets = Vec::new();
    next.shutdown(|p| {
        packets.push(p.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(packets, vec![vec![DATA_LAYOUT, 255]]);
}

#[test]
fn sm62_layout_takeover_clears_before_successor_volume() {
    for explicit_shutdown in [false, true] {
        for sm62 in [false, true] {
            let owner = Arc::new(HostOutputOwner::default());
            let unrelated = Arc::new(HostOutputOwner::default());
            let other = unrelated.claim(
                HostDataMode {
                    volume: true,
                    ..Default::default()
                },
                false,
            );
            let old = owner.claim_sm62(
                HostDataMode {
                    volume: true,
                    layout: true,
                    ..Default::default()
                },
                false,
                sm62,
            );
            let mut packets = Vec::new();
            old.write(&[DATA_LAYOUT, 1], |p| {
                packets.push(p.to_vec());
                Ok(())
            })
            .unwrap();
            if explicit_shutdown {
                old.shutdown(|p| {
                    packets.push(p.to_vec());
                    Ok(())
                })
                .unwrap();
            }
            let next = owner.claim_sm62(
                HostDataMode {
                    volume: true,
                    ..Default::default()
                },
                false,
                sm62,
            );
            old.shutdown(|_| panic!("superseded shutdown wrote"))
                .unwrap();
            assert!(old
                .write(&[DATA_LAYOUT, 1], |_| panic!("superseded lease wrote"))
                .is_err());
            next.write(&[DATA_VOLUME, 50], |p| {
                packets.push(p.to_vec());
                Ok(())
            })
            .unwrap();
            let expected = if sm62 {
                vec![
                    vec![DATA_LAYOUT, 1],
                    vec![DATA_LAYOUT, 255],
                    vec![DATA_VOLUME, 50],
                ]
            } else {
                vec![vec![DATA_LAYOUT, 1], vec![DATA_VOLUME, 50]]
            };
            assert_eq!(
                packets, expected,
                "explicit={explicit_shutdown} sm62={sm62}"
            );
            assert!(other.is_current(), "another physical owner was revoked");
            other
                .shutdown(|_| panic!("unrelated owner got output"))
                .unwrap();
        }
    }
}
