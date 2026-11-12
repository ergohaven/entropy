#[test]
fn sm62_runtime_restores_complete_standard_catalog_without_other_device_changes() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();let mut app=name_review_device_app(true);
    app.app_settings.application_layouts.insert("unrelated-k04".into(),Default::default());
    let other=app.app_settings.application_layouts["unrelated-k04"].clone();
    app.update_application_layout_runtime_with(Default::default());
    let s=app.application_layout_settings().unwrap();let presets=crate::application_layouts::builtin_application_layout_presets();
    assert!(!presets.is_empty());
    for preset in &presets {assert!(s.layouts.values().any(|p|p.executable==preset.executable),"missing standard SM62 profile {} after normal runtime update",preset.id);}
    let base=&s.layouts["default"].layers;
    for p in s.layouts.values(){assert_eq!(&p.layers,base,"standard SM62 metadata must not apply Macropad bindings");assert!(p.encoder_stacks.iter().all(Vec::is_empty));}
    assert_eq!(app.app_settings.application_layouts["unrelated-k04"],other);
    let key=app.application_layout_device_key().unwrap();let saved=crate::app::app_storage::load_app_settings();
    assert_eq!(saved.application_layouts[&key],*app.application_layout_settings().unwrap());
}
