use super::sm62_shared_write_tests::{click_shared, drain, frame, frame_events};
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn setup() -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
    let ctx = egui::Context::default();
    let mut fonts = egui::FontDefinitions::default();
    fonts.families.insert(
        egui::FontFamily::Name("display_preview".into()),
        fonts.families[&egui::FontFamily::Proportional].clone(),
    );
    ctx.set_fonts(fonts);
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = EntropyApp::new(&cc);
    app.app_settings.language = crate::i18n::Language::English;
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    // Initialize actual device storage, then read it through the production loader.
    for (qsid, value) in (230..=249).zip([
        0, 0, 20, 0, 1, 0, 0, 20, 0, 1, 1, 1, 200, 200, 1, 1, 1, 1, 0, 0,
    ]) {
        if (242..=243).contains(&qsid) {
            hid.set_qmk_setting_u16(qsid, value).unwrap();
        } else {
            hid.set_qmk_setting_u8(qsid, value as u8).unwrap();
        }
    }
    hid.set_qmk_setting_u8(DISPLAY_TIMEOUT_QSID, 0).unwrap();
    let qsids: Vec<_> = (230..=249).chain([DISPLAY_TIMEOUT_QSID]).collect();
    app.display_settings = crate::app::app_state::load_display_settings(&hid, &qsids).unwrap();
    app.hid_device = Some(hid);
    (app, ctx, recorder)
}

fn fail_get(recorder: &crate::hid::TestHidRecorder) -> Arc<AtomicBool> {
    let fail = Arc::new(AtomicBool::new(true));
    let device_fail = fail.clone();
    recorder.respond_by(move |request| {
        if request[1] == 0x0A && device_fail.load(Ordering::SeqCst) {
            let mut response = [0; 32];
            response[0] = 1;
            Some(Ok(response))
        } else {
            None
        }
    });
    fail
}

fn confirmed_toggle(app: &EntropyApp, qsid: u16) -> u8 {
    let s = &app.display_settings;
    let side = (qsid % 2) as usize;
    match qsid {
        240..=241 => s.confirmed_sm62_date_values[side],
        244..=245 => s.confirmed_sm62_language_values[side],
        246..=247 => s.confirmed_sm62_stars_values[side],
        248..=249 => s.confirmed_sm62_screens_off_values[side],
        _ => unreachable!(),
    }
}

fn toggle_reversal(label: &str, qsids: [u16; 2], original: u8) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = setup();
    let fail = fail_get(&recorder);
    let start = recorder.requests().len();
    click_shared(&mut app, &ctx, label);
    drain(&mut app, &ctx);
    for qsid in qsids {
        assert_eq!(
            confirmed_toggle(&app, qsid),
            original,
            "failed GET retains last confirmed value"
        );
        assert!(matches!(
            app.settings_write_status(qsid),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Failed(_))
        ));
        let requests = recorder.requests();
        assert!(requests[start..].iter().any(|r| r[1] == 0x0B
            && u16::from_le_bytes([r[2], r[3]]) == qsid
            && r[4] == 1 - original));
        assert_eq!(
            requests[start..]
                .iter()
                .filter(|r| r[1] == 0x0A && u16::from_le_bytes([r[2], r[3]]) == qsid)
                .count(),
            3
        );
    }
    assert!(
        app.hid_device.is_some(),
        "invalid replies do not disconnect"
    );
    let before = app.display_settings.clone();
    let status = app.status_msg.clone();
    let count = recorder.requests().len();
    for _ in 0..3 {
        frame(&mut app, &ctx);
        app.poll_settings_write(&ctx);
    }
    assert_eq!(recorder.requests().len(), count, "no retry on repaint");
    assert_eq!(app.display_settings, before);
    assert_eq!(app.status_msg, status);
    fail.store(false, Ordering::SeqCst);
    for qsid in qsids {
        assert_eq!(
            app.hid_device
                .as_ref()
                .unwrap()
                .get_qmk_setting_u8(qsid)
                .unwrap(),
            1 - original,
            "SET really changed device storage"
        );
    }
    let correction_start = recorder.requests().len();
    click_shared(&mut app, &ctx, label);
    drain(&mut app, &ctx);
    for qsid in qsids {
        assert_eq!(
            app.hid_device
                .as_ref()
                .unwrap()
                .get_qmk_setting_u8(qsid)
                .unwrap(),
            original,
            "explicit UI reversal must correct uncertain device qsid={qsid}"
        );
        assert!(recorder.requests()[correction_start..]
            .iter()
            .any(|r| r[1] == 0x0B && u16::from_le_bytes([r[2], r[3]]) == qsid && r[4] == original));
        assert!(matches!(
            app.settings_write_status(qsid),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
        ));
    }
}

