// Production row + real descriptor parser; no replacement slider implementation.
fn phenom_speed_app() -> EntropyApp {
    let mut app = test_app();
    let json: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-f6d3d4d5.json"
    ))
    .unwrap();
    app.module_settings =
        EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
    let mut layout = encoder_visibility_layout("Hide left encoder", "Hide right encoder");
    layout.name = "Phenom".into();
    app.layout = Some(layout);
    app
}

#[test]
fn phenom_precise_copy_painted_endpoint_and_hover_ru_en_both_sides() {
    use crate::i18n::Language::{Russian, English};
    for lang in [Russian, English] {
        for side in 0..=1 {
            for module in [1, 2] {
                let mut app = phenom_speed_app();
                let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
                app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
                app.app_settings.language = lang;
                app.module_settings.set_selected_module_group(side);
                app.module_settings.set_value(372 + side as u16, module);
                let before = app.module_settings.values.clone();
                let (hid, recorder) = crate::hid::HidDevice::test_device();
                app.hid_device = Some(hid.into());
                let ctx = egui::Context::default();
                ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
                let (precise, slower, faster, hint) = if lang == Russian {
                    ("Точнее", "Медленнее", "Быстрее", "Регулировка скорости указателя в точном режиме: передвиньте влево для большей точности, вправо - для большей скорости")
                } else {
                    ("More precise", "Slower", "Faster", "Pointer speed adjustment in Sniper mode: move left for greater precision, right for greater speed")
                };
                let (smoother, scroll_hint) = if lang == Russian {
                    ("Плавнее", "Регулировка скорости указателя в режиме прокрутки: передвиньте влево для большей плавности, вправо - для большей скорости")
                } else {
                    ("Smoother", "Pointer speed adjustment in Scroll mode: move left for smoother scrolling, right for greater speed")
                };
                for mode in 1..=3 {
                    app.module_settings.set_value(134 + side as u16, mode);
                    let mut text = Vec::new();
                    for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]); }
                    let left = match mode { 1 => precise, 2 => smoother, _ => slower };
                    let left_rect = text.iter().find(|(s, _)| s == left).unwrap_or_else(|| panic!("module {module} side {side} mode {mode}: missing {left}")).1;
                    let right_rect = text.iter().find(|(s, _)| s == faster).expect("right endpoint unchanged").1;
                    assert!(left_rect.center().x < right_rect.center().x);
                    for other in [precise, smoother, slower] {
                        if other != left { assert!(!text.iter().any(|(s, _)| s == other)); }
                    }
                    if mode != 3 {
                        let label = match (lang, mode) {
                            (Russian, 1) => "Точность указателя",
                            (Russian, _) => "Скорость прокрутки",
                            (_, 1) => "Sniper mode speed",
                            _ => "Scroll speed",
                        };
                        let expected_hint = if mode == 1 { hint } else { scroll_hint };
                        let pos = text.iter().find(|(s, _)| s == label).expect("unchanged field name").1.center();
                        for _ in 0..12 { text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]); }
                        assert!(text.iter().any(|(s, _)| s == expected_hint), "module {module} side {side}: missing exact tooltip {expected_hint}");
                    }
                    assert_eq!(app.module_settings.values.iter().filter(|(id, _)| **id != 134 + side as u16).collect::<std::collections::BTreeMap<_, _>>(), before.iter().filter(|(id, _)| **id != 134 + side as u16).collect::<std::collections::BTreeMap<_, _>>());
                    assert!(app.settings_write_task.is_none());
                    assert!(recorder.requests().is_empty());
                }
            }
        }
    }
}

