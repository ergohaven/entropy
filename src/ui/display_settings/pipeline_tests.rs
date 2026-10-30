//! Real App::ui, picker pointer events and real task completion on an inert
//! bootstrap. No App::logic (OS scans/config polling), real HID or user files.
use super::*;
use std::time::{Duration, Instant};

#[path = "tap_dance_tests.rs"]
mod tap_dance_tests;
#[path = "delete_catalog_tests.rs"]
mod delete_catalog_tests;

fn pictogram_canvas_rect(output: &egui::FullOutput) -> egui::Rect {
    let lines: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::LineSegment { points, stroke }
                if (stroke.width - 0.7).abs() < 0.001
                    && points[0].x == points[1].x
                    && points[1].y - points[0].y >= 96.0 =>
            {
                Some(*points)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        lines.len(),
        PICTOGRAM_WIDTH + 1,
        "actual editor grid must be visible"
    );
    egui::Rect::from_min_max(lines.first().unwrap()[0], lines.last().unwrap()[1])
}

fn undo_shortcut(modifiers: egui::Modifiers) -> Vec<egui::Event> {
    [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key: egui::Key::Z,
            physical_key: Some(egui::Key::Z),
            pressed,
            repeat: false,
            modifiers,
        })
        .collect()
}

#[test]
fn full_ui_pictogram_ctrl_z_undoes_drawing_like_the_button_without_hid_writes() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.display_settings.pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
    let original = app.display_settings.pictograms.source_levels.clone();
    let output = frame(&mut app, &ctx, vec![]);
    let canvas = pictogram_canvas_rect(&output);
    let cell = canvas.width() / PICTOGRAM_WIDTH as f32;
    click(
        &mut app,
        &ctx,
        canvas.min + egui::vec2(cell * 2.5, cell * 2.5),
    );
    let first_stroke = app.display_settings.pictograms.source_levels.clone();
    assert_ne!(first_stroke, original, "real pointer event must draw");
    click(
        &mut app,
        &ctx,
        canvas.min + egui::vec2(cell * 4.5, cell * 4.5),
    );
    assert_ne!(app.display_settings.pictograms.source_levels, first_stroke);
    assert_eq!(app.display_settings.pictograms.undo.len(), 2);
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(
        app.display_settings.pictograms.source_levels, first_stroke,
        "Ctrl+Z must undo exactly one real drawing stroke"
    );
    assert_eq!(app.display_settings.pictograms.undo.len(), 1);
    let undo_label = crate::i18n::tr_catalog(app.app_settings.language, "display_settings.undo");
    click_label(&mut app, &ctx, undo_label);
    assert_eq!(app.display_settings.pictograms.source_levels, original);
    assert!(app.display_settings.pictograms.undo.is_empty());
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(app.display_settings.pictograms.source_levels, original);
    assert!(
        recorder.requests().is_empty(),
        "drawing undo is local, never a device write"
    );
}

