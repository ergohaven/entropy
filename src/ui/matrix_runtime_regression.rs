#[test]
fn sm62_matrix_idle_ticks_do_not_serialize_unchanged_profile_library(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.main_menu_tab=MainMenuTab::Settings;app.settings_tab=SettingsTab::MatrixTester;
    app.update_application_layout_runtime_with(Default::default());
    crate::app::app_storage::take_settings_serialization_count();
    for _ in 0..8 {app.update_application_layout_runtime_with(Default::default());}
    assert_eq!(crate::app::app_storage::take_settings_serialization_count(),0,"idle Matrix Tester frames repeatedly serialize/read all profiles on UI thread");
    let key=app.application_layout_device_key().unwrap();
    let id=app.app_settings.application_layouts[&key].layouts.keys().find(|id|id.as_str()!="default").unwrap().clone();
    app.app_settings.application_layouts.get_mut(&key).unwrap().layouts.get_mut(&id).unwrap().layers[15][61]=42;
    app.update_application_layout_runtime_with(Default::default());
    assert!(crate::app::app_storage::take_settings_serialization_count()>0,"changed library must still be persisted before publication");
    let loaded=crate::app::app_storage::load_app_settings();
    assert_eq!(loaded.application_layouts[&key].layouts[&id].layers[15][61],42);
}
#[test]
fn sm62_matrix_persistence_proof_does_not_hide_external_storage_failure(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_device_app(true);
    app.update_application_layout_runtime_with(Default::default());
    let path=crate::app::app_storage::app_settings_path();
    std::fs::write(&path,b"{broken-settings").unwrap();
    crate::app::app_storage::take_settings_serialization_count();
    app.update_application_layout_runtime_with(Default::default());
    assert!(crate::app::app_storage::take_settings_serialization_count()>0);
    assert!(app.status_msg.contains("persistence failed"),"{}",app.status_msg);
    assert_eq!(std::fs::read(path).unwrap(),b"{broken-settings");
}
