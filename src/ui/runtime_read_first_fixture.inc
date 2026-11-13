// Stateful OS HID boundary, shared by the historical runtime regressions.
// No ready/owner-intent injection: native GETs feed poll_connect, the real
// bridge performs capability/ENUM/READ, and the ordinary runtime adopts FULL.
fn runtime_fixture_crc(raw: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in raw { crc ^= u32::from(*byte); for _ in 0..8 {
        crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
    }}
    !crc
}
fn runtime_fixture_full(snapshot: &crate::application_layouts::ApplicationLayoutSnapshot) -> Vec<u8> {
    let packets = crate::qmk_hid_host::test_application_layout_transmission_packets(snapshot);
    let mut raw = packets[0].to_vec(); raw.extend_from_slice(packets.last().unwrap());
    for p in &packets[1..packets.len()-1] { match p[0] {
        0xe2 => raw.extend_from_slice(&p[5..5+usize::from(p[4])*2]),
        0xe7 => raw.extend_from_slice(&p[5..5+usize::from(p[4])]),
        0xe9 => raw.extend_from_slice(&p[4..10+usize::from(p[5])]),
        0xe4 => raw.extend_from_slice(&p[3..4+usize::from(p[3])]),
        _ => panic!("unexpected production FULL packet"),
    }}
    raw
}
fn runtime_fixture_hid(
    resident: Vec<(u8, Vec<u8>)>,
) -> (crate::hid::HidDevice, crate::hid::TestHidRecorder, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    runtime_fixture_hid_identity(resident, 0x42, 8)
}
fn runtime_fixture_hid_identity(
    resident: Vec<(u8, Vec<u8>)>, uid: u64, version: u8,
) -> (crate::hid::HidDevice, crate::hid::TestHidRecorder, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let unlock = pending.clone();
    rec.respond_by(move |p| {
        let mut r = *p;
        match p[0] {
            0x12 => { // VIA native keymap GET_BUFFER, not cached fixture assignments.
                let offset = u16::from_be_bytes([p[1],p[2]]) as usize;
                for i in 0..usize::from(p[3]) { r[4+i] = 4u16.to_be_bytes()[(offset+i)%2]; }
            },
            0xe6 => { r.fill(0); r[..3].copy_from_slice(&[0xe6,version,0x5a]); },
            0xeb => {
                if resident.is_empty() { return Some(Ok(*p)); } // correlated unsupported
                r[2]=0x5a;
                if p[3]==2 {
                    r[8..15].copy_from_slice(&[2,100,0xd0,0x11,16,62,22]);
                    r[16..20].copy_from_slice(&4194304u32.to_le_bytes());
                    r[20..24].copy_from_slice(&15u32.to_le_bytes());r[24]=2;
                } else { r[8]=3;r[9]=0;r[10..12].copy_from_slice(&4560u16.to_le_bytes()); }
            },
            0xef if p[1]==9 => {
                r[2]|=128;
                let record=resident.iter().find(|(slot,_)|*slot==p[3]);
                let key=record.map(|(_,raw)| { let mut k=[0;24];k[..20].copy_from_slice(&raw[43..63]);k[20..].copy_from_slice(&runtime_fixture_crc(raw).to_le_bytes());k });
                match p[2] {
                    3 => { r[8..].fill(0);r[3]=if let Some(k)=key {r[8..].copy_from_slice(&k);0}else{1}; },
                    6 => { let raw=&record.expect("READ of absent slot").1;let offset=u16::from_le_bytes([p[8],p[9]]) as usize;let n=raw.len().saturating_sub(offset).min(20);r[10]=n as u8;r[11]=0;r[12..].fill(0);r[12..12+n].copy_from_slice(&raw[offset..offset+n]); },
                    0|2 => { r[3]=if key.as_ref().is_some_and(|k|k[..]==p[8..32]) {0}else{1}; },
                    _ => panic!("read-first fixture must not STORE/DELETE: {}",p[2]),
                }
            },
            0xfe => match p[1] {
                0 => { r.fill(0);r[..4].copy_from_slice(&6u32.to_le_bytes());r[4..12].copy_from_slice(&uid.to_le_bytes()); },
                5 => { r.fill(255);r[..4].fill(0); },
                6 => { unlock.store(true,std::sync::atomic::Ordering::SeqCst);r.fill(0); },
                7 => { unlock.store(false,std::sync::atomic::Ordering::SeqCst);r.fill(0);r[0]=1; },
                0x0a => { let qsid=u16::from_le_bytes([p[2],p[3]]);if !(200..216).contains(&qsid){return None;}r.fill(0);r[1..7].copy_from_slice(b"Native"); },
                _ => return None,
            },
            _ => return None,
        }
        Some(Ok(r))
    });
    (hid,rec,pending)
}

