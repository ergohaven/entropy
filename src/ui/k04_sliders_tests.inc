fn k04_slider_definitions() -> std::collections::BTreeMap<String, serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/k04-rmk-module-ui-series.json"
    ))
    .unwrap()
}

#[test]
fn k04_sliders_painted_page_ru_en_sides_modules_modes_and_no_writes() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    for definition in k04_slider_definitions().values() {
        for lang in [
            crate::i18n::Language::Russian,
            crate::i18n::Language::English,
        ] {
            let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, 2, 0);
            app.app_settings.language = lang;
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1400.0, 1400.0))
                .with_accessibility_check(false)
                .allow_missing_glyphs()
                .build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()),
                    app,
                );
            for side in 0..=1 {
                for module in [2, 3] {
                    for mode in 0..=3 {
                        h.state_mut()
                            .module_settings
                            .set_selected_module_group(side);
                        h.state_mut()
                            .module_settings
                            .set_value(149 + side as u16, module);
                        h.state_mut()
                            .module_settings
                            .set_value(134 + side as u16, mode);
                        let before = h.state().module_settings.values.clone();
                        h.run();
                        assert_eq!(
                            h.get_all_by_role(Role::Slider).count(),
                            if mode == 0 { 1 } else { 2 }
                        );
                        if mode != 0 {
                            h.get_by_label(crate::i18n::tr_catalog(
                                lang,
                                match mode {
                                    1 => "modules_settings.speed_more_precise",
                                    2 => "modules_settings.speed_smoother",
                                    _ => "modules_settings.speed_slower",
                                },
                            ));
                            h.get_by_label(crate::i18n::tr_catalog(
                                lang,
                                "modules_settings.speed_faster",
                            ));
                        }
                        assert_eq!(h.state().module_settings.values, before);
                        assert!(h.state().settings_write_task.is_none());
                        for id in [124, 125, 126, 127, 128, 129, 324] {
                            assert_eq!(h.state().pending_settings_write_value(id), None);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn k04_sliders_all_loaded_bytes_and_unknown_timeout_survive_paint() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    for definition in k04_slider_definitions().values() {
        for id in [124, 125, 126, 127, 128, 129, 324] {
            let app = series_ui_app(
                definition,
                FirmwareProtocol::Rynk,
                usize::from(id >= 127 && id <= 129),
                2,
                1,
            );
            let gi = app
                .module_settings
                .groups
                .iter()
                .position(|g| g.fields.iter().any(|f| f.qsid == id))
                .unwrap();
            let fi = app.module_settings.groups[gi]
                .fields
                .iter()
                .position(|f| f.qsid == id)
                .unwrap();
            let mut h = egui_kittest::Harness::builder()
                .with_accessibility_check(false)
                .allow_missing_glyphs()
                .build_ui_state(
                    move |ui, app: &mut EntropyApp| {
                        app.draw_module_settings_field_row(ui, gi, fi, 700.0, 48.0, false)
                    },
                    app,
                );
            for raw in 0..=255 {
                h.state_mut().module_settings.set_value(id, raw);
                h.run_steps(1);
                let slider = h.get_by_role(Role::Slider);
                if id != 324 && raw > 0 {
                    assert_eq!(
                        slider.accesskit_node().numeric_value(),
                        Some(f64::from(256 - raw))
                    );
                }
                if id == 324 && raw <= 5 {
                    assert_eq!(
                        slider.accesskit_node().numeric_value(),
                        Some(f64::from(raw))
                    );
                }
                assert_eq!(h.state().module_settings.value(id), raw);
                assert_eq!(h.state().pending_settings_write_value(id), None);
                assert!(h.query_by_role(Role::TextInput).is_none());
            }
        }
    }
}

#[test]
fn k04_sliders_real_rynk_owner_accessibility_pointer_set_reload() {
    use crate::rynk::session::testing::{board, link, Stored};
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    use rmk_types_rynk::protocol::rynk::Cmd;
    for (name, definition) in k04_slider_definitions() {
        for module in [2, 3] {
            for id in [124, 125, 126, 127, 128, 129, 324] {
                let mut app = series_ui_app(
                    &definition,
                    FirmwareProtocol::Rynk,
                    usize::from(id >= 127 && id <= 129),
                    module,
                    1,
                );
                let gi = app
                    .module_settings
                    .groups
                    .iter()
                    .position(|g| g.fields.iter().any(|f| f.qsid == id))
                    .unwrap();
                let fi = app.module_settings.groups[gi]
                    .fields
                    .iter()
                    .position(|f| f.qsid == id)
                    .unwrap();
                if id == 324 {
                    assert_eq!(
                        app.module_settings.groups[gi].fields[fi].variants,
                        ["250 ms", "500 ms", "750 ms", "1000 ms", "1250 ms", "1500 ms"]
                    );
                }
                let mut stored = Stored::new(8, 8, 1);
                for (&key, &value) in &app.module_settings.values {
                    stored.settings.insert(key, vec![value as u8]);
                }
                stored
                    .settings
                    .insert(id, vec![if id == 324 { 5 } else { 16 }]);
                app.module_settings
                    .set_value(id, if id == 324 { 5 } else { 16 });
                let (board, state) = board(stored);
                let session = crate::rynk::session::RynkSession::new(link(
                    &format!("k04-slider-{name}-{module}-{id}"),
                    &board,
                ));
                app.hid_device = Some(session.into());
                let mut h = egui_kittest::Harness::builder()
                    .with_accessibility_check(false)
                    .allow_missing_glyphs()
                    .build_ui_state(
                        move |ui, app: &mut EntropyApp| {
                            app.draw_module_settings_field_row(ui, gi, fi, 700.0, 48.0, false)
                        },
                        app,
                    );
                h.run();
                assert!(state.lock().unwrap().writes().is_empty());
                let edits: Vec<u16> = if id == 324 {
                    (0..6).collect()
                } else {
                    vec![1, 2, 64, 128, 200, 254, 255]
                };
                for displayed in edits {
                    let raw = if id == 324 {
                        displayed
                    } else {
                        256 - displayed
                    };
                    let before = state.lock().unwrap().settings.clone();
                    let count = state.lock().unwrap().writes().len();
                    h.get_by_role(Role::Slider)
                        .set_value_accesskit(f64::from(displayed));
                    h.run_steps(1);
                    assert_eq!(h.state().pending_settings_write_value(id), Some(raw));
                    drain_hid_writes(h.state_mut(), &egui::Context::default());
                    {
                        let s = state.lock().unwrap();
                        assert_eq!(s.writes().len(), count + 1);
                        assert_eq!(s.writes().last(), Some(&Cmd::SetSetting));
                        assert_eq!(s.settings.get(&id), Some(&vec![raw as u8]));
                        for (key, value) in &before {
                            if *key != id {
                                assert_eq!(s.settings.get(key), Some(value));
                            }
                        }
                    }
                    let session = h.state().hid_device.as_ref().unwrap().as_rynk().unwrap();
                    let loaded = EntropyApp::read_module_settings(&definition, &[id], session);
                    assert_eq!(loaded.value(id), raw);
                }
                let rect = h.get_by_role(Role::Slider).rect();
                for (pos, raw) in [
                    (rect.left_center(), if id == 324 { 0 } else { 255 }),
                    (rect.right_center(), if id == 324 { 5 } else { 1 }),
                ] {
                    h.drag_at(pos);
                    h.run_steps(1);
                    h.drop_at(pos);
                    h.run_steps(1);
                    drain_hid_writes(h.state_mut(), &egui::Context::default());
                    assert_eq!(
                        state.lock().unwrap().settings.get(&id),
                        Some(&vec![raw as u8])
                    );
                    let loaded = EntropyApp::read_module_settings(
                        &definition,
                        &[id],
                        h.state().hid_device.as_ref().unwrap().as_rynk().unwrap(),
                    );
                    assert_eq!(loaded.value(id), raw);
                }
            }
        }
    }
}

#[test]
fn k04_sliders_contract_guards_do_not_extend_rotations_or_other_protocols() {
    for definition in k04_slider_definitions().values() {
        for protocol in [FirmwareProtocol::Vial, FirmwareProtocol::Rynk] {
            let mut app = series_ui_app(definition, protocol, 0, 2, 1);
            let field = app.module_settings.groups[0]
                .fields
                .iter()
                .find(|f| f.qsid == 124)
                .unwrap()
                .clone();
            let auto = app
                .module_settings
                .groups
                .iter()
                .find(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
                .unwrap()
                .fields
                .iter()
                .find(|f| f.qsid == 324)
                .unwrap()
                .clone();
            assert_eq!(
                app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left, &field),
                protocol == FirmwareProtocol::Rynk
            );
            assert_eq!(
                app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &auto),
                protocol == FirmwareProtocol::Rynk
            );
            if protocol == FirmwareProtocol::Vial {
                continue;
            }
            for module in [0, 1] {
                app.module_settings.set_value(149, module);
                assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left, &field));
            }
            app.module_settings.set_value(149, 2);
            for n in 0..6 {
                let mut f = field.clone();
                match n {
                    0 => f.qsid = 130,
                    1 => f.width = 2,
                    2 => f.min = 0,
                    3 => f.max = 256,
                    4 => f.kind = ModuleSettingKind::Select,
                    _ => f.title = "Other sens".into(),
                };
                assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left, &f));
                let mut f = auto.clone();
                match n {
                    0 => f.qsid = 325,
                    1 => f.width = 2,
                    2 => f.min = 1,
                    3 => f.variants.reverse(),
                    4 => f.variants[0] = "250".into(),
                    _ => f.max = 6,
                };
                assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &f));
            }
            for g in &app.module_settings.groups {
                for f in &g.fields {
                    assert!(!app.phenom_ball_rotation_field(g.kind, f));
                    assert!(!app.phenom_left_touch_rotation_field(g.kind, f));
                }
            }
            app.layout.as_mut().unwrap().name = "HPD".into();
            assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left, &field));
            assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &auto));
        }
    }
}