#[test]
fn review_screens_off_ui_reversal_corrects_unverified_set() {
    toggle_reversal("Turn off displays", [248, 249], 0);
}
#[test]
fn review_language_ui_reversal_corrects_unverified_set() {
    toggle_reversal("Show current layout RU/EN", [244, 245], 1);
}
#[test]
fn review_date_ui_reversal_corrects_unverified_set() {
    toggle_reversal("Date on clock", [240, 241], 1);
}
#[test]
fn review_stars_ui_reversal_corrects_unverified_set() {
    toggle_reversal("Star animation", [246, 247], 1);
}

fn click_at(app: &mut EntropyApp, ctx: &egui::Context, pos: egui::Pos2) {
    frame_events(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
    );
    frame(app, ctx);
}

fn text_position(app: &mut EntropyApp, ctx: &egui::Context, text: &str) -> egui::Pos2 {
    let output = frame_events(app, ctx, vec![]);
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == text => {
                let pos = t.pos + t.galley.size() / 2.0;
                s.clip_rect.contains(pos).then_some(pos)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "visible control text missing: {text}; texts={:?}",
                output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t.galley.job.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            )
        })
}

fn click_text(app: &mut EntropyApp, ctx: &egui::Context, text: &str) {
    let pos = text_position(app, ctx, text);
    click_at(app, ctx, pos);
    frame(app, ctx);
}

fn edit_hold(app: &mut EntropyApp, ctx: &egui::Context, old: u16, new: u16) {
    let pos = text_position(app, ctx, &old.to_string());
    click_at(app, ctx, pos);
    click_at(app, ctx, pos);
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame_events(
        app,
        ctx,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            egui::Event::Text(new.to_string()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            },
        ],
    );
    frame(app, ctx);
    assert_eq!(
        app.display_settings.sm62_hold_ms, [new; 2],
        "real numeric editor input"
    );
}

#[test]
fn review_hold_ui_reversal_corrects_unverified_set() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = setup();
    frame(&mut app, &ctx);
    let fail = fail_get(&recorder);
    edit_hold(&mut app, &ctx, 200, 300);
    drain(&mut app, &ctx);
    assert_eq!(app.display_settings.confirmed_sm62_hold_ms, [200; 2]);
    for qsid in [242, 243] {
        assert!(matches!(
            app.settings_write_status(qsid),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Failed(_))
        ));
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|r| r[1] == 0x0A && u16::from_le_bytes([r[2], r[3]]) == qsid)
                .count(),
            4,
            "initial load plus all three failed GET attempts"
        );
    }
    let count = recorder.requests().len();
    let status = app.status_msg.clone();
    for _ in 0..3 {
        frame(&mut app, &ctx);
        app.poll_settings_write(&ctx);
    }
    assert_eq!(recorder.requests().len(), count);
    assert_eq!(app.status_msg, status);
    fail.store(false, Ordering::SeqCst);
    for qsid in [242, 243] {
        assert_eq!(
            app.hid_device
                .as_ref()
                .unwrap()
                .get_qmk_setting_u16(qsid)
                .unwrap(),
            300
        );
    }
    edit_hold(&mut app, &ctx, 300, 200);
    drain(&mut app, &ctx);
    for qsid in [242, 243] {
        assert_eq!(
            app.hid_device
                .as_ref()
                .unwrap()
                .get_qmk_setting_u16(qsid)
                .unwrap(),
            200,
            "UI correction after unverified hold SET"
        );
        assert!(matches!(
            app.settings_write_status(qsid),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
        ));
    }
}

