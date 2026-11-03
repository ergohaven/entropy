use super::*;

fn device(name: &str, vendor_id: u16, product_id: u16) -> crate::device::Device {
    crate::device::Device {
        name: name.into(), vendor_id, product_id, manufacturer: "Ergohaven".into(),
        serial_number: String::new(), bus_type: "Usb".into(), path: format!("test-menu-{product_id}"),
        instance_token: String::new(), firmware: FirmwareProtocol::Vial,
    }
}

fn labels(app: &mut EntropyApp, ctx: &egui::Context, lang: crate::i18n::Language) -> Vec<String> {
    let layout = app.layout.clone().unwrap();
    let mut output = None;
    // Areas need one layout pass before their visible paint pass.
    for _ in 0..2 {
        output = Some(crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 1600.0))),
            ..Default::default()
        }, |ctx| {
            ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                app.draw_layout_settings_dropdown(ui, &layout, lang,
                    Some(egui::Rect::from_min_size(egui::pos2(500.0, 0.0), egui::vec2(80.0, 30.0))),
                    false, false, true);
            }));
        })));
    }
    output.unwrap().shapes.into_iter().filter_map(|s| match s.shape {
        egui::Shape::Text(t) => Some(t.galley.text().to_owned()), _ => None,
    }).collect()
}

fn fixture(ctx: &egui::Context) -> EntropyApp {
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = EntropyApp::new(&cc);
    let definition: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/k04-vial-b4709ab9.json")).unwrap();
    app.layout = Some(KeyboardLayout::from_vial_json(&definition).unwrap());
    app.supported_qmk_settings = definition["settings"].as_array().unwrap().iter()
        .flat_map(|group| group["fields"].as_array().unwrap())
        .map(|field| field["qsid"].as_u64().unwrap() as u16).collect();
    assert!(!app.supported_qmk_settings.iter().any(|id| (230..=239).contains(id)),
        "recorded K04 definition has no display IDs; collisions below are deliberate synthetic controls");
    app.module_settings = EntropyApp::module_settings_from_definition(&definition, &app.supported_qmk_settings);
    app.layer_led_settings.supported = true;
    app.firmware = FirmwareProtocol::Vial;
    app
}

fn screenless_menu(lang: crate::i18n::Language) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid);
    for keyboard in [device("Ergohaven K:04", 0xE126, 0x0074), device("Generic screenless", 0x1234, 0x5678)] {
        app.device_manager.replace_devices(vec![keyboard]);
        app.selected_device = Some(0);
        app.display_settings = DisplaySettingsState::default();
        app.supported_qmk_settings.retain(|id| !(230..=239).contains(id));
        let baseline = labels(&mut app, &ctx, lang);
        let title = crate::i18n::tr_catalog(lang, "display_settings.title");
        assert!(!baseline.iter().any(|label| label == title));
        assert!(baseline.iter().any(|label| label == crate::i18n::tr(lang, crate::i18n::Key::LayerLedsTitle)));
        assert!(baseline.iter().any(|label| label == crate::i18n::tr_catalog(lang, app.module_settings_title_key())));
        for state in 0..4 {
            app.display_settings.supported = state == 0;
            app.display_settings.sm62_supported = state == 1;
            app.supported_qmk_settings.retain(|id| !(230..=239).contains(id));
            if state == 2 { app.supported_qmk_settings.push(230); }
            if state == 3 { app.supported_qmk_settings.extend(230..=239); }
            let actual = labels(&mut app, &ctx, lang);
            assert!(!actual.iter().any(|label| label == title),
                "screenless selected device must not paint Display ({lang:?}, synthetic stale/collision state {state}): {actual:?}");
            assert_eq!(actual, baseline, "all unrelated menu entries and their order must remain unchanged");
        }
    }
    assert!(recorder.requests().is_empty(), "opening settings must not issue HID requests");
}