#[test]
fn k04_sliders_unsent_coalescing_keeps_inflight_and_other_fields() {
    use crate::rynk::session::testing::{board, link, Stored};
    use rmk_types_rynk::protocol::rynk::Cmd;
    let defs = k04_slider_definitions();
    let definition = defs.values().next().unwrap();
    let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, 2, 1);
    app.module_settings.set_value(150, 2);
    let (board, state) = board(Stored::new(8, 8, 1));
    app.hid_device =
        Some(crate::rynk::session::RynkSession::new(link("k04-slider-coalesce", &board)).into());
    let mut fields = vec![];
    for id in [124, 125, 127, 324, 130] {
        let gi = app
            .module_settings
            .groups
            .iter()
            .position(|g| g.fields.iter().any(|f| f.qsid == id))
            .unwrap();
        fields.push((
            gi,
            app.module_settings.groups[gi]
                .fields
                .iter()
                .find(|f| f.qsid == id)
                .unwrap()
                .clone(),
        ));
    }
    let (gi, f) = &fields[0];
    app.write_module_setting_value(*gi, f, 90);
    // queue_module_setting_write starts the real owner immediately.
    assert!(app.settings_write_task.is_some());
    assert!(app.hid_device.is_none());
    for value in [80, 70, 60] {
        app.write_module_setting_value(*gi, f, value);
    }
    for (gi, f) in &fields[1..4] {
        for value in [1, 2, 3] {
            app.write_module_setting_value(*gi, f, value);
        }
    }
    let (gi, f) = &fields[4];
    for value in [1, 2] {
        app.write_module_setting_value(*gi, f, value);
    }
    app.module_settings.set_selected_module_group(1);
    app.module_settings.set_value(150, 3);
    drain_hid_writes(&mut app, &egui::Context::default());
    let s = state.lock().unwrap();
    assert_eq!(
        s.writes().iter().filter(|c| **c == Cmd::SetSetting).count(),
        7,
        "inflight + four final sliders + two ordered rotations"
    );
    for (id, value) in [(124, 60), (125, 3), (127, 3), (324, 3), (130, 2)] {
        assert_eq!(s.settings.get(&id), Some(&vec![value]));
    }
}