#[test]
fn phenom_text_copy_painted_labels_and_hover_ru_en_both_sides() {
    use crate::i18n::Language::{Russian, English};
    for lang in [Russian, English] {
        let expected = if lang == Russian {
            [("Перемещение в тексте", "Регулировка скорости указателя в режиме текста: передвиньте влево для уменьшения скорости, вправо - для увеличения скорости"),
             ("Инверсия по вертикали", "Изменение направления перемещения вверх и вниз на противоположное"),
             ("Инверсия по горизонтали", "Изменение направления перемещения влево и вправо на противоположное")]
        } else {
            [("Text navigation speed", "Pointer speed adjustment in Text mode: move left to decrease speed, right to increase speed"),
             ("Vertical inversion", "Reverse the direction of movement up and down"),
             ("Horizontal inversion", "Reverse the direction of movement left and right")]
        };
        for side in 0..=1 {
            for module in [1, 2] {
                let mut app = phenom_speed_app();
                let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
                app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
                app.app_settings.language = lang;
                app.module_settings.set_selected_module_group(side);
                app.module_settings.set_value(372 + side as u16, module);
                app.module_settings.set_value(134 + side as u16, 3);
                let before = app.module_settings.values.clone();
                let (hid, recorder) = crate::hid::HidDevice::test_device();
                app.hid_device = Some(hid.into());
                let ctx = egui::Context::default();
                ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
                for (label, hint) in expected {
                    let mut painted = Vec::new();
                    for _ in 0..6 { painted = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]); }
                    let rows: Vec<_> = painted.iter().filter(|(s, _)| s == label).collect();
                    assert_eq!(rows.len(), 1, "module {module} side {side}: one exact {label}");
                    let pos = rows[0].1.center();
                    for _ in 0..12 { painted = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]); }
                    assert!(painted.iter().any(|(s, _)| s == hint), "module {module} side {side}: exact tooltip {hint}");
                }
                assert_eq!(app.module_settings.values, before);
                assert!(app.settings_write_task.is_none());
                assert!(recorder.requests().is_empty());
            }
        }
    }
}

#[test]
fn phenom_scroll_inversion_copy_painted_labels_and_hover_ru_en_both_sides() {
    use crate::i18n::Language::{Russian, English};
    for lang in [Russian, English] {
        let scroll = if lang == Russian {
            [("Инверсия по вертикали", "Изменение направления прокрутки вверх и вниз на противоположное"),
             ("Инверсия по горизонтали", "Изменение направления прокрутки влево и вправо на противоположное")]
        } else {
            [("Vertical inversion", "Reverse the direction of scrolling up and down"),
             ("Horizontal inversion", "Reverse the direction of scrolling left and right")]
        };
        let text_inversions = if lang == Russian {
            [("Инверсия по вертикали", "Изменение направления перемещения вверх и вниз на противоположное"),
             ("Инверсия по горизонтали", "Изменение направления перемещения влево и вправо на противоположное")]
        } else {
            [("Vertical inversion", "Reverse the direction of movement up and down"),
             ("Horizontal inversion", "Reverse the direction of movement left and right")]
        };
        for side in 0..=1 {
            for module in [1, 2] {
                let mut app = phenom_speed_app();
                let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
                app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
                app.app_settings.language = lang;
                app.module_settings.set_selected_module_group(side);
                app.module_settings.set_value(372 + side as u16, module);
                let (hid, recorder) = crate::hid::HidDevice::test_device();
                app.hid_device = Some(hid.into());
                let ctx = egui::Context::default();
                ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
                for mode in [2, 3] {
                    app.module_settings.set_value(134 + side as u16, mode);
                    let before = app.module_settings.values.clone();
                    let (expected, absent) = if mode == 2 { (scroll, text_inversions) } else { (text_inversions, scroll) };
                    for (label, hint) in expected {
                        let mut painted = Vec::new();
                        for _ in 0..6 { painted = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]); }
                        let rows: Vec<_> = painted.iter().filter(|(s, _)| s == label).collect();
                        assert_eq!(rows.len(), 1, "module {module} side {side} mode {mode}: one exact {label}");
                        // Axis labels now intentionally match across modes; tooltips must remain distinct.
                        let pos = rows[0].1.center();
                        for _ in 0..12 { painted = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]); }
                        assert!(painted.iter().any(|(s, _)| s == hint), "module {module} side {side} mode {mode}: exact tooltip {hint}");
                        assert!(absent.iter().all(|(_, other_hint)| !painted.iter().any(|(s, _)| s == other_hint)));
                    }
                    assert_eq!(app.module_settings.values, before);
                    assert!(app.settings_write_task.is_none());
                    assert!(recorder.requests().is_empty());
                }
            }
        }
    }
}