#[test]
fn full_ui_pictogram_undo_does_not_treat_shift_alt_or_plain_z_as_undo() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.display_settings.pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
    let canvas = pictogram_canvas_rect(&frame(&mut app, &ctx, vec![]));
    click(&mut app, &ctx, canvas.center());
    let drawing = app.display_settings.pictograms.source_levels.clone();
    let count = app.display_settings.pictograms.undo.len();
    assert_eq!(count, 1);
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers {
            ctrl: true,
            command: true,
            shift: true,
            ..Default::default()
        },
        egui::Modifiers {
            ctrl: true,
            command: true,
            alt: true,
            ..Default::default()
        },
    ] {
        frame(&mut app, &ctx, undo_shortcut(modifiers));
        assert!(
            app.display_settings.pictograms.source_levels == drawing,
            "unsupported chord must not undo: {modifiers:?}"
        );
        assert_eq!(app.display_settings.pictograms.undo.len(), count);
    }
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_pictogram_undo_leaves_name_text_and_other_tabs_in_control() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.display_settings.pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
    let canvas = pictogram_canvas_rect(&frame(&mut app, &ctx, vec![]));
    click(&mut app, &ctx, canvas.center());
    let drawing = app.display_settings.pictograms.source_levels.clone();
    app.display_settings.pictograms.editor_name = "Undo name fixture".into();
    click_label(&mut app, &ctx, "Undo name fixture");
    assert!(
        ctx.egui_wants_keyboard_input(),
        "the real name field must own focus"
    );
    frame(&mut app, &ctx, vec![egui::Event::Text("edited".into())]);
    let edited = app.display_settings.pictograms.editor_name.clone();
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_ne!(
        app.display_settings.pictograms.editor_name, edited,
        "TextEdit retains text undo"
    );
    assert_eq!(app.display_settings.pictograms.source_levels, drawing);
    assert_eq!(app.display_settings.pictograms.undo.len(), 1);
    click_label(&mut app, &ctx, "Основной экран");
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(app.display_settings.pictograms.source_levels, drawing);
    assert_eq!(app.display_settings.pictograms.undo.len(), 1);
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_pictogram_cmd_z_and_pending_picker_keep_undo_local() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.display_settings.pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
    let original = app.display_settings.pictograms.source_levels.clone();
    let canvas = pictogram_canvas_rect(&frame(&mut app, &ctx, vec![]));
    click(&mut app, &ctx, canvas.center());
    let drawing = app.display_settings.pictograms.source_levels.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    app.pending_file_dialog = Some((
        crate::app::file_dialog::FileDialogAction::ExportPictograms,
        app.connection_generation,
        rx,
    ));
    let modifiers = egui::Modifiers {
        mac_cmd: true,
        command: true,
        ..Default::default()
    };
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(app.display_settings.pictograms.source_levels, drawing);
    assert_eq!(app.display_settings.pictograms.undo.len(), 1);
    tx.send(None).unwrap();
    app.poll_file_dialog(&ctx);
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(app.display_settings.pictograms.source_levels, original);
    assert!(app.display_settings.pictograms.undo.is_empty());
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_pictogram_undo_does_not_break_a_held_pointer_stroke() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.display_settings.pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
    let original = app.display_settings.pictograms.source_levels.clone();
    let canvas = pictogram_canvas_rect(&frame(&mut app, &ctx, vec![]));
    let cell = canvas.width() / PICTOGRAM_WIDTH as f32;
    click(
        &mut app,
        &ctx,
        canvas.min + egui::vec2(cell * 2.5, cell * 2.5),
    );
    let first = app.display_settings.pictograms.source_levels.clone();
    let pos = canvas.center();
    frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    assert!(app.display_settings.pictograms.editor_last_cell.is_some());
    assert_eq!(app.display_settings.pictograms.undo.len(), 2);
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(
        app.display_settings.pictograms.undo.len(),
        2,
        "a held drawing stroke must retain its undo boundary"
    );
    let next = pos + egui::vec2(cell * 2.0, cell * 2.0);
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(next)]);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: next,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(app.display_settings.pictograms.source_levels, first);
    frame(&mut app, &ctx, undo_shortcut(modifiers));
    assert_eq!(app.display_settings.pictograms.source_levels, original);
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_pictogram_undo_does_not_change_drawing_behind_app_modals() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for close_prompt in [true, false] {
        let (mut app, ctx, recorder) = app(true);
        click_label(&mut app, &ctx, "Пиктограммы");
        app.display_settings.pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
        let canvas = pictogram_canvas_rect(&frame(&mut app, &ctx, vec![]));
        click(&mut app, &ctx, canvas.center());
        let drawing = app.display_settings.pictograms.source_levels.clone();
        if close_prompt {
            app.close_to_tray_prompt_open = true;
        } else {
            app.import_report_open = true;
            app.import_report_title = "Undo modal fixture".into();
        }
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(&mut app, &ctx, undo_shortcut(modifiers));
        assert!(
            app.display_settings.pictograms.source_levels == drawing,
            "app modal must block background drawing undo: close_prompt={close_prompt}"
        );
        assert_eq!(app.display_settings.pictograms.undo.len(), 1);
        assert!(recorder.requests().is_empty());
    }
}

