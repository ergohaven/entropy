#[test]
fn phenom_auto_layer_timeout_busy_owner_coalesces_and_preserves_unknown_loaded_values() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let mut app = phenom_auto_app();
    let group_idx = app
        .module_settings
        .groups
        .iter()
        .position(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
        .unwrap();
    let field_idx = app.module_settings.groups[group_idx]
        .fields
        .iter()
        .position(|f| f.qsid == 324)
        .unwrap();
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
    let mut h = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .build_ui_state(
            move |ui, app: &mut EntropyApp| {
                app.draw_module_settings_field_row(ui, group_idx, field_idx, 700.0, 48.0, false);
            },
            app,
        );
    h.run();
    for raw in [14, 255] {
        h.state_mut().module_settings.set_value(324, raw);
        h.run_steps(1);
        assert_eq!(h.state().module_settings.value(324), raw);
        assert!(h.state().pending_settings_write_value(324).is_none());
    }
    h.state_mut().module_settings.set_value(324, 0);
    h.run_steps(1);
    for raw in [1, 3, 8, 12, 13] {
        h.get_by_role(Role::Slider)
            .set_value_accesskit(f64::from(raw));
        h.run_steps(1);
    }
    h.state_mut().module_settings.set_selected_module_group(1);
    assert_eq!(h.state().pending_settings_write_value(324), Some(13));
    drain_hid_writes(h.state_mut(), &egui::Context::default());
    let requests = recorder.requests();
    let writes: Vec<_> = requests
        .iter()
        .filter(|r| r[2..4] == 324u16.to_le_bytes())
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0][4], 13);
}

#[test]
fn phenom_auto_layer_scope_preserves_descriptor_and_other_models() {
    let mut app = phenom_auto_app();
    let group_idx = app
        .module_settings
        .groups
        .iter()
        .position(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
        .unwrap();
    let field = app.module_settings.groups[group_idx]
        .fields
        .iter()
        .find(|f| f.qsid == 324)
        .unwrap()
        .clone();
    assert!(app.module_settings.groups[group_idx]
        .fields
        .iter()
        .any(|f| f.qsid == 374));
    for name in ["Phenom", "Ergohaven Phenom Mini", "Phenom Micro"] {
        app.layout.as_mut().unwrap().name = name.into();
        assert!(app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &field));
    }
    for name in ["HPD", "K:04", "Unknown"] {
        app.layout.as_mut().unwrap().name = name.into();
        assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &field));
        assert!(visible_module_qsids(&app).contains(&374));
        assert_eq!(
            app.module_settings_group_label(&app.module_settings.groups[group_idx]),
            "Auto layer"
        );
    }
    app.layout.as_mut().unwrap().name = "Phenom".into();
    app.layout.as_mut().unwrap().firmware = FirmwareProtocol::Rynk;
    assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &field));
    app.layout.as_mut().unwrap().firmware = FirmwareProtocol::Vial;
    for change in 0..5 {
        let mut malformed = field.clone();
        match change {
            0 => malformed.qsid = 325,
            1 => malformed.width = 2,
            2 => malformed.max -= 1,
            3 => malformed.variants[0] = "bad".into(),
            _ => malformed.variants.swap(0, 1),
        }
        assert!(!app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, &malformed));
    }
}

fn phenom_auto_app() -> EntropyApp {
    let mut app = phenom_speed_app();
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    app.module_settings =
        EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
    app.module_settings.set_value(372, 1);
    app
}

