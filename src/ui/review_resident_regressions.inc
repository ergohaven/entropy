#[test]
fn sm62_review_deleted_known_host_profile_does_not_resurrect_on_next_tick() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_device_app(true);app.update_application_layout_runtime_with(Default::default());
    let key=app.application_layout_device_key().unwrap();
    let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_ascii_lowercase().contains("firefox")).unwrap().id.clone();
    let device=app.device_manager.devices()[0].clone();let (hid,rec)=crate::hid::HidDevice::test_device();
    rec.respond_by(|p|{if p[0]==0xeb{return Some(Ok(*p));}if p[0]==0xe6{let mut r=[0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(r));}None});
    let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
    bridge.bind_selected_application_layout_device(key);bridge.set_selected_application_layout_support(true);
    bridge.test_set_device_inventory_for_host_id(&id);let record=bridge.resident_recoveries().remove(0);
    // Upgrade from the previous delivered reader, which created a second host ID.
    let orphan=crate::qmk_hid_host::decode_device_profile(&record).unwrap();
    let settings=app.application_layout_settings_mut().unwrap();
    settings.resident_slots.insert(orphan.id.clone(),7);
    settings.device_raw_profiles.insert(orphan.id.clone(),record.raw.clone());
    settings.layouts.insert(orphan.id.clone(),orphan);
    app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    app.update_application_layout_runtime_with(Default::default());
    let s=app.application_layout_settings().unwrap();
    assert_eq!(s.layouts[&id].layers[0][0],9,"standard menu profile ignored the confirmed device-owned bindings and retained its Default-derived template");
    assert_eq!(s.resident_slots.get(&id),Some(&7));
    assert!(s.device_raw_profiles.contains_key(&id));
    assert!(!s.layouts.keys().any(|id|id.starts_with("device-resident-7-")),"one device record appeared twice with different host identities");
    app.activate_application_layout("default");assert!(app.activate_application_layout(&id));app.update_application_layout_runtime_with(Default::default());
    let snapshot=app.qmk_hid_hosts[&device.path].test_published_snapshot();
    assert!(snapshot.active);assert_eq!(snapshot.resident_slot,Some(7));
    assert_eq!(snapshot.device_raw.as_ref(),Some(&record.raw));
    assert_eq!(snapshot.device_identity.as_ref().map(|x|x.as_slice()),Some(&record.identity_crc[..20]));
    assert!(app.application_layout_settings_mut().unwrap().remove(&id));
    app.update_application_layout_runtime_with(Default::default());
    let settings=app.application_layout_settings().unwrap();
    assert!(settings.resident_deleted.contains_key(&id), "deletion intent must survive until the device confirms deletion");
    assert!(!settings.layouts.values().any(|p| settings.snapshot_for_layout(p).device_identity.as_ref().map(|identity|identity.as_slice())==Some(&record.identity_crc[..20])), "a deleted known-host resident profile resurrected under a generated identity on the next ordinary tick");
    assert!(!settings.layouts.contains_key(&id));
}