#[test]
fn translated_catalog_reuses_storage_per_language() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let english = pictogram_catalog(crate::i18n::Language::English);
    let russian = pictogram_catalog(crate::i18n::Language::Russian);
    let english_again = pictogram_catalog(crate::i18n::Language::English);
    assert_eq!(
        english.as_ptr(),
        english_again.as_ptr(),
        "unchanged catalog rebuilt"
    );
    assert_ne!(english[0].name, russian[0].name);
}

#[test]
fn saved_filter_does_not_retranslate_legacy_names_for_each_record() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let icon = SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: "User artwork".into(),
        color: [84, 189, 191],
        bitmap: vec![1; PICTOGRAM_BYTES],
    };
    assert!(!is_known_migrated_stock_pictogram(&icon));
    LEGACY_NAME_LOOKUPS.with(|count| count.set(0));
    for _ in 0..10 {
        assert!(!is_known_migrated_stock_pictogram(&icon));
    }
    LEGACY_NAME_LOOKUPS.with(|count| assert_eq!(count.get(), 0, "legacy translations repeated"));
}

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
    crate::app::discard_texture_deltas(ctx.run_ui(
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
    ))
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

// Historical stock export/promoted-user snapshot: current catalog keys plus
// c518c3c's packed firmware artwork and the older 32px builtins. The binary is
// copied verbatim from that delivered source base, not generated by the fix.
fn historical_promoted_stock_snapshot(language: crate::i18n::Language) -> Vec<SavedPictogram> {
    let old_firmware = include_bytes!("../../../assets/legacy-firmware-builtin-icons-c518c3c.bin");
    let firmware_start = 38 + crate::action_icons::ACTION_ICON_COUNT;
    let mut records: Vec<_> = BUILTIN_PICTOGRAM_KEYS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let bitmap = if (firmware_start
                ..firmware_start + crate::firmware_builtin_icons::FIRMWARE_ICON_COUNT)
                .contains(&index)
            {
                let offset = (index - firmware_start) * PICTOGRAM_BYTES;
                old_firmware[offset..offset + PICTOGRAM_BYTES].to_vec()
            } else {
                builtin_pictogram_bitmap(index).to_vec()
            };
            SavedPictogram {
                origin: PictogramOrigin::Legacy,
                name: crate::i18n::tr_catalog(language, key).into(),
                color: [84, 189, 191],
                bitmap,
            }
        })
        .collect();
    records.extend((0..38).map(|index| SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: crate::i18n::tr_catalog(language, BUILTIN_PICTOGRAM_KEYS[index]).into(),
        color: [84, 189, 191],
        bitmap: legacy_builtin_pictogram_bitmap(index).to_vec(),
    }));
    let pre_thinning = include_bytes!("../../../assets/legacy-stock-catalog-pre-thinning.bin");
    records.extend(
        BUILTIN_PICTOGRAM_KEYS
            .iter()
            .take(pre_thinning.len() / PICTOGRAM_BYTES)
            .enumerate()
            .map(|(index, key)| SavedPictogram {
                origin: PictogramOrigin::Legacy,
                name: crate::i18n::tr_catalog(language, key).into(),
                color: [84, 189, 191],
                bitmap: pre_thinning[index * PICTOGRAM_BYTES..(index + 1) * PICTOGRAM_BYTES]
                    .to_vec(),
            }),
    );
    // Preserve the historical JSON record shape used in pictograms.json/user.
    serde_json::from_slice(&serde_json::to_vec(&records).unwrap()).unwrap()
}

