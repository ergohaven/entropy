use super::*;

fn app_with_geometry(ctx: &egui::Context) -> EntropyApp {
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = EntropyApp::new(&creation);
    app.app_settings.language = crate::i18n::Language::Russian;
    app.app_settings.layout_sync_enabled = false;
    app.layout = Some(KeyboardLayout {
        name: "Geometry test".into(),
        rows: 0,
        cols: 0,
        keys: vec![],
        encoders: vec![],
        layers: vec![],
        encoder_layers: vec![],
        layer_names: vec![],
        custom_keycodes: vec![],
        layout_options: vec![
            LayoutOption {
                label: "Universal Symbols geometry".into(),
                choices: vec!["ANSI".into(), "ISO".into()],
            },
            LayoutOption {
                label: "Hide left encoder".into(),
                choices: vec![],
            },
            LayoutOption {
                label: "Hide right encoder".into(),
                choices: vec![],
            },
        ],
        live_features: Default::default(),
        supports_rgb: false,
        lighting_mode: None,
        firmware: FirmwareProtocol::Vial,
    });
    app.layout_options_value = Some(3);
    app
}

fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
        egui::Shape::Vec(v) => {
            for s in v {
                texts(s, out);
            }
        }
        _ => {}
    }
}

#[test]
fn universal_geometry_follows_layout_sync_even_when_sync_disabled() {
    for enabled in [false, true] {
        let ctx = egui::Context::default();
        let mut app = app_with_geometry(&ctx);
        app.app_settings.layout_sync_enabled = enabled;
        app.device_manager
            .replace_devices(vec![crate::device::Device {
                name: "Test".into(),
                vendor_id: 0x1209,
                product_id: 0x2327,
                manufacturer: "Entropy".into(),
                serial_number: "test".into(),
                bus_type: "Bluetooth".into(),
                path: "geometry-test".into(),
                instance_token: String::new(),
                firmware: FirmwareProtocol::Vial,
            }]);
        app.selected_device = Some(0);
        app.layout
            .as_mut()
            .unwrap()
            .custom_keycodes
            .push(crate::keyboard::CustomKeycode {
                name: "LG_SYNC".into(),
                label: "RuEn\nSync".into(),
                title: "Sync language".into(),
            });
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.draw_live_features_settings_page(ui, ui.max_rect()),
        );
        let mut rendered = vec![];
        for s in output.shapes {
            texts(&s.shape, &mut rendered);
        }
        let sync = rendered
            .iter()
            .position(|s| s == "Синхронизация раскладки")
            .expect("sync row");
        let symbols = rendered
            .iter()
            .position(|s| s == "Universal Symbols")
            .expect("symbols block");
        assert!(
            sync < symbols,
            "Universal Symbols must follow sync; enabled={enabled}, {rendered:?}"
        );
        assert_eq!(app.app_settings.layout_sync_enabled, enabled);
        assert_eq!(app.layout_options_value, Some(3));
    }
}

#[test]
fn universal_geometry_keeps_live_menu_available_for_vial_without_host_data() {
    let ctx = egui::Context::default();
    let mut app = app_with_geometry(&ctx);
    app.device_manager
        .replace_devices(vec![crate::device::Device {
            name: "Test".into(),
            vendor_id: 0x1209,
            product_id: 0x2327,
            manufacturer: "Entropy".into(),
            serial_number: "test".into(),
            bus_type: "Usb".into(),
            path: "geometry-test".into(),
            instance_token: String::new(),
            firmware: FirmwareProtocol::Vial,
        }]);
    app.selected_device = Some(0);
    assert!(app.live_features_available_for_selected_device());
}

#[test]
fn universal_geometry_changes_preserve_encoder_bits_and_restore_ansi() {
    let ctx = egui::Context::default();
    let mut app = app_with_geometry(&ctx);
    for encoders in 0..4 {
        app.layout_options_value = Some(encoders);
        app.set_layout_option_value(0, 1);
        assert_eq!(app.layout_options_value, Some(encoders | 4));
        app.set_layout_option_value(0, 0);
        assert_eq!(app.layout_options_value, Some(encoders));
    }
    assert!(!EntropyApp::is_universal_geometry_layout_option(
        &LayoutOption {
            label: "Enter Key".into(),
            choices: vec!["ANSI".into(), "ISO".into()]
        }
    ));
}

#[test]
fn universal_geometry_is_in_live_integrations_without_live_support_or_toggle() {
    let ctx = egui::Context::default();
    let mut app = app_with_geometry(&ctx);
    assert!(!app.live_features_available_for_selected_device());
    for language in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        app.app_settings.language = language;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.draw_live_features_settings_page(ui, ui.max_rect()),
        );
        let mut rendered = vec![];
        for s in output.shapes {
            texts(&s.shape, &mut rendered);
        }
        assert!(
            rendered.iter().any(|s| s == "Universal Symbols"),
            "{rendered:?}"
        );
        let label = if language == crate::i18n::Language::Russian {
            "Тип клавиатуры для Universal Symbols"
        } else {
            "Keyboard type for Universal Symbols"
        };
        assert!(rendered.iter().any(|s| s == label), "{rendered:?}");
        assert!(rendered.iter().any(|s| s == "ANSI"));
    }
    let layout = app.layout.as_ref().unwrap();
    assert!(
        app.user_layout_option_indices(layout).is_empty(),
        "geometry must leave Display Presets"
    );
    assert_eq!(
        EntropyApp::encoder_layout_option_indices(layout),
        vec![1, 2]
    );
    assert_eq!(app.layout_options_value, Some(3));
}