#[test]
fn review_timeout_ui_reversal_corrects_unverified_set() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = setup();
    frame(&mut app, &ctx);
    frame(&mut app, &ctx);
    let fail = fail_get(&recorder);
    let never = crate::i18n::tr_catalog(crate::i18n::Language::English, "display_settings.never");
    let minute =
        crate::i18n::tr_catalog(crate::i18n::Language::English, "display_settings.minute_1");
    click_text(&mut app, &ctx, never);
    click_text(&mut app, &ctx, minute);
    assert_eq!(app.display_settings.display_timeout, 1);
    drain(&mut app, &ctx);
    assert_eq!(app.display_settings.confirmed_display_timeout, 0);
    assert!(matches!(
        app.settings_write_status(DISPLAY_TIMEOUT_QSID),
        Some(super::super::settings_write_queue::SettingsWriteStatus::Failed(_))
    ));
    let count = recorder.requests().len();
    let status = app.status_msg.clone();
    for _ in 0..3 {
        frame(&mut app, &ctx);
        app.poll_settings_write(&ctx);
    }
    assert_eq!(recorder.requests().len(), count);
    assert_eq!(app.status_msg, status);
    fail.store(false, Ordering::SeqCst);
    assert_eq!(
        app.hid_device
            .as_ref()
            .unwrap()
            .get_qmk_setting_u8(DISPLAY_TIMEOUT_QSID)
            .unwrap(),
        1
    );
    click_text(&mut app, &ctx, minute);
    click_text(&mut app, &ctx, never);
    assert_eq!(app.display_settings.display_timeout, 0);
    drain(&mut app, &ctx);
    assert_eq!(
        app.hid_device
            .as_ref()
            .unwrap()
            .get_qmk_setting_u8(DISPLAY_TIMEOUT_QSID)
            .unwrap(),
        0,
        "UI correction after unverified timeout SET"
    );
    assert!(matches!(
        app.settings_write_status(DISPLAY_TIMEOUT_QSID),
        Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
    ));
}

fn layout_no_input(connected: bool, enabled: bool) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = setup();
    let hid = app.hid_device.as_ref().unwrap();
    hid.set_qmk_setting_u8(230, 3).unwrap();
    hid.set_qmk_setting_u8(233, 0).unwrap();
    app.display_settings.sm62_values[0] = 3;
    app.display_settings.sm62_values[3] = 0;
    app.display_settings.confirmed_sm62_values = app.display_settings.sm62_values;
    if !connected {
        app.hid_device = None;
    }
    let before = app.display_settings.clone();
    let start = recorder.requests().len();
    for _ in 0..3 {
        ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 2200.0),
                )),
                ..Default::default()
            },
            |ctx| {
                ctx.root_ui(|ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            app.draw_sm62_display_settings_page(ui, ui.max_rect())
                        });
                    })
                });
            },
        )
        .drop_without_applying_deltas();
    }
    drain(&mut app, &ctx);
    assert_eq!(
        recorder.requests().len(),
        start,
        "input-free Layout/keypress=0 must send zero requests"
    );
    assert_eq!(
        app.display_settings, before,
        "input-free rendering retains stored/desired values"
    );
    assert_eq!(app.settings_write_status(233), None);
}

#[test]
fn review_disabled_layout_page_is_input_free() {
    layout_no_input(true, false);
}
#[test]
fn review_disconnected_layout_page_is_input_free() {
    layout_no_input(false, true);
    layout_no_input(false, false);
}
#[test]
fn review_enabled_layout_page_is_input_free() {
    layout_no_input(true, true);
}

#[test]
fn review_explicit_layout_mode_selection_couples_keypress_setting() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, ctx, recorder) = setup();
    frame(&mut app, &ctx);
    frame(&mut app, &ctx);
    let start = recorder.requests().len();
    click_text(&mut app, &ctx, "Clock");
    click_text(&mut app, &ctx, "Layout");
    drain(&mut app, &ctx);
    assert_eq!(app.display_settings.sm62_values[0], 3);
    assert_eq!(app.display_settings.sm62_values[3], 1);
    for (qsid, value) in [(230, 3), (233, 1)] {
        assert_eq!(
            app.hid_device
                .as_ref()
                .unwrap()
                .get_qmk_setting_u8(qsid)
                .unwrap(),
            value
        );
        assert!(recorder.requests()[start..]
            .iter()
            .any(|r| r[1] == 0x0B && u16::from_le_bytes([r[2], r[3]]) == qsid && r[4] == value));
    }
}
