use super::*;
use std::time::{Duration, Instant};

pub(super) fn frame(app: &mut EntropyApp, ctx: &egui::Context) {
    frame_events(app, ctx, vec![]);
}

pub(super) fn frame_events(
    app: &mut EntropyApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    crate::app::discard_texture_deltas(ctx.run_pass(
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 2200.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ctx.root_ui(|ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    app.draw_sm62_display_settings_page(ui, ui.max_rect());
                })
            });
        },
    ))
}

pub(super) fn drain(app: &mut EntropyApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.qmk_settings_write_busy() && Instant::now() < deadline {
        app.poll_settings_write(ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !app.qmk_settings_write_busy(),
        "bounded worker must terminate"
    );
}

fn failed_shared_write_does_not_restart_on_repaint(failed_opcode: u8) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for qsid in 240..=249u16 {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.families.insert(
            egui::FontFamily::Name("display_preview".into()),
            fonts.families[&egui::FontFamily::Proportional].clone(),
        );
        ctx.set_fonts(fonts);
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        // Stateful device storage and a wire-level rejected reply. Production
        // HID decoding, retry bounds, worker and completion build the failure.
        let fail = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let device_fail = fail.clone();
        recorder.respond_by(move |request| {
            if request[1] == failed_opcode && device_fail.load(std::sync::atomic::Ordering::SeqCst)
            {
                let mut response = [0; 32];
                response[0] = 1;
                Some(Ok(response))
            } else {
                None
            }
        });
        app.hid_device = Some(hid);
        let s = &mut app.display_settings;
        s.sm62_supported = true;
        s.sm62_date_supported = true;
        s.sm62_hold_supported = true;
        s.sm62_language_supported = true;
        s.sm62_stars_supported = true;
        s.sm62_screens_off_supported = true;
        let side = (qsid % 2) as usize;
        let old = match qsid {
            240..=241 => {
                let old = s.confirmed_sm62_date_values[side];
                s.sm62_date_values[side] = 1 - old;
                old as u16
            }
            242..=243 => {
                let old = s.confirmed_sm62_hold_ms[side];
                s.sm62_hold_ms[side] = old + 1;
                old
            }
            244..=245 => {
                let old = s.confirmed_sm62_language_values[side];
                s.sm62_language_values[side] = 1 - old;
                old as u16
            }
            246..=247 => {
                let old = s.confirmed_sm62_stars_values[side];
                s.sm62_stars_values[side] = 1 - old;
                old as u16
            }
            _ => {
                let old = s.confirmed_sm62_screens_off_values[side];
                s.sm62_screens_off_values[side] = 1 - old;
                old as u16
            }
        };
        let requested = if (242..=243).contains(&qsid) {
            old + 1
        } else {
            1 - old
        };
        let desired = app.display_settings.clone();
        app.queue_display_setting_write("shared regression".into(), qsid, old, requested);
        drain(&mut app, &ctx);
        let status = app.settings_write_status(qsid).cloned();
        assert!(matches!(
            status,
            Some(super::super::settings_write_queue::SettingsWriteStatus::Failed(_))
        ));
        let requests = recorder.requests();
        assert_eq!(
            requests.iter().filter(|r| r[1] == failed_opcode).count(),
            3,
            "exhaust actual I/O retries qsid={qsid}"
        );
        let count = requests.len();
        let message = app.status_msg.clone();
        for _ in 0..3 {
            frame(&mut app, &ctx);
            assert!(
                !app.qmk_settings_write_busy(),
                "ordinary repaint restarted terminal failure qsid={qsid}"
            );
            app.poll_settings_write(&ctx);
            assert_eq!(app.settings_write_status(qsid), status.as_ref());
            assert_eq!(app.status_msg, message);
        }
        assert_eq!(
            recorder.requests().len(),
            count,
            "no extra HID requests after failure"
        );
        assert_eq!(
            app.display_settings, desired,
            "retain desired/confirmed separation and independent fields"
        );
        // Explicit retry remains possible, and only its confirmed field changes.
        fail.store(false, std::sync::atomic::Ordering::SeqCst);
        app.queue_display_setting_write("explicit retry".into(), qsid, old, requested);
        drain(&mut app, &ctx);
        assert!(matches!(
            app.settings_write_status(qsid),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
        ));
        let hid = app.hid_device.as_ref().unwrap();
        let actual = if (242..=243).contains(&qsid) {
            hid.get_qmk_setting_u16(qsid).unwrap()
        } else {
            hid.get_qmk_setting_u8(qsid).unwrap() as u16
        };
        assert_eq!(actual, requested);
        let after_retry = recorder.requests().len();
        frame(&mut app, &ctx);
        assert_eq!(recorder.requests().len(), after_retry);
    }
}

