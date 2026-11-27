#[test]
fn module_side_selector_unified_copy_compact_width_and_real_switch() {
    use crate::i18n::Language::{Russian, English};
    for lang in [Russian, English] {
      for name in ["Phenom", "HPD", "K:04"] {
        for module in [0, 1, 2] {
            let mut app = phenom_speed_app();
            app.app_settings.language = lang;
            app.layout.as_mut().unwrap().name = name.into();
            let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
            app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
            for id in [372, 373] { app.module_settings.set_value(id, module); }
            let before = app.module_settings.values.clone();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            let ctx = egui::Context::default();
            ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
            let (sides, label, hint) = if lang == Russian {
                (["Левая", "Правая"], "Сторона модуля", "Половинка клавиатуры, на которой находится настраиваемый модуль")
            } else {
                (["Left", "Right"], "Module side", "The keyboard half containing the module being configured")
            };
            for side in 0..=1 {
                let mut text = Vec::new();
                for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]); }
                assert_eq!(app.module_settings.selected_module_group(), Some(side));
                let selected = text.iter().find(|(s, _)| s == sides[side]).unwrap_or_else(|| panic!("module {module} missing {}", sides[side])).1;
                assert!(!text.iter().any(|(s, _)| ["Левая половинка", "Правая половинка", "Left half", "Right half"].contains(&s.as_str())));
                // Inspect the actual painted dropdown frames, not a width constant.
                let out = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 2000.0))), ..Default::default()
                }, |ui| { let rect = ui.available_rect_before_wrap(); app.draw_module_settings_page(ui, rect); }));
                fn rectangles(shape: &egui::Shape, rects: &mut Vec<egui::Rect>) {
                    match shape {
                        egui::Shape::Rect(r) => rects.push(r.rect),
                        egui::Shape::Vec(v) => for s in v { rectangles(s, rects); },
                        _ => {}
                    }
                }
                let mut rects = Vec::new();
                for s in out.shapes { rectangles(&s.shape, &mut rects); }
                let dropdown = rects.iter().filter(|r| r.contains(selected.center()) && r.height() < 50.0 && r.width() > selected.width()).min_by(|a,b| a.width().total_cmp(&b.width())).expect("painted side dropdown");
                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(&ctx);
                assert!((dropdown.width() - metrics.value(120.0)).abs() < 1.0, "module {module} dropdown width {}", dropdown.width());
                let pos = text.iter().find(|(s, _)| s == label).unwrap().1.center();
                let mut hovered = Vec::new();
                for _ in 0..12 { hovered = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]); }
                assert!(hovered.iter().any(|(s, _)| s == hint), "module {module}: missing tooltip {hint}");
                for _ in 0..6 { phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]); }
                phenom_page_click(&mut app, &ctx, selected.center());
                for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![]); }
                let target = text.iter().rev().find(|(s, _)| s == sides[1-side]).unwrap().1.center();
                phenom_page_click(&mut app, &ctx, target);
            }
            assert_eq!(app.module_settings.selected_module_group(), Some(0));
            assert_eq!(app.module_settings.values, before);
            assert!(recorder.requests().is_empty(), "render/side switch must not write");
        }
      }
    }
}

#[test]
fn module_side_selector_module_type_copy_and_hover_all_views() {
    use crate::i18n::Language::{Russian, English};
    for lang in [Russian, English] {
        for name in ["Phenom", "HPD", "K:04"] {
            for module in [0, 1, 2] {
                for side in 0..=1 {
                    let mut app = phenom_speed_app();
                    app.app_settings.language = lang;
                    app.layout.as_mut().unwrap().name = name.into();
                    let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
                    app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
                    app.module_settings.set_selected_module_group(side);
                    app.module_settings.set_value(372 + side as u16, module);
                    let before = app.module_settings.values.clone();
                    let (hid, recorder) = crate::hid::HidDevice::test_device();
                    app.hid_device = Some(hid.into());
                    let ctx = egui::Context::default();
                    ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
                    let (label, hint) = if lang == Russian {
                        ("Тип модуля", "Модуль, установленный на выбранной половинке")
                    } else {
                        ("Module type", "The module installed on the selected half")
                    };
                    let mut text = Vec::new();
                    for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![]); }
                    let labels: Vec<_> = text.iter().filter(|(s, _)| s == label).collect();
                    assert_eq!(labels.len(), 1, "{name} module {module}, side {side}: {label}");
                    let pos = labels[0].1.center();
                    for _ in 0..12 { text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]); }
                    assert!(text.iter().any(|(s, _)| s == hint), "{name} module {module} side {side}: tooltip");
                    assert_eq!(app.module_settings.values, before);
                    assert!(recorder.requests().is_empty());
                    for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![egui::Event::PointerGone]); }
                    let variants = ["Encoder", "Trackball", "Touchpad"];
                    let current_label = module_setting_variant_label(lang, variants[module as usize]);
                    let selected = text.iter().find(|(s, _)| s == &current_label).expect("module dropdown selection").1.center();
                    phenom_page_click(&mut app, &ctx, selected);
                    for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![]); }
                    let next = (module + 1) % 3;
                    let next_label = module_setting_variant_label(lang, variants[next as usize]);
                    let target = text.iter().rev().find(|(s, _)| s == &next_label).expect("module popup option").1.center();
                    phenom_page_click(&mut app, &ctx, target);
                    let qsid = 372 + side as u16;
                    assert_eq!(app.pending_settings_write_value(qsid), Some(next));
                    drain_hid_writes(&mut app, &ctx);
                    assert_eq!(app.module_settings.value(qsid), next);
                    let requests = recorder.requests();
                    assert_eq!(requests.len(), 1);
                    assert_eq!(requests[0][2..4], qsid.to_le_bytes());
                    assert_eq!(requests[0][4], next as u8);
                }
            }
        }
    }
}

#[test]
fn module_side_selector_page_subtitle_approved_ru_en() {
    for (lang, expected) in [
        (crate::i18n::Language::Russian, "Настройки энкодера, трекбола и тачпада"),
        (crate::i18n::Language::English, "Encoder, trackball and touchpad settings"),
    ] {
        let mut app = phenom_speed_app();
        app.app_settings.language = lang;
        let json = serde_json::from_str(include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")).unwrap();
        app.module_settings = EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
        let ctx = egui::Context::default();
        let mut text = Vec::new();
        for _ in 0..6 { text = phenom_page_frame(&mut app, &ctx, vec![]); }
        let heading = crate::i18n::tr_catalog(lang, "modules_settings.title");
        let heading_y = text.iter().find(|(s, _)| s == heading).expect("module heading").1.center().y;
        let subtitle = text.iter().find(|(s, _)| s == expected).expect("approved subtitle painted verbatim");
        assert!(subtitle.1.center().y > heading_y);
    }
}
