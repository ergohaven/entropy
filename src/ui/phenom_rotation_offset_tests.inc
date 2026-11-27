fn rotation_frame(
    app: &mut EntropyApp,
    ctx: &egui::Context,
    qsid: u16,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let side = usize::from(qsid % 2 == 1);
    let idx = app.module_settings.groups[side]
        .fields
        .iter()
        .position(|f| f.qsid == qsid)
        .unwrap();
    let output = crate::app::discard_texture_deltas(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.draw_module_settings_field_row(ui, side, idx, 600.0, 48.0, true),
    ));
    fn collect(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(t) => out.push((
                t.galley.job.text.clone(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            egui::Shape::Vec(v) => {
                for s in v {
                    collect(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut out);
    }
    out
}

fn rotation_click(app: &mut EntropyApp, ctx: &egui::Context, qsid: u16, pos: egui::Pos2) {
    for pressed in [true, false] {
        rotation_frame(
            app,
            ctx,
            qsid,
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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn phenom_rotation_offset_all_choices_write_original_enum_and_reload_both_sides() {
    let json = serde_json::from_str(include_str!(
        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
    ))
    .unwrap();
    for qsid in [130, 131] {
        let mut app = phenom_speed_app();
        app.module_settings =
            EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
        app.module_settings.set_value(372 + qsid - 130, 1);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        hid.set_qmk_setting_u8(372 + qsid - 130, 1).unwrap();
        app.hid_device = Some(hid.into());
        let ctx = egui::Context::default();
        for (label, wire) in [("0°", 1), ("90°", 2), ("180°", 3), ("270°", 0)] {
            // Start on a different choice so every click is a real edit.
            app.module_settings.set_value(qsid, (wire + 1) % 4);
            let mut text = Vec::new();
            for _ in 0..6 {
                text = rotation_frame(&mut app, &ctx, qsid, vec![]);
            }
            let button = text
                .iter()
                .find(|(s, _)| s.ends_with('°'))
                .unwrap()
                .1
                .center();
            rotation_click(&mut app, &ctx, qsid, button);
            for _ in 0..6 {
                text = rotation_frame(&mut app, &ctx, qsid, vec![]);
            }
            // Popup rows are painter text, not accessibility nodes.
            let option = text
                .iter()
                .rev()
                .find(|(s, _)| s == label)
                .unwrap()
                .1
                .center();
            rotation_click(&mut app, &ctx, qsid, option);
            assert_eq!(
                app.pending_settings_write_value(qsid),
                Some(wire),
                "{qsid} UI {label}"
            );
            let pending = rotation_frame(&mut app, &ctx, qsid, vec![]);
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
            let rendered = rotation_frame(&mut app, &ctx, qsid, vec![]);
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

#[test]
fn phenom_rotation_offset_scope_and_all_raw_read_values() {
    for fixture in [
        include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json"),
        include_str!("../../tests/fixtures/phenom-qmk-settings-f6d3d4d5.json"),
    ] {
        let json = serde_json::from_str(fixture).unwrap();
        for name in [
            "Phenom",
            "Ergohaven Phenom Mini",
            "Phenom Micro",
            "HPD",
            "K:04",
        ] {
            for qsid in [130, 131, 132, 133] {
                let mut app = phenom_speed_app();
                app.layout.as_mut().unwrap().name = name.into();
                app.module_settings = EntropyApp::module_settings_from_definition(
                    &json,
                    &(120..400).collect::<Vec<_>>(),
                );
                app.module_settings.set_value(372 + qsid % 2, 1);
                let ctx = egui::Context::default();
                for raw in 0..4 {
                    app.module_settings.set_value(qsid, raw);
                    let expected = if name != "HPD" && name != "K:04" && qsid < 132 {
                        (raw + 3) % 4
                    } else {
                        raw
                    } * 90;
                    let mut text = Vec::new();
                    for _ in 0..3 {
                        text = rotation_frame(&mut app, &ctx, qsid, vec![]);
                    }
                    assert!(
                        text.iter().any(|(s, _)| s == &format!("{expected}°")),
                        "{name} {qsid} raw {raw}: {text:?}"
                    );
                    assert_eq!(app.module_settings.value(qsid), raw);
                    assert_eq!(app.pending_settings_write_value(qsid), None);
                    assert!(app.settings_write_task.is_none());
                }
            }
        }
    }
    let app = phenom_speed_app();
    let field = app.module_settings.groups[0]
        .fields
        .iter()
        .find(|f| f.qsid == 130)
        .unwrap();
    let mut malformed = field.clone();
    malformed.qsid = 132;
    assert!(!app.phenom_ball_rotation_field(ModuleSettingsGroupKind::Left, &malformed));
    malformed = field.clone();
    malformed.variants.swap(0, 1);
    assert!(!app.phenom_ball_rotation_field(ModuleSettingsGroupKind::Left, &malformed));
    assert!(!app.phenom_ball_rotation_field(ModuleSettingsGroupKind::Right, field));
}

#[test]
fn phenom_rotation_offset_firmware_90_renders_zero_without_writes() {
    for qsid in [130, 131] {
        let mut app = phenom_speed_app();
        app.module_settings.set_value(qsid, 1); // Wire enum 1 is firmware 90 degrees.
        let ctx = egui::Context::default();
        let mut text = Vec::new();
        for _ in 0..6 {
            text = rotation_frame(&mut app, &ctx, qsid, vec![]);
        }
        assert!(
            text.iter().any(|(s, _)| s == "0°"),
            "wire 1 must display UI 0°, got {text:?}"
        );
        assert!(!text.iter().any(|(s, _)| s == "90°"));
        assert_eq!(app.module_settings.value(qsid), 1);
        assert_eq!(app.pending_settings_write_value(qsid), None);
        assert!(app.settings_write_task.is_none());
    }
}