fn k04_threshold_painted_text(out: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn collect(s: &egui::Shape, v: &mut Vec<(String, egui::Rect)>) {
        match s {
            egui::Shape::Text(t) => v.push((
                t.galley.job.text.clone(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            egui::Shape::Vec(shapes) => {
                for s in shapes {
                    collect(s, v);
                }
            }
            _ => {}
        }
    }
    let mut v = vec![];
    for s in &out.shapes {
        collect(&s.shape, &mut v);
    }
    v
}

#[test]
fn k04_threshold_layout_no_raw_number_after_more() {
    use egui_kittest::kittest::Queryable;
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(
        include_str!("../../tests/fixtures/k04-upstream-9e2aef92.json"),
    )
    .unwrap();
    let definition = definitions.values().next().unwrap();
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, 2, 3);
        app.app_settings.language = lang;
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1192.0, 1600.0))
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()),
                app,
            );
        for value in [0, 9, 10, 99, 100, 255] {
            h.state_mut().module_settings.set_value(334, value);
            h.run();
            let more = h
                .get_by_label(crate::i18n::tr_catalog(
                    lang,
                    "modules_settings.k04_threshold_more",
                ))
                .rect();
            let text = k04_threshold_painted_text(h.output());
            assert!(
                !text.iter().any(|(s, r)| s == &value.to_string()
                    && (r.center().y - more.center().y).abs() < 1.0
                    && r.left() >= more.right()),
                "raw threshold {value} still painted right of More: {text:?}"
            );
            assert!(
                text.iter().any(|(s, _)| s.ends_with(" ms")),
                "timeout milliseconds retained"
            );
            let threshold = text
                .iter()
                .find(|(s, _)| s == crate::i18n::tr_catalog(lang, "modules_settings.k04_threshold"))
                .unwrap()
                .1;
            for title in ["Auto layer deactivate on key", "Sticky mode"] {
                let field = h.state().module_settings.groups.iter().find_map(|g| {
                    g.fields
                        .iter()
                        .find(|f| f.title.eq_ignore_ascii_case(title))
                        .map(|f| (g.kind, f))
                });
                if let Some((kind, field)) = field {
                    let label = h.state().module_setting_label(kind, &field.title);
                    let row = text.iter().find(|(s, _)| s == &label).unwrap().1;
                    assert!(
                        (row.left() - threshold.left()).abs() < 0.1,
                        "loaded {value}: lower row drift"
                    );
                }
            }
            assert_eq!(h.state().module_settings.value(334), value);
            assert_eq!(h.state().pending_settings_write_value(334), None);
        }
    }
}