#[test]
fn phenom_encoder_steps_approved_copy_ru_en() {
    use crate::i18n::Language::{Russian, English};
    let mut app = phenom_speed_app();
    let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
    app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
    for (lang, title, hint) in [(Russian, "Шаг энкодера", "Количество действий за один шаг энкодера"),
                               (English, "Encoder steps", "Number of actions per encoder step")] {
        app.app_settings.language = lang;
        for side in 0..=1 {
            let group = &app.module_settings.groups[side];
            let field = group.fields.iter().find(|f| f.qsid == 376 + side as u16).unwrap();
            assert_eq!(app.module_setting_label(group.kind, &field.title), title);
            assert_eq!(app.module_setting_tooltip(group.kind, field), hint);
        }
    }
}

#[test]
fn phenom_speed_copy_is_concise_and_device_neutral() {
    let mut app = phenom_speed_app();
    app.app_settings.language = crate::i18n::Language::Russian;
    for (qsid, label, tooltip) in [
        (130, "Поворот трекбола", "Изменение направления движения указателя при повороте модуля трекбола"),
        (124, "Точность указателя", "Регулировка скорости указателя в точном режиме: передвиньте влево для большей точности, вправо - для большей скорости"),
        (125, "Скорость прокрутки", "Регулировка скорости указателя в режиме прокрутки: передвиньте влево для большей плавности, вправо - для большей скорости"),
        (126, "Перемещение в тексте", "Регулировка скорости указателя в режиме текста: передвиньте влево для уменьшения скорости, вправо - для увеличения скорости"),
    ] {
        let group = &app.module_settings.groups[0];
        let field = group.fields.iter().find(|f| f.qsid == qsid).unwrap();
        assert_eq!(app.module_setting_label(group.kind, &field.title), label);
        assert_eq!(app.module_setting_tooltip(group.kind, field), tooltip);
    }
}

#[test]
fn phenom_speed_touchpad_uses_same_shared_mode_slider() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let mut app = phenom_speed_app();
    let mut selector = module_filter_field("Module", 372);
    selector.variants = vec!["Encoder".into(), "Trackball".into(), "Touchpad".into()];
    app.module_settings.groups[0].fields.push(selector);
    app.module_settings.set_value(372, 2);
    let idx = app.module_settings.groups[0]
        .fields
        .iter()
        .position(|f| f.qsid == 125)
        .unwrap();
    let mut harness = egui_kittest::Harness::builder()
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            move |ui, app: &mut EntropyApp| {
                app.draw_module_settings_field_row(ui, 0, idx, 600.0, 48.0, false)
            },
            app,
        );
    harness.run();
    harness.get_by_role(Role::Slider);
}

