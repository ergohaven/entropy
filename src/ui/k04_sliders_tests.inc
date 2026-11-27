fn k04_slider_definitions() -> std::collections::BTreeMap<String, serde_json::Value> {
    serde_json::from_str(include_str!("../../tests/fixtures/k04-rmk-module-ui-series.json")).unwrap()
}

#[test]
fn k04_sliders_painted_page_ru_en_sides_modules_modes_and_no_writes() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    for definition in k04_slider_definitions().values() {
        for lang in [crate::i18n::Language::Russian, crate::i18n::Language::English] {
            let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, 2, 0);
            app.app_settings.language = lang;
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 1400.0))
                .with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()), app);
            for side in 0..=1 { for module in [2, 3] { for mode in 0..=3 {
                h.state_mut().module_settings.set_selected_module_group(side);
                h.state_mut().module_settings.set_value(149 + side as u16, module);
                h.state_mut().module_settings.set_value(134 + side as u16, mode);
                let before = h.state().module_settings.values.clone();
                h.run();
                assert_eq!(h.get_all_by_role(Role::Slider).count(), if mode == 0 {1} else {2});
                if mode != 0 {
                    h.get_by_label(crate::i18n::tr_catalog(lang, match mode {
                        1 => "modules_settings.speed_more_precise", 2 => "modules_settings.speed_smoother", _ => "modules_settings.speed_slower" }));
                    h.get_by_label(crate::i18n::tr_catalog(lang, "modules_settings.speed_faster"));
                }
                assert_eq!(h.state().module_settings.values, before);
                assert!(h.state().settings_write_task.is_none());
                for id in [124,125,126,127,128,129,324] {assert_eq!(h.state().pending_settings_write_value(id),None);}
            }}}
        }
    }
}

#[test]
fn k04_sliders_all_loaded_bytes_and_unknown_timeout_survive_paint() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    for definition in k04_slider_definitions().values() {
        for id in [124,125,126,127,128,129,324] {
            let app = series_ui_app(definition, FirmwareProtocol::Rynk, usize::from(id >= 127 && id <= 129), 2, 1);
            let gi = app.module_settings.groups.iter().position(|g|g.fields.iter().any(|f|f.qsid==id)).unwrap();
            let fi = app.module_settings.groups[gi].fields.iter().position(|f|f.qsid==id).unwrap();
            let mut h = egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs()
                .build_ui_state(move |ui,app:&mut EntropyApp| app.draw_module_settings_field_row(ui,gi,fi,700.0,48.0,false), app);
            for raw in 0..=255 {
                h.state_mut().module_settings.set_value(id,raw);
                h.run_steps(1);
                let slider=h.get_by_role(Role::Slider);
                if id != 324 && raw > 0 {assert_eq!(slider.accesskit_node().numeric_value(),Some(f64::from(256-raw)));}
                if id == 324 && raw <= 5 {assert_eq!(slider.accesskit_node().numeric_value(),Some(f64::from(raw)));}
                assert_eq!(h.state().module_settings.value(id),raw);
                assert_eq!(h.state().pending_settings_write_value(id),None);
                assert!(h.query_by_role(Role::TextInput).is_none());
            }
        }
    }
}

#[test]
fn k04_sliders_real_rynk_owner_accessibility_pointer_set_reload() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    use crate::rynk::session::testing::{board,link,Stored};
    use rmk_types_rynk::protocol::rynk::Cmd;
    for (name, definition) in k04_slider_definitions() {
        for module in [2,3] { for id in [124,125,126,127,128,129,324] {
            let mut app=series_ui_app(&definition,FirmwareProtocol::Rynk,usize::from(id>=127 && id<=129),module,1);
            let gi=app.module_settings.groups.iter().position(|g|g.fields.iter().any(|f|f.qsid==id)).unwrap();
            let fi=app.module_settings.groups[gi].fields.iter().position(|f|f.qsid==id).unwrap();
            if id==324 {assert_eq!(app.module_settings.groups[gi].fields[fi].variants,["250 ms","500 ms","750 ms","1000 ms","1250 ms","1500 ms"]);}
            let mut stored=Stored::new(8,8,1);
            for (&key,&value) in &app.module_settings.values {stored.settings.insert(key,vec![value as u8]);}
            stored.settings.insert(id,vec![if id==324 {5}else{16}]);
            app.module_settings.set_value(id,if id==324 {5}else{16});
            let (board,state)=board(stored);
            let session=crate::rynk::session::RynkSession::new(link(&format!("k04-slider-{name}-{module}-{id}"),&board));
            app.hid_device=Some(session.into());
            let mut h=egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs()
                .build_ui_state(move |ui,app:&mut EntropyApp|app.draw_module_settings_field_row(ui,gi,fi,700.0,48.0,false),app);
            h.run(); assert!(state.lock().unwrap().writes().is_empty());
            let edits:Vec<u16>=if id==324 {(0..6).collect()}else{vec![1,2,64,128,200,254,255]};
            for displayed in edits {
                let raw=if id==324 {displayed}else{256-displayed};
                let before=state.lock().unwrap().settings.clone();
                let count=state.lock().unwrap().writes().len();
                h.get_by_role(Role::Slider).set_value_accesskit(f64::from(displayed)); h.run_steps(1);
                assert_eq!(h.state().pending_settings_write_value(id),Some(raw));
                drain_hid_writes(h.state_mut(),&egui::Context::default());
                {
                    let s=state.lock().unwrap();
                    assert_eq!(s.writes().len(),count+1); assert_eq!(s.writes().last(),Some(&Cmd::SetSetting));
                    assert_eq!(s.settings.get(&id),Some(&vec![raw as u8]));
                    for (key,value) in &before {if *key!=id {assert_eq!(s.settings.get(key),Some(value));}}
                }
                let session=h.state().hid_device.as_ref().unwrap().as_rynk().unwrap();
                let loaded=EntropyApp::read_module_settings(&definition,&[id],session);
                assert_eq!(loaded.value(id),raw);
            }
            let rect=h.get_by_role(Role::Slider).rect();
            for (pos,raw) in [(rect.left_center(),if id==324 {0}else{255}),(rect.right_center(),if id==324 {5}else{1})] {
                h.drag_at(pos);h.run_steps(1);h.drop_at(pos);h.run_steps(1);
                drain_hid_writes(h.state_mut(),&egui::Context::default());
                assert_eq!(state.lock().unwrap().settings.get(&id),Some(&vec![raw as u8]));
                let loaded=EntropyApp::read_module_settings(&definition,&[id],h.state().hid_device.as_ref().unwrap().as_rynk().unwrap());
                assert_eq!(loaded.value(id),raw);
            }
        }}
    }
}

