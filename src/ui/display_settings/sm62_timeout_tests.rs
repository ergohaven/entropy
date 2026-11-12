use super::*;
use std::time::{Duration, Instant};

fn field_rect(output: &egui::FullOutput, value: &str) -> egui::Rect {
    let center = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == value => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "missing field {value}; text={:?}",
                output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(&t.galley.job.text),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            )
        });
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.rect.contains(center)
                    && rect.rect.width() > 150.0
                    && rect.rect.width() < 240.0
                    && rect.rect.height() > 24.0
                    && rect.rect.height() < 40.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("actual field background")
}

#[test]
fn sm62_timeout_right_edge_matches_shared_hold_field() {
    let ctx = egui::Context::default();
    let mut app = EntropyApp::new_inert_for_test();
    app.app_settings.language = crate::i18n::Language::English;
    app.display_settings.display_timeout_supported = true;
    app.display_settings.display_timeout = 4;
    let render = |app: &mut EntropyApp| {
        crate::app::discard_texture_deltas(ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 1000.0),
                )),
                ..Default::default()
            },
            |ctx| {
                ctx.root_ui(|ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.set_width(900.0);
                        let mut values = [0; 10];
                        let mut changed = Vec::new();
                        let mut hold = [350; 2];
                        sm62_shared_controls(
                            ui,
                            &mut values,
                            &mut changed,
                            false,
                            None,
                            Some(&mut hold),
                            None,
                            None,
                            None,
                        );
                        app.draw_sm62_timeout_control(ui);
                    })
                });
            },
        ))
    };
    render(&mut app);
    let output = render(&mut app);
    let hold = field_rect(&output, "350 ms");
    let timeout = field_rect(&output, "10 minutes");
    assert!(
        (hold.right() - timeout.right()).abs() < 0.5,
        "timeout right edge {} differs from hold {}",
        timeout.right(),
        hold.right()
    );
}

fn frame(app: &mut EntropyApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    crate::app::discard_texture_deltas(ctx.run_pass(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 2200.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            ctx.root_ui(|ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    app.draw_display_settings_page(ui, ui.max_rect());
                })
            });
        },
    ))
}
fn click(app: &mut EntropyApp, ctx: &egui::Context, label: &str) {
    let mut output = frame(app, ctx, vec![]);
    let position = |output: &egui::FullOutput| {
        output.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                let pos = t.pos + t.galley.size() / 2.0;
                s.clip_rect.contains(pos).then_some(pos)
            }
            _ => None,
        })
    };
    if position(&output).is_none() {
        // The production compact dropdown scrolls after its first four rows.
        // Scroll the actual popup, never click clipped paint outside its viewport.
        let anchor = output
            .shapes
            .iter()
            .rev()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if s.clip_rect.contains(t.pos + t.galley.size() / 2.0) => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap();
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(anchor),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0.0, -160.0),
                    modifiers: Default::default(),
                    source: egui::MouseWheelSource::Unknown,
                },
            ],
        );
        for _ in 0..30 {
            output = frame(app, ctx, vec![]);
        }
    }
    let pos = position(&output).unwrap_or_else(|| panic!("missing dropdown label {label}"));
    frame(
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
    frame(app, ctx, vec![]);
    frame(app, ctx, vec![]);
}

