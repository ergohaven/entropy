#[test]
fn sm62_transient_foreground_status_is_published_without_debounce() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=EntropyApp::new_inert_for_test();
    let mut device=m4cr0pad_v3_device();device.name="SM62".into();device.product_id=0x00b2;
    device.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
    app.device_manager.replace_devices(vec![device.clone()]);app.selected_device=Some(0);
    app.current_keyboard_id=Some(0x37434f4c32364d53);app.current_device_name="SM62".into();
    app.layout=Some(sm62_test_layout());mark_application_layout_protocol_supported(&mut app);
    let (hid,packets,_)=runtime_fixture_hid(Vec::new());
    let bridge=crate::qmk_hid_host::test_start_profile_handoff_bridge(device.clone(),hid.shared_output());
    bridge.set_selected_application_layout_support(true);app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    runtime_fixture_connect(&mut app,&device,hid);
    let key=app.application_layout_device_key().unwrap();
    let browser=crate::application_layouts::DetectedApplication{executable:"lease-browser".into(),display_name:"Browser".into(),..Default::default()};
    let settings=app.application_layout_settings_mut().unwrap();
    let profile=settings.create_for_application(&browser);
    settings.layouts.get_mut(&profile).unwrap().layers=vec![vec![0x22;62];16];
    assert!(app.activate_application_layout(&profile));
    let step=|app:&mut EntropyApp,state:crate::app_discovery::ForegroundState| {
        let foreground=match &state{crate::app_discovery::ForegroundState::Focused(a)=>Some(a.clone()),_=>None};
        app.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot{
            foreground,foreground_status:crate::app_discovery::ForegroundStatus{backend:"synthetic OS boundary".into(),state},..Default::default()});
        app.qmk_hid_hosts[&device.path].test_application_layout_snapshot()
    };
    use crate::app_discovery::ForegroundState as F;
    for transient in [F::BackendUnavailable("temporary error".into()),F::NoFocusedWindow,F::UnidentifiedWindow("transition".into())] {
        assert_eq!(step(&mut app,F::Focused(browser.clone())).profile_id.as_ref(),Some(&profile));
        let s=step(&mut app,transient.clone());
        let expected=if matches!(transient,F::BackendUnavailable(_)){profile.as_str()}else{crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID};
        assert_eq!(s.profile_id.as_deref(),Some(expected));
        assert_eq!(s.active,expected!="default","native Default exits via EA, applications remain active");
        assert_eq!(step(&mut app,F::Focused(browser.clone())).profile_id.as_ref(),Some(&profile));
    }
    assert_eq!(app.app_settings.application_layouts[&key].active_layout_id,profile);
}

#[test]
#[ignore="requires SM62_LEASE_REPLAY production firmware harness"]
fn sm62_real_settings_worker_recovers_expired_active_profile() {
    use std::io::Write;
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=EntropyApp::new_inert_for_test();
    let mut device=m4cr0pad_v3_device();device.name="SM62".into();device.product_id=0x00b2;
    device.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
    app.device_manager.replace_devices(vec![device.clone()]);app.selected_device=Some(0);
    app.current_keyboard_id=Some(0x37434f4c32364d53);app.current_device_name="SM62".into();
    app.layout=Some(sm62_test_layout());mark_application_layout_protocol_supported(&mut app);
    let (hid,packets)=crate::hid::HidDevice::test_device();
    let (entered_tx,entered_rx)=std::sync::mpsc::channel();
    let (release_tx,release_rx)=std::sync::mpsc::channel();
    packets.respond_by(move |p|{
        let mut r=[0;32];
        if p[0]==0xe6 {r[..3].copy_from_slice(&[0xe6,8,0x5a]);Some(Ok(r))}
        else if p[..2]==[0xfe,0x0b] && p[2]==230 {entered_tx.send(()).unwrap();release_rx.recv_timeout(std::time::Duration::from_secs(8)).unwrap();Some(Ok(r))}
        else if p[..2]==[0xfe,0x0a] && p[2]==230 {r[1]=1;Some(Ok(r))}
        else{None}
    });
    let start=std::time::Instant::now();let trace=std::sync::Arc::new(std::sync::Mutex::new(String::new()));let t=trace.clone();
    packets.on_output(move|p|{let mut t=t.lock().unwrap();t.push_str(&format!("{}",start.elapsed().as_millis()));for i in 0..32{t.push_str(&format!(" {:02x}",p.get(i).copied().unwrap_or(0)));}t.push('\n');});
    let bridge=crate::qmk_hid_host::test_start_profile_handoff_bridge(device.clone(),hid.shared_output());
    bridge.set_selected_application_layout_support(true);app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());
    let browser=crate::application_layouts::DetectedApplication{executable:"lease-browser".into(),display_name:"Browser".into(),..Default::default()};
    let settings=app.application_layout_settings_mut().unwrap();let id=settings.create_for_application(&browser);
    settings.layouts.get_mut(&id).unwrap().layers=vec![vec![0x22;62];16];
    app.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot{foreground:Some(browser.clone()),foreground_status:crate::app_discovery::ForegroundStatus{backend:"synthetic OS".into(),state:crate::app_discovery::ForegroundState::Focused(browser)},..Default::default()});
    let wait=|f:&dyn Fn()->bool|{let d=std::time::Instant::now()+std::time::Duration::from_secs(5);while !f(){assert!(std::time::Instant::now()<d);std::thread::sleep(std::time::Duration::from_millis(5));}};
    wait(&||packets.requests().iter().any(|p|p[0]==0xe3&&p[2]!=0));
    app.queue_display_setting_write("synthetic slow display setting".into(),230,0,1);
    app.continue_pending_settings_writes(&egui::Context::default());
    entered_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2800));
    let before=packets.requests().len();release_tx.send(()).unwrap();
    wait(&||packets.requests()[before..].iter().filter(|p|p[0]==0xe5).count()>=2);
    app.poll_settings_write(&egui::Context::default());
    assert_eq!(app.settings_write_status(230),Some(&super::super::settings_write_queue::SettingsWriteStatus::Saved));
    let capture=trace.lock().unwrap().clone();if let Ok(p)=std::env::var("SM62_WORKER_TRACE"){std::fs::write(p,&capture).unwrap();}
    let mut child=std::process::Command::new(std::env::var("SM62_LEASE_REPLAY").unwrap()).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(capture.as_bytes()).unwrap();let o=child.wait_with_output().unwrap();println!("worker firmware replay: {}",String::from_utf8_lossy(&o.stdout));assert!(o.status.success());
}