/// Opt-in local replay: never embeds or modifies the supplied private library.
#[test]
#[ignore = "requires ENTROPY_PICTOGRAM_REPLAY_FILE"]
fn full_ui_supplied_library_replay_hides_known_stock_preserves_unknown_artwork() {
    let path = std::path::PathBuf::from(std::env::var_os("ENTROPY_PICTOGRAM_REPLAY_FILE").unwrap());
    let original = std::fs::read(&path).unwrap();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.app_settings.saved_pictograms = read_pictogram_file(&path).unwrap();
    let visible = visible_saved_pictograms(
        &app.app_settings.saved_pictograms,
        pictogram_catalog(crate::i18n::Language::Russian),
    );
    let leaked: Vec<_> = visible.iter().map(|p| p.name.clone()).collect();
    app.display_settings.pictograms.search_query = "no-matching-fixture-name".into();
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
    let texts = rendered_texts(&frame(&mut app, &ctx, vec![]));
    // Exact old-stock provenance cannot establish authorship of altered pixels.
    // Supply the reviewed remaining names; default [] keeps the all-stock gate.
    let expected: Vec<String> = serde_json::from_str(
        &std::env::var("ENTROPY_PICTOGRAM_REPLAY_VISIBLE_NAMES").unwrap_or_else(|_| "[]".into()),
    )
    .unwrap();
    assert_eq!(
        leaked, expected,
        "unrecognized artwork requires provenance review"
    );
    assert_eq!(
        texts.iter().any(|text| text == "Свои пиктограммы"),
        !expected.is_empty()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_library_reader_hides_legacy_stock_independently_of_display_rgb() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for language in crate::i18n::Language::ALL {
        for color in [[84, 189, 191], [255, 255, 255], [210, 180, 140], [1, 2, 3]] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("pictograms.json");
            let mut records = historical_promoted_stock_snapshot(language);
            for icon in &mut records {
                icon.color = color;
            }
            let json = serde_json::json!({"version":2,"width":35,"height":35,
                "builtin":[],"user":records});
            let bytes = serde_json::to_vec(&json).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            let (mut app, ctx, recorder) = app(true);
            click_label(&mut app, &ctx, "Пиктограммы");
            app.app_settings.saved_pictograms = read_pictogram_file(&path).unwrap();
            let before = app.app_settings.saved_pictograms.clone();
            app.display_settings.pictograms.search_query = "no-matching-fixture-name".into();
            click_label(&mut app, &ctx, "Выбрать");
            assert!(egui::Popup::is_any_open(&ctx));
            let texts = rendered_texts(&frame(&mut app, &ctx, vec![]));
            assert!(
                !texts.iter().any(|text| text == "Свои пиктограммы"),
                "library-reader stock leaked: {language:?}, RGB={color:?}"
            );
            assert_eq!(app.app_settings.saved_pictograms, before);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert!(recorder.requests().is_empty());
        }
    }
}

#[test]
fn confirmed_agent_layer_masks_are_stock_but_authored_and_pixel_edits_survive() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let masks = include_bytes!("../../../assets/legacy-layer-navigation-agent-confirmed.bin");
    let start = 38 + crate::action_icons::ACTION_ICON_COUNT + 20;
    for language in crate::i18n::Language::ALL {
        for slot in 0..2 {
            let stock = SavedPictogram {
                name: crate::i18n::tr_catalog(language, BUILTIN_PICTOGRAM_KEYS[start + slot])
                    .into(),
                color: [84, 189, 191],
                bitmap: masks[slot * PICTOGRAM_BYTES..(slot + 1) * PICTOGRAM_BYTES].to_vec(),
                origin: PictogramOrigin::Legacy,
            };
            assert!(is_known_migrated_stock_pictogram(&stock));
            let mut drawn = stock.clone();
            drawn.bitmap[0] ^= 0x80;
            assert!(!is_known_migrated_stock_pictogram(&drawn));
            let mut authored = stock;
            authored.origin = PictogramOrigin::User;
            assert!(!is_known_migrated_stock_pictogram(&authored));
        }
    }
}