#[test]
#[ignore = "requires production C firmware bridge via SM62_PRODUCTION_C_BRIDGE"]
fn sm62_review_confirmed_deletion_does_not_reimport_stale_recovery() {
    use std::io::{BufRead,Write};
    struct Peer { child:std::process::Child,input:std::process::ChildStdin,output:std::io::BufReader<std::process::ChildStdout>,trace:Vec<serde_json::Value>,trace_path:std::path::PathBuf }
    impl Peer {
        fn line(&mut self,s:&str)->serde_json::Value {writeln!(self.input,"{s}").unwrap();self.input.flush().unwrap();let mut s=String::new();assert!(self.output.read_line(&mut s).unwrap()>0,"production C bridge exited");serde_json::from_str(&s).unwrap()}
        fn packet(&mut self,p:&[u8])->Vec<[u8;32]> {self.line("TICK 1");let mut full=[0;32];full[..p.len()].copy_from_slice(p);let s=full.iter().map(|b|format!("{b:02x}")).collect::<String>();let reply=self.line(&s);self.trace.push(serde_json::json!({"request":full,"reply":reply}));reply["responses"].as_array().unwrap().iter().map(|s|{let s=s.as_str().unwrap();assert_eq!(s.len(),64);std::array::from_fn(|i|u8::from_str_radix(&s[2*i..2*i+2],16).unwrap())}).collect()}
    }
    impl Drop for Peer {fn drop(&mut self){let _=std::fs::write(&self.trace_path,serde_json::to_vec_pretty(&self.trace).unwrap());let _=self.child.kill();let _=self.child.wait();}}
    let exe=std::path::PathBuf::from(std::env::var_os("SM62_PRODUCTION_C_BRIDGE").expect("explicitly supply the qualified production C backend"));
    let out=std::path::PathBuf::from(std::env::var_os("AGENT_BUILD_CACHE").unwrap()).join("delete-c-interop");std::fs::create_dir_all(&out).unwrap();
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
        app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        loop {app.update_application_layout_runtime_with(Default::default());if app.application_layout_settings().unwrap().layouts[&id].layers[0][0]==9 {break;}assert!(std::time::Instant::now()<deadline,"confirmed C profile never reached the standard menu, role={role}, dedicated={dedicated}");std::thread::sleep(std::time::Duration::from_millis(5));}
        assert!(app.application_layout_settings_mut().unwrap().remove(&id));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
        loop {
            app.update_application_layout_runtime_with(Default::default());
            if !app.application_layout_settings().unwrap().resident_deleted.contains_key(&id) {break;}
            assert!(std::time::Instant::now()<deadline, "production C DELETE never confirmed absent");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Once absence is confirmed, old readback must not survive the tombstone.
        for _ in 0..5 {
            app.update_application_layout_runtime_with(Default::default());
            let settings=app.application_layout_settings().unwrap();
            assert!(!settings.layouts.contains_key(&id));
            assert!(!settings.layouts.keys().any(|key|key.starts_with("device-resident-7-")), "confirmed deletion resurrected an obsolete cached recovery after the tombstone was retired");
        }
        assert!(app.qmk_hid_hosts[&device.path].resident_recoveries().iter().all(|r|r.slot!=7), "confirmed device absence did not retire the owner's cached readback");
        app.qmk_hid_hosts.clear();
    }}
}