#[test]
fn catalog_production_loader_to_menu_matrix_preserves_existing_routes_ru_en() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let rows: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/display-menu-catalog.json")).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 49);
    for row in rows.as_array().unwrap() {
        let definition = &row["definition"];
        let supported: Vec<u16> = definition["settings"].as_array().into_iter().flatten()
            .flat_map(|g| g["fields"].as_array().into_iter().flatten())
            .filter_map(|f| f["qsid"].as_u64().map(|n| n as u16)).collect();
        for bluetooth in [false, true].into_iter().filter(|ble| !*ble || row["bluetooth"] == true) {
            for lang in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
                let ctx = egui::Context::default();
                let mut app = fixture(&ctx);
                app.layout = Some(KeyboardLayout::from_vial_json(definition).unwrap());
                app.supported_qmk_settings = supported.clone();
                app.module_settings = EntropyApp::module_settings_from_definition(definition, &supported);
                let (hid, recorder) = crate::hid::HidDevice::test_device();
                app.bluetooth_settings = EntropyApp::read_bluetooth_settings(definition, &supported, &hid);
                let mut keyboard = device(row["name"].as_str().unwrap(), row["vendor_id"].as_u64().unwrap() as u16, row["product_id"].as_u64().unwrap() as u16);
                keyboard.bus_type = if bluetooth { "Bluetooth" } else { "Usb" }.into();
                app.display_settings = super::super::device_connect_task::load_selected_display_settings(&keyboard, &hid, &supported).unwrap();
                let dedicated = row["dedicated"].as_bool().unwrap();
                if !dedicated {
                    assert!(!app.display_settings.supported && !app.display_settings.sm62_supported, "{row}");
                }
                app.hid_device = Some(hid);
                app.device_manager.replace_devices(vec![keyboard]);
                app.selected_device = Some(0);
                let baseline = labels(&mut app, &ctx, lang);
                assert_eq!(baseline.iter().any(|s| s == crate::i18n::tr_catalog(lang, "display_settings.title")), dedicated, "{}", row["name"]);
                if !app.user_layout_option_indices(app.layout.as_ref().unwrap()).is_empty() {
                    assert!(baseline.iter().any(|s| s == crate::i18n::tr(lang, crate::i18n::Key::DisplayPresetsTitle)), "{}", row["name"]);
                }
                let count = recorder.requests().len();
                // Retained capabilities from either dedicated schema must not
                // alter ANY other entry or its ordering on selection change.
                app.display_settings.supported = true;
                app.display_settings.sm62_supported = true;
                app.supported_qmk_settings.extend(230..=239);
                assert_eq!(labels(&mut app, &ctx, lang), baseline, "{} / BLE={bluetooth}", row["name"]);
                assert_eq!(recorder.requests().len(), count);
            }
        }
    }
}

#[test]
fn unknown_vial_full_and_partial_qsid_collisions_never_probe_dedicated_schema() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for ids in [vec![230], (230..=239).collect(), vec![320, 321, 322]] {
        let keyboard = device("M4CR0Pad v3 arbitrary third-party descriptor", 0x1234, 0x5678);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let state = super::super::device_connect_task::load_selected_display_settings(&keyboard, &hid, &ids).unwrap();
        assert!(!state.supported && !state.sm62_supported);
        assert!(recorder.requests().is_empty());
        assert!(!keyboard.supports_dedicated_display_settings());
    }
}

#[test]
fn legacy_lcd_contract_and_oled_preset_page_stay_separate_from_dedicated_page() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let rows: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/display-menu-catalog.json")).unwrap();
    let mut lcd_count = 0;
    let mut oled_count = 0;
    for row in rows.as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let lcd = name.contains("/k03pro/");
        let oled = name.contains("QMK/") && ["/k03/", "/k02/", "/imperial44/"].iter().any(|s| name.contains(s));
        if !lcd && !oled { continue; }
        let definition = &row["definition"];
        if lcd {
            assert!(crate::qmk_hid_host::supports_legacy_lcd_clock(definition, &[318,319]));
            assert!(!crate::qmk_hid_host::supports_legacy_lcd_clock(definition, &[318]));
            lcd_count += 1;
        }
        for lang in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
            let ctx = egui::Context::default();
            let mut app = fixture(&ctx);
            app.layout = Some(KeyboardLayout::from_vial_json(definition).unwrap());
            app.app_settings.language = lang;
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid);
            app.layout_options_value = Some(0);
            app.device_manager.replace_devices(vec![device(name, row["vendor_id"].as_u64().unwrap() as u16, row["product_id"].as_u64().unwrap() as u16)]);
            app.selected_device = Some(0);
            assert!(!labels(&mut app, &ctx, lang).iter().any(|s| s == crate::i18n::tr_catalog(lang, "display_settings.title")));
            let indices = app.user_layout_option_indices(app.layout.as_ref().unwrap());
            if oled && !indices.is_empty() {
                let options = app.layout.as_ref().unwrap().layout_options.clone();
                let out = crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0,1600.0));
                        app.draw_layout_options_settings_page(ui, rect);
                    }));
                }));
                let painted: Vec<String> = out.shapes.into_iter().filter_map(|s| match s.shape {egui::Shape::Text(t) => Some(t.galley.text().to_owned()), _=>None}).collect();
                assert!(indices.iter().any(|&i| painted.iter().any(|s| s == &crate::i18n::tr_text(lang, &options[i].label))), "working OLED preset controls: {name} {painted:?}");
                oled_count += 1;
            }
            assert!(recorder.requests().is_empty());
        }
    }
    assert_eq!(lcd_count, 2);
    assert!(oled_count >= 8);
}