#[test]
fn k04_threshold_layout_bottom_rows_stable_during_real_pointer_worker_edits() {
    use crate::rynk::session::testing::{board, link, Stored};
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(
        include_str!("../../tests/fixtures/k04-upstream-9e2aef92.json"),
    )
    .unwrap();
    let definition = definitions.values().next().unwrap();
    for width in [600.0, 1192.0] {
        let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, 2, 3);
        app.app_settings.language = crate::i18n::Language::Russian;
        app.module_settings.set_value(334, 0);
        let mut stored = Stored::new(8, 8, 1);
        for (&id, &value) in &app.module_settings.values {
            stored.settings.insert(id, vec![value as u8]);
        }
        let (board, state) = board(stored);
        app.hid_device =
            Some(crate::rynk::session::RynkSession::new(link("k04-layout-drag", &board)).into());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(width, 1600.0))
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()),
                app,
            );
        h.run();
        let positions = |out: &egui::FullOutput| {
            let text = k04_threshold_painted_text(out);
            [
                "Порог срабатывания",
                "Выключение автослоя по клавише",
                "Фиксация режима модуля",
            ]
            .map(|label| {
                text.iter()
                    .find(|(s, _)| s == label)
                    .unwrap_or_else(|| panic!("missing {label}"))
                    .1
            })
        };
        let baseline = positions(h.output());
        // Every row must start in the same frozen list column, not inherit
        // an expanded threshold child width from the preceding row.
        for row in &baseline[1..] {
            assert!(
                (row.left() - baseline[0].left()).abs() < 0.1,
                "initial row drift: {baseline:?}"
            );
        }
        let track = h
            .get_all_by_role(Role::Slider)
            .map(|n| n.rect())
            .max_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap();
        let ctx = egui::Context::default();
        for value in [9, 10, 99, 100, 0] {
            let radius = track.height() / 2.5;
            let pos = egui::pos2(
                track.left() + radius + (track.width() - 2.0 * radius) * value as f32 / 100.0,
                track.center().y,
            );
            let start = if value == 0 {
                track.right_center()
            } else {
                track.left_center()
            };
            h.hover_at(start);
            h.run_steps(1);
            h.drag_at(start);
            h.run_steps(1);
            h.hover_at(pos);
            h.run_steps(1);
            assert!(
                h.state().settings_write_task.is_some(),
                "real setting worker admitted"
            );
            assert!(h.state().hid_device.is_none(), "real handle handoff");
            assert_eq!(
                positions(h.output()),
                baseline,
                "rows moved while dragging {value}"
            );
            h.drop_at(pos);
            h.run_steps(1);
            assert_eq!(
                positions(h.output()),
                baseline,
                "rows moved on release {value}"
            );
            let requested = h
                .state()
                .pending_settings_write_value(334)
                .unwrap_or_else(|| h.state().module_settings.value(334));
            drain_hid_writes(h.state_mut(), &ctx);
            h.run_steps(1);
            assert_eq!(
                positions(h.output()),
                baseline,
                "rows moved after worker {value}"
            );
            assert_eq!(
                state.lock().unwrap().settings.get(&334),
                Some(&vec![requested as u8])
            );
            let loaded = EntropyApp::read_module_settings(
                definition,
                &[334],
                h.state().hid_device.as_ref().unwrap().as_rynk().unwrap(),
            );
            assert_eq!(loaded.value(334), requested);
        }
    }
}