// An ambiguous saved host profile is not an automatically provisioned preset.
// Keep zero authorship so publication tests still require a new explicit action.
fn name_review_cached_sm62_app() -> EntropyApp {
    let mut app = name_review_device_app(true);
    let settings = app.application_layout_settings_mut().unwrap();
    let preset = crate::application_layouts::builtin_application_layout_presets()
        .into_iter()
        .find(|preset| preset.id == "firefox")
        .unwrap();
    let id = settings.create_from_preset(&preset);
    let profile = settings.layouts.get_mut(&id).unwrap();
    profile.user_edit_revision = 0;
    profile.automatic_switching = false;
    settings.editor_layout_id = "default".into();
    app
}

// Encode device-owned records with the production snapshot encoder. The known
// built-in menu ID is only matching metadata; each owner returns its own FULL.
fn runtime_fixture_builtin_record(preset: &str, name: &str, code: u16) -> (String, Vec<u8>) {
    runtime_fixture_builtin_record_at_slot(preset, name, code, 7)
}
fn runtime_fixture_builtin_record_at_slot(preset: &str, name: &str, code: u16, slot: u8) -> (String, Vec<u8>) {
    let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
    settings.layouts.get_mut("default").unwrap().layers = vec![vec![4;62];16];
    // Preserve historical catalog-derived IDs in these device-record fixtures.
    // Production no longer creates the host placeholders automatically.
    for preset in crate::application_layouts::builtin_application_layout_presets() {
        let id = settings.create_from_preset(&preset);
        let profile = settings.layouts.get_mut(&id).unwrap();
        profile.user_edit_revision = 0;
        profile.automatic_switching = false;
    }
    let executable = crate::application_layouts::builtin_application_layout_presets()
        .into_iter().find(|p|p.id == preset).unwrap().executable.to_owned();
    let id = settings.layouts.values().find(|p|p.executable == executable).unwrap().id.clone();
    let layout = settings.layouts.get_mut(&id).unwrap();
    layout.name = name.into();
    layout.layers = vec![vec![code;62];16];
    layout.layer_names = (0..16).map(|i|format!("{name} {i}")).collect();
    layout.encoder_stacks.iter_mut().for_each(Vec::clear);
    let mut snapshot = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(layout);
    // Exercise the full production transmission encoder, including its identity
    // extension, rather than only the snapshot's bare application frames.
    snapshot.resident_slot = Some(slot);
    let raw = runtime_fixture_full(&snapshot);
    crate::qmk_hid_host::attach_device_profile(&mut snapshot, layout, &raw);
    assert_eq!(snapshot.device_raw.as_ref(), Some(&raw), "fixture FULL failed production validation");
    // The real connect path provisions known menu metadata. Production resolves
    // this known host ID by its forward digest only after validating device FULL.
    assert!(crate::qmk_hid_host::application_layout_identity_matches(&id, &raw[43..63]));
    (id, raw)
}