#[test]
fn dedicated_production_loader_retains_macropad_and_sm62_controls() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for (pid, ids) in [(0x41, vec![320,321,322]), (0x42, vec![320,321,322]), (0xb2, (230..=239).collect())] {
        let keyboard = device("Dedicated control", 0xe126, pid);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let state = super::super::device_connect_task::load_selected_display_settings(&keyboard, &hid, &ids).unwrap();
        assert!(state.supported);
        assert_eq!(state.sm62_supported, pid == 0xb2);
        assert!(!recorder.requests().is_empty());
    }
}

#[test]
fn k04_definition_bluetooth_color_ids_do_not_advertise_a_display() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for lang in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
        let ctx = egui::Context::default();
        let mut app = fixture(&ctx);
        let definition: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/k04-vial-b4709ab9.json")).unwrap();
        for (id, title) in [(320, "BT profile 2 color"), (321, "BT profile 3 color"), (322, "BT profile 4 color")] {
            assert!(definition["settings"].as_array().unwrap().iter().flat_map(|g| g["fields"].as_array().unwrap())
                .any(|f| f["qsid"] == id && f["title"] == title));
        }
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        // Production connect uses this loader for all three IDs. The actual K04
        // definition assigns them to BT profile colors, not Macropad display RGB.
        let keyboard = device("K:04", 0xE126, 0x0074);
        app.display_settings = super::super::device_connect_task::load_selected_display_settings(&keyboard, &hid, &app.supported_qmk_settings).unwrap();
        assert!(!app.display_settings.supported);
        assert!(recorder.requests().is_empty(), "BT color IDs must not launch display probes");
        let request_count = recorder.requests().len();
        app.hid_device = Some(hid);
        app.device_manager.replace_devices(vec![device("K:04", 0xE126, 0x0074)]);
        app.selected_device = Some(0);
        let actual = labels(&mut app, &ctx, lang);
        assert!(!actual.iter().any(|label| label == crate::i18n::tr_catalog(lang, "display_settings.title")),
            "K04 Bluetooth color IDs must not paint Display ({lang:?}): {actual:?}");
        assert_eq!(recorder.requests().len(), request_count);
    }
}

#[test]
fn k04_actual_settings_dropdown_excludes_display_english() { screenless_menu(crate::i18n::Language::English); }
#[test]
fn k04_actual_settings_dropdown_excludes_display_russian() { screenless_menu(crate::i18n::Language::Russian); }

#[test]
fn display_devices_keep_actual_menu_through_reload_selection_and_inventory_order() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for lang in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
        for display in [device("M4CR0Pad v2", 0xE126, 0x0041), device("Macropad", 0xE126, 0x0042), device("SM62", 0xE126, 0x00B2)] {
            for display_first in [true, false] {
                let ctx = egui::Context::default();
                let mut app = fixture(&ctx);
                let (hid, recorder) = crate::hid::HidDevice::test_device();
                app.hid_device = Some(hid);
                let k04 = device("K:04", 0xE126, 0x0074);
                app.device_manager.replace_devices(if display_first { vec![display.clone(), k04] } else { vec![k04, display.clone()] });
                let index = usize::from(!display_first);
                for (selected, supported) in [(index, false), (index, true), (1-index, true), (index, false)] {
                    app.selected_device = Some(selected);
                    app.display_settings.supported = supported;
                    app.display_settings.sm62_supported = supported;
                    app.supported_qmk_settings = if supported { (230..=239).collect() } else { Vec::new() };
                    let actual = labels(&mut app, &ctx, lang);
                    assert_eq!(actual.iter().any(|label| label == crate::i18n::tr_catalog(lang, "display_settings.title")), selected == index,
                        "selected identity, not pending or stale capability cache, determines visibility: {actual:?}");
                }
                app.selected_device = None;
                assert!(!labels(&mut app, &ctx, lang).iter().any(|label| label == crate::i18n::tr_catalog(lang, "display_settings.title")));
                assert!(recorder.requests().is_empty());
            }
        }
    }
}
