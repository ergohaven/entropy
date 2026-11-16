#[test]
fn k04_module_page_shows_only_the_selected_pointing_mode() {
    fn text(shape:&egui::Shape,out:&mut Vec<String>){match shape {egui::Shape::Text(t)=>out.push(t.galley.job.text.clone()),egui::Shape::Vec(v)=>for s in v{text(s,out)},_=>{}}}
    for lang in [crate::i18n::Language::English,crate::i18n::Language::Russian] {
        let mut app=EntropyApp::new_inert_for_test();add_filterable_module_groups(&mut app);
        app.module_settings.supported=true;app.module_settings.set_value(149,2);
        app.app_settings.language=lang;
        let mut layout=encoder_visibility_layout("Hide left encoder","Hide right encoder");layout.name="K:04".into();app.layout=Some(layout);
        let (hid,rec)=crate::hid::HidDevice::test_device();app.hid_device=Some(hid.into());
        let labels:Vec<_>=[124,125,126].iter().map(|qsid|{let g=&app.module_settings.groups[0];let f=g.fields.iter().find(|f|f.qsid==*qsid).unwrap();app.module_setting_label(g.kind,&f.title)}).collect();
        for mode in 0..4 {
            app.module_settings.set_value(134,mode);let ctx=egui::Context::default();let mut painted=Vec::new();
            for _ in 0..3 {let out=crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1600.0,1800.0))),..Default::default()},|ui|{let r=ui.available_rect_before_wrap();app.draw_module_settings_page(ui,r);}));painted.clear();for s in &out.shapes{text(&s.shape,&mut painted)}}
            // Restore the selected-mode list on K:04 without changing the
            // active firmware mode or issuing hardware writes while painting.
            for (index, label) in labels.iter().enumerate() {
                assert_eq!(painted.contains(label), mode as usize == index + 1,
                    "K:04 selected-mode list: mode={mode}, label={label}, lang={lang:?}");
            }
            assert_eq!(app.module_settings.values.get(&134), Some(&mode));
        }
        assert!(rec.requests().is_empty(),"ordinary module-page paint wrote hardware");
    }
}