#[test]
fn phenom_auto_layer_painted_order_copy_hover_and_mode_flags() {
    use crate::i18n::Language::{English, Russian};
    for lang in [Russian, English] {
        for side in 0..=1 {
            for module in [1, 2] {
                for mode in 0..=3 {
                    for enabled in [0, 1] {
                        let mut app = phenom_auto_app();
                        app.app_settings.language = lang;
                        app.module_settings.set_selected_module_group(side);
                        app.module_settings.set_value(372 + side as u16, module);
                        app.module_settings.set_value(134 + side as u16, mode);
                        for id in [142, 144, 145, 146] {
                            app.module_settings.set_value(id, enabled);
                        }
                        let before = app.module_settings.values.clone();
                        let (hid, recorder) = crate::hid::HidDevice::test_device();
                        app.hid_device = Some(hid.into());
                        let ctx = egui::Context::default();
                        ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
                        let mut text = Vec::new();
                        for _ in 0..6 {
                            text =
                                phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]);
                        }
                        let group = app
                            .module_settings
                            .groups
                            .iter()
                            .find(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
                            .unwrap();
                        let active = [142, 144, 145, 146][mode as usize];
                        let row = |id| group.fields.iter().find(|f| f.qsid == id).unwrap();
                        let labels = [
                            app.module_settings_group_label(group),
                            app.module_setting_label(group.kind, &row(active).title),
                            app.module_setting_label(group.kind, &row(143).title),
                            app.module_setting_label(group.kind, &row(324).title),
                            app.module_setting_label(group.kind, &row(375).title),
                            app.module_setting_label(
                                app.module_settings.groups[side].kind,
                                "Sticky mode",
                            ),
                        ];
                        let ys: Vec<_> = labels
                            .iter()
                            .map(|label| {
                                let hits: Vec<_> =
                                    text.iter().filter(|(s, _)| s == label).collect();
                                assert_eq!(hits.len(), 1, "one painted {label}");
                                hits[0].1.center().y
                            })
                            .collect();
                        assert!(
                            ys.windows(2).all(|p| p[0] < p[1]),
                            "mode {mode}: {labels:?} {ys:?}"
                        );
                        let visible = visible_module_qsids(&app);
                        assert!(!visible.contains(&374));
                        for id in [142, 144, 145, 146] {
                            assert_eq!(visible.contains(&id), id == active);
                        }
                        let expected = if lang == Russian {
                            [(142, "Автослой, обычный режим", "Включение автоматического слоя мыши в обычном режиме"), (143, "Номер автослоя", "Номер слоя с раскладкой, который автоматически активируется при работе с модулями"), (324, "Таймаут автослоя", "Время активности автослоя после остановки движения модуля"), (375, "Выключение автослоя по клавише", "Выключение автослоя при нажатии обычной клавиши"), (140 + side as u16, "Фиксация режима модуля", "Точный режим, прокрутка и текст остаются активными до выбора другого режима или повторного нажатия клавиши текущего режима")]
                        } else {
                            [(142, "Auto layer, Normal mode", "Enable the automatic mouse layer in Normal mode"), (143, "Auto-layer number", "Keymap layer number activated automatically when using modules"), (324, "Auto layer timeout", "How long the auto layer remains active after module movement stops"), (375, "Auto layer deactivate on key", "Deactivate the auto layer when an ordinary key is pressed"), (140 + side as u16, "Sticky module mode", "Sniper, Scroll and Text modes remain active until another mode is selected or the current mode key is pressed again")]
                        };
                        let mode_copy = if lang == Russian {
                            [
                                (
                                    "Автослой, обычный режим",
                                    "Включение автоматического слоя мыши в обычном режиме",
                                ),
                                (
                                    "Автослой, точный режим",
                                    "Включение автоматического слоя мыши в точном режиме",
                                ),
                                (
                                    "Автослой, режим прокрутки",
                                    "Включение автоматического слоя мыши в режиме прокрутки",
                                ),
                                (
                                    "Автослой, режим текста",
                                    "Включение автоматического слоя мыши в режиме текста",
                                ),
                            ]
                        } else {
                            [
                                (
                                    "Auto layer, Normal mode",
                                    "Enable the automatic mouse layer in Normal mode",
                                ),
                                (
                                    "Auto layer, Sniper mode",
                                    "Enable the automatic mouse layer in Sniper mode",
                                ),
                                (
                                    "Auto layer, Scroll mode",
                                    "Enable the automatic mouse layer in Scroll mode",
                                ),
                                (
                                    "Auto layer, Text mode",
                                    "Enable the automatic mouse layer in Text mode",
                                ),
                            ]
                        };
                        for (i, id) in [142, 144, 145, 146].into_iter().enumerate() {
                            assert_eq!(
                                row(id).title,
                                [
                                    "Auto layer in Normal",
                                    "Auto layer in Sniper",
                                    "Auto layer in Scroll",
                                    "Auto layer in Text"
                                ][i]
                            );
                            assert_eq!(
                                text.iter().filter(|(s, _)| s == mode_copy[i].0).count(),
                                usize::from(i == mode as usize)
                            );
                        }
                        let (mode_label, mode_hint) = mode_copy[mode as usize];
                        if enabled == 0 {
                            let pos = text
                                .iter()
                                .find(|(s, _)| s == mode_label)
                                .expect("exact selected-mode label painted")
                                .1
                                .center();
                            for _ in 0..12 {
                                text = phenom_page_frame(
                                    &mut app,
                                    &ctx,
                                    vec![egui::Event::PointerMoved(pos)],
                                );
                            }
                            assert!(
                                text.iter().any(|(s, _)| s == mode_hint),
                                "exact mode hover: {mode_hint}"
                            );
                        }
                        // Other approved section tooltips remain unchanged.
                        if mode == 0 && enabled == 0 {
                            for (_, label, hint) in expected {
                                let pos = text
                                    .iter()
                                    .find(|(s, _)| s == label)
                                    .unwrap_or_else(|| panic!("missing {label}"))
                                    .1
                                    .center();
                                for _ in 0..12 {
                                    text = phenom_page_frame(
                                        &mut app,
                                        &ctx,
                                        vec![egui::Event::PointerMoved(pos)],
                                    );
                                }
                                assert!(
                                    text.iter().any(|(s, _)| s == hint),
                                    "missing hover: {hint}"
                                );
                            }
                        }
                        assert_eq!(app.module_settings.values, before);
                        assert!(recorder.requests().is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn phenom_auto_layer_timeout_slider_all_presets_exact_wire_reload_and_pointer() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    let mut app = phenom_auto_app();
    let group_idx = app
        .module_settings
        .groups
        .iter()
        .position(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
        .unwrap();
    let field_idx = app.module_settings.groups[group_idx]
        .fields
        .iter()
        .position(|f| f.qsid == 324)
        .unwrap();
    let variants = app.module_settings.groups[group_idx].fields[field_idx]
        .variants
        .clone();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid.into());
    let mut h = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .build_ui_state(
            move |ui, app: &mut EntropyApp| {
                app.draw_module_settings_field_row(ui, group_idx, field_idx, 700.0, 48.0, false);
            },
            app,
        );
    h.run();
    h.get_by_role(Role::Slider);
    assert!(
        h.query_by_role(Role::TextInput).is_none(),
        "no numeric entry"
    );
    for raw in 0..14 {
        h.state_mut().module_settings.set_value(324, raw);
        h.run_steps(1);
        assert_eq!(
            h.get_by_role(Role::Slider).accesskit_node().numeric_value(),
            Some(f64::from(raw))
        );
        assert_eq!(h.state().module_settings.value(324), raw);
        assert!(h.state().pending_settings_write_value(324).is_none());
    }
    assert!(recorder.requests().is_empty(), "no writes on paint");
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    for raw in 0..14 {
        let start = recorder.requests().len();
        h.get_by_role(Role::Slider)
            .set_value_accesskit(f64::from(raw));
        h.run_steps(1);
        assert_eq!(h.state().pending_settings_write_value(324), Some(raw));
        drain_hid_writes(h.state_mut(), &egui::Context::default());
        let requests = recorder.requests();
        assert_eq!(requests.len(), start + 1);
        assert_eq!(requests[start][2..5], [68, 1, raw as u8]);
        let loaded = EntropyApp::read_module_settings(
            &json,
            &[324],
            h.state()
                .hid_device
                .as_ref()
                .unwrap()
                .require_vial()
                .unwrap(),
        );
        assert_eq!(loaded.value(324), raw);
        assert_eq!(
            variants[raw as usize],
            [
                "50 ms", "100 ms", "150 ms", "200 ms", "250 ms", "300 ms", "350 ms", "400 ms",
                "450 ms", "500 ms", "750 ms", "1000 ms", "1250 ms", "1500 ms"
            ][raw as usize]
        );
    }
    let rect = h.get_by_role(Role::Slider).rect();
    for (pos, expected) in [(rect.left_center(), 0), (rect.right_center(), 13)] {
        h.drag_at(pos);
        h.run_steps(1);
        h.drop_at(pos);
        h.run_steps(1);
        drain_hid_writes(h.state_mut(), &egui::Context::default());
        assert_eq!(h.state().module_settings.value(324), expected);
    }
}
