#[test]
fn r1_first_owner_layer_rename_is_explicit_without_accessor_prewarm() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    let id=app.application_layout_settings().unwrap().layouts.values()
        .find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    assert!(app.rename_application_layout_layer(&id,0,"Explicit first-owner rename".into()));
    for _ in 0..2 { app.update_application_layout_runtime_with(Default::default()); }
    let library=app.qmk_hid_hosts[&path].test_published_library();
    let profile=library.iter().find(|p|p.profile_id.as_ref()==Some(&id))
        .expect("first-owner layer rename was consumed as a baseline instead of explicit user intent");
    assert_eq!(profile.layer_names[0],"Explicit first-owner rename");
}
