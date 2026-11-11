#[test]
#[ignore = "requires production C firmware bridge via SM62_PRODUCTION_C_BRIDGE"]
fn sm62_cold_foreground_uses_device_profile_before_any_catalog_write() {
    use std::io::{BufRead,Write};
    struct Peer { child:std::process::Child,input:std::process::ChildStdin,output:std::io::BufReader<std::process::ChildStdout>,trace:Vec<serde_json::Value>,trace_path:std::path::PathBuf }
    impl Peer {
        fn line(&mut self,s:&str)->serde_json::Value {writeln!(self.input,"{s}").unwrap();self.input.flush().unwrap();let mut s=String::new();assert!(self.output.read_line(&mut s).unwrap()>0,"production C bridge exited");serde_json::from_str(&s).unwrap()}
        fn packet(&mut self,p:&[u8])->Vec<[u8;32]> {self.line("TICK 1");let mut full=[0;32];full[..p.len()].copy_from_slice(p);let s=full.iter().map(|b|format!("{b:02x}")).collect::<String>();let reply=self.line(&s);self.trace.push(serde_json::json!({"request":full,"reply":reply}));reply["responses"].as_array().unwrap().iter().map(|s|{let s=s.as_str().unwrap();assert_eq!(s.len(),64);std::array::from_fn(|i|u8::from_str_radix(&s[2*i..2*i+2],16).unwrap())}).collect()}
    }
    impl Drop for Peer {fn drop(&mut self){let _=std::fs::write(&self.trace_path,serde_json::to_vec_pretty(&self.trace).unwrap());let _=self.child.kill();let _=self.child.wait();}}
    let exe=std::path::PathBuf::from(std::env::var_os("SM62_PRODUCTION_C_BRIDGE").expect("explicitly supply the qualified production C backend"));
    let out=std::path::PathBuf::from(std::env::var_os("AGENT_BUILD_CACHE").unwrap()).join("cold-c-interop");std::fs::create_dir_all(&out).unwrap();
    for role in ["left","right"] {for dedicated in [false,true] {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();let mut app=name_review_device_app(true);
        let mut device=app.device_manager.devices()[0].clone();device.serial_number=format!("{role}-mcu");device.instance_token=format!("{role}-{dedicated}");app.device_manager.replace_devices(vec![device.clone()]);
        app.update_application_layout_runtime_with(Default::default());
        let key=app.application_layout_device_key().unwrap();let s=app.application_layout_settings_mut().unwrap();
        let id=s.layouts.values().find(|p|p.executable.to_ascii_lowercase().contains("firefox")).unwrap().id.clone();
        // Intentionally delete unrelated templates in the fixture. This check
        // isolates cold read/SELECT of an existing record, not auto-provisioning.
        s.layouts.retain(|k,_|k=="default"||k==&id);s.normalize();
        let mut seeded=s.snapshot_for_layout(&s.layouts[&id]);seeded.layers=std::array::from_fn(|_|vec![9;62]);seeded.resident_slot=Some(7);
        let disk=out.join(format!("{role}-{dedicated}-{}.bin",std::process::id()));assert!(!disk.exists());
        let mut child=std::process::Command::new(&exe).arg(&disk).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).spawn().unwrap();
        let peer=std::sync::Arc::new(std::sync::Mutex::new(Peer{input:child.stdin.take().unwrap(),output:std::io::BufReader::new(child.stdout.take().unwrap()),child,trace:Vec::new(),trace_path:out.join(format!("wire-{role}-{dedicated}-{}.json",std::process::id()))}));
        let (hid,rec)=crate::hid::HidDevice::test_device();let p=peer.clone();rec.on_output(move|payload|assert!(p.lock().unwrap().packet(payload).is_empty(),"unexpected real VIA reply on write-only packet"));
        let p=peer.clone();rec.respond_by(move|payload|{let r=p.lock().unwrap().packet(payload);assert!(r.len()<=1,"real response FIFO needs more than one report");Some(r.first().copied().ok_or_else(||"production C emitted no reply".to_owned()))});
        let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
        bridge.bind_selected_application_layout_device(key.clone());bridge.set_selected_application_layout_support(true);bridge.set_application_layout_snapshot(seeded);
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        while peer.lock().unwrap().line("KEY 0 0")["active"]!=1 {assert!(std::time::Instant::now()<deadline,"real C firmware never accepted seed profile, role={role}, dedicated={dedicated}");std::thread::sleep(std::time::Duration::from_millis(5));}
        let before=peer.lock().unwrap().line("INFO");
        bridge.stop();drop(bridge);
        // Connect a fresh host owner to an already-populated virtual keyboard.
        let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
        bridge.bind_selected_application_layout_device(key);bridge.set_selected_application_layout_support(true);
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::firmware_default());
        if dedicated {assert!(bridge.adopt_selected_hid(hid).is_ok());} else {app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());}
        let settings=app.application_layout_settings_mut().unwrap();settings.automatic_switching_enabled=true;
        let profile=settings.layouts.get_mut(&id).unwrap();profile.automatic_switching=true;
        let cold_foreground=crate::application_layouts::DetectedApplication{executable:profile.executable.clone(),..Default::default()};
        app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        loop {app.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot{foreground:Some(cold_foreground.clone()),foreground_status:crate::app_discovery::ForegroundStatus{backend:"cold foreground fixture".into(),state:crate::app_discovery::ForegroundState::Focused(cold_foreground.clone())},..Default::default()});if app.application_layout_settings().unwrap().layouts[&id].layers[0][0]==9 {break;}assert!(std::time::Instant::now()<deadline,"confirmed C profile never reached the standard menu, role={role}, dedicated={dedicated}");std::thread::sleep(std::time::Duration::from_millis(5));}
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        app.activate_application_layout("default");app.update_application_layout_runtime_with(Default::default());
        while peer.lock().unwrap().line("KEY 0 0")["active"]!=0 {app.update_application_layout_runtime_with(Default::default());assert!(std::time::Instant::now()<deadline,"native Default never deactivated the seeded overlay");std::thread::sleep(std::time::Duration::from_millis(5));}
        let cursor=rec.requests().len();
        app.activate_application_layout("default");assert!(app.activate_application_layout(&id));app.update_application_layout_runtime_with(Default::default());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        loop {let state=peer.lock().unwrap().line("KEY 0 0");if state["active"]==1&&state["key"]==9{break;}if std::time::Instant::now()>=deadline {let p=peer.lock().unwrap();std::fs::write(&p.trace_path,serde_json::to_vec_pretty(&p.trace).unwrap()).unwrap();let published=app.qmk_hid_hosts[&device.path].test_published_snapshot();panic!("manual GUI selection did not apply on production C, role={role}, dedicated={dedicated}, state={state}, trace={:?}, app_status={}, active={}, id={:?}, slot={:?}, raw={}",p.trace_path,app.status_msg,published.active,published.profile_id,published.resident_slot,published.device_raw.is_some());}std::thread::sleep(std::time::Duration::from_millis(5));}
        let manual_requests=rec.requests();assert!(manual_requests[cursor..].iter().any(|r|r[..3]==[0xef,9,2]),"manual selection had no fresh SELECT");
        for layer in 0..16 {for control in 0..62 {let v=peer.lock().unwrap().line(&format!("KEY {layer} {control}"));assert_eq!(v["key"],9);assert_eq!(v["active"],1);}}
        assert!(app.activate_application_layout("default"));app.update_application_layout_runtime_with(Default::default());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        while peer.lock().unwrap().line("KEY 0 0")["active"]!=0 {assert!(std::time::Instant::now()<deadline,"Default did not deactivate before automatic selection");std::thread::sleep(std::time::Duration::from_millis(5));}
        let cursor=rec.requests().len();
        let settings=app.application_layout_settings_mut().unwrap();settings.automatic_switching_enabled=true;
        let profile=settings.layouts.get_mut(&id).unwrap();profile.automatic_switching=true;
        let foreground=crate::application_layouts::DetectedApplication{executable:profile.executable.clone(),..Default::default()};
        app.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot{foreground:Some(foreground.clone()),foreground_status:crate::app_discovery::ForegroundStatus{backend:"injected OS foreground; real host and firmware".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},..Default::default()});
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        loop {let state=peer.lock().unwrap().line("KEY 0 0");if state["active"]==1&&state["key"]==9{break;}assert!(std::time::Instant::now()<deadline,"automatic foreground selection did not apply on production C, role={role}, dedicated={dedicated}, state={state}");std::thread::sleep(std::time::Duration::from_millis(5));}
        for layer in 0..16 {for control in 0..62 {let v=peer.lock().unwrap().line(&format!("KEY {layer} {control}"));assert_eq!(v["key"],9);assert_eq!(v["active"],1);}}
        let after=peer.lock().unwrap().line("INFO");assert_eq!(after["nor_writes"],before["nor_writes"],"cold existing profile selection rewrote NOR");assert_eq!(after["eeprom_writes"],before["eeprom_writes"]);
        let packets=rec.requests();assert!(packets[cursor..].iter().any(|r|r[..3]==[0xef,9,2]),"no fresh schema2 SELECT reached actual C firmware after the manual action");
        std::fs::write(out.join(format!("{role}-{dedicated}.json")),serde_json::to_vec_pretty(&serde_json::json!({"role":role,"dedicated":dedicated,"before":before,"after":after,"verified_controls_per_selection":16usize*62,"manual_and_auto_checked":true,"requests":packets})).unwrap()).unwrap();
        app.qmk_hid_hosts.clear();
        {let p=peer.lock().unwrap();std::fs::write(&p.trace_path,serde_json::to_vec_pretty(&p.trace).unwrap()).unwrap();}
    }}
}
