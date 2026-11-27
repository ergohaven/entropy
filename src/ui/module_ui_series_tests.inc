fn series_ui_app(definition: &serde_json::Value, firmware: FirmwareProtocol, side: usize, module: u16, mode: u16) -> EntropyApp {
    let mut app = phenom_speed_app();
    let mut layout = crate::keyboard::KeyboardLayout::from_vial_json(definition).unwrap();
    layout.firmware = firmware;
    app.layout = Some(layout);
    app.module_settings = EntropyApp::module_settings_from_definition(definition, &(0..500).collect::<Vec<_>>());
    app.module_settings.set_selected_module_group(side);
    let selector = app.module_settings.groups[side].module_selector_field().unwrap().qsid;
    app.module_settings.set_value(selector, module);
    app.module_settings.set_value(134 + side as u16, mode);
    app
}

#[test]
fn module_series_k04_rmk_all_six_descriptors_order_copy_and_painted_controls() {
    let definitions: std::collections::BTreeMap<String,serde_json::Value> = serde_json::from_str(include_str!("../../tests/fixtures/k04-rmk-module-ui-series.json")).unwrap();
    assert_eq!(definitions.len(),6);
    let phenom: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
    for (variant, definition) in definitions {
        for language in [crate::i18n::Language::Russian,crate::i18n::Language::English] {
            for side in 0..=1 { for (k_module,p_module) in [(2,1),(3,2)] { for mode in 0..=3 {
                let mut app = series_ui_app(&definition,FirmwareProtocol::Rynk,side,k_module,mode);
                let mut reference = phenom_speed_app();
                reference.module_settings = EntropyApp::module_settings_from_definition(&phenom,&(0..500).collect::<Vec<_>>());
                reference.module_settings.set_selected_module_group(side);
                reference.module_settings.set_value(372+side as u16,p_module);
                app.app_settings.language=language; reference.app_settings.language=language;
                let before = app.module_settings.values.clone();
                let owners = format!("{:?}",app.module_settings.groups);
                let kind=app.module_settings.groups[side].kind;
                let fields=app.module_settings.groups[side].fields.clone();
                for field in &fields {
                    assert_eq!(app.module_setting_label(kind,&field.title),reference.module_setting_label(kind,&field.title),"{variant}: {}",field.title);
                    assert_eq!(app.module_setting_tooltip(kind,field),reference.module_setting_tooltip(kind,field),"{variant}: {}",field.title);
                    // Sliders are approved; Phenom's rotation origins remain excluded.
                    assert!(!app.phenom_ball_rotation_field(kind,field));
                    assert!(!app.phenom_left_touch_rotation_field(kind,field));
                    assert_eq!(app.phenom_mode_speed_field(kind,field), (124..=129).contains(&field.qsid));
                }
                let auto = app.module_settings.groups.iter().find(|g|g.kind==ModuleSettingsGroupKind::AutoLayer).unwrap();
                for field in &auto.fields {
                    assert_eq!(app.module_setting_label(auto.kind,&field.title),reference.module_setting_label(auto.kind,&field.title));
                    assert_eq!(app.module_setting_tooltip(auto.kind,field),reference.module_setting_tooltip(auto.kind,field));
                    assert_eq!(app.phenom_auto_timeout_field(auto.kind,field), field.qsid == 324, "K04 keeps its six advertised preset indices");
                }
                let rows=app.module_settings_rows();
                let visible:Vec<_>=rows.iter().filter_map(|row| if let ModuleSettingsRow::Field{group_idx,field_idx}=row {
                    Some((*group_idx,app.module_settings.groups[*group_idx].fields[*field_idx].title.as_str()))
                }else{None}).collect();
                let position=|title:&str| visible.iter().position(|(_,t)|t.eq_ignore_ascii_case(title)).unwrap();
                let shared=if k_module==2 {vec!["Module","Ball DPI","Acceleration","Ball axis","Mode"]} else {vec!["Module","Touch DPI","Acceleration","Touch gestures","Touch axis","Mode"]};
                let places:Vec<_>=shared.iter().map(|t|position(t)).collect();
                assert!(places.windows(2).all(|p|p[0]<p[1]),"{variant}: {visible:?}");
                assert!(position("Sticky mode")>position("Auto layer timeout"));
                assert!(position("Auto layer timeout")>position("Auto layer"));
                let ctx=egui::Context::default();
                let mut painted=vec![];
                for _ in 0..6 {painted=phenom_page_frame(&mut app,&ctx,vec![]);}
                let ys:Vec<_>=shared.iter().map(|t|{
                    let label=app.module_setting_label(kind,t);
                    painted.iter().find(|(s,_)|s==&label).unwrap_or_else(||panic!("{variant}: missing {label}")).1.center().y
                }).collect();
                assert!(ys.windows(2).all(|p|p[0]<p[1]));
                if mode==0 {
                    ctx.global_style_mut(|s|s.interaction.tooltip_delay=0.0);
                    let rotation=if k_module==2 {"Ball axis"}else{"Touch axis"};
                    let field=fields.iter().find(|f|f.title==rotation).unwrap();
                    let label=app.module_setting_label(kind,rotation);
                    let hint=app.module_setting_tooltip(kind,field);
                    let pos=painted.iter().find(|(s,_)|s==&label).unwrap().1.center();
                    for _ in 0..12 {painted=phenom_page_frame(&mut app,&ctx,vec![egui::Event::PointerMoved(pos)]);}
                    assert!(painted.iter().any(|(s,_)|s==&hint),"{variant}: missing hover {hint}");
                }
                assert_eq!(app.module_settings.values,before);
                assert_eq!(format!("{:?}",app.module_settings.groups),owners);
            }}}
        }
    }
}

#[test]
fn module_series_phenom_family_keeps_presentation_and_k04_vial_remains_original() {
    let phenom:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
    for name in ["Phenom","Phenom Mini","Phenom Micro"] {
        let mut app=phenom_speed_app(); app.layout.as_mut().unwrap().name=name.into();
        app.module_settings=EntropyApp::module_settings_from_definition(&phenom,&(0..500).collect::<Vec<_>>());
        app.module_settings.set_value(372,1);
        let rows=app.module_settings_field_rows(0,Some(ModuleDeviceKind::Trackball),Some(PointerModeKind::Normal));
        let titles:Vec<_>=rows.iter().filter_map(|r|if let ModuleSettingsRow::Field{field_idx,..}=r{Some(app.module_settings.groups[0].fields[*field_idx].title.as_str())}else{None}).collect();
        assert!(titles.iter().position(|s|*s=="Ball DPI")<titles.iter().position(|s|*s=="Mode"));
    }
    let defs:std::collections::BTreeMap<String,serde_json::Value>=serde_json::from_str(include_str!("../../tests/fixtures/k04-rmk-module-ui-series.json")).unwrap();
    for def in defs.values() {
        let app=series_ui_app(def,FirmwareProtocol::Vial,0,2,0);
        let rows=app.module_settings_field_rows(0,Some(ModuleDeviceKind::Trackball),Some(PointerModeKind::Normal));
        let ids:Vec<_>=rows.iter().filter_map(|r|if let ModuleSettingsRow::Field{field_idx,..}=r{Some(*field_idx)}else{None}).collect();
        assert!(ids.windows(2).all(|p|p[0]<p[1]),"unapproved Vial K04 order changed");
    }
}