#[test]
#[ignore = "requires production C firmware bridge via SM62_PRODUCTION_C_BRIDGE"]
fn sm62_review_blank_connection_does_not_store_automatic_templates() {
    use std::io::{BufRead,Write};
    struct Peer { child:std::process::Child,input:std::process::ChildStdin,output:std::io::BufReader<std::process::ChildStdout>,trace:Vec<serde_json::Value>,trace_path:std::path::PathBuf }
    impl Peer {
        fn line(&mut self,s:&str)->serde_json::Value {writeln!(self.input,"{s}").unwrap();self.input.flush().unwrap();let mut s=String::new();assert!(self.output.read_line(&mut s).unwrap()>0,"production C bridge exited");serde_json::from_str(&s).unwrap()}
        fn packet(&mut self,p:&[u8])->Vec<[u8;32]> {self.line("TICK 1");let mut full=[0;32];full[..p.len()].copy_from_slice(p);let s=full.iter().map(|b|format!("{b:02x}")).collect::<String>();let reply=self.line(&s);self.trace.push(serde_json::json!({"request":full,"reply":reply}));reply["responses"].as_array().unwrap().iter().map(|s|{let s=s.as_str().unwrap();assert_eq!(s.len(),64);std::array::from_fn(|i|u8::from_str_radix(&s[2*i..2*i+2],16).unwrap())}).collect()}
    }
    impl Drop for Peer {fn drop(&mut self){let _=std::fs::write(&self.trace_path,serde_json::to_vec_pretty(&self.trace).unwrap());let _=self.child.kill();let _=self.child.wait();}}
    let exe=std::path::PathBuf::from(std::env::var_os("SM62_PRODUCTION_C_BRIDGE").expect("explicitly supply the qualified production C backend"));
    let out=std::path::PathBuf::from(std::env::var_os("AGENT_BUILD_CACHE").unwrap()).join("blank-no-template-c-interop");std::fs::create_dir_all(&out).unwrap();
    for role in ["left", "right"] {for dedicated in [false,true] {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let mut app=name_review_device_app(true);
        let mut device=app.device_manager.devices()[0].clone();
        device.serial_number=format!("{role}-mcu");device.instance_token=format!("blank-{role}-{dedicated}");
        app.device_manager.replace_devices(vec![device.clone()]);
        app.update_application_layout_runtime_with(Default::default());
        let key=app.application_layout_device_key().unwrap();
        // Do not trim the catalog: all ordinary automatic presets remain present.
        assert!(app.application_layout_settings().unwrap().layouts.len()>2);
        let disk=out.join(format!("{role}-{dedicated}-{}.bin",std::process::id()));assert!(!disk.exists());
        let mut child=std::process::Command::new(&exe).arg(&disk).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).spawn().unwrap();
        let peer=std::sync::Arc::new(std::sync::Mutex::new(Peer{input:child.stdin.take().unwrap(),output:std::io::BufReader::new(child.stdout.take().unwrap()),child,trace:Vec::new(),trace_path:out.join(format!("wire-{role}-{dedicated}-{}.json",std::process::id()))}));
        let before=peer.lock().unwrap().line("INFO");
        let (hid,rec)=crate::hid::HidDevice::test_device();
        let p=peer.clone();rec.on_output(move|payload|assert!(p.lock().unwrap().packet(payload).is_empty()));
        let p=peer.clone();rec.respond_by(move|payload|{let r=p.lock().unwrap().packet(payload);assert!(r.len()<=1);Some(r.first().copied().ok_or_else(||"production C emitted no reply".to_owned()))});
        let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
        bridge.bind_selected_application_layout_device(key);bridge.set_selected_application_layout_support(true);
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::firmware_default());
        if dedicated {assert!(bridge.adopt_selected_hid(hid).is_ok());} else {app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());}
        app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        loop {
            app.update_application_layout_runtime_with(Default::default());
            if rec.requests().iter().any(|r|r[..4]==[0xef,9,3,99]) {break;}
            assert!(std::time::Instant::now()<deadline, "blank production C inventory never completed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let settle=std::time::Instant::now()+std::time::Duration::from_secs(3);
        while std::time::Instant::now()<settle {
            app.update_application_layout_runtime_with(Default::default());
            if peer.lock().unwrap().line("INFO")["nor_writes"]!=before["nor_writes"] {break;}
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let after=peer.lock().unwrap().line("INFO");
        assert_eq!(after["nor_writes"],before["nor_writes"], "ordinary blank-device connection uploaded unmodified automatic templates without user action");
        assert_eq!(after["eeprom_writes"],before["eeprom_writes"]);
        assert!(!rec.requests().iter().any(|r|r[..3]==[0xef,9,1]), "unrequested STORE reached production C");
        app.qmk_hid_hosts.clear();
    }}
}
#[test]
#[ignore = "requires production C firmware bridge via SM62_PRODUCTION_C_BRIDGE"]
fn sm62_p2_background_store_recovers_export_without_reconnect() {
    use std::io::{BufRead,Write};
    struct Peer { child:std::process::Child,input:std::process::ChildStdin,output:std::io::BufReader<std::process::ChildStdout>,trace:Vec<serde_json::Value>,trace_path:std::path::PathBuf }
    impl Peer {
        fn line(&mut self,s:&str)->serde_json::Value {writeln!(self.input,"{s}").unwrap();self.input.flush().unwrap();let mut s=String::new();assert!(self.output.read_line(&mut s).unwrap()>0,"production C bridge exited");serde_json::from_str(&s).unwrap()}
        fn packet(&mut self,p:&[u8])->Vec<[u8;32]> {self.line("TICK 1");let mut full=[0;32];full[..p.len()].copy_from_slice(p);let s=full.iter().map(|b|format!("{b:02x}")).collect::<String>();let reply=self.line(&s);self.trace.push(serde_json::json!({"request":full,"reply":reply}));reply["responses"].as_array().unwrap().iter().map(|s|{let s=s.as_str().unwrap();assert_eq!(s.len(),64);std::array::from_fn(|i|u8::from_str_radix(&s[2*i..2*i+2],16).unwrap())}).collect()}
    }
    impl Drop for Peer {fn drop(&mut self){let _=std::fs::write(&self.trace_path,serde_json::to_vec_pretty(&self.trace).unwrap());let _=self.child.kill();let _=self.child.wait();}}
    let exe=std::path::PathBuf::from(std::env::var_os("SM62_PRODUCTION_C_BRIDGE").expect("explicitly supply the qualified production C backend"));
    let out=std::path::PathBuf::from(std::env::var_os("AGENT_BUILD_CACHE").unwrap()).join("p2-background-readback-c-interop");std::fs::create_dir_all(&out).unwrap();
    for role in ["left", "right"] {for dedicated in [false,true] {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let mut app=name_review_device_app(true);
        let mut device=app.device_manager.devices()[0].clone();
        device.serial_number=format!("{role}-mcu");device.instance_token=format!("blank-{role}-{dedicated}");
        app.device_manager.replace_devices(vec![device.clone()]);
        app.update_application_layout_runtime_with(Default::default());
        let key=app.application_layout_device_key().unwrap();
        // Do not trim the catalog: all ordinary automatic presets remain present.
        assert!(app.application_layout_settings().unwrap().layouts.len()>2);
        let disk=out.join(format!("{role}-{dedicated}-{}.bin",std::process::id()));assert!(!disk.exists());
        let mut child=std::process::Command::new(&exe).arg(&disk).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).spawn().unwrap();
        let peer=std::sync::Arc::new(std::sync::Mutex::new(Peer{input:child.stdin.take().unwrap(),output:std::io::BufReader::new(child.stdout.take().unwrap()),child,trace:Vec::new(),trace_path:out.join(format!("wire-{role}-{dedicated}-{}.json",std::process::id()))}));
        let before=peer.lock().unwrap().line("INFO");
        let (hid,rec)=crate::hid::HidDevice::test_device();
        let p=peer.clone();rec.on_output(move|payload|assert!(p.lock().unwrap().packet(payload).is_empty()));
        let p=peer.clone();rec.respond_by(move|payload|{let r=p.lock().unwrap().packet(payload);assert!(r.len()<=1);Some(r.first().copied().ok_or_else(||"production C emitted no reply".to_owned()))});
        let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
        bridge.bind_selected_application_layout_device(key);bridge.set_selected_application_layout_support(true);
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::firmware_default());
        if dedicated {assert!(bridge.adopt_selected_hid(hid).is_ok());} else {app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());}
        app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(12);
        loop {
            app.update_application_layout_runtime_with(Default::default());
            if rec.requests().iter().any(|r|r[..4]==[0xef,9,3,99]) {break;}
            assert!(std::time::Instant::now()<deadline, "blank production C inventory never completed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // The layout fixture supplies an already-loaded native keymap and a
        // successful QMK feature QUERY, independent of resident READ proof.
        app.layout.as_mut().unwrap().live_features.qmk_settings_confirmed=true;
        // The empty inventory is exhausted before the first explicit publication.
        let generation=app.qmk_hid_hosts[&device.path].application_layout_publication_generation().unwrap();
        let incarnation=app.qmk_hid_hosts[&device.path].application_layout_owner_incarnation();
        let settings=app.application_layout_settings_mut().unwrap();
        let id=settings.layouts.values().find(|p|p.executable.to_ascii_lowercase().contains("firefox")).unwrap().id.clone();
        settings.layouts.retain(|k,_|k=="default"||k==&id);settings.normalize();
        let profile=settings.layouts.get_mut(&id).unwrap();profile.layers=vec![vec![9;62];16];
        let snapshot=settings.snapshot_for_layout(&settings.layouts[&id]);
        assert!(app.qmk_hid_hosts[&device.path].publish_resident_library_for_generation(vec![snapshot],generation));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
        while app.qmk_hid_hosts[&device.path].resident_sync_status(&id)!=Some(crate::qmk_hid_host::ResidentSyncStatus::Synced) {
            assert!(std::time::Instant::now()<deadline,"background STORE did not complete");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(rec.requests().iter().any(|r|r[..3]==[0xef,9,1]));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        loop {
            app.update_application_layout_runtime_with(Default::default());
            let settings=app.application_layout_settings().unwrap();
            if app.qmk_hid_hosts[&device.path].resident_export_inventory_confirmed(settings) {break;}
            assert!(std::time::Instant::now()<deadline,"P2: successful background STORE never acquired validated READ export proof on the same connection");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.qmk_hid_hosts[&device.path].application_layout_publication_generation(),Some(generation));
        assert_eq!(app.qmk_hid_hosts[&device.path].application_layout_owner_incarnation(),incarnation);
        assert!(rec.requests().iter().any(|r|r[..3]==[0xef,9,6]),"STORE ack is not READ proof");
        assert!(app.test_unattended_resident_snapshot_available(),"unattended backup remained unavailable after verified readback");
        assert_eq!(rec.requests().iter().filter(|r|r[..3]==[0xef,9,1]).count(),1,"no automatic STORE retry");
        app.qmk_hid_hosts.clear();
    }}
}
