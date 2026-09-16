//! Real App::ui, picker pointer events and real task completion on an inert
//! bootstrap. No App::logic (OS scans/config polling), real HID or user files.
use super::*;
use std::time::{Duration, Instant};

fn app(unlocked: bool) -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
    let mut app = EntropyApp::new_inert_for_test();
    let ctx = egui::Context::default();
    let mut fonts = egui::FontDefinitions::default();
    for family in ["display_preview", "clock_montserrat", "emoji_preview"] {
        fonts.families.insert(
            egui::FontFamily::Name(family.into()),
            fonts.families[&egui::FontFamily::Proportional].clone(),
        );
    }
    ctx.set_fonts(fonts);
    app.app_settings.ui_scale = 1.0;
    app.app_settings.language = crate::i18n::Language::Russian;
    app.app_settings.onboarding_tour_seen_version = ONBOARDING_TOUR_VERSION;
    app.app_settings.sticky_layout_window = false;
    app.app_settings.show_made_by_signature = false;
    let device = Device {
        name: "Inert picker fixture".into(),
        vendor_id: 0xFFFF,
        product_id: 0xFFFE,
        manufacturer: "fixture".into(),
        serial_number: "fixture".into(),
        bus_type: "USB".into(),
        path: "never-opened-picker-fixture".into(),
        instance_token: "fixture-enumeration".into(),
        firmware: FirmwareProtocol::Vial,
    };
    app.device_manager.replace_devices(vec![device]);
    app.selected_device = Some(0);
    app.layout = Some(
        KeyboardLayout::from_vial_json(&serde_json::json!({
            "name": "Inert picker fixture", "matrix": {"rows": 1, "cols": 1},
            "layouts": {"keymap": [["0,0"]]}
        }))
        .unwrap(),
    );
    app.layer_count = 1;
    app.main_menu_tab = MainMenuTab::Settings;
    app.settings_tab = SettingsTab::Display;
    app.display_settings.supported = true;
    app.display_settings.clock_settings_supported = true;
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = true;
    app.keycode_picker.macro_count = 4;
    app.vial_unlocked = Some(unlocked);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.shared_hid_output = hid.shared_output();
    app.hid_device = Some(hid);
    (app, ctx, recorder)
}

fn frame(app: &mut EntropyApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    // The audited UI branches stay inert: no scaling/theme edits, local metadata
    // dirty flags, tour, tray, file dialogs or real device/desktop services.
    assert!(app.current_device_name.is_empty());
    assert!(!app.app_settings.sticky_layout_window);
    assert!(!app.keycode_picker.macro_metadata_dirty && !app.keycode_picker.tap_dance_dirty);
    assert!(!app.combo_names_dirty && !app.combo_colors_dirty);
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 1100.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let mut frame = eframe::Frame::_new_kittest();
            eframe::App::ui(app, ui, &mut frame);
        },
    )
}
fn rendered_texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            panic!("missing {label:?}; rendered {labels:?}")
        })
}
fn click(app: &mut EntropyApp, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        frame(
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
fn click_label(app: &mut EntropyApp, ctx: &egui::Context, label: &str) {
    let output = frame(app, ctx, vec![]);
    click(app, ctx, text_position(&output, label));
}
fn finish(app: &mut EntropyApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.vial_hid_task_active() {
        assert!(
            Instant::now() < deadline,
            "HID task did not finish: {}",
            app.status_msg
        );
        app.poll_vial_hid_task(ctx);
        frame(app, ctx, vec![]);
        std::thread::yield_now();
    }
}

#[test]
fn slot_save_keeps_pictogram_page_visible_without_library_loading_message() {
    let (mut app, ctx, _recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.display_settings.pictograms.saving = true;

    let output = frame(&mut app, &ctx, vec![]);
    let texts = rendered_texts(&output);
    assert!(
        texts.iter().any(|text| text == "Редактор 35 × 35"),
        "rendered texts: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text == "Чтение пиктограмм…"),
        "rendered texts: {texts:?}"
    );
    assert!(!matches!(app.connect_state, ConnectState::Loading { .. }));
}

#[test]
fn full_ui_repeated_assignments_use_actual_popup_without_reunlock() {
    for initially_unlocked in [false, true] {
        let (mut app, ctx, recorder) = app(initially_unlocked);
        click_label(&mut app, &ctx, "Пиктограммы");
        let generation = app.connection_generation;
        for index in 0..3 {
            if index == 2 {
                app.display_settings.pictograms.selected_slot = 1;
            }
            click_label(&mut app, &ctx, "Выбрать");
            let output = frame(&mut app, &ctx, vec![]);
            assert!(
                egui::Popup::is_any_open(&ctx),
                "Select failed on assignment {index}"
            );
            let tiles: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if (rect.rect.width() - 48.0).abs() < 0.1
                            && (rect.rect.height() - 48.0).abs() < 0.1 =>
                    {
                        Some(rect.rect.center())
                    }
                    _ => None,
                })
                .collect();
            assert!(
                tiles.len() > index + 1,
                "missing assignment tiles: {tiles:?}"
            );
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            click(&mut app, &ctx, tiles[index + 1]);
            assert!(
                app.vial_hid_task_active(),
                "actual picker did not submit upload on assignment {index}"
            );
            assert!(app.display_settings.pictograms.saving);
            assert!(!app.display_settings.pictograms.loading);
            finish(&mut app, &ctx);
            assert!(app.display_settings.pictograms.loaded);
            assert!(!app.display_settings.pictograms.busy());
            assert_eq!(app.connection_generation, generation);
            assert_eq!(app.vial_unlocked, Some(initially_unlocked));
            assert!(!app.unlock_open && !app.vial_unlock_polling);
            assert_eq!(
                app.display_settings
                    .pictograms
                    .library
                    .bitmap(PictogramKind::Macro, if index == 2 { 1 } else { 0 }),
                Some(builtin_pictogram_bitmap(index).as_slice())
            );
        }
        let requests = recorder.requests();
        assert_eq!(requests.len(), 27);
        for transaction in requests.chunks_exact(9) {
            assert_eq!(transaction[0][0], 0xC0);
            assert_eq!(transaction[1][0], 0xC7);
            assert!(transaction[2..8].iter().all(|request| request[0] == 0xC8));
            assert_eq!(transaction[8][0], 0xC9);
        }
        assert!(requests.iter().all(|request| {
            !matches!(request[0], 0xC1 | 0xC2 | 0xC3 | 0xC4 | 0xC6 | 0xCA | 0xCB)
                && !request.starts_with(&[0xFE, 6])
                && !request.starts_with(&[0xFE, 8])
        }));
    }
}