#[test]
fn shared_sm62_exhausted_set_failure_is_stable_until_explicit_retry() {
    failed_shared_write_does_not_restart_on_repaint(0x0B);
}

#[test]
fn shared_sm62_exhausted_get_failure_is_stable_until_explicit_retry() {
    failed_shared_write_does_not_restart_on_repaint(0x0A);
}

pub(super) fn click_shared(app: &mut EntropyApp, ctx: &egui::Context, label: &str) {
    frame(app, ctx);
    let output = frame_events(app, ctx, vec![]);
    let y = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                Some(t.pos.y + t.galley.size().y / 2.0)
            }
            _ => None,
        })
        .expect("shared control label");
    let pos = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.rect.width() == 46.0
                    && r.rect.height() == 24.0
                    && (r.rect.center().y - y).abs() < 16.0 =>
            {
                assert!(s.clip_rect.contains(r.rect.center()));
                Some(r.rect.center())
            }
            _ => None,
        })
        .expect("shared switch on labelled row");
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
}

#[test]
fn shared_sm62_real_edits_coalesce_across_fields_without_repaint_writes() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
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
    app.hid_device = Some(hid);
    app.display_settings.sm62_supported = true;
    app.display_settings.sm62_stars_supported = true;
    app.display_settings.sm62_screens_off_supported = true;
    let before = app.display_settings.clone();
    click_shared(&mut app, &ctx, "Star animation");
    assert_eq!(app.display_settings.sm62_stars_values, [0, 0]);
    click_shared(&mut app, &ctx, "Turn off displays");
    assert_eq!(app.display_settings.sm62_screens_off_values, [1, 1]);
    click_shared(&mut app, &ctx, "Star animation");
    assert_eq!(app.display_settings.sm62_stars_values, [1, 1]);
    drain(&mut app, &ctx);
    assert_eq!(app.display_settings.confirmed_sm62_stars_values, [1, 1]);
    assert_eq!(
        app.display_settings.confirmed_sm62_screens_off_values,
        [1, 1]
    );
    let mut after = app.display_settings.clone();
    after.sm62_screens_off_values = before.sm62_screens_off_values;
    after.confirmed_sm62_screens_off_values = before.confirmed_sm62_screens_off_values;
    assert_eq!(
        after, before,
        "independent modes/date/language/hold remain unchanged"
    );
    let count = recorder.requests().len();
    for _ in 0..3 {
        frame(&mut app, &ctx);
        app.poll_settings_write(&ctx);
    }
    assert_eq!(recorder.requests().len(), count);
    assert!(recorder
        .requests()
        .iter()
        .all(|r| (246..=249).contains(&u16::from_le_bytes([r[2], r[3]]))));
}

#[test]
fn shared_sm62_disabled_page_does_not_enqueue_unconfirmed_values() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for read_only in [false, true] {
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
        if read_only {
            app.hid_device = Some(hid);
        }
        app.display_settings.sm62_supported = true;
        app.display_settings.sm62_stars_supported = true;
        app.display_settings.sm62_screens_off_supported = true;
        app.display_settings.sm62_stars_values = [0, 0];
        app.display_settings.sm62_screens_off_values = [1, 1];
        let before = app.display_settings.clone();
        if read_only {
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
                                ui.add_enabled_ui(false, |ui| {
                                    app.draw_sm62_display_settings_page(ui, ui.max_rect())
                                });
                            })
                        });
                    },
                )
                .drop_without_applying_deltas();
            }
        } else {
            click_shared(&mut app, &ctx, "Star animation");
            frame(&mut app, &ctx);
        }
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(app.display_settings, before);
        assert!(recorder.requests().is_empty());
    }
}