#[test]
fn full_ui_legacy_32px_library_reader_keeps_same_name_pixel_edits() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for language in crate::i18n::Language::ALL {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.json");
        let mut records = Vec::new();
        // Index 30 is a later native-35px mirror of index 29, not a migrated
        // 32px design; it cannot be losslessly inverted into this v1 fixture.
        for index in (0..38).filter(|index| *index != 30) {
            let native = legacy_builtin_pictogram_bitmap(index);
            // Recover the original 32px source from production's nearest-neighbor
            // migration: every source pixel is represented in the 35px mask.
            let mut bitmap = vec![0u8; 128];
            for y in 0..32 {
                for x in 0..32 {
                    let dx = (0..35).find(|dx| (2 * dx + 1) * 32 / 70 == x).unwrap();
                    let dy = (0..35).find(|dy| (2 * dy + 1) * 32 / 70 == y).unwrap();
                    let pixel = dy * 35 + dx;
                    if native[pixel / 8] & (0x80 >> (pixel % 8)) != 0 {
                        let pixel = y * 32 + x;
                        bitmap[pixel / 8] |= 0x80 >> (pixel % 8);
                    }
                }
            }
            assert_eq!(normalize_pictogram_bitmap(&bitmap).unwrap(), native);
            records.push(serde_json::json!({"name":crate::i18n::tr_catalog(language, BUILTIN_PICTOGRAM_KEYS[index]),
                "color":[210,180,140], "bitmap":bitmap}));
        }
        let json =
            serde_json::json!({"version":1,"width":32,"height":32,"builtin":[],"user":records});
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let (mut app, ctx, recorder) = app(true);
        click_label(&mut app, &ctx, "Пиктограммы");
        app.app_settings.saved_pictograms = read_pictogram_file(&path).unwrap();
        app.display_settings.pictograms.search_query = "no-matching-fixture-name".into();
        click_label(&mut app, &ctx, "Выбрать");
        assert!(!rendered_texts(&frame(&mut app, &ctx, vec![]))
            .iter()
            .any(|s| s == "Свои пиктограммы"));
        let mut edited = json;
        let byte = edited["user"][0]["bitmap"][0].as_u64().unwrap();
        edited["user"][0]["bitmap"][0] = (byte ^ 0x80).into();
        std::fs::write(&path, serde_json::to_vec(&edited).unwrap()).unwrap();
        app.app_settings.saved_pictograms = read_pictogram_file(&path).unwrap();
        let visible = visible_saved_pictograms(
            &app.app_settings.saved_pictograms,
            pictogram_catalog(crate::i18n::Language::Russian),
        );
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].origin, PictogramOrigin::Legacy);
        assert_eq!(visible[0].color, [210, 180, 140]);
        assert!(rendered_texts(&frame(&mut app, &ctx, vec![]))
            .iter()
            .any(|s| s == "Свои пиктограммы"));
        assert!(recorder.requests().is_empty());
    }
}

#[test]
fn actual_import_preserves_explicit_user_authorship_over_a_hidden_stock_duplicate() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored.json");
    let mut records = historical_promoted_stock_snapshot(crate::i18n::Language::English);
    records.truncate(1);
    let (mut app, ctx, recorder) = app(true);
    let mut hidden = records[0].clone();
    hidden.origin = PictogramOrigin::Stock;
    app.app_settings.saved_pictograms.push(hidden);
    records[0].origin = PictogramOrigin::User;
    write_pictogram_file(&path, &records).unwrap();
    crate::app::file_dialog::set_test_dialog_result(Some(path));
    app.spawn_file_dialog(
        crate::app::file_dialog::FileDialogAction::ImportPictograms,
        rfd::FileDialog::new(),
        false,
    );
    app.poll_file_dialog(&ctx);
    assert_eq!(app.app_settings.saved_pictograms.len(), 1);
    assert_eq!(
        app.app_settings.saved_pictograms[0].origin,
        PictogramOrigin::User
    );
    assert_eq!(
        read_pictogram_file(&pictogram_library_path()).unwrap(),
        app.app_settings.saved_pictograms
    );
    assert!(recorder.requests().is_empty());
}