// Assemble the worker result from actual production HID decoders, then use the
// real completion path. Only unrelated resource families are absent here.
fn runtime_fixture_connect(app: &mut EntropyApp, device: &crate::device::Device, hid: crate::hid::HidDevice) {
    let mut layout=sm62_test_layout();
    let (_,uid)=hid.get_keyboard_id().unwrap();
    let supported=hid.probe_application_layout_protocol_version(8).unwrap();assert!(supported);
    let keys=hid.get_keymap_buffer(16,layout.rows,layout.cols).unwrap();
    layout.layers=(0..16).map(|layer|layout.keys.iter().map(|key| {
        crate::keyboard::KeyBinding::Vial(keys[layer*layout.rows*layout.cols+usize::from(key.row)*layout.cols+usize::from(key.col)])
    }).collect()).collect();
    layout.layer_names=(200..216).map(|qsid|hid.get_qmk_setting_string(qsid).unwrap()).collect();
    let result=ConnectResult {
        device_name:device.name.clone(),keyboard_id:uid,vial_unlock_status:None,
        hid_device:Some(hid.into()),layout,layer_count:16,
        about_info:DeviceAboutInfo { supports_application_layouts:supported,vendor_id:device.vendor_id,product_id:device.product_id,path:device.path.clone(),..Default::default() },
        macro_texts:Default::default(),supports_macro_ext_keycodes:false,
        supports_rmk_native_key_actions:false,supports_universal_numero:false,supports_universal_symbols:false,
        supports_universal_russian_letters:false,supports_rmk_native_combo_output:false,
        supports_rmk_native_tap_dance_actions:false,supports_rmk_combo_layers:false,
        macro_ext_keycodes_disabled_reason:None,tap_dance_entries:Default::default(),combo_entries:Default::default(),
        combo_term:None,auto_shift_options:Default::default(),auto_shift_timeout:None,
        mouse_keys_settings:Default::default(),touchpad_settings:Default::default(),bluetooth_settings:Default::default(),
        module_settings:Default::default(),tap_hold_settings:Default::default(),magic_settings:Default::default(),
        one_shot_settings:Default::default(),grave_escape_settings:Default::default(),layer_led_settings:Default::default(),
        rgb_settings:Default::default(),display_settings:Default::default(),layout_options_value:None,
        key_override_entries:Default::default(),alt_repeat_entries:Default::default(),vial_features:Default::default(),
        layer_names_from_firmware:vec![true;16],supported_qmk_settings:(200..216).collect(),
        deferred_load:DeferredDeviceLoadState::complete(16),rynk_read_losses:Vec::new(),
    };
    // start_connect normally captures this inventory selection before spawning
    // its worker; this fixture supplies only the completed worker result.
    app.selected_device = Some(app.device_manager.devices().iter().position(|d|
        d.path == device.path && d.instance_token == device.instance_token).unwrap());
    let (tx,rx)=std::sync::mpsc::channel();let now=std::time::Instant::now();
    app.connect_state=ConnectState::Loading {device:device.clone(),cancel:Default::default(),rx,started_at:now,last_progress_at:now,reconnect:None};
    tx.send(ConnectTaskMessage::Done(Box::new(Ok(result)))).unwrap();
    crate::app_discovery::with_test_application_discovery(Default::default(),||app.poll_connect(&egui::Context::default()));
    assert!(matches!(app.connect_state,ConnectState::Idle));
    app.update_application_layout_runtime_with(Default::default());
    assert_eq!(app.qmk_hid_hosts[&device.path].application_layout_device_key(),app.application_layout_device_key().as_deref());
}

fn runtime_fixture_adopt(app: &mut EntropyApp, path: &str, key: &str, id: &str, raw: &[u8], rec: &crate::hid::TestHidRecorder) -> String {
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
    let adopted_id = loop {
        app.update_application_layout_runtime_with(Default::default());
        if let Some(found) = app.app_settings.application_layouts.get(key)
            .and_then(|s|s.device_raw_profiles.iter().find(|(_, bytes)| bytes.as_slice()==raw)) {
            break found.0.clone();
        }
        assert!(std::time::Instant::now()<deadline,
            "actual ENUM/READ never adopted {id} on {path}; actual IDs={:?}; recent packets={:02x?}",
            app.app_settings.application_layouts.get(key).map(|s|s.device_raw_profiles.keys().cloned().collect::<Vec<_>>()),
            rec.requests().into_iter().rev().take(4).collect::<Vec<_>>());
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let requests=rec.requests();
    assert!(requests.iter().any(|p|p[..3]==[0xef,9,3]));
    assert!(requests.iter().any(|p|p[..3]==[0xef,9,6]));
    assert!(!requests.iter().any(|p|p[0]==5||p[..2]==[0xfe,0x0b]||p[..2]==[0xef,9]&&matches!(p[2],1|4)),"adoption mutated device data");
    adopted_id
}
fn runtime_fixture_wait_select(rec: &crate::hid::TestHidRecorder, cursor: usize, raw: &[u8]) {
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
    loop {if rec.requests()[cursor..].iter().any(|p|p[..3]==[0xef,9,2]&&p[8..28]==raw[43..63]&&p[28..32]==runtime_fixture_crc(raw).to_le_bytes()){break;}
        assert!(std::time::Instant::now()<deadline,"no fresh SELECT for confirmed device FULL");std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