#[test]
fn phenom_speed_all_loaded_bytes_survive_render_on_both_sides() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    for qsid in 124..=129 {
        let app = phenom_speed_app();
        let group_idx = usize::from(qsid >= 127);
        let idx = app.module_settings.groups[group_idx]
            .fields
            .iter()
            .position(|f| f.qsid == qsid)
            .unwrap();
        let mut harness = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .build_ui_state(
                move |ui, app: &mut EntropyApp| {
                    app.draw_module_settings_field_row(ui, group_idx, idx, 600.0, 48.0, false)
                },
                app,
            );
        for raw in 1..=255 {
            harness.state_mut().module_settings.set_value(qsid, raw);
            harness.run_steps(1);
            assert_eq!(
                harness
                    .get_by_role(Role::Slider)
                    .accesskit_node()
                    .numeric_value(),
                Some(f64::from(256 - raw)),
                "qsid {qsid}, raw {raw}"
            );
            assert_eq!(harness.state().module_settings.value(qsid), raw);
            assert_eq!(harness.state().pending_settings_write_value(qsid), None);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn phenom_speed_real_slider_interaction_serializes_and_reloads_exact_bytes() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    for qsid in 124..=129 {
        let mut app = phenom_speed_app();
        let group_idx = usize::from(qsid >= 127);
        let idx = app.module_settings.groups[group_idx]
            .fields
            .iter()
            .position(|f| f.qsid == qsid)
            .unwrap();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        app.module_settings.set_value(qsid, 16);
        let mut harness = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .build_ui_state(
                move |ui, app: &mut EntropyApp| {
                    app.draw_module_settings_field_row(ui, group_idx, idx, 600.0, 48.0, false)
                },
                app,
            );
        harness.run();
        // Exercise real accessibility actions, then pointer drag: both reach the
        // production response.changed -> queue -> HID setter, not a proxy mapper.
        for speed in [1, 2, 64, 128, 200, 254, 255] {
            harness
                .get_by_role(Role::Slider)
                .set_value_accesskit(f64::from(speed));
            harness.run_steps(1);
            assert_eq!(
                harness.state().pending_settings_write_value(qsid),
                Some(256 - speed)
            );
            drain_hid_writes(harness.state_mut(), &egui::Context::default());
            assert_eq!(harness.state().module_settings.value(qsid), 256 - speed);
            let json = serde_json::from_str(include_str!(
                "../../tests/fixtures/phenom-qmk-settings-f6d3d4d5.json"
            ))
            .unwrap();
            let reloaded = EntropyApp::read_module_settings(
                &json,
                &[qsid],
                harness
                    .state()
                    .hid_device
                    .as_ref()
                    .unwrap()
                    .require_vial()
                    .unwrap(),
            );
            assert_eq!(reloaded.value(qsid), 256 - speed);
        }
        let rect = harness.get_by_role(Role::Slider).rect();
        harness.drag_at(rect.left_center());
        harness.run_steps(1);
        harness.drop_at(rect.left_center());
        harness.run_steps(1);
        drain_hid_writes(harness.state_mut(), &egui::Context::default());
        assert_eq!(harness.state().module_settings.value(qsid), 255);
        harness.drag_at(rect.right_center());
        harness.run_steps(1);
        harness.drop_at(rect.right_center());
        harness.run_steps(1);
        drain_hid_writes(harness.state_mut(), &egui::Context::default());
        assert_eq!(harness.state().module_settings.value(qsid), 1);
        let requests = recorder.requests();
        assert!(requests.iter().all(|r| r[2..4] == qsid.to_le_bytes()));
        assert!(requests.iter().any(|r| r[4] == 255));
        assert!(requests.iter().any(|r| r[4] == 1));
    }
}

#[test]
fn phenom_speed_current_descriptor_scope_and_mode_visibility() {
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    for name in ["Phenom", "Ergohaven Phenom Mini", "Phenom Micro"] {
        let mut app = phenom_speed_app();
        app.layout.as_mut().unwrap().name = name.into();
        app.module_settings =
            EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
        for side in 0..=1 {
            app.module_settings.set_selected_module_group(side);
            let selector = 372 + side as u16;
            for module in 0..=2 {
                app.module_settings.set_value(selector, module);
                for mode in 0..=3 {
                    app.module_settings.set_value(134 + side as u16, mode);
                    let visible = visible_module_qsids(&app);
                    for offset in 0..=2 {
                        let id = 124 + side as u16 * 3 + offset;
                        let field = app.module_settings.groups[side]
                            .fields
                            .iter()
                            .find(|f| f.qsid == id)
                            .unwrap();
                        assert_eq!(
                            app.phenom_mode_speed_field(
                                app.module_settings.groups[side].kind,
                                field
                            ),
                            module != 0
                        );
                        assert_eq!(visible.contains(&id), module != 0 && mode == offset + 1);
                    }
                }
            }
        }
        app.layout.as_mut().unwrap().name = "HPD".into();
        let field = app.module_settings.groups[0]
            .fields
            .iter()
            .find(|f| f.qsid == 124)
            .unwrap();
        assert!(!app.phenom_mode_speed_field(ModuleSettingsGroupKind::Left, field));
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn phenom_speed_busy_owner_coalesces_and_side_switch_retains_pending_edit() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let mut app = phenom_speed_app();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid.into());
    app.combo_entries = vec![ComboEntry {
        keys: [4, 5, 0, 0],
        output: 6.into(),
        layer: None,
    }];
    app.combo_synced_entries = vec![ComboEntry::default()];
    app.mark_combo_dirty();
    app.maybe_start_combo_write(&egui::Context::default());
    assert!(app.hid_device.is_none());
    let mut harness = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, app: &mut EntropyApp| {
                let side = app.module_settings.selected_module_group().unwrap();
                let idx = app.module_settings.groups[side]
                    .fields
                    .iter()
                    .position(|f| f.qsid == 124 + side as u16 * 3)
                    .unwrap();
                app.draw_module_settings_field_row(ui, side, idx, 600.0, 48.0, false);
            },
            app,
        );
    harness.run();
    for speed in [1, 22, 101, 200, 255] {
        harness
            .get_by_role(Role::Slider)
            .set_value_accesskit(f64::from(speed));
        harness.run_steps(1);
    }
    assert_eq!(harness.state().pending_settings_write_value(124), Some(1));
    harness
        .state_mut()
        .module_settings
        .set_selected_module_group(1);
    harness.run_steps(1);
    harness.get_by_role(Role::Slider).set_value_accesskit(128.0);
    harness.run_steps(1);
    assert_eq!(harness.state().pending_settings_write_value(124), Some(1));
    assert_eq!(harness.state().pending_settings_write_value(127), Some(128));
    drain_hid_writes(harness.state_mut(), &egui::Context::default());
    assert_eq!(harness.state().module_settings.value(124), 1);
    assert_eq!(harness.state().module_settings.value(127), 128);
    for (id, value) in [(124u16, 1), (127, 128)] {
        let requests = recorder.requests();
        let writes: Vec<_> = requests
            .iter()
            .filter(|r| r[2..4] == id.to_le_bytes())
            .collect();
        assert_eq!(writes.len(), 1, "only the final pending value is sent");
        assert_eq!(writes[0][4], value);
    }
}

