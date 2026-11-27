// Render the production page, including its section and row layout, not a label proxy.
#[test]
fn phenom_sticky_is_in_auto_layer_section_and_remains_independently_editable() {
    fn collect(
        shape: &egui::Shape,
        texts: &mut Vec<(String, egui::Rect)>,
        switches: &mut Vec<egui::Rect>,
    ) {
        match shape {
            egui::Shape::Text(t) => texts.push((
                t.galley.job.text.clone(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            egui::Shape::Rect(r)
                if (r.rect.width() / r.rect.height() - 46.0 / 24.0).abs() < 0.02
                    && (12.0..=48.0).contains(&r.rect.height()) =>
            {
                switches.push(r.rect)
            }
            egui::Shape::Vec(shapes) => {
                for s in shapes {
                    collect(s, texts, switches);
                }
            }
            _ => {}
        }
    }
    for side in 0..=1 {
        for module in [1, 2] {
            // Trackball and Touchpad, from the real Phenom descriptor.
            for sticky in [0, 1] {
                for auto_enabled in [0, 1] {
                    let mut app = phenom_speed_app();
                    let json = serde_json::from_str(include_str!(
                        "../../tests/fixtures/phenom-qmk-settings-a39f2668.json"
                    ))
                    .unwrap();
                    app.module_settings = EntropyApp::module_settings_from_definition(
                        &json,
                        &(120..400).collect::<Vec<_>>(),
                    );
                    app.app_settings.language = crate::i18n::Language::Russian;
                    app.module_settings.set_selected_module_group(side);
                    app.module_settings.set_value(372 + side as u16, module);
                    app.module_settings.set_value(134 + side as u16, 2);
                    let qsid = 140 + side as u16;
                    app.module_settings.set_value(qsid, sticky);
                    app.module_settings.set_value(141 - side as u16, 1 - sticky);
                    for id in [142, 144, 145, 146] {
                        app.module_settings.set_value(id, auto_enabled);
                    }
                    let before = app.module_settings.values.clone();
                    let (hid, recorder) = crate::hid::HidDevice::test_device();
                    app.hid_device = Some(hid.into());
                    let ctx = egui::Context::default();
                    let mut texts = Vec::new();
                    let mut switches = Vec::new();
                    for _ in 0..3 {
                        let out = crate::app::discard_texture_deltas(ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(1600.0, 2000.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                let rect = ui.available_rect_before_wrap();
                                app.draw_module_settings_page(ui, rect);
                            },
                        ));
                        texts.clear();
                        switches.clear();
                        for shape in &out.shapes {
                            collect(&shape.shape, &mut texts, &mut switches);
                        }
                    }
                    let positions = |label: &str| {
                        texts
                            .iter()
                            .filter(|(s, _)| s == label)
                            .map(|(_, r)| r.center().y)
                            .collect::<Vec<_>>()
                    };
                    let sticky_y = positions("Фиксация режима модуля");
                    assert_eq!(sticky_y.len(), 1, "exactly one selected-side sticky row");
                    let section_y = positions("Настройки автослоя мыши");
                    assert!(!section_y.is_empty(), "auto-layer section must be painted");
                    assert!(
                        section_y[0] < sticky_y[0],
                        "sticky belongs below the auto-layer heading, not in the device block"
                    );
                    let timeout_label = crate::i18n::tr_catalog(
                        crate::i18n::Language::Russian,
                        "modules_settings.auto_layer_timeout",
                    );
                    assert!(
                        positions(timeout_label)[0]
                            < positions("Выключение автослоя по клавише")[0]
                    );
                    assert!(
                        positions("Выключение автослоя по клавише")[0] < sticky_y[0],
                        "sticky is last"
                    );
                    assert!(!texts.iter().any(|(s, _)| s == "Порог автослоя"));
                    assert_eq!(
                        app.module_settings.values, before,
                        "painting and switching sides must preserve saved settings"
                    );
                    assert!(recorder.requests().is_empty());
                    assert_eq!(app.pending_settings_write_value(qsid), None);
                    // Click the actual painted switch, including when all automatic activation flags are off.
                    let switch = switches
                        .iter()
                        .min_by(|a, b| {
                            (a.center().y - sticky_y[0])
                                .abs()
                                .total_cmp(&(b.center().y - sticky_y[0]).abs())
                        })
                        .unwrap_or_else(|| {
                            panic!("sticky switch: y={:?}, switches={switches:?}", sticky_y)
                        })
                        .center();
                    assert!(
                        (switch.y - sticky_y[0]).abs() < 20.0,
                        "switch must be in the sticky row: {switch:?}, {sticky_y:?}"
                    );
                    for pressed in [true, false] {
                        let _ = crate::app::discard_texture_deltas(ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(1600.0, 2000.0),
                                )),
                                events: vec![
                                    egui::Event::PointerMoved(switch),
                                    egui::Event::PointerButton {
                                        pos: switch,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: egui::Modifiers::NONE,
                                    },
                                ],
                                ..Default::default()
                            },
                            |ui| {
                                let rect = ui.available_rect_before_wrap();
                                app.draw_module_settings_page(ui, rect);
                            },
                        ));
                    }
                    assert_eq!(app.pending_settings_write_value(qsid), Some(1 - sticky));
                    drain_hid_writes(&mut app, &ctx);
                    let mut expected = before;
                    expected.insert(qsid, 1 - sticky);
                    assert_eq!(app.module_settings.values, expected);
                    assert_eq!(recorder.requests().len(), 1);
                    assert_eq!(recorder.requests()[0][2..4], qsid.to_le_bytes());
                    let reloaded = EntropyApp::read_module_settings(
                        &json,
                        &[qsid],
                        app.hid_device.as_ref().unwrap().require_vial().unwrap(),
                    );
                    assert_eq!(reloaded.value(qsid), 1 - sticky);
                }
            }
        }
    }
}