#[test]
fn k04_followon_threshold_copy_painted_and_hover_ru_en() {
    use crate::i18n::Language::{English, Russian};
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(
        include_str!("../../tests/fixtures/k04-upstream-9e2aef92.json"),
    )
    .unwrap();
    for definition in definitions.values() {
        for lang in [Russian, English] {
            for side in 0..2 {
                let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, side, 2, 0);
                app.app_settings.language = lang;
                let (label, hint, less, more) = if lang == Russian {
                    ("Порог срабатывания", "Количество движения шара для включения автослоя. Чем выше значение, тем больше нужно провернуть шар", "Меньше", "Больше")
                } else {
                    ("Activation threshold", "Amount of ball movement needed to activate the auto layer. Higher values require more ball movement", "Less", "More")
                };
                let ctx = egui::Context::default();
                ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
                let before = app.module_settings.values.clone();
                let mut text = Vec::new();
                for _ in 0..6 {
                    text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]);
                }
                let pos = text
                    .iter()
                    .find(|(s, _)| s == label)
                    .expect("threshold label")
                    .1
                    .center();
                let left = text
                    .iter()
                    .find(|(s, _)| s == less)
                    .expect("left endpoint")
                    .1;
                let right = text
                    .iter()
                    .find(|(s, _)| s == more)
                    .expect("right endpoint")
                    .1;
                assert!(left.center().x < right.center().x);
                for _ in 0..12 {
                    text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
                }
                assert!(
                    text.iter().any(|(s, _)| s == hint),
                    "exact threshold tooltip"
                );
                assert_eq!(app.module_settings.values, before);
                assert_eq!(app.pending_settings_write_value(334), None);
                let field = app
                    .module_settings
                    .groups
                    .iter()
                    .flat_map(|g| &g.fields)
                    .find(|f| f.qsid == 334)
                    .unwrap()
                    .clone();
                // A same-labelled legacy Vial or unrelated model keeps generic copy.
                app.layout.as_mut().unwrap().firmware = FirmwareProtocol::Vial;
                assert!(!app.k04_threshold_field(ModuleSettingsGroupKind::AutoLayer, &field));
                assert_ne!(
                    app.module_setting_label(ModuleSettingsGroupKind::AutoLayer, &field.title),
                    label
                );
                app.layout.as_mut().unwrap().firmware = FirmwareProtocol::Rynk;
                let model = app.layout.as_ref().unwrap().name.clone();
                app.layout.as_mut().unwrap().name = "HPD".into();
                assert!(!app.k04_threshold_field(ModuleSettingsGroupKind::AutoLayer, &field));
                assert_ne!(
                    app.module_setting_label(ModuleSettingsGroupKind::AutoLayer, &field.title),
                    label
                );
                app.layout.as_mut().unwrap().name = model;
                assert!(app.k04_threshold_field(ModuleSettingsGroupKind::AutoLayer, &field));
                for malformed in 0..5 {
                    let mut f = field.clone();
                    match malformed {
                        0 => f.qsid = 335,
                        1 => f.width = 2,
                        2 => f.min = 1,
                        3 => f.max = 101,
                        _ => f.kind = ModuleSettingKind::Select,
                    }
                    assert!(!app.k04_threshold_field(ModuleSettingsGroupKind::AutoLayer, &f));
                }
            }
        }
    }
}