#[test]
fn phenom_speed_page_renders_one_mode_slider_in_both_languages() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    for language in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        for width in [600.0, 1400.0] {
            let mut app = phenom_speed_app();
            app.module_settings =
                EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
            app.app_settings.language = language;
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(width, 1200.0))
                .with_accessibility_check(false)
                .allow_missing_glyphs()
                .build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()),
                    app,
                );
            for side in 0..=1 {
                harness
                    .state_mut()
                    .module_settings
                    .set_selected_module_group(side);
                for module in [1, 2] {
                    harness
                        .state_mut()
                        .module_settings
                        .set_value(372 + side as u16, module);
                    for mode in 0..=3 {
                        harness
                            .state_mut()
                            .module_settings
                            .set_value(134 + side as u16, mode);
                        harness.run();
                        assert_eq!(harness.get_all_by_role(Role::Slider).count(), if mode == 0 { 1 } else { 2 });
                        if mode != 0 {
                            harness.get_by_label(crate::i18n::tr_catalog(
                                language,
                                match mode {
                                    1 => "modules_settings.speed_more_precise",
                                    2 => "modules_settings.speed_smoother",
                                    _ => "modules_settings.speed_slower",
                                },
                            ));
                            harness.get_by_label(crate::i18n::tr_catalog(
                                language,
                                "modules_settings.speed_faster",
                            ));
                        }
                        assert!(harness.state().settings_write_task.is_none());
                    }
                }
            }
        }
    }
}

#[test]
fn phenom_speed_production_row_is_slider_preserving_loaded_value() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let mut app = phenom_speed_app();
    let idx = app.module_settings.groups[0]
        .fields
        .iter()
        .position(|f| f.qsid == 124)
        .unwrap();
    app.module_settings.set_value(124, 16);
    let mut harness = egui_kittest::Harness::builder()
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            move |ui, app: &mut EntropyApp| {
                app.draw_module_settings_field_row(ui, 0, idx, 600.0, 48.0, false)
            },
            app,
        );
    harness.run();
    harness.get_by_role(Role::Slider);
    assert_eq!(harness.state().module_settings.value(124), 16);
    assert!(harness.state().settings_write_task.is_none());
    assert_eq!(harness.state().pending_settings_write_value(124), None);
}
