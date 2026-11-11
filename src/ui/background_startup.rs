//! Cold tray startup does not need a graphics window or the GUI font set.
use super::*;

#[derive(Debug, PartialEq, Eq)]
enum Next {
    Quit,
    Show,
    Wait(std::time::Duration),
}

fn next_step(close_requested: bool, needs_ui: bool, output: &egui::FullOutput) -> Next {
    let root = &output.viewport_output[&egui::ViewportId::ROOT];
    let canceled = root.commands.contains(&egui::ViewportCommand::CancelClose);
    if close_requested && !canceled {
        return Next::Quit;
    }
    if needs_ui {
        return Next::Show;
    }
    // Polling is a fallback for worker callbacks and single-instance signals.
    // Native tray messages wake the wait immediately. Never spin on a zero delay.
    Next::Wait(root.repaint_delay.clamp(
        std::time::Duration::from_millis(10),
        std::time::Duration::from_millis(250),
    ))
}

// Shared with native tests so the windowless production frame is exercised
// without creating a Windows tray/message window on other test platforms.
#[cfg(any(target_os = "windows", test))]
fn run_windowless_frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    mut update: impl FnMut(&egui::Context),
) -> egui::FullOutput {
    // There is no renderer in the windowless phase. Release unused texture
    // changes explicitly while retaining viewport commands for quit/restore.
    let mut output = ctx.run_ui(input, |ui| update(ui.ctx()));
    output.textures_delta.clear();
    output
}