#[test]
fn sm62_timeout_failed_set_get_and_mismatch_never_confirm_requested_value() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for (after, fault) in [
        (0, crate::hid::TestHidFault::IgnoreQmkSettingSet),
        (0, crate::hid::TestHidFault::Timeout),
        (1, crate::hid::TestHidFault::Timeout),
        (1, crate::hid::TestHidFault::Disconnect),
    ] {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        // A disconnect must exercise recovery without enumerating real host HID.
        let (scan_requests, scans) = std::sync::mpsc::channel();
        app.test_device_scan_requests = Some(scan_requests);
        let disconnect = matches!(
            fault,
            crate::hid::TestHidFault::Disconnect | crate::hid::TestHidFault::Timeout
        );
        let (hid, recorder) =
            crate::hid::HidDevice::test_device_with_fault_after_requests(Some((after, fault)));
        app.hid_device = Some(hid.into());
        app.display_settings.sm62_supported = true;
        app.display_settings.display_timeout_supported = true;
        app.display_settings.display_timeout = 6;
        app.display_settings.confirmed_display_timeout = 4;
        app.queue_display_setting_write("timeout".into(), DISPLAY_TIMEOUT_QSID, 4, 6);
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.qmk_settings_write_busy() && Instant::now() < deadline {
            app.poll_settings_write(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.qmk_settings_write_busy());
        assert!(matches!(
            app.settings_write_status(DISPLAY_TIMEOUT_QSID),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Failed(_))
        ));
        assert_ne!(app.display_settings.confirmed_display_timeout, 6);
        let count = recorder.requests().len();
        for _ in 0..3 {
            let output = crate::app::discard_texture_deltas(ctx.run_pass(
                egui::RawInput::default(),
                |ctx| {
                    ctx.root_ui(|ui| {
                        egui::CentralPanel::default()
                            .show(ui, |ui| app.draw_sm62_timeout_control(ui))
                    });
                },
            ));
            assert!(
                output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Text(t) if t.galley.job.text == "!")),
                "SM62 timeout failure remains visible"
            );
        }
        assert_eq!(
            recorder.requests().len(),
            count,
            "painting a failure must not retry"
        );
        assert_eq!(
            scans.try_iter().count(),
            usize::from(disconnect),
            "transport failure starts discovery once; a readback mismatch does not"
        );
    }
}

#[test]
fn sm62_timeout_unsupported_control_never_queues_and_does_not_mutate_defaults() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let ctx = egui::Context::default();
    let mut app = EntropyApp::new_inert_for_test();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid.into());
    let before = app.display_settings.clone();
    ctx.run_pass(egui::RawInput::default(), |ctx| {
        ctx.root_ui(|ui| {
            egui::CentralPanel::default().show(ui, |ui| app.draw_sm62_timeout_control(ui))
        });
    })
    .drop_without_applying_deltas();
    assert_eq!(app.display_settings, before);
    assert!(recorder.requests().is_empty());
    assert_eq!(
        super::super::settings_write_queue::qmk_setting_write_policy(DISPLAY_TIMEOUT_QSID),
        super::super::settings_write_queue::QmkSettingWritePolicy::SetOnly,
        "unrelated generic settings retain legacy policy"
    );
}