#[test]
fn library_reader_marks_recognized_legacy_stock_before_import_can_rename_it() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("icons.json");
    let mut records = historical_promoted_stock_snapshot(crate::i18n::Language::English);
    records.truncate(1);
    let json = serde_json::json!({"version":2,"width":35,"height":35,"builtin":[],"user":records});
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let mut loaded = read_pictogram_file(&path).unwrap();
    assert_eq!(loaded[0].origin, PictogramOrigin::Stock);
    loaded[0].name.push_str(" 2");
    assert!(
        visible_saved_pictograms(&loaded, pictogram_catalog(crate::i18n::Language::English))
            .is_empty()
    );
}

#[test]
fn full_ui_library_reader_preserves_explicit_user_origin_even_for_exact_stock_pixels() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pictograms.json");
    let mut records = historical_promoted_stock_snapshot(crate::i18n::Language::English);
    records.truncate(1);
    let mut json = serde_json::json!({"version":2,"width":35,"height":35,
        "builtin":[],"user":records});
    json["user"][0]["origin"] = "user".into();
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.app_settings.saved_pictograms = read_pictogram_file(&path).unwrap();
    app.display_settings.pictograms.search_query = "Play".into();
    click_label(&mut app, &ctx, "Выбрать");
    let output = frame(&mut app, &ctx, vec![]);
    assert!(
        rendered_texts(&output)
            .iter()
            .any(|text| text == "Свои пиктограммы"),
        "explicit user artwork hidden by stock recognition"
    );
    write_pictogram_file(&path, &app.app_settings.saved_pictograms).unwrap();
    let exported: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(exported["user"][0]["origin"], "user");
    assert_eq!(
        read_pictogram_file(&path).unwrap(),
        app.app_settings.saved_pictograms
    );
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_historical_promoted_stock_does_not_create_my_pictograms_group() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for language in crate::i18n::Language::ALL {
        let (mut app, ctx, recorder) = app(true);
        click_label(&mut app, &ctx, "Пиктограммы");
        app.app_settings.saved_pictograms = historical_promoted_stock_snapshot(language);
        let before = app.app_settings.saved_pictograms.clone();
        // Filter stock display out of the viewport; a leaked Saved group still
        // emits its header, so this assertion exercises the actual chooser UI.
        app.display_settings.pictograms.search_query = "no-matching-fixture-name".into();
        click_label(&mut app, &ctx, "Выбрать");
        assert!(egui::Popup::is_any_open(&ctx));
        let texts = rendered_texts(&frame(&mut app, &ctx, vec![]));
        assert!(
            !texts.iter().any(|text| text
                == crate::i18n::tr_catalog(
                    crate::i18n::Language::Russian,
                    "display_settings.pictogram_group_saved"
                )),
            "historical {language:?} stock snapshots leaked into My pictograms"
        );
        assert_eq!(
            app.app_settings.saved_pictograms, before,
            "filter must not mutate artwork"
        );
        assert!(recorder.requests().is_empty());
    }
}

#[test]
fn full_ui_saved_picker_deduplicates_exact_records_but_preserves_edited_variants() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    let stock_index = 38;
    let original = SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: crate::i18n::tr_catalog(
            crate::i18n::Language::English,
            BUILTIN_PICTOGRAM_KEYS[stock_index],
        )
        .into(),
        color: [84, 189, 191],
        bitmap: builtin_pictogram_bitmap(stock_index).to_vec(),
    };
    let mut edited_bitmap = original.clone();
    edited_bitmap.bitmap[0] ^= 1;
    let mut edited_color = original.clone();
    edited_color.color = [255, 0, 0];
    edited_color.origin = PictogramOrigin::User;
    let mut renamed = original.clone();
    renamed.name = "Personal artwork".into();
    app.app_settings.saved_pictograms = vec![
        original,
        edited_bitmap.clone(),
        edited_bitmap,
        edited_color,
        renamed,
    ];
    let before = app.app_settings.saved_pictograms.clone();
    // Filter out catalog tiles while retaining same-name custom edits. Exact
    // records alone are deduplicated; name/bitmap matches separately are unsafe.
    app.display_settings.pictograms.search_query = "link".into();
    click_label(&mut app, &ctx, "Выбрать");
    let output = frame(&mut app, &ctx, vec![]);
    let tiles = output
        .shapes
        .iter()
        .filter(|shape| {
            matches!(&shape.shape,
        egui::Shape::Rect(rect) if (rect.rect.width() - 48.0).abs() < 0.1
            && (rect.rect.height() - 48.0).abs() < 0.1)
        })
        .count();
    assert_eq!(
        tiles, 3,
        "only None and two same-name custom variants should render"
    );
    let visible = visible_saved_pictograms(
        &app.app_settings.saved_pictograms,
        pictogram_catalog(crate::i18n::Language::Russian),
    );
    assert_eq!(
        visible.len(),
        3,
        "stock hidden, exact duplicate hidden, all custom variants retained"
    );
    assert!(egui::Popup::is_any_open(&ctx));
    let texts = rendered_texts(&output);
    assert!(texts.iter().any(|text| text
        == crate::i18n::tr_catalog(
            crate::i18n::Language::Russian,
            "display_settings.pictogram_group_saved"
        )));
    assert_eq!(app.app_settings.saved_pictograms, before);
    assert!(recorder.requests().is_empty());
}

