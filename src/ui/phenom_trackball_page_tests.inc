fn phenom_page_frame(
    app: &mut EntropyApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let out = crate::app::discard_texture_deltas(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 2000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let rect = ui.available_rect_before_wrap();
            app.draw_module_settings_page(ui, rect);
        },
    ));
    fn collect(shape: &egui::Shape, texts: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(t) => texts.push((
                t.galley.job.text.clone(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            egui::Shape::Vec(v) => {
                for s in v {
                    collect(s, texts);
                }
            }
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for s in out.shapes {
        collect(&s.shape, &mut texts);
    }
    texts
}

fn phenom_page_click(app: &mut EntropyApp, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        phenom_page_frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}

#[test]
fn phenom_trackball_page_mode_rows_follow_selector_including_pending_choices() {
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        for side in 0..=1 {
            for pending in [false, true] {
                for mode in 0..=3 {
                    let mut app = phenom_speed_app();
                    app.app_settings.language = lang;
                    let json = serde_json::from_str(include_str!(
                        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
                    ))
                    .unwrap();
                    app.module_settings = EntropyApp::module_settings_from_definition(
                        &json,
                        &(120..400).collect::<Vec<_>>(),
                    );
                    app.module_settings.set_selected_module_group(side);
                    app.module_settings.set_value(372 + side as u16, 1);
                    let qsid = 134 + side as u16;
                    app.module_settings
                        .set_value(qsid, if pending { (mode + 1) % 4 } else { mode });
                    let (hid, recorder) = crate::hid::HidDevice::test_device();
                    app.hid_device = Some(hid.into());
                    if pending {
                        let field = app.module_settings.groups[side]
                            .fields
                            .iter()
                            .find(|f| f.qsid == qsid)
                            .unwrap()
                            .clone();
                        app.write_module_setting_value(side, &field, mode);
                        assert_eq!(app.pending_settings_write_value(qsid), Some(mode));
                    }
                    let before = app.module_settings.values.clone();
                    let ctx = egui::Context::default();
                    let mut text = Vec::new();
                    for _ in 0..6 {
                        text = phenom_page_frame(&mut app, &ctx, vec![]);
                    }
                    // Resolve row labels through production translation; the separate copy regression
                    // pins literal approved labels. This test specifically catches placement/visibility.
                    let label = |title| {
                        app.module_setting_label(app.module_settings.groups[side].kind, title)
                    };
                    let pos = |title| {
                        text.iter()
                            .find(|(s, _)| s == &label(title))
                            .unwrap_or_else(|| panic!("missing {title}: {text:?}"))
                            .1
                            .center()
                            .y
                    };
                    let mode_y = pos("Mode");
                    for shared in ["Ball DPI", "Acceleration", "Ball axis"] {
                        assert!(
                            pos(shared) < mode_y,
                            "shared {shared} must be above mode selector"
                        );
                    }
                    let mode_fields = [
                        ("Sniper sens", 1),
                        ("Scroll sens", 2),
                        ("Invert scroll vertical", 2),
                        ("Invert scroll horizontal", 2),
                        ("Text sens", 3),
                        ("Invert text vertical", 3),
                        ("Invert text horizontal", 3),
                    ];
                    for (title, owner) in mode_fields {
                        let expected_visible = mode_fields
                            .iter()
                            .any(|(other, active)| mode == *active && label(other) == label(title));
                        assert_eq!(
                            text.iter().any(|(s, _)| s == &label(title)),
                            expected_visible,
                            "{title}, mode {mode}, pending {pending}"
                        );
                        if mode == owner {
                            assert!(pos(title) > mode_y);
                        }
                    }
                    let selected: Vec<_> = mode_fields
                        .into_iter()
                        .filter(|(_, owner)| mode == *owner)
                        .map(|(title, _)| pos(title))
                        .collect();
                    assert!(selected.windows(2).all(|p| p[0] < p[1]));
                    let section =
                        crate::i18n::tr_catalog(lang, "modules_settings.phenom_auto_section");
                    let section_y = text
                        .iter()
                        .find(|(s, _)| s == section)
                        .unwrap()
                        .1
                        .center()
                        .y;
                    let between: Vec<_> = text
                        .iter()
                        .filter(|(_, r)| r.center().y > mode_y && r.center().y < section_y)
                        .map(|(s, _)| s)
                        .collect();
                    for (title, owner) in mode_fields {
                        if mode == owner {
                            assert!(between.contains(&&label(title)));
                        }
                    }
                    // No shared row may follow the selector in the side block.
                    assert!(pos("Sticky mode") > section_y);
                    assert!(selected.iter().all(|y| *y < section_y));
                    assert_eq!(app.module_settings.values, before);
                    let requests = recorder.requests();
                    assert!(
                        requests.iter().all(|r| pending
                            && r[2..4] == qsid.to_le_bytes()
                            && r[4] == mode as u8),
                        "only the explicitly queued mode SET is allowed: {requests:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn phenom_touchpad_page_mode_rows_follow_selector_including_pending_choices() {
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        for side in 0..=1 {
            for pending in [false, true] {
                for mode in 0..=3 {
                    let mut app = phenom_speed_app();
                    app.app_settings.language = lang;
                    let json = serde_json::from_str(include_str!(
                        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
                    ))
                    .unwrap();
                    app.module_settings = EntropyApp::module_settings_from_definition(
                        &json,
                        &(120..400).collect::<Vec<_>>(),
                    );
                    app.module_settings.set_selected_module_group(side);
                    app.module_settings.set_value(372 + side as u16, 2);
                    let qsid = 134 + side as u16;
                    app.module_settings
                        .set_value(qsid, if pending { (mode + 1) % 4 } else { mode });
                    let (hid, recorder) = crate::hid::HidDevice::test_device();
                    app.hid_device = Some(hid.into());
                    if pending {
                        let field = app.module_settings.groups[side]
                            .fields
                            .iter()
                            .find(|f| f.qsid == qsid)
                            .unwrap()
                            .clone();
                        app.write_module_setting_value(side, &field, mode);
                        assert_eq!(app.pending_settings_write_value(qsid), Some(mode));
                    }
                    let before = app.module_settings.values.clone();
                    let ctx = egui::Context::default();
                    let mut text = Vec::new();
                    for _ in 0..6 {
                        text = phenom_page_frame(&mut app, &ctx, vec![]);
                    }
                    // Resolve row labels through production translation; the separate copy regression
                    // pins literal approved labels. This test specifically catches placement/visibility.
                    let label = |title| {
                        app.module_setting_label(app.module_settings.groups[side].kind, title)
                    };
                    let pos = |title| {
                        text.iter()
                            .find(|(s, _)| s == &label(title))
                            .unwrap_or_else(|| panic!("missing {title}: {text:?}"))
                            .1
                            .center()
                            .y
                    };
                    let mode_y = pos("Mode");
                    let ordered = if lang == crate::i18n::Language::Russian {
                        [
                            "Сторона модуля",
                            "Тип модуля",
                            "Чувствительность тачпада, DPI",
                            "Ускорение",
                            "Жесты тачпада",
                            "Поворот тачпада",
                            "Режим модуля",
                        ]
                    } else {
                        [
                            "Module side",
                            "Module type",
                            "Touchpad sensitivity, DPI",
                            "Acceleration",
                            "Touch gestures",
                            "Touchpad rotation",
                            "Module mode",
                        ]
                    };
                    let ys: Vec<_> = ordered
                        .iter()
                        .map(|label| {
                            text.iter()
                                .find(|(s, _)| s == label)
                                .unwrap_or_else(|| panic!("missing {label}"))
                                .1
                                .center()
                                .y
                        })
                        .collect();
                    assert!(ys.windows(2).all(|p| p[0] < p[1]), "Touchpad order {ys:?}");
                    for shared in ["Touch DPI", "Acceleration", "Touch axis", "Touch gestures"] {
                        assert!(
                            pos(shared) < mode_y,
                            "shared {shared} must be above mode selector"
                        );
                    }
                    let mode_fields = [
                        ("Sniper sens", 1),
                        ("Scroll sens", 2),
                        ("Invert scroll vertical", 2),
                        ("Invert scroll horizontal", 2),
                        ("Text sens", 3),
                        ("Invert text vertical", 3),
                        ("Invert text horizontal", 3),
                    ];
                    for (title, owner) in mode_fields {
                        let expected_visible = mode_fields
                            .iter()
                            .any(|(other, active)| mode == *active && label(other) == label(title));
                        assert_eq!(
                            text.iter().any(|(s, _)| s == &label(title)),
                            expected_visible,
                            "{title}, mode {mode}, pending {pending}"
                        );
                        if mode == owner {
                            assert!(pos(title) > mode_y);
                        }
                    }
                    let selected: Vec<_> = mode_fields
                        .into_iter()
                        .filter(|(_, owner)| mode == *owner)
                        .map(|(title, _)| pos(title))
                        .collect();
                    assert!(selected.windows(2).all(|p| p[0] < p[1]));
                    let section =
                        crate::i18n::tr_catalog(lang, "modules_settings.phenom_auto_section");
                    let section_y = text
                        .iter()
                        .find(|(s, _)| s == section)
                        .unwrap()
                        .1
                        .center()
                        .y;
                    let between: Vec<_> = text
                        .iter()
                        .filter(|(_, r)| r.center().y > mode_y && r.center().y < section_y)
                        .map(|(s, _)| s)
                        .collect();
                    for (title, owner) in mode_fields {
                        if mode == owner {
                            assert!(between.contains(&&label(title)));
                        }
                    }
                    // No shared row may follow the selector in the side block.
                    assert!(pos("Sticky mode") > section_y);
                    assert!(selected.iter().all(|y| *y < section_y));
                    assert_eq!(app.module_settings.values, before);
                    let requests = recorder.requests();
                    assert!(
                        requests.iter().all(|r| pending
                            && r[2..4] == qsid.to_le_bytes()
                            && r[4] == mode as u8),
                        "only the explicitly queued mode SET is allowed: {requests:?}"
                    );
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn phenom_touchpad_page_rotation_choices_keep_original_enum_and_reload_both_sides() {
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        for qsid in [132, 133] {
            let mut app = phenom_speed_app();
            app.app_settings.language = lang;
            app.module_settings =
                EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
            app.module_settings
                .set_selected_module_group(usize::from(qsid == 133));
            app.module_settings.set_value(372 + qsid - 132, 2);
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            hid.set_qmk_setting_u8(372 + qsid - 132, 2).unwrap();
            app.hid_device = Some(hid.into());
            let ctx = egui::Context::default();
            for (ui_index, label) in ["0°", "90°", "180°", "270°"].into_iter().enumerate() {
                let wire = ((ui_index + if qsid == 132 { 2 } else { 0 }) % 4) as u16;
                // Start on a different choice so every click is a real edit.
                app.module_settings.set_value(qsid, (wire + 1) % 4);
                let mut text = Vec::new();
                for _ in 0..6 {
                    text = phenom_page_frame(&mut app, &ctx, vec![]);
                }
                let button = text
                    .iter()
                    .find(|(s, _)| s.ends_with('°'))
                    .unwrap()
                    .1
                    .center();
                phenom_page_click(&mut app, &ctx, button);
                for _ in 0..6 {
                    text = phenom_page_frame(&mut app, &ctx, vec![]);
                }
                // Popup rows are painter text, not accessibility nodes.
                let option = text
                    .iter()
                    .rev()
                    .find(|(s, _)| s == label)
                    .unwrap()
                    .1
                    .center();
                phenom_page_click(&mut app, &ctx, option);
                assert_eq!(
                    app.pending_settings_write_value(qsid),
                    Some(wire),
                    "{qsid} UI {label}"
                );
                let pending = phenom_page_frame(&mut app, &ctx, vec![]);
                assert!(pending.iter().any(|(s, _)| s == label));
                drain_hid_writes(&mut app, &ctx);
                assert_eq!(app.module_settings.value(qsid), wire);
                let requests = recorder.requests();
                let last = requests.last().unwrap();
                assert_eq!(&last[2..4], &qsid.to_le_bytes());
                assert_eq!(last[4], wire as u8);
                app.module_settings = EntropyApp::read_module_settings(
                    &json,
                    &(120..400).collect::<Vec<_>>(),
                    app.hid_device.as_ref().unwrap().require_vial().unwrap(),
                );
                assert_eq!(app.module_settings.value(qsid), wire);
                app.module_settings
                    .set_selected_module_group(usize::from(qsid == 133));
                let rendered = phenom_page_frame(&mut app, &ctx, vec![]);
                assert!(
                    rendered.iter().any(|(s, _)| s == label),
                    "reloaded {label}: {rendered:?}"
                );
                assert_eq!(app.pending_settings_write_value(qsid), None);
            }
            let requests = recorder.requests();
            let writes: Vec<_> = requests.iter().filter(|r| r[1] == 0x0b).collect(); // Vial QMK_SETTINGS_SET
            assert_eq!(writes.len(), 5); // Fixture module selector plus four rotation SETs.
            assert!(writes[1..].iter().all(|r| r[2..4] == qsid.to_le_bytes()));
        }
    }
}

#[test]
fn phenom_touchpad_rotation_loaded_angles_paint_without_writes() {
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        for side in 0..=1 {
            for raw in [2, 0, 1, 3] {
                let mut app = phenom_speed_app();
                app.app_settings.language = lang;
                let json = serde_json::from_str(include_str!(
                    "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
                ))
                .unwrap();
                app.module_settings = EntropyApp::module_settings_from_definition(
                    &json,
                    &(120..400).collect::<Vec<_>>(),
                );
                app.module_settings.set_selected_module_group(side);
                app.module_settings.set_value(372 + side as u16, 2);
                let qsid = 132 + side as u16;
                app.module_settings.set_value(qsid, raw);
                let before = app.module_settings.values.clone();
                let (hid, recorder) = crate::hid::HidDevice::test_device();
                app.hid_device = Some(hid.into());
                let ctx = egui::Context::default();
                let mut text = Vec::new();
                for _ in 0..6 {
                    text = phenom_page_frame(&mut app, &ctx, vec![]);
                }
                let expected = format!("{}°", ((raw + if side == 0 { 2 } else { 0 }) % 4) * 90);
                assert!(
                    text.iter().any(|(s, _)| s == &expected),
                    "side {side} fw {} must paint {expected}: {text:?}",
                    raw * 90
                );
                assert_eq!(app.module_settings.values, before);
                assert_eq!(app.pending_settings_write_value(qsid), None);
                assert!(app.settings_write_task.is_none());
                assert!(recorder.requests().is_empty());
            }
        }
    }
}

#[test]
fn phenom_touchpad_rotation_contract_scope_and_pending_module_guards() {
    for fixture in [include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    )] {
        let json = serde_json::from_str(fixture).unwrap();
        for name in [
            "Phenom",
            "Phenom Mini",
            "Ergohaven Phenom Micro",
            "HPD",
            "K:04",
        ] {
            for module in 0..=2 {
                for pending in [false, true] {
                    for qsid in [132, 133] {
                        let side = usize::from(qsid == 133);
                        let mut app = phenom_speed_app();
                        app.layout.as_mut().unwrap().name = name.into();
                        app.module_settings = EntropyApp::module_settings_from_definition(
                            &json,
                            &(120..400).collect::<Vec<_>>(),
                        );
                        let selector = 372 + side as u16;
                        app.module_settings
                            .set_value(selector, if pending { (module + 1) % 3 } else { module });
                        let (hid, recorder) = crate::hid::HidDevice::test_device();
                        app.hid_device = Some(hid.into());
                        if pending {
                            let field = app.module_settings.groups[side]
                                .fields
                                .iter()
                                .find(|f| f.qsid == selector)
                                .unwrap()
                                .clone();
                            app.write_module_setting_value(side, &field, module);
                        }
                        let ctx = egui::Context::default();
                        for raw in 0..4 {
                            app.module_settings.set_value(qsid, raw);
                            let projected =
                                name != "HPD" && name != "K:04" && module == 2 && qsid == 132;
                            let expected =
                                format!("{}°", ((raw + if projected { 2 } else { 0 }) % 4) * 90);
                            let mut text = Vec::new();
                            for _ in 0..3 {
                                text = rotation_frame(&mut app, &ctx, qsid, vec![]);
                            }
                            assert!(text.iter().any(|(s, _)| s == &expected), "{name}, module {module}, pending {pending}, {qsid}, raw {raw}: {text:?}");
                            assert_eq!(app.module_settings.value(qsid), raw);
                            assert_eq!(app.pending_settings_write_value(qsid), None);
                        }
                        assert!(recorder
                            .requests()
                            .iter()
                            .all(|r| pending && r[1] == 0x0b && r[2..4] == selector.to_le_bytes()));
                    }
                }
            }
        }
    }
    let mut app = phenom_speed_app();
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    app.module_settings =
        EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
    app.module_settings.set_value(372, 2);
    let field = app.module_settings.groups[0]
        .fields
        .iter()
        .find(|f| f.qsid == 132)
        .unwrap()
        .clone();
    assert!(app.phenom_left_touch_rotation_field(ModuleSettingsGroupKind::Left, &field));
    assert!(!app.phenom_left_touch_rotation_field(ModuleSettingsGroupKind::Right, &field));
    for mutation in 0..7 {
        let mut invalid = field.clone();
        match mutation {
            0 => invalid.qsid = 133,
            1 => invalid.title = "Ball axis".into(),
            2 => invalid.width = 2,
            3 => invalid.min = 1,
            4 => invalid.max = 4,
            5 => invalid.variants.swap(0, 1),
            _ => invalid.kind = ModuleSettingKind::Integer,
        }
        assert!(!app.phenom_left_touch_rotation_field(ModuleSettingsGroupKind::Left, &invalid));
    }
    app.layout.as_mut().unwrap().firmware = FirmwareProtocol::Rynk;
    assert!(!app.phenom_left_touch_rotation_field(ModuleSettingsGroupKind::Left, &field));
}

#[test]
fn phenom_touchpad_page_device_correct_hover_copy_both_languages_and_sides() {
    use crate::i18n::Language::{English, Russian};
    for lang in [Russian, English] {
        let expected = if lang == Russian {
            [("Жесты тачпада", "Включение жестов тачпада: касание одним пальцем — левый клик, двумя — правый. Удержание пальца — перетаскивание, движение двумя пальцами — прокрутка"),
             ("Чувствительность тачпада, DPI", "Чем выше DPI, тем дальше перемещается указатель при одинаковом движении пальца по тачпаду"),
             ("Ускорение", "При быстрых движениях пальца по тачпаду указатель перемещается дальше"),
             ("Поворот тачпада", "Изменение направления движения указателя при повороте модуля тачпада")]
        } else {
            [("Touch gestures", "Enable touchpad gestures: tap with one finger for a left click, or two for a right click. Hold a finger to drag, or move two fingers to scroll"),
             ("Touchpad sensitivity, DPI", "Higher DPI moves the pointer farther for the same finger movement on the touchpad"),
             ("Acceleration", "Fast finger movements on the touchpad move the pointer farther"),
             ("Touchpad rotation", "Adjust the pointer movement direction to match the touchpad module orientation")]
        };
        for side in 0..=1 {
            let mut app = phenom_speed_app();
            let json = serde_json::from_str(include_str!(
                "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
            ))
            .unwrap();
            app.module_settings =
                EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
            app.app_settings.language = lang;
            app.module_settings.set_selected_module_group(side);
            app.module_settings.set_value(372 + side as u16, 2);
            let before = app.module_settings.values.clone();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            let ctx = egui::Context::default();
            ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
            for (label, hint) in expected {
                let mut painted = Vec::new();
                for _ in 0..6 {
                    painted = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]);
                }
                let pos = painted.iter().find(|(s, _)| s == label).unwrap().1.center();
                for _ in 0..12 {
                    painted =
                        phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
                }
                assert!(
                    painted.iter().any(|(s, _)| s == hint),
                    "side {side}: {hint}"
                );
                assert!(!painted
                    .iter()
                    .any(|(s, _)| s.contains("шара") || s.contains("ball movement")));
            }
            assert_eq!(app.module_settings.values, before);
            assert!(recorder.requests().is_empty());
        }
    }
}

#[test]
fn phenom_trackball_page_preserves_other_module_views() {
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        for (name, module) in [
            ("Phenom", 0),
            ("HPD", 1),
            ("HPD", 2),
            ("K:04", 1),
            ("K:04", 2),
        ] {
            for side in 0..=1 {
                let mut app = phenom_speed_app();
                app.layout.as_mut().unwrap().name = name.into();
                app.app_settings.language = lang;
                let json = serde_json::from_str(include_str!(
                    "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
                ))
                .unwrap();
                app.module_settings = EntropyApp::module_settings_from_definition(
                    &json,
                    &(120..400).collect::<Vec<_>>(),
                );
                app.module_settings.set_selected_module_group(side);
                app.module_settings.set_value(372 + side as u16, module);
                app.module_settings.set_value(134 + side as u16, 2);
                let group = &app.module_settings.groups[side];
                let expected: Vec<_> = group
                    .fields
                    .iter()
                    .filter(|f| {
                        group.field_visible_for_selection(
                            f,
                            group.selected_module_kind(module),
                            group.selected_pointer_mode(2),
                        ) && f.title != "Sticky mode"
                    })
                    .map(|f| app.module_setting_label(group.kind, &f.title))
                    .collect();
                assert_eq!(
                    app.module_setting_label(group.kind, "Module"),
                    crate::i18n::tr_catalog(lang, "modules_settings.phenom_module_type")
                );
                assert_eq!(
                    app.module_setting_label(group.kind, "Mode"),
                    crate::i18n::tr_catalog(lang, "modules_settings.mode")
                );
                let ctx = egui::Context::default();
                let mut text = Vec::new();
                for _ in 0..6 {
                    text = phenom_page_frame(&mut app, &ctx, vec![]);
                }
                let positions: Vec<_> = expected
                    .iter()
                    .map(|label| {
                        text.iter()
                            .find(|(s, _)| s == label)
                            .unwrap_or_else(|| panic!("{name} {module} missing {label}: {text:?}"))
                            .1
                            .center()
                            .y
                    })
                    .collect();
                assert!(
                    positions.windows(2).all(|p| p[0] < p[1]),
                    "{name} {module}: original descriptor row order {positions:?}"
                );
                let half = crate::i18n::tr_catalog(
                    lang,
                    if side == 0 {
                        "modules_settings.phenom_left"
                    } else {
                        "modules_settings.phenom_right"
                    },
                );
                assert!(text.iter().any(|(s, _)| s == half));
            }
        }
    }
}

#[test]
fn phenom_trackball_page_module_selector_retains_real_write_and_pending_visibility() {
    for side in 0..=1 {
        let mut app = phenom_speed_app();
        app.app_settings.language = crate::i18n::Language::English;
        let json = serde_json::from_str(include_str!(
            "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
        ))
        .unwrap();
        app.module_settings =
            EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
        app.module_settings.set_selected_module_group(side);
        app.module_settings.set_value(372 + side as u16, 1);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        let ctx = egui::Context::default();
        let mut text = Vec::new();
        for _ in 0..6 {
            text = phenom_page_frame(&mut app, &ctx, vec![]);
        }
        let button = text
            .iter()
            .find(|(s, _)| s == "Trackball")
            .unwrap()
            .1
            .center();
        phenom_page_click(&mut app, &ctx, button);
        for _ in 0..6 {
            text = phenom_page_frame(&mut app, &ctx, vec![]);
        }
        let option = text
            .iter()
            .rev()
            .find(|(s, _)| s == "Touchpad")
            .unwrap()
            .1
            .center();
        phenom_page_click(&mut app, &ctx, option);
        assert_eq!(app.pending_settings_write_value(372 + side as u16), Some(2));
        for _ in 0..6 {
            text = phenom_page_frame(&mut app, &ctx, vec![]);
        }
        assert!(text.iter().any(|(s, _)| s == "Touchpad sensitivity, DPI"));
        assert!(!text.iter().any(|(s, _)| s == "Trackball sensitivity, DPI"));
        drain_hid_writes(&mut app, &ctx);
        assert_eq!(app.module_settings.value(372 + side as u16), 2);
        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0][2..4], (372 + side as u16).to_le_bytes());
        assert_eq!(requests[0][4], 2);
    }
}

#[test]
fn phenom_trackball_page_approved_order_copy_and_real_side_switch() {
    use crate::i18n::Language::{English, Russian};
    for lang in [Russian, English] {
        let (labels, hints, sides) = if lang == Russian {
            (
                [
                    "Сторона модуля",
                    "Тип модуля",
                    "Чувствительность трекбола, DPI",
                    "Ускорение",
                    "Поворот трекбола",
                    "Режим модуля",
                ],
                [
                    "Половинка клавиатуры, на которой находится настраиваемый модуль",
                    "Модуль, установленный на выбранной половинке",
                    "Чем выше DPI, тем дальше перемещается указатель при одинаковом движении шара",
                    "При быстрых движениях шара указатель перемещается дальше",
                    "Изменение направления движения указателя при повороте модуля трекбола",
                ],
                ["Левая", "Правая"],
            )
        } else {
            (["Module side", "Module type", "Trackball sensitivity, DPI", "Acceleration", "Trackball rotation", "Module mode"],
             ["The keyboard half containing the module being configured", "The module installed on the selected half", "Higher DPI moves the pointer farther for the same ball movement", "Fast ball movements move the pointer farther", "Adjust the pointer movement direction to match the trackball module orientation"],
             ["Left", "Right"])
        };
        let mut app = phenom_speed_app();
        app.app_settings.language = lang;
        let json = serde_json::from_str(include_str!(
            "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
        ))
        .unwrap();
        app.module_settings =
            EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
        for id in [372, 373] {
            app.module_settings.set_value(id, 1);
        }
        app.module_settings.set_value(130, 1);
        app.module_settings.set_value(131, 1);
        app.module_settings.set_value(120, 2);
        app.module_settings.set_value(121, 3);
        let before = app.module_settings.values.clone();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        let ctx = egui::Context::default();
        ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
        for side in 0..=1 {
            let mut text = Vec::new();
            for _ in 0..6 {
                text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]);
            }
            assert_eq!(app.module_settings.selected_module_group(), Some(side));
            let positions: Vec<_> = labels
                .iter()
                .map(|label| {
                    let matching: Vec<_> = text.iter().filter(|(s, _)| s == label).collect();
                    assert_eq!(matching.len(), 1, "one {label}: {text:?}");
                    matching[0].1.center().y
                })
                .collect();
            assert!(
                positions.windows(2).all(|p| p[0] < p[1]),
                "approved row order: {positions:?}"
            );
            assert!(text.iter().any(|(s, _)| s == sides[side]));
            assert!(text
                .iter()
                .any(|(s, _)| s == if side == 0 { "600" } else { "800" }));
            assert!(text.iter().any(|(s, _)| s == "0°"));
            for (label, hint) in labels.iter().zip(hints) {
                let pos = text.iter().find(|(s, _)| s == label).unwrap().1.center();
                let mut hovered = Vec::new();
                for _ in 0..12 {
                    hovered =
                        phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
                }
                assert!(
                    hovered.iter().any(|(s, _)| s == hint),
                    "{label} tooltip {hint}: {hovered:?}"
                );
                for _ in 0..6 {
                    phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]);
                }
            }
            if side == 0 {
                let button = text.iter().find(|(s, _)| s == sides[0]).unwrap().1.center();
                phenom_page_click(&mut app, &ctx, button);
                for _ in 0..6 {
                    text = phenom_page_frame(&mut app, &ctx, vec![]);
                }
                let target = text
                    .iter()
                    .rev()
                    .find(|(s, _)| s == sides[1])
                    .unwrap()
                    .1
                    .center();
                phenom_page_click(&mut app, &ctx, target);
            }
        }
        assert_eq!(app.module_settings.values, before);
        assert!(
            recorder.requests().is_empty(),
            "render/side choice must not write device settings"
        );
    }
}