#[test]
fn k04_followon_threshold_is_slider_and_encoder_interval_is_hidden() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    for definition in serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(
        include_str!("../../tests/fixtures/k04-upstream-9e2aef92.json"),
    )
    .unwrap()
    .values()
    {
        for side in 0..2 {
            for module in [2, 3] {
                let app = series_ui_app(definition, FirmwareProtocol::Rynk, side, module, 0);
                let mut h = egui_kittest::Harness::builder()
                    .with_size(egui::vec2(1400.0, 1400.0))
                    .with_accessibility_check(false)
                    .allow_missing_glyphs()
                    .build_ui_state(
                        |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()),
                        app,
                    );
                h.run();
                assert_eq!(h.get_all_by_role(Role::Slider).count(), 2);
                let before = h.state().module_settings.values.clone();
                h.run();
                assert_eq!(h.state().module_settings.values, before);
                h.state_mut()
                    .module_settings
                    .set_value(149 + side as u16, 1);
                let rows = h.state().module_settings_rows();
                let visible: Vec<_> = rows
                    .iter()
                    .filter_map(|row| {
                        if let ModuleSettingsRow::Field {
                            group_idx,
                            field_idx,
                        } = row
                        {
                            Some(
                                h.state().module_settings.groups[*group_idx].fields[*field_idx]
                                    .qsid,
                            )
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(!visible.contains(&325) && !visible.contains(&326));
                assert!(visible.contains(&(332 + side as u16)));
            }
        }
    }
}

#[test]
fn k04_followon_threshold_unsent_coalescing_keeps_inflight_and_ordered_controls() {
    use crate::rynk::session::testing::{board, link, Stored};
    use rmk_types_rynk::protocol::rynk::Cmd;
    let definitions: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(
        include_str!("../../tests/fixtures/k04-upstream-9e2aef92.json"),
    )
    .unwrap();
    let definition = definitions.values().next().unwrap();
    let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, 2, 0);
    let gi = app
        .module_settings
        .groups
        .iter()
        .position(|g| g.fields.iter().any(|f| f.qsid == 334))
        .unwrap();
    let threshold = app.module_settings.groups[gi]
        .fields
        .iter()
        .find(|f| f.qsid == 334)
        .unwrap()
        .clone();
    let rotation = app.module_settings.groups[0]
        .fields
        .iter()
        .find(|f| f.qsid == 130)
        .unwrap()
        .clone();
    let (board, state) = board(Stored::new(8, 8, 1));
    app.hid_device = Some(
        crate::rynk::session::RynkSession::new(link("k04-threshold-coalescing", &board)).into(),
    );
    app.write_module_setting_value(gi, &threshold, 20);
    assert!(app.settings_write_task.is_some());
    assert!(app.hid_device.is_none());
    for value in [30, 40, 50] {
        app.write_module_setting_value(gi, &threshold, value);
    }
    for value in [1, 2] {
        app.write_module_setting_value(0, &rotation, value);
    }
    drain_hid_writes(&mut app, &egui::Context::default());
    let s = state.lock().unwrap();
    assert_eq!(
        s.writes().iter().filter(|c| **c == Cmd::SetSetting).count(),
        4
    );
    assert_eq!(s.settings.get(&334), Some(&vec![50]));
    assert_eq!(s.settings.get(&130), Some(&vec![2]));
}