#[test]
fn sm62_timeout_real_dropdown_queues_each_index_and_reloads_without_other_writes() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for lang in [
        crate::i18n::Language::English,
        crate::i18n::Language::Russian,
    ] {
        for index in 0..8u8 {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            app.app_settings.language = lang;
            let mut fonts = egui::FontDefinitions::default();
            fonts.families.insert(
                egui::FontFamily::Name("display_preview".into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
            ctx.set_fonts(fonts);
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let values = [2, 1, 20, 1, 1, 0, 1, 45, 1, 1];
            for (offset, value) in values.into_iter().enumerate() {
                hid.set_qmk_setting_u8(230 + offset as u16, value).unwrap();
            }
            // Start with a distinct saved value, including Never.
            let initial = (index + 1) % 8;
            hid.set_qmk_setting_u8(DISPLAY_TIMEOUT_QSID, initial)
                .unwrap();
            let qsids: Vec<_> = (230..=239).chain([DISPLAY_TIMEOUT_QSID]).collect();
            app.display_settings =
                crate::app::app_state::load_display_settings(&hid, &qsids).unwrap();
            let before = app.display_settings.clone();
            let request_start = recorder.requests().len();
            app.hid_device = Some(hid.into());
            frame(&mut app, &ctx, vec![]);
            frame(&mut app, &ctx, vec![]);
            assert_eq!(
                recorder.requests().len(),
                request_start,
                "render must not write"
            );
            let keys = [
                "display_settings.never",
                "display_settings.minute_1",
                "display_settings.minutes_2",
                "display_settings.minutes_5",
                "display_settings.minutes_10",
                "display_settings.minutes_15",
                "display_settings.minutes_30",
                "display_settings.minutes_60",
            ];
            let initial_label = crate::i18n::tr_catalog(lang, keys[initial as usize]);
            click(&mut app, &ctx, initial_label);
            let label = crate::i18n::tr_catalog(lang, keys[index as usize]);
            click(&mut app, &ctx, label);
            assert_eq!(
                app.display_settings.display_timeout, index,
                "actual pointer selection"
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.qmk_settings_write_busy() && Instant::now() < deadline {
                app.poll_settings_write(&ctx);
                frame(&mut app, &ctx, vec![]);
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!app.qmk_settings_write_busy());
            assert!(matches!(
                app.settings_write_status(DISPLAY_TIMEOUT_QSID),
                Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
            ));
            assert_eq!(app.display_settings.confirmed_display_timeout, index);
            let mut after = app.display_settings.clone();
            after.display_timeout = before.display_timeout;
            after.confirmed_display_timeout = before.confirmed_display_timeout;
            assert_eq!(
                after, before,
                "Ship, independent sides and all unrelated settings unchanged"
            );
            let requests = recorder.requests();
            let sets: Vec<_> = requests[request_start..]
                .iter()
                .filter(|r| r[1] == 0x0B)
                .collect();
            assert_eq!(sets.len(), 1);
            assert_eq!(
                u16::from_le_bytes([sets[0][2], sets[0][3]]),
                DISPLAY_TIMEOUT_QSID
            );
            assert_eq!(sets[0][4], index);
            assert!(requests[request_start..]
                .iter()
                .any(|r| r[1] == 0x0A && u16::from_le_bytes([r[2], r[3]]) == DISPLAY_TIMEOUT_QSID));
            let loaded = crate::app::app_state::load_display_settings(
                app.hid_device.as_ref().unwrap().as_vial().unwrap(),
                &qsids,
            )
            .unwrap();
            assert_eq!(
                loaded.confirmed_display_timeout, index,
                "fresh production read from device storage"
            );
            assert_eq!(loaded.sm62_values, values);
        }
    }
}

#[test]
fn sm62_timeout_saved_check_is_hidden_but_generic_saved_status_is_retained() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let ctx = egui::Context::default();
    let mut app = EntropyApp::new_inert_for_test();
    let (hid, _) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid.into());
    app.display_settings.sm62_supported = true;
    app.display_settings.display_timeout_supported = true;
    app.queue_display_setting_write("timeout".into(), DISPLAY_TIMEOUT_QSID, 0, 1);
    // Do not poll completion yet: the real queue must still paint progress.
    for _ in 0..2 {
        let output =
            crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
                ctx.root_ui(|ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.draw_sm62_timeout_control(ui))
                });
            }));
        assert!(
            output.shapes.iter().any(|s| matches!(&s.shape,
            egui::Shape::Path(p) if p.stroke.width == 3.0 && p.points.len() >= 8)),
            "pending spinner remains visible"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.qmk_settings_write_busy() && Instant::now() < deadline {
        app.poll_settings_write(&ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        app.settings_write_status(DISPLAY_TIMEOUT_QSID),
        Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
    ));
    let draw = |app: &mut EntropyApp, timeout: bool| {
        crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
            ctx.root_ui(|ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if timeout {
                        app.draw_sm62_timeout_control(ui);
                    } else {
                        app.draw_settings_write_status(
                            ui,
                            DISPLAY_TIMEOUT_QSID,
                            crate::ui_style::ResponsiveMetrics::from_ctx(ctx),
                            false,
                        );
                    }
                })
            });
        }))
    };
    let checks = |out: &egui::FullOutput| {
        out.shapes
            .iter()
            .filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "\u{2713}"))
            .count()
    };
    draw(&mut app, true);
    let output = draw(&mut app, true);
    assert_eq!(
        checks(&output),
        0,
        "SM62 timeout must not paint a green Saved check"
    );
    draw(&mut app, false);
    assert_eq!(
        checks(&draw(&mut app, false)),
        1,
        "generic status renderer must retain Saved checks"
    );
    assert_eq!(app.display_settings.confirmed_display_timeout, 1);
    assert!(
        matches!(
            app.settings_write_status(DISPLAY_TIMEOUT_QSID),
            Some(super::super::settings_write_queue::SettingsWriteStatus::Saved)
        ),
        "hide presentation only; retain confirmed state"
    );
}