impl EntropyApp {
    #[cfg(target_os = "windows")]
    pub(crate) fn run_windowless_until_restore(&mut self, ctx: &egui::Context) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW, TranslateMessage, MSG,
            MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, WM_QUIT,
        };
        self.windows_window_hidden_to_tray = true;
        self.ensure_tray_icon(ctx);
        if self.tray_icon.is_none() {
            self.windows_window_hidden_to_tray = false;
            return false;
        }
        let started = std::time::Instant::now();
        let mut close_requested = false;
        loop {
            // tray-icon owns its native message window on this thread.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if message.message == WM_QUIT {
                        self.force_close_requested = true;
                        close_requested = true;
                    } else {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
            let mut input = egui::RawInput {
                time: Some(started.elapsed().as_secs_f64()),
                ..Default::default()
            };
            if close_requested {
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            // Match the native integration: preserve this loop's ROOT epoch
            // before background services run, including the graphics handoff.
            eframe::App::raw_input_hook(self, ctx, &mut input);
            let output = run_windowless_frame(ctx, input, |ctx| self.tick_background(ctx));
            let needs_ui = !self.windows_window_hidden_to_tray
                || self.app_settings.sticky_layout_window
                || self.unlock_open
                || self.vial_unlock_polling
                || self.import_pending();
            match next_step(close_requested, needs_ui, &output) {
                Next::Quit => {
                    eframe::App::on_exit(self, None);
                    return true;
                }
                Next::Show => {
                    self.windows_window_hidden_to_tray = false;
                    self.native_time_offset = started.elapsed().as_secs_f64();
                    return false;
                }
                Next::Wait(delay) => {
                    close_requested = output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .contains(&egui::ViewportCommand::Close);
                    if close_requested {
                        continue;
                    }
                    unsafe {
                        MsgWaitForMultipleObjectsEx(
                            0,
                            std::ptr::null(),
                            delay.as_millis() as u32,
                            QS_ALLINPUT,
                            MWMO_INPUTAVAILABLE,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windowless_production_frame_consumes_texture_deltas_without_losing_commands() {
        let ctx = egui::Context::default();
        let mut output = run_windowless_frame(&ctx, egui::RawInput::default(), |ctx| {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        });
        let textures_handled = output.textures_delta.is_empty();
        let decision = next_step(true, false, &output);
        // Preserve clean teardown even for the deliberately failing predecessor.
        output.textures_delta.clear();
        assert!(textures_handled, "windowless production frame retained unapplied texture deltas");
        assert!(matches!(decision, Next::Wait(_)), "texture cleanup lost CancelClose");
    }

    #[test]
    fn background_services_scan_and_release_images_without_drawing_the_ui() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        #[cfg(target_os = "windows")]
        {
            app.windows_window_hidden_to_tray = true;
        }
        #[cfg(target_os = "macos")]
        {
            app.macos_window_hidden_to_menu_bar = true;
        }
        #[cfg(target_os = "linux")]
        {
            app.linux_background_since = Some(0.0);
        }
        let (requests, scans) = std::sync::mpsc::channel();
        app.test_device_scan_requests = Some(requests);
        let before = ctx.tex_manager().read().num_allocated();
        drop(
            crate::ui_textures::texture(&ctx, "preview", 0, || {
                Some(egui::ColorImage::filled([240, 280], Color32::WHITE))
            })
            .unwrap(),
        );
        assert_eq!(ctx.tex_manager().read().num_allocated(), before + 1);
        let mut input = egui::RawInput {
            time: Some(10.0),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .minimized = Some(true);
        let output = crate::app::discard_texture_deltas(ctx.run_ui(input, |ui| app.tick_background(ui.ctx())));
        assert!(
            scans.try_recv().is_ok(),
            "tray startup must begin discovery"
        );
        assert_eq!(ctx.tex_manager().read().num_allocated(), before);
        assert!(
            output.shapes.is_empty(),
            "background services must not paint"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_background_mode_keeps_images_if_the_window_can_still_paint() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.linux_background_since = Some(0.0);
        let (requests, _scans) = std::sync::mpsc::channel();
        app.test_device_scan_requests = Some(requests);
        let mut decodes = 0;
        for _ in 0..2 {
            let _ = crate::app::discard_texture_deltas(ctx.run_ui(Default::default(), |ui| {
                app.tick_background(ui.ctx());
                drop(
                    crate::ui_textures::texture(ui.ctx(), "still-visible", 0, || {
                        decodes += 1;
                        Some(egui::ColorImage::filled([8, 8], Color32::WHITE))
                    })
                    .unwrap(),
                );
            }));
        }
        assert_eq!(
            decodes, 1,
            "do not decode and upload again on every background frame"
        );
    }

    #[test]
    fn windowless_root_clock_survives_graphics_handoff_without_changing_epoch() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.test_set_app_clock(1000.0);
        app.last_device_scan_at = 10.0;
        app.last_full_device_scan_at = 10.0;
        let (requests, scans) = std::sync::mpsc::channel();
        app.test_device_scan_requests = Some(requests);

        let mut input = egui::RawInput {
            time: Some(12.0),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        let output = run_windowless_frame(&ctx, input, |ctx| app.tick_background(ctx));
        assert!(output.textures_delta.is_empty());
        assert!(output.shapes.is_empty(), "windowless services must not paint");
        let (usb_only, complete) = scans.try_recv().expect("ROOT deadline must dispatch discovery");
        assert!(!usb_only);
        assert_eq!(app.last_device_scan_at, 12.0, "do not mix the app-clock epoch into native deadlines");
        complete.send(Ok(vec![])).unwrap();

        app.native_time_offset = 12.5;
        let mut input = egui::RawInput {
            time: Some(0.25),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert_eq!(input.time, Some(12.75));
        let mut frame = eframe::Frame::_new_kittest();
        let _output = ctx.run_logic(&input, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        assert_eq!(ctx.input(|i| i.time), 12.0, "logic-only input must not advance painted time");
        assert_eq!(app.last_device_scan_at, 12.0);
        assert!(scans.try_recv().is_err(), "handoff must retain the 2s scan rate limit");
    }

    #[test]
    fn graphics_handoff_preserves_background_deadlines() {
        let mut app = EntropyApp::new_inert_for_test();
        app.native_time_offset = 3600.0;
        let mut input = egui::RawInput {
            time: Some(0.25),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &egui::Context::default(), &mut input);
        assert_eq!(input.time, Some(3600.25));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn restore_without_a_graphics_window_leaves_the_tray_loop() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.windows_window_hidden_to_tray = true;
        assert!(app.windows_hwnd.is_none());
        app.restore_from_tray(&ctx);
        assert!(!app.windows_window_hidden_to_tray);
    }

    #[test]
    fn quit_waits_for_hid_drain_and_unlock_can_show_the_ui() {
        let ctx = egui::Context::default();
        let canceled = crate::app::discard_texture_deltas(ctx.run_ui(Default::default(), |ui| {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }));
        assert!(matches!(next_step(true, false, &canceled), Next::Wait(_)));
        assert_eq!(next_step(true, true, &canceled), Next::Show);
        let finished = crate::app::discard_texture_deltas(ctx.run_ui(Default::default(), |_| {}));
        assert_eq!(next_step(true, false, &finished), Next::Quit);
    }

    #[test]
    fn worker_timers_have_a_bounded_wait_without_busy_spinning() {
        let ctx = egui::Context::default();
        let mut output = crate::app::discard_texture_deltas(ctx.run_ui(Default::default(), |_| {}));
        for (requested, expected) in [(0, 10), (40, 40), (10000, 250)] {
            output
                .viewport_output
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .repaint_delay = std::time::Duration::from_millis(requested);
            assert_eq!(
                next_step(false, false, &output),
                Next::Wait(std::time::Duration::from_millis(expected))
            );
        }
    }
}
