fn product_page_texts(app:&mut EntropyApp,page:u8,dark:bool)->Vec<String>{
    let ctx=egui::Context::default();
    ctx.set_visuals(if dark {egui::Visuals::dark()}else{egui::Visuals::light()});
    let rect=egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1400.,2400.));
    let output=crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput{screen_rect:Some(rect),..Default::default()},|ui|{
        match page{
            0=>app.draw_application_layouts_settings_page(ui,rect),
            1=>app.draw_about_device_page(ui,rect),
            2=>app.draw_app_settings_page(ui,rect),
            _=>unreachable!(),
        }
    }));
    fn collect(s:&egui::Shape,out:&mut Vec<String>){match s{egui::Shape::Text(t)=>out.push(t.galley.text().to_owned()),egui::Shape::Vec(v)=>for s in v{collect(s,out)},_=>{}}}
    let mut texts=Vec::new();for s in &output.shapes{collect(&s.shape,&mut texts);}output.drop_without_applying_deltas();texts
}
#[test]
fn final_product_autolayer_has_no_internal_sync_status_or_retry(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for language in [crate::i18n::Language::English,crate::i18n::Language::Russian]{for dark in [false,true]{
        let mut app=name_review_device_app(true);app.app_settings.language=language;
        let texts=product_page_texts(&mut app,0,dark);let all=texts.join("\n");
        assert!(all.contains("Autolayer")||all.contains("Автослой"),"fixture must reach actual autolayer page: {texts:?}");
        assert!(!all.contains("native Default")&&!all.contains("Native Default")&&!all.contains("Нативный Default")&&!all.contains("Retry sync")&&!all.contains("Повторить синхронизацию"),"internal resident tooling leaked into final UI: {texts:?}");
        let key=app.application_layout_device_key().unwrap();
        assert_eq!(app.app_settings.application_layouts[&key].layouts.len(),1,"SM62 has no automatic Macropad presets");
    }}
}
#[test]
fn final_product_about_page_has_no_measurement_controls(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for language in [crate::i18n::Language::English,crate::i18n::Language::Russian]{for dark in [false,true]{
        let mut app=name_review_device_app(true);app.app_settings.language=language;
        let texts=product_page_texts(&mut app,1,dark);let all=texts.join("\n");
        assert!(!texts.is_empty());
        assert!(!all.contains("FPS")&&!all.contains("0x19")&&!all.contains("ARM")&&!all.contains("Copy log")&&!all.contains("Копировать путь лога"),"measurement controls leaked into final About page: {texts:?}");
        assert!(app.sm62_fps.is_none());
    }}
}
#[test]
fn final_product_app_settings_keeps_existing_diagnostics_and_tail_controls(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for language in [crate::i18n::Language::English,crate::i18n::Language::Russian]{for dark in [false,true]{
        let mut app=name_review_device_app(true);app.app_settings.language=language;
        let texts=product_page_texts(&mut app,2,dark);
        let diagnostic=crate::i18n::tr_catalog(language,"ui.diagnostics_mode_label");
        assert!(texts.iter().any(|t|t==diagnostic),"preexisting Diagnostics switch must remain: {texts:?}");
        let mut keys=vec!["ui.show_made_by_signature_label","ui.middle_click_transparent_label"];
        if super::super::update_notifications::SUPPORTED {keys.extend(["notifications.app_label","notifications.firmware_label"]);}
        for key in keys{
            let label=crate::i18n::tr_catalog(language,key);
            assert!(texts.iter().any(|t|t==label),"normal trailing control disappeared: {key}, {texts:?}");
        }
    }}
}
