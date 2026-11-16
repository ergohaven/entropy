#[test]
fn sm62_manual_switch_with_real_probe_sends_complete_profile_to_each_usb_owner() {
    sm62_read_first_usb_role_change(true);
}

// Legacy titles describe the user-visible role change, not a common model or
// inferred pair identity. Each attachment independently supplies its own FULL.
fn sm62_read_first_usb_role_change(manual: bool) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let mut app = EntropyApp::new_inert_for_test();
    let mut previous_owner = None;
    for (half, serial, code) in [
        ("main", "C281FE0DD04A01230000000000000000", 0x41),
        ("app", "C281FE0DD04A04560000000000000000", 0x42),
    ] {
        app.qmk_hid_hosts.clear();
        app.selected_device = None;
        app.clear_connected_keyboard_state("");
        let mut device = m4cr0pad_v3_device();
        device.name = "SM62".into(); device.product_id = 0x00b2;
        device.serial_number = serial.into();
        device.path = format!("{}/src/{half}.rs", env!("CARGO_MANIFEST_DIR"));
        device.instance_token = format!("role-attachment-{half}");
        app.device_manager.replace_devices(vec![device.clone()]);
        let (id, raw) = runtime_fixture_builtin_record("firefox", &format!("USB {half}"), code);
        let (hid, packets, unlock_pending) = runtime_fixture_hid(vec![(7,raw.clone())]);
        runtime_fixture_connect(&mut app, &device, hid);
        let key = app.application_layout_device_key().unwrap();
        if let Some((previous_key, previous_id)) = previous_owner.as_ref() {
            assert_ne!(&key, previous_key, "USB half identity is not common-library authority");
            assert_ne!(app.app_settings.application_layouts[previous_key].device_raw_profiles[previous_id], raw);
        }
        let id = runtime_fixture_adopt(&mut app, &device.path, &key, &id, &raw, &packets);
        assert_eq!(app.app_settings.application_layouts[&key].layouts[&id].layers[15][61], code);
        assert_eq!(app.app_settings.application_layouts[&key].layouts[&id].name, format!("USB {half}"));
        assert!(app.application_layout_editor_options().iter().any(|(candidate,_)|candidate==&id));
        assert!(!packets.requests().iter().any(|p|p[..3]==[0xef,9,2]), "readback alone selected a profile");
        // The ordinary per-profile automatic-switching checkbox is an explicit
        // host matching action AFTER readback, never a cached payload grant.
        let profile = app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap();
        profile.executable = "firefox".into();
        profile.automatic_switching = true;
        let foreground = crate::application_layouts::DetectedApplication {
            executable: app.app_settings.application_layouts[&key].layouts[&id].executable.clone(),
            display_name: "Browser".into(), ..Default::default()
        };
        let discovery = crate::app_discovery::ApplicationDiscoverySnapshot {
            foreground: Some(foreground.clone()),
            foreground_status: crate::app_discovery::ForegroundStatus { backend:"confirmed USB owner".into(), state:crate::app_discovery::ForegroundState::Focused(foreground) },
            ..Default::default()
        };
        if manual {
            app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().automatic_switching = false;
            app.app_settings.application_layouts.get_mut(&key).unwrap().pending_builtin_auto_bind.remove(&id);
            app.update_application_layout_runtime_with(discovery.clone());
            assert_eq!(app.app_settings.application_layouts[&key].active_layout_id, "default",
                "manual positive was already satisfied by automatic matching");
        }
        let cursor = packets.requests().len();
        if manual {
            app.activate_application_layout("default");
            assert!(app.activate_application_layout(&id));
        }
        app.update_application_layout_runtime_with(discovery);
        assert_eq!(app.app_settings.application_layouts[&key].active_layout_id, id);
        runtime_fixture_wait_select(&packets, cursor, &raw);
        let snapshot = app.app_settings.application_layouts[&key].snapshot_for_layout(&app.app_settings.application_layouts[&key].layouts[&id]);
        assert_eq!(snapshot.device_raw.as_ref(), Some(&raw));
        assert_eq!(app.qmk_hid_hosts[&device.path].test_published_snapshot().device_raw.as_ref(), Some(&raw));
        if !manual && half == "app" {
            // Firmware's overlay lease expires while physical unlock is pending.
            // A fresh SELECT of the already confirmed record reactivates it.
            let ctx = egui::Context::default();
            app.unlock_open = true;
            assert_eq!(app.start_vial_unlock(&ctx), super::super::vial_hid_task::VialHidTaskStart::Started);
            let deadline = std::time::Instant::now()+std::time::Duration::from_secs(2);
            while app.vial_hid_task_active() {
                app.poll_vial_hid_task(&ctx);
                assert!(std::time::Instant::now()<deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(unlock_pending.load(std::sync::atomic::Ordering::SeqCst));
            std::thread::sleep(std::time::Duration::from_millis(2600));
            let trace_start = packets.requests().len();
            assert_eq!(app.start_vial_unlock_poll(&ctx), super::super::vial_hid_task::VialHidTaskStart::Started);
            let deadline = std::time::Instant::now()+std::time::Duration::from_secs(2);
            while app.vial_hid_task_active() {
                app.poll_vial_hid_task(&ctx);
                assert!(std::time::Instant::now()<deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(app.vial_unlocked, Some(true));
            runtime_fixture_wait_select(&packets, trace_start, &raw);
            if let Some(path) = std::env::var_os("ENTROPY_SM62_UNLOCK_TRACE") {
                std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({
                    "initial":snapshot.packets(), "resident_full":raw,
                    "keepalive":snapshot.keepalive_packet(), "after_unlock":&packets.requests()[trace_start..]
                })).unwrap()).unwrap();
            }
        }
        if let Some(root) = std::env::var_os("ENTROPY_MANUAL_ROLE_TRACE") {
            std::fs::write(std::path::PathBuf::from(root).join(format!("{serial}.json")),
                serde_json::to_vec_pretty(&serde_json::json!({"serial":serial,"profile":id,"requests":packets.requests()})).unwrap()).unwrap();
        }
        assert!(!packets.requests().iter().any(|p|p[0]==5||p[..2]==[0xfe,0x0b]||p[..2]==[0xef,9]&&matches!(p[2],1|4)), "confirmed record selection rewrote device data");
        previous_owner = Some((key, id));
    }
}
