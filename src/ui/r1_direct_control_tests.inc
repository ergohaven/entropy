fn r1_first_tick_direct_action_fixture() -> (EntropyApp,String,String) {
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    let key=app.application_layout_device_key().unwrap();
    let id=app.app_settings.application_layouts[&key].layouts.values()
        .find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    // Choose the fixture target without prewarming the mutable accessor.
    app.app_settings.application_layouts.get_mut(&key).unwrap().active_layout_id=id.clone();
    app.application_layout_editor_active=true;
    (app,path,id)
}
#[test]
fn r1_first_owner_control_undo_preserves_explicit_intent() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,path,id)=r1_first_tick_direct_action_fixture();
    let key=app.application_layout_device_key().unwrap();
    app.undo_application_layout_control(&key,&id,0,0,37);
    app.update_application_layout_runtime_with(Default::default());
    let library=app.qmk_hid_hosts[&path].test_published_library();
    let p=library.iter().find(|p|p.profile_id.as_ref()==Some(&id)).expect("explicit first-owner undo was lost");
    assert_eq!(p.layers[0][0],37);
}
#[test]
fn r1_first_owner_layer_paste_preserves_explicit_intent() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,path,id)=r1_first_tick_direct_action_fixture();
    let layout=app.layout.as_ref().unwrap();
    let snapshot=super::super::layer_operations::LayerSnapshot {
        keycodes:vec![crate::keyboard::KeyBinding::Vial(37);layout.keys.len()],
        encoder_keycodes:vec![37;layout.encoders.len()],
    };
    app.apply_layer_snapshot(0,snapshot,"layer_actions.paste");
    app.update_application_layout_runtime_with(Default::default());
    let library=app.qmk_hid_hosts[&path].test_published_library();
    let p=library.iter().find(|p|p.profile_id.as_ref()==Some(&id)).expect("explicit first-owner paste was lost");
    assert!(p.layers[0].iter().all(|k|*k==37));
}
