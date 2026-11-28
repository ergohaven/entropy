#[test]
fn r1_first_owner_layer_rename_is_explicit_without_accessor_prewarm() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    let id=app.application_layout_settings().unwrap().layouts.values()
        .find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    let key=app.application_layout_device_key().unwrap();
    app.app_settings.application_layouts.get_mut(&key).unwrap().active_layout_id=id.clone();
    r1_confirm_empty_archive(&mut app,&path);
    assert!(app.rename_application_layout_layer(&id,0,"Explicit first-owner rename".into()));
    for _ in 0..2 { app.update_application_layout_runtime_with(Default::default()); }
    let profile=r1_assert_active_ram(&app,&path,&id);
    assert!(app.application_layout_owner_intents.values().any(|ids|ids.contains(&id)),"first-owner rename intent was consumed as a baseline");
    assert_eq!(profile.layer_names[0],"Explicit first-owner rename");
}