#[test]
fn k04_sliders_contract_guards_do_not_extend_rotations_or_other_protocols() {
    for definition in k04_slider_definitions().values() {
        for protocol in [FirmwareProtocol::Vial,FirmwareProtocol::Rynk] {
            let mut app=series_ui_app(definition,protocol,0,2,1);
            let field=app.module_settings.groups[0].fields.iter().find(|f|f.qsid==124).unwrap().clone();
            let auto=app.module_settings.groups.iter().find(|g|g.kind==ModuleSettingsGroupKind::AutoLayer).unwrap().fields.iter().find(|f|f.qsid==324).unwrap().clone();
            assert_eq!(app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left,&field),protocol==FirmwareProtocol::Rynk);
            assert_eq!(app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer,&auto),protocol==FirmwareProtocol::Rynk);
            if protocol==FirmwareProtocol::Vial {continue;}
            for module in [0,1] {app.module_settings.set_value(149,module);assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left,&field));}
            app.module_settings.set_value(149,2);
            for n in 0..6 {
                let mut f=field.clone(); match n {0=>f.qsid=130,1=>f.width=2,2=>f.min=0,3=>f.max=256,4=>f.kind=ModuleSettingKind::Select,_=>f.title="Other sens".into()};
                assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left,&f));
                let mut f=auto.clone();match n {0=>f.qsid=325,1=>f.width=2,2=>f.min=1,3=>f.variants.reverse(),4=>f.variants[0]="250".into(),_=>f.max=6};
                assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer,&f));
            }
            for g in &app.module_settings.groups {for f in &g.fields {assert!(!app.phenom_ball_rotation_field(g.kind,f));assert!(!app.phenom_left_touch_rotation_field(g.kind,f));}}
            app.layout.as_mut().unwrap().name="HPD".into();
            assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left,&field));
            assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer,&auto));
        }
    }
}

#[test]
fn k04_sliders_unsent_coalescing_keeps_inflight_and_other_fields() {
    use crate::rynk::session::testing::{board,link,Stored};
    use rmk_types_rynk::protocol::rynk::Cmd;
    let defs=k04_slider_definitions();let definition=defs.values().next().unwrap();
    let mut app=series_ui_app(definition,FirmwareProtocol::Rynk,0,2,1);
    app.module_settings.set_value(150,2);
    let (board,state)=board(Stored::new(8,8,1));
    app.hid_device=Some(crate::rynk::session::RynkSession::new(link("k04-slider-coalesce",&board)).into());
    let mut fields=vec![];
    for id in [124,125,127,324,130] {
        let gi=app.module_settings.groups.iter().position(|g|g.fields.iter().any(|f|f.qsid==id)).unwrap();
        fields.push((gi,app.module_settings.groups[gi].fields.iter().find(|f|f.qsid==id).unwrap().clone()));
    }
    let (gi,f)=&fields[0]; app.write_module_setting_value(*gi,f,90);
    // queue_module_setting_write starts the real owner immediately.
    assert!(app.settings_write_task.is_some());
    assert!(app.hid_device.is_none());
    for value in [80,70,60] {app.write_module_setting_value(*gi,f,value);}
    for (gi,f) in &fields[1..4] {
        for value in [1,2,3] {app.write_module_setting_value(*gi,f,value);}
    }
    let (gi,f)=&fields[4]; for value in [1,2] {app.write_module_setting_value(*gi,f,value);}
    app.module_settings.set_selected_module_group(1);app.module_settings.set_value(150,3);
    drain_hid_writes(&mut app,&egui::Context::default());
    let s=state.lock().unwrap();
    assert_eq!(s.writes().iter().filter(|c|**c==Cmd::SetSetting).count(),7,"inflight + four final sliders + two ordered rotations");
    for (id,value) in [(124,60),(125,3),(127,3),(324,3),(130,2)] {assert_eq!(s.settings.get(&id),Some(&vec![value]));}
}
