#[test]
fn resident_save_preserves_prior_valid_backup_and_migrates_inactive_libraries_once() {
    let _scope=ScopedTempConfig::new();let path=app_settings_path();std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut settings=AppSettings::default();let mut legacy=crate::application_layouts::DeviceApplicationLayouts::default();
    legacy.layouts.get_mut("default").unwrap().layers=vec![vec![4;62];16];
    settings.application_layouts.insert("inactive-previous-sm62".into(),legacy);
    settings.application_layouts.insert("unrelated-macropad".into(),Default::default());
    let other=settings.application_layouts["unrelated-macropad"].clone();
    let original=serde_json::to_vec_pretty(&settings).unwrap();std::fs::write(&path,&original).unwrap();
    let mut earlier=settings.clone();earlier.last_application_layout_device_name=Some("earlier preserved state".into());
    let backup=path.with_extension("resident100-backup.json");let earlier_bytes=serde_json::to_vec_pretty(&earlier).unwrap();std::fs::write(&backup,&earlier_bytes).unwrap();
    settings.last_application_layout_device_name=Some("current selected SM62".into());
    try_save_app_settings(&settings).expect("a valid prior backup must not permanently disable saving and profile publication");
    assert_eq!(std::fs::read(&backup).unwrap(),earlier_bytes,"never overwrite a prior backup");
    assert!(std::fs::read_dir(path.parent().unwrap()).unwrap().filter_map(Result::ok).any(|e|e.path()!=path&&e.path()!=backup&&std::fs::read(e.path()).ok().as_ref()==Some(&original)),"the exact current legacy source must have its own durable backup");
    for name in ["edit one","edit two"] {settings.last_application_layout_device_name=Some(name.into());try_save_app_settings(&settings).unwrap();}
    let stored:AppSettings=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored.application_layouts["inactive-previous-sm62"].resident_mapping_version,1);
    assert_eq!(stored.application_layouts["unrelated-macropad"],other);
}

#[test]
fn resident_save_still_refuses_a_malformed_existing_backup_without_overwriting_source() {
    let _scope=ScopedTempConfig::new();let path=app_settings_path();std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut settings=AppSettings::default();let mut legacy=crate::application_layouts::DeviceApplicationLayouts::default();legacy.layouts.get_mut("default").unwrap().layers=vec![vec![4;62];16];settings.application_layouts.insert("sm62".into(),legacy);
    let original=serde_json::to_vec_pretty(&settings).unwrap();std::fs::write(&path,&original).unwrap();let backup=path.with_extension("resident100-backup.json");std::fs::write(&backup,b"malformed preserved data").unwrap();
    settings.last_application_layout_device_name=Some("changed".into());assert!(try_save_app_settings(&settings).is_err());assert_eq!(std::fs::read(&path).unwrap(),original);assert_eq!(std::fs::read(&backup).unwrap(),b"malformed preserved data");
}