#[test]
fn full_ui_closed_picker_skips_saved_filter_and_repeated_name_translation() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    app.app_settings.saved_pictograms = (0..512)
        .map(|i| SavedPictogram {
            origin: PictogramOrigin::Legacy,
            name: format!("User icon {i}"),
            color: [84, 189, 191],
            bitmap: vec![1; PICTOGRAM_BYTES],
        })
        .collect();
    frame(&mut app, &ctx, vec![]);
    NAME_LIST_LOOKUPS.with(|count| count.set(0));
    SAVED_FILTER_RUNS.with(|count| count.set(0));
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    NAME_LIST_LOOKUPS.with(|count| assert_eq!(count.get(), 0, "name list rebuilt each frame"));
    SAVED_FILTER_RUNS
        .with(|count| assert_eq!(count.get(), 0, "closed picker filtered saved artwork"));
    assert!(recorder.requests().is_empty());
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
    SAVED_FILTER_RUNS.with(|count| assert!(count.get() > 0));
}

#[test]
fn cached_stock_records_keep_exact_legacy_matching_for_every_language_and_bitmap() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for language in crate::i18n::Language::ALL {
        for index in 0..BUILTIN_PICTOGRAM_KEYS.len() {
            let mut variants = vec![builtin_pictogram_bitmap(index)];
            if index < 38 {
                variants.push(legacy_builtin_pictogram_bitmap(index));
            }
            for bitmap in variants {
                let original = SavedPictogram {
                    origin: PictogramOrigin::Legacy,
                    name: format!(
                        "  {}  ",
                        crate::i18n::tr_catalog(language, BUILTIN_PICTOGRAM_KEYS[index])
                    ),
                    color: [84, 189, 191],
                    bitmap: bitmap.to_vec(),
                };
                assert!(
                    is_known_migrated_stock_pictogram(&original),
                    "missing {language:?}/{index}"
                );
                let mut custom = original.clone();
                custom.name = format!("Personal artwork {index}");
                assert!(!is_known_migrated_stock_pictogram(&custom));
                custom = original.clone();
                custom.color = [85, 189, 191];
                assert!(is_known_migrated_stock_pictogram(&custom));
                custom.origin = PictogramOrigin::User;
                assert!(!is_known_migrated_stock_pictogram(&custom));
                custom = original.clone();
                custom.bitmap = vec![0xA5; PICTOGRAM_BYTES];
                assert!(!is_known_migrated_stock_pictogram(&custom));
            }
        }
    }
}