#[test]
fn k04_followon_threshold_real_rynk_set_reload_and_unknown_loaded_value() {
    use crate::rynk::session::testing::{board, link, Stored};
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    use rmk_types_rynk::protocol::rynk::Cmd;
    for definition in serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(
        include_str!("../../tests/fixtures/k04-upstream-9e2aef92.json"),
    )
    .unwrap()
    .values()
    {
        for module in [2, 3] {
            let mut app = series_ui_app(definition, FirmwareProtocol::Rynk, 0, module, 0);
            let gi = app
                .module_settings
                .groups
                .iter()
                .position(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
                .unwrap();
            let fi = app.module_settings.groups[gi]
                .fields
                .iter()
                .position(|f| f.qsid == 334)
                .unwrap();
            let mut stored = Stored::new(8, 8, 1);
            stored.settings.insert(334, vec![255]);
            let (board, state) = board(stored);
            app.hid_device =
                Some(crate::rynk::session::RynkSession::new(link("k04-threshold", &board)).into());
            app.module_settings.set_value(334, 255);
            let mut h = egui_kittest::Harness::builder()
                .with_accessibility_check(false)
                .allow_missing_glyphs()
                .build_ui_state(
                    move |ui, app: &mut EntropyApp| {
                        app.draw_module_settings_field_row(ui, gi, fi, 700.0, 48.0, false)
                    },
                    app,
                );
            h.run();
            assert_eq!(h.state().module_settings.value(334), 255);
            assert!(state.lock().unwrap().writes().is_empty());
            assert!(h.query_by_role(Role::TextInput).is_none());
            for raw in [0, 1, 5, 9, 10, 50, 99, 100] {
                h.get_by_role(Role::Slider)
                    .set_value_accesskit(f64::from(raw));
                h.run_steps(1);
                assert_eq!(
                    h.get_by_role(Role::Slider).accesskit_node().numeric_value(),
                    Some(f64::from(raw))
                );
                drain_hid_writes(h.state_mut(), &egui::Context::default());
                assert_eq!(
                    state.lock().unwrap().writes().last(),
                    Some(&Cmd::SetSetting)
                );
                assert_eq!(
                    state.lock().unwrap().settings.get(&334),
                    Some(&vec![raw as u8])
                );
                let loaded = EntropyApp::read_module_settings(
                    definition,
                    &[334],
                    h.state().hid_device.as_ref().unwrap().as_rynk().unwrap(),
                );
                assert_eq!(loaded.value(334), raw);
            }
        }
    }
}
