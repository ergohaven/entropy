// Synthetic transport/time boundary, real bridge, owner lock and firmware parser.
#[test]
#[ignore = "requires SM62_LEASE_REPLAY production firmware harness"]
fn sm62_active_profile_recovers_after_settings_owner_starvation() {
    use crate::application_layouts::{ApplicationLayout,ApplicationLayoutSnapshot};
    use std::io::Write;
    let replay=std::env::var("SM62_LEASE_REPLAY").unwrap();
    let mut target=crate::device::test_usb_device("lease",5,0x00b2);
    target.name="SM62".into();
    target.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
    let (hid,recorder)=crate::hid::HidDevice::test_device();
    recorder.respond_by(|p| {if p[0]==0xe6 {let mut r=[0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]);Some(Ok(r))}else{None}});
    let start=Instant::now();
    let trace=Arc::new(Mutex::new(String::new()));
    let output_trace=trace.clone();
    recorder.on_output(move |p| {
        let mut t=output_trace.lock().unwrap();
        t.push_str(&format!("{}",start.elapsed().as_millis()));
        for i in 0..32 {t.push_str(&format!(" {:02x}",p.get(i).copied().unwrap_or(0)));}
        t.push('\n');
    });
    let mut bridge=test_start_bridge(target,Default::default(),hid.shared_output(),None,HostProtocol::Selected(false),||None);
    bridge.set_selected_application_layout_support(true);
    bridge.bind_selected_application_layout_device("vial-lease".into());
    let settings=crate::application_layouts::DeviceApplicationLayouts::default();
    let mut profile=settings.active_layout().unwrap().clone();
    profile.id="lease-profile".into();profile.layers=vec![vec![0x22;62];16];profile.name="lease".into();
    let snapshot=ApplicationLayoutSnapshot::from_layout(&profile);
    assert!(snapshot.active);
    bridge.set_application_layout_snapshot(snapshot.clone());
    let wait_for = |predicate:&dyn Fn()->bool| {
        let deadline=Instant::now()+Duration::from_secs(5);
        while !predicate(){assert!(Instant::now()<deadline,"bridge stalled");thread::sleep(Duration::from_millis(5));}
    };
    wait_for(&|| recorder.requests().iter().any(|p|p[0]==0xe3&&p[2]!=0));
    // Same public owner exclusion used by settings_write_queue's qsid 230..249 worker.
    // No endpoint open, alternate reader, profile mutation or reconnect.
    let (entered_tx,entered_rx)=std::sync::mpsc::channel();
    let (release_tx,release_rx)=std::sync::mpsc::channel();
    let settings=thread::spawn(move || {
        let _events=hid.pause_application_event_polling();
        hid.with_host_output_paused_for_settings(|| {
            entered_tx.send(()).unwrap();release_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        });hid
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    thread::sleep(Duration::from_millis(2800));
    let before=recorder.requests().len();
    release_tx.send(()).unwrap();let _hid=settings.join().unwrap();
    wait_for(&||recorder.requests()[before..].iter().filter(|p|p[0]==0xe5).count()>=2);
    assert_eq!(bridge.test_application_layout_snapshot(),snapshot,"desired profile changed");
    let capture=trace.lock().unwrap().clone();
    if let Ok(path)=std::env::var("SM62_LEASE_TRACE"){std::fs::write(path,&capture).unwrap();}
    let mut child=std::process::Command::new(replay).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(capture.as_bytes()).unwrap();
    let out=child.wait_with_output().unwrap();
    println!("firmware replay: {}",String::from_utf8_lossy(&out.stdout));
    bridge.stop();
    assert!(out.status.success(),"renewed KEEPALIVEs left desired active SM62 profile in firmware native fallback");
}

// Synthetic transport/time boundary, real bridge, owner lock and firmware parser.
#[test]
#[ignore = "requires SM62_LEASE_REPLAY production firmware harness"]
fn sm62_active_profile_recovers_after_staging_owner_starvation() {
    staging_owner_recovery(false, false);
}
#[test]
#[ignore = "requires SM62_STAGING_REPLAY production firmware harness"]
fn sm62_cold_profile_recovers_after_staging_owner_starvation() {
    staging_owner_recovery(true, false);
}
#[test]
#[ignore = "requires SM62_STAGING_REPLAY production firmware harness"]
fn sm62_active_profile_recovers_after_late_commit_owner_starvation() {
    staging_owner_recovery(false, true);
}

fn staging_owner_recovery(cold: bool, late: bool) {
    use crate::application_layouts::ApplicationLayoutSnapshot;
    use std::io::Write;
    let replay = std::env::var(if cold {
        "SM62_STAGING_REPLAY"
    } else {
        "SM62_LEASE_REPLAY"
    })
    .unwrap();
    let mut target = crate::device::test_usb_device("lease", 5, 0x00b2);
    target.name = "SM62".into();
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|p| {
        if p[0] == 0xe6 {
            let mut r = [0; 32];
            r[..3].copy_from_slice(&[0xe6, 8, 0x5a]);
            Some(Ok(r))
        } else {
            None
        }
    });
    let settings = crate::application_layouts::DeviceApplicationLayouts::default();
    let mut profile = settings.active_layout().unwrap().clone();
    profile.id = "lease-profile".into();
    profile.layers = vec![vec![0x22; 62]; 16];
    profile.name = "lease".into();
    let snapshot = ApplicationLayoutSnapshot::from_layout(&profile);
    let packets = snapshot.packets();
    let trigger = packets[if late { packets.len() - 2 } else { 0 }];
    let start = Instant::now();
    let trace = Arc::new(Mutex::new(String::new()));
    let output_trace = trace.clone();
    let (begin_tx, begin_rx) = std::sync::mpsc::channel();
    let interrupt = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let armed = interrupt.clone();
    recorder.on_output(move |p| {
        let mut t = output_trace.lock().unwrap();
        t.push_str(&format!("{}", start.elapsed().as_millis()));
        for i in 0..32 {
            t.push_str(&format!(" {:02x}", p.get(i).copied().unwrap_or(0)));
        }
        t.push('\n');
        if p == trigger && armed.swap(false, Ordering::SeqCst) {
            begin_tx.send(()).unwrap();
        }
    });
    let mut bridge = test_start_bridge(
        target,
        Default::default(),
        hid.shared_output(),
        None,
        HostProtocol::Selected(false),
        || None,
    );
    bridge.set_selected_application_layout_support(true);
    bridge.bind_selected_application_layout_device("vial-lease".into());
    let wait_for = |predicate: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(6);
        while !predicate() {
            assert!(Instant::now() < deadline, "bridge stalled");
            thread::sleep(Duration::from_millis(5));
        }
    };
    if !cold {
        bridge.set_application_layout_snapshot(snapshot.clone());
        wait_for(&|| {
            recorder
                .requests()
                .iter()
                .any(|p| p[0] == 0xe3 && p[2] != 0)
        });
    }
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let settings = thread::spawn(move || {
        begin_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let _events = hid.pause_application_event_polling();
        hid.with_host_output_paused_for_settings(|| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        });
        hid
    });
    interrupt.store(true, Ordering::SeqCst);
    if cold {
        bridge.set_application_layout_snapshot(snapshot.clone());
    } else {
        bridge.force_application_layout_resend();
    }
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let generation = bridge
        .application_layout_snapshot
        .lock()
        .unwrap()
        .snapshot_generation;
    let held = recorder.requests();
    assert!(
        held.iter()
            .rev()
            .find(|p| matches!(p[0], 0xe1..=0xe9))
            .is_some_and(|p| *p == trigger),
        "settings did not acquire immediately after delivered staging packet"
    );
    thread::sleep(Duration::from_millis(2800));
    let before = recorder.requests().len();
    release_tx.send(()).unwrap();
    let _hid = settings.join().unwrap();
    wait_for(&|| {
        recorder.requests()[before..]
            .iter()
            .filter(|p| p[0] == 0xe5)
            .count()
            >= 2
    });
    assert_eq!(
        bridge.test_application_layout_snapshot(),
        snapshot,
        "desired profile changed"
    );
    assert_eq!(
        bridge
            .application_layout_snapshot
            .lock()
            .unwrap()
            .snapshot_generation,
        generation,
        "recovery changed owner generation"
    );
    let capture = trace.lock().unwrap().clone();
    if let Ok(path) = std::env::var("SM62_LEASE_TRACE") {
        std::fs::write(format!("{path}-{cold}-{late}"), &capture).unwrap();
    }
    let mut child = std::process::Command::new(replay)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(capture.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    println!(
        "firmware replay cold={cold} late={late}: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    bridge.stop();
    assert!(out.status.success(),"interrupted full transfer was optimistically confirmed; healthy updates never restored firmware overlay");
}