#[test]
fn saved_filter_tracks_in_place_name_color_bitmap_and_collection_edits() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let index = 0;
    let original = SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: crate::i18n::tr_catalog(
            crate::i18n::Language::English,
            BUILTIN_PICTOGRAM_KEYS[index],
        )
        .into(),
        color: [84, 189, 191],
        bitmap: legacy_builtin_pictogram_bitmap(index).to_vec(),
    };
    let mut saved = vec![original.clone()];
    for lang in crate::i18n::Language::ALL {
        let catalog = pictogram_catalog(lang);
        assert!(visible_saved_pictograms(&saved, catalog).is_empty());
        saved[0].name = "Personal artwork".into();
        assert_eq!(visible_saved_pictograms(&saved, catalog).len(), 1);
        saved[0] = original.clone();
        saved[0].color = [255, 0, 0];
        assert!(visible_saved_pictograms(&saved, catalog).is_empty());
        saved[0].origin = PictogramOrigin::User;
        assert_eq!(visible_saved_pictograms(&saved, catalog).len(), 1);
        saved[0] = original.clone();
        saved[0].bitmap[0] ^= 1;
        assert_eq!(
            visible_saved_pictograms(&saved, catalog)[0].bitmap,
            saved[0].bitmap
        );
        saved.push(SavedPictogram {
            origin: PictogramOrigin::Legacy,
            name: "Added".into(),
            color: [84, 189, 191],
            bitmap: vec![1; PICTOGRAM_BYTES],
        });
        assert_eq!(visible_saved_pictograms(&saved, catalog).len(), 2);
        saved.remove(0);
        assert_eq!(visible_saved_pictograms(&saved, catalog)[0].name, "Added");
        saved.clear();
        assert!(visible_saved_pictograms(&saved, catalog).is_empty());
        saved.push(original.clone());
    }
}

#[test]
fn full_ui_open_picker_refreshes_language_and_custom_artwork_without_stale_filter() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = app(true);
    click_label(&mut app, &ctx, "Пиктограммы");
    let original = SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: crate::i18n::tr_catalog(crate::i18n::Language::English, BUILTIN_PICTOGRAM_KEYS[0])
            .into(),
        color: [84, 189, 191],
        bitmap: legacy_builtin_pictogram_bitmap(0).to_vec(),
    };
    app.app_settings.saved_pictograms.push(original);
    click_label(&mut app, &ctx, "Выбрать");
    let output = frame(&mut app, &ctx, vec![]);
    assert!(!rendered_texts(&output).iter().any(|s| s
        == crate::i18n::tr_catalog(
            crate::i18n::Language::Russian,
            "display_settings.pictogram_group_saved"
        )));
    let custom_bitmap = vec![0xA5; PICTOGRAM_BYTES];
    app.app_settings.saved_pictograms[0].name = "Personal artwork".into();
    app.app_settings.saved_pictograms[0].bitmap = custom_bitmap.clone();
    app.app_settings.saved_pictograms[0].color = [255, 0, 0];
    app.app_settings.language = crate::i18n::Language::English;
    app.display_settings.pictograms.search_query = "Personal artwork".into();
    let output = frame(&mut app, &ctx, vec![]);
    let texts = rendered_texts(&output);
    assert!(texts.iter().any(|s| s
        == crate::i18n::tr_catalog(
            crate::i18n::Language::English,
            "display_settings.pictogram_assign_title"
        )));
    assert!(texts.iter().any(|s| s
        == crate::i18n::tr_catalog(
            crate::i18n::Language::English,
            "display_settings.pictogram_group_saved"
        )));
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
    assert_eq!(
        tiles.len(),
        2,
        "only None and searched custom artwork should remain"
    );
    recorder.respond_with(test_pictogram_upload_responses(true, 0));
    click(&mut app, &ctx, tiles[1]);
    assert!(app.vial_hid_task_active());
    finish(&mut app, &ctx);
    assert_eq!(
        app.display_settings
            .pictograms
            .library
            .bitmap(PictogramKind::Macro, 0),
        Some(custom_bitmap.as_slice())
    );
    assert_eq!(app.app_settings.saved_pictograms[0].bitmap, custom_bitmap);
    assert_eq!(app.app_settings.saved_pictograms[0].color, [255, 0, 0]);
}

#[test]
fn slot_save_keeps_pictogram_page_visible_without_library_loading_message() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
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
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let catalog = pictogram_catalog(crate::i18n::Language::Russian);
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
                Some(catalog[index].bitmap.as_slice())
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
