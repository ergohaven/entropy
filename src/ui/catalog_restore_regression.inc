#[test]
fn sm62_runtime_omits_macropad_catalog_without_other_device_changes() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();let mut app=name_review_device_app(true);
    app.app_settings.application_layouts.insert("unrelated-k04".into(),Default::default());
    let other=app.app_settings.application_layouts["unrelated-k04"].clone();
    app.update_application_layout_runtime_with(Default::default());
    let s=app.application_layout_settings().unwrap();
    assert_eq!(s.layouts.len(),1,"normal SM62 runtime must not restore Macropad presets");
    let options=app.application_layout_editor_options();
    assert_eq!(options.len(),1);
    assert_eq!(options[0].0,crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID);
    assert!(app.application_layout_editor_grouped_options(&options).is_empty());
    let base=&s.layouts["default"].layers;
    for p in s.layouts.values(){assert_eq!(&p.layers,base,"standard SM62 metadata must not apply Macropad bindings");assert!(p.encoder_stacks.iter().all(Vec::is_empty));}
    assert_eq!(app.app_settings.application_layouts["unrelated-k04"],other);
    let key=app.application_layout_device_key().unwrap();let saved=crate::app::app_storage::load_app_settings();
    assert_eq!(saved.application_layouts[&key],*app.application_layout_settings().unwrap());
}
