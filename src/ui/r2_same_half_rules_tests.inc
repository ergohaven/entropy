#[test]
fn sm62_review_same_half_replug_restores_only_confirmed_profile_rules() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let serial_a="C281FE0DD04A01230000000000000000";
    let serial_b="C281FE0DD04A04560000000000000000";
    let mut first=name_review_device_app(true);
    let mut device=first.device_manager.devices()[0].clone();
    device.serial_number=serial_a.into();device.instance_token="attachment-before-replug".into();
    first.device_manager.replace_devices(vec![device.clone()]);
    first.update_application_layout_runtime_with(Default::default());
    let old_key=first.application_layout_device_key().unwrap();
    let settings=first.application_layout_settings_mut().unwrap();
    let id=settings.create_for_application_named(&crate::application_layouts::DetectedApplication{
        executable:"rule-repair-helper".into(),identities:vec!["org.example.RuleRepair".into()],display_name:"User rule".into(),..Default::default()
    },Some("My custom rule"),"work-session");
    settings.category_names.insert("custom:77".into(),"My work".into());
    settings.layouts.get_mut(&id).unwrap().custom_category_id=Some("custom:77".into());
    let original_identities=settings.layouts[&id].application_identities.clone();
    let path=r1_attach_inert_owner(&mut first);
    first.qmk_hid_hosts[&path].test_set_device_inventory_for_host_id(&id);
    let record=first.qmk_hid_hosts[&path].completed_legacy_archive().unwrap().remove(0);
    // Historical persisted rule fixture: only matching metadata was saved by
    // the former resident reader. Current runtime must not invent this evidence.
    let rule=crate::application_layouts::Sm62HostRule::capture(first.application_layout_settings().unwrap(),
        &first.application_layout_settings().unwrap().layouts[&id],record.identity_crc[..20].try_into().unwrap());
    first.app_settings.sm62_host_rules.insert(device.sm62_host_rule_key().unwrap(),
        std::collections::BTreeMap::from([(id.clone(),rule)]));
    first.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,12);
    first.application_layout_settings_mut().unwrap().layouts.get_mut("default").unwrap().layers[0][0]=42;
    first.update_application_layout_runtime_with(Default::default());
    assert_eq!(first.application_layout_settings().unwrap().layouts[&id].layers[0][0],12,"archive overwrote a newer host draft");
    runtime_fixture_assert_archive_backup(&first,&old_key,&record.raw);
    let saved=crate::app::app_storage::load_app_settings();
    let old_library=saved.application_layouts[&old_key].clone();
    first.qmk_hid_hosts.clear();
    for (serial,restore) in [(serial_a,true),(serial_b,false)] {
        let mut next=name_review_device_app(true);
        next.app_settings=saved.clone();
        let mut device=next.device_manager.devices()[0].clone();
        device.serial_number=serial.into();device.instance_token=format!("attachment-after-{serial}");
        next.device_manager.replace_devices(vec![device.clone()]);
        let native_default=next.application_layer_seed(next.layout.as_ref().unwrap())[0][0];
        let (hid,rec,_)=runtime_fixture_hid(vec![(7,record.raw.clone())]);
        runtime_fixture_connect(&mut next,&device,hid);
        let key=next.application_layout_device_key().unwrap();
        assert_ne!(key,old_key,"replug cannot inherit the previous enumeration's payload namespace");
        let adopted=runtime_fixture_adopt(&mut next,&device.path,&key,&id,&record.raw,&rec);
        let settings=next.application_layout_settings().unwrap();
        if restore {
            assert_eq!(adopted,id,"same-half legacy rule lost its stable host ID");
            let profile=&settings.layouts[&id];
            assert_eq!(profile.executable,"rule-repair-helper");
            assert_eq!(profile.application_identities,original_identities);
            assert_eq!(profile.title_contains,"work-session");
            assert!(profile.automatic_switching);
            assert_eq!(profile.custom_category_id.as_deref(),Some("custom:77"));
            assert_eq!(settings.category_names.get("custom:77").map(String::as_str),Some("My work"));
            assert_eq!(profile.layers[0][0],9,"legacy rule restored obsolete host payload instead of readback");
        } else {
            assert_ne!(adopted,id);
            assert!(!settings.layouts.contains_key(&id),"another physical half inherited saved matching metadata");
            let profile=&settings.layouts[&adopted];
            assert!(profile.executable.is_empty());assert!(!profile.automatic_switching);
            assert_eq!(profile.layers[0][0],9);
        }
        assert_eq!(settings.device_raw_profiles[&adopted],record.raw);
        assert_eq!(settings.layouts["default"].layers[0][0],native_default);
        assert!(settings.resident_deleted.is_empty());
        assert_eq!(next.app_settings.application_layouts[&old_key],old_library,"replug changed the old host draft backup");
        runtime_fixture_assert_archive_backup(&next,&key,&record.raw);
        let foreground=crate::application_layouts::DetectedApplication {
            executable:"rule-repair-helper".into(),identities:vec!["org.example.RuleRepair".into()],
            window_title:"work-session - project".into(),..Default::default()
        };
        let cursor=rec.requests().len();
        next.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot {
            foreground:Some(foreground.clone()),
            foreground_status:crate::app_discovery::ForegroundStatus {backend:"same-half rule recovery fixture".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},
            ..Default::default()
        });
        assert_eq!(next.application_layout_settings().unwrap().active_layout_id,if restore {id.as_str()} else {"default"});
        if restore {runtime_fixture_wait_ram(&rec,cursor,&record.raw);}
        else {assert!(!next.qmk_hid_hosts[&device.path].test_published_snapshot().active);}
        assert!(!rec.requests().iter().any(|p|p[0]==5||p[..2]==[0xfe,0x0b]||p[..2]==[0xef,9]&&matches!(p[2],1|2|4)),"rule recovery mutated native state or the archive");
        next.qmk_hid_hosts.clear();
    }
}
