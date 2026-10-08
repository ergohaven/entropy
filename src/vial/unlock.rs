#[cfg(not(target_arch = "wasm32"))]
use super::vial_hid_task::VialHidTaskStart;
use super::*;

const VIAL_UNLOCK_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
const VIAL_UNLOCK_PROGRESS_ANIMATION_TIME: f32 = 0.16;

impl EntropyApp {
    pub(super) fn stop_vial_unlock_with_status(&mut self, status: impl Into<String>) {
        self.status_msg = status.into();
        self.unlock_open = false;
        self.vial_unlock_session_started = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.vial_unlock_counter = self.vial_unlock_total;
        self.vial_unlock_best = self.vial_unlock_total;
        self.pending_layout_indicator_open_after_unlock = false;
        self.macro_auto_unlock_cancelled = true;
    }

    /// Dismiss only before the HID worker can have sent UNLOCK_START. Once it has
    /// been submitted, closing this overlay could strand the keyboard mid-unlock.
    pub(super) fn dismiss_vial_unlock_preflight(&mut self) {
        if !self.unlock_open || self.vial_unlock_session_started || self.vial_unlock_polling {
            return;
        }
        self.vial_unlocked = Some(false);
        self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
            self.app_settings.language,
            "status_messages.device_unlock_cancelled",
        ));
        // A protected feature selected from the menus must not become visible
        // merely because the preflight overlay was dismissed. Keep its selection
        // for a later explicit click, but return to the keyboard canvas now.
        if matches!(
            self.main_menu_tab,
            MainMenuTab::Advanced | MainMenuTab::Settings
        ) && matches!(
            self.settings_tab,
            SettingsTab::MatrixTester
                | SettingsTab::Macros
                | SettingsTab::TapDance
                | SettingsTab::AutoShift
        ) {
            self.main_menu_tab = MainMenuTab::Keyboard;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn begin_vial_unlock(&mut self, ctx: &egui::Context) {
        if !self.unlock_open || self.vial_unlock_session_started || self.vial_unlock_polling {
            return;
        }
        match self.start_vial_unlock(ctx) {
            VialHidTaskStart::Started => {
                self.vial_unlock_session_started = true;
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            VialHidTaskStart::Busy => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16))
            }
            VialHidTaskStart::NoDevice => {
                self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.unlock_cancelled_disconnected",
                ));
            }
        }
    }

    fn complete_vial_unlock(&mut self) {
        self.vial_unlocked = Some(true);
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            "status_messages.device_unlocked",
        )
        .into();
        self.unlock_open = false;
        self.vial_unlock_session_started = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.macro_auto_unlock_cancelled = false;
        if self.pending_layout_indicator_open_after_unlock {
            self.pending_layout_indicator_open_after_unlock = false;
            self.app_settings.sticky_layout_window = true;
            self.sticky_layout_last_size = None;
            save_app_settings(&self.app_settings);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn finish_vial_unlock_start(&mut self, unlocked: bool, keys: Vec<(u8, u8)>) {
        self.vial_unlock_keys = keys;
        if unlocked {
            self.complete_vial_unlock();
            return;
        }

        self.vial_unlocked = Some(false);
        self.vial_unlock_polling = true;
        let total = u8::try_from(self.vial_unlock_keys.len())
            .unwrap_or(u8::MAX)
            .max(1);
        self.vial_unlock_counter = total;
        self.vial_unlock_best = total;
        self.vial_unlock_total = total;
        self.vial_unlock_last_poll = Some(std::time::Instant::now());
        self.vial_unlock_animation_nonce = self.vial_unlock_animation_nonce.wrapping_add(1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn finish_vial_unlock_poll(
        &mut self,
        unlocked: bool,
        in_progress: bool,
        counter: u8,
    ) {
        self.vial_unlock_counter = counter;
        if counter > self.vial_unlock_total {
            self.vial_unlock_total = counter;
        }
        if unlocked && !in_progress {
            self.complete_vial_unlock();
        } else if in_progress {
            // Some firmware keeps the unlocked bit set while a new physical
            // hold is pending. Normal commands remain gated in that state.
            self.vial_unlocked = Some(false);
        } else {
            self.vial_unlocked = Some(false);
            self.macro_auto_unlock_cancelled = true;
            self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
                self.app_settings.language,
                "status_messages.device_unlock_cancelled",
            ));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn fail_vial_unlock_start(&mut self, error: String) {
        self.vial_unlocked = Some(false);
        self.stop_vial_unlock_with_status(crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "status_messages.unlock_start_failed",
            &[("error", &error)],
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn fail_vial_unlock_poll(&mut self, error: String) {
        log::warn!("Vial unlock poll failed; retrying: {error}");
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "status_messages.unlock_poll_retry",
            &[("error", &error)],
        );
    }

    pub(super) fn draw_vial_unlock_overlay(&mut self, ctx: &egui::Context) {
        // Vial unlock modal
        if self.unlock_open && self.firmware == FirmwareProtocol::Vial {
            let preflight = !self.vial_unlock_session_started && !self.vial_unlock_polling;
            if preflight && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.dismiss_vial_unlock_preflight();
                return;
            }

            // Match Vial's polling cadence. Vial QMK resets the unlock counter whenever
            // UNLOCK_POLL arrives before its internal ~100ms timer has elapsed, even if the
            // correct keys are held. Polling too fast makes progress stick near zero.
            // The overlay still repaints independently for smooth progress animation.
            if self.vial_unlock_polling {
                let now = std::time::Instant::now();
                let should_poll = self
                    .vial_unlock_last_poll
                    .map(|last_poll| now.duration_since(last_poll) >= VIAL_UNLOCK_POLL_INTERVAL)
                    .unwrap_or(true);
                if should_poll {
                    match self.start_vial_unlock_poll(ctx) {
                        VialHidTaskStart::Started => {
                            self.vial_unlock_last_poll = Some(now);
                        }
                        VialHidTaskStart::Busy => {}
                        VialHidTaskStart::NoDevice => {
                            self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "status_messages.unlock_cancelled_disconnected",
                            ));
                            return;
                        }
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            // Fullscreen overlay with layout and highlighted keys
            let unlock_keys = self.vial_unlock_keys.clone();
            let counter = self.vial_unlock_counter;
            let total = self.vial_unlock_total;
            let layout_options_value = self.layout_options_value;

            egui::Area::new(egui::Id::new("unlock_overlay"))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let screen = ui.ctx().content_rect();
                    let dark = ui.visuals().dark_mode;
                    let screen_bg = app_panel_fill(dark);
                    let title_color = if dark {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(28)
                    };
                    let subtitle_color = if dark {
                        Color32::from_gray(180)
                    } else {
                        Color32::from_gray(96)
                    };
                    let bar_bg = if dark {
                        Color32::from_gray(40)
                    } else {
                        Color32::from_gray(220)
                    };
                    let inactive_key_bg = if dark {
                        Color32::from_rgb(48, 48, 52)
                    } else {
                        Color32::from_rgb(255, 255, 255)
                    };
                    let inactive_key_border = if dark {
                        Color32::from_rgb(54, 54, 58)
                    } else {
                        Color32::from_rgb(230, 230, 233)
                    };
                    ui.painter().rect_filled(screen, 0.0, screen_bg);
                    ui.interact(
                        screen,
                        egui::Id::new("unlock_overlay_blocker"),
                        Sense::click_and_drag(),
                    );

                    let center_x = screen.center().x;
                    // Center the preflight title, text, and buttons as a group.
                    // Leave the active unlock layout in its existing position.
                    let top_y = if preflight {
                        screen.center().y - 65.0
                    } else {
                        screen.min.y + 40.0
                    };

                    // Title
                    ui.painter().text(
                        egui::pos2(center_x, top_y),
                        egui::Align2::CENTER_CENTER,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "app_chrome.unlock_unlock_keyboard",
                        ),
                        FontId::proportional(24.0),
                        title_color,
                    );

                    if preflight {
                        let warning_rect = egui::Rect::from_center_size(
                            egui::pos2(center_x, top_y + 65.0),
                            egui::vec2(screen.width().min(560.0), 54.0),
                        );
                        crate::ui_style::allocate_ui_at_rect(ui, warning_rect, |ui| {
                            ui.add_sized(
                                warning_rect.size(),
                                egui::Label::new(crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "unlock.safety_warning",
                                ))
                                .wrap()
                                .halign(egui::Align::Center),
                            );
                        });
                        let buttons_rect = egui::Rect::from_center_size(
                            egui::pos2(center_x, top_y + 130.0),
                            egui::vec2(260.0, 36.0),
                        );
                        crate::ui_style::allocate_ui_at_rect(ui, buttons_rect, |ui| {
                            ui.horizontal(|ui| {
                                if crate::ui_style::modern_button(
                                    ui,
                                    crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "unlock.cancel",
                                    ),
                                    egui::vec2(120.0, 34.0),
                                    true,
                                )
                                .clicked()
                                {
                                    self.dismiss_vial_unlock_preflight();
                                }
                                if crate::ui_style::modern_button(
                                    ui,
                                    crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "unlock.start",
                                    ),
                                    egui::vec2(120.0, 34.0),
                                    true,
                                )
                                .clicked()
                                {
                                    #[cfg(not(target_arch = "wasm32"))]
                                    self.begin_vial_unlock(ctx);
                                }
                            });
                        });
                        return;
                    }

                    ui.painter().text(
                        egui::pos2(center_x, top_y + 30.0),
                        egui::Align2::CENTER_CENTER,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "unlock.highlighted_keys_hint",
                        ),
                        FontId::proportional(14.0),
                        subtitle_color,
                    );

                    // Progress bar
                    let target_progress = if total > 0 {
                        1.0 - (counter as f32 / total as f32)
                    } else {
                        0.0
                    };
                    let progress = ui.ctx().animate_value_with_time(
                        egui::Id::new(("vial_unlock_progress", self.vial_unlock_animation_nonce)),
                        target_progress.clamp(0.0, 1.0),
                        VIAL_UNLOCK_PROGRESS_ANIMATION_TIME,
                    );
                    let bar_w = 300.0f32;
                    let bar_h = 12.0f32;
                    let bar_y = top_y + 55.0;
                    let bar_rect = egui::Rect::from_min_size(
                        egui::pos2(center_x - bar_w / 2.0, bar_y),
                        egui::Vec2::new(bar_w, bar_h),
                    );
                    ui.painter().rect(
                        bar_rect,
                        4.0,
                        bar_bg,
                        egui::Stroke::NONE,
                        egui::StrokeKind::Inside,
                    );
                    let fill_rect = egui::Rect::from_min_size(
                        bar_rect.min,
                        egui::Vec2::new(bar_w * progress, bar_h),
                    );
                    ui.painter().rect(
                        fill_rect,
                        4.0,
                        app_accent(),
                        egui::Stroke::NONE,
                        egui::StrokeKind::Inside,
                    );

                    // Draw layout keys with highlighted unlock keys. Always compute geometry
                    // against the fullscreen unlock overlay: `last_layout_geometry` belongs to
                    // the normal layout viewport and can be stale or off-screen after switching
                    // from Settings/Advanced pages.
                    if let Some(layout) = &self.layout {
                        let is_visible_key = |key: &PhysicalKey| {
                            Self::layout_condition_visible(
                                layout,
                                key.layout_condition,
                                layout_options_value,
                            )
                        };
                        let is_visible_encoder = |encoder: &PhysicalEncoder| {
                            Self::layout_condition_visible(
                                layout,
                                encoder.layout_condition,
                                layout_options_value,
                            )
                        };
                        let geometry = layout_geometry_with_reserved_and_filter(
                            ui.ctx(),
                            layout,
                            screen,
                            clamp_ui_scale(self.app_settings.ui_scale),
                            LAYOUT_TOP_RESERVED_H,
                            LAYOUT_BOTTOM_RESERVED_H,
                            LAYOUT_FIT_MARGIN,
                            None,
                            is_visible_key,
                            is_visible_encoder,
                        );
                        for key in &layout.keys {
                            if !is_visible_key(key) {
                                continue;
                            }
                            let is_unlock = unlock_keys
                                .iter()
                                .any(|(r, c)| key.row == *r && key.col == *c);
                            let rect = layout_physical_key_rect(key, geometry);
                            let bg = if is_unlock {
                                app_accent()
                            } else {
                                inactive_key_bg
                            };
                            let border = if is_unlock {
                                app_accent()
                            } else {
                                inactive_key_border
                            };
                            paint_layout_keycap(
                                ui.painter(),
                                rect,
                                key.rotation,
                                bg,
                                Stroke::new(1.0_f32, border),
                            );
                        }
                    }
                });
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn frame(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw_vial_unlock_overlay(ui.ctx()),
        )
    }

    fn click_preflight_button(app: &mut EntropyApp, ctx: &egui::Context, start: bool) {
        frame(app, ctx, Vec::new());
        // The preflight row is centered in the 1100px test viewport.
        let pos = egui::pos2(if start { 600.0 } else { 480.0 }, 465.0);
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
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
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn opening_unlock_preflight_sends_no_hid_command_and_escape_dismisses_it() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.firmware = FirmwareProtocol::Vial;
        app.unlock_open = true;
        app.pending_layout_indicator_open_after_unlock = true;

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw_vial_unlock_overlay(ui.ctx());
        });
        assert!(app.unlock_open);
        assert!(!app.vial_unlock_session_started);
        assert!(app.vial_hid_task.is_none());
        assert!(recorder.requests().is_empty());

        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.draw_vial_unlock_overlay(ui.ctx()),
        );
        assert!(!app.unlock_open);
        assert!(!app.pending_layout_indicator_open_after_unlock);
        assert!(app.macro_auto_unlock_cancelled);
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn cancel_button_dismisses_preflight_without_hid_command() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.firmware = FirmwareProtocol::Vial;
        app.unlock_open = true;

        click_preflight_button(&mut app, &ctx, false);
        assert!(!app.unlock_open);
        assert!(app.vial_hid_task.is_none());
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn cancelling_locked_feature_navigation_returns_to_keyboard() {
        for tab in [
            SettingsTab::Macros,
            SettingsTab::TapDance,
            SettingsTab::AutoShift,
            SettingsTab::MatrixTester,
        ] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid);
            app.firmware = FirmwareProtocol::Vial;
            app.vial_unlocked = Some(false);
            app.settings_tab = tab;
            app.main_menu_tab = if tab == SettingsTab::MatrixTester {
                MainMenuTab::Settings
            } else {
                MainMenuTab::Advanced
            };
            app.unlock_open = true;

            click_preflight_button(&mut app, &ctx, false);
            assert!(!app.unlock_open);
            assert!(app.main_menu_tab == MainMenuTab::Keyboard);
            assert!(app.settings_tab == tab);
            assert_eq!(app.vial_unlocked, Some(false));
            assert!(recorder.requests().is_empty());
        }
    }

    #[test]
    fn cancelling_direct_unlock_does_not_change_unrelated_page() {
        let mut app = EntropyApp::new_inert_for_test();
        app.firmware = FirmwareProtocol::Vial;
        app.main_menu_tab = MainMenuTab::Settings;
        app.settings_tab = SettingsTab::AboutDevice;
        app.unlock_open = true;

        app.dismiss_vial_unlock_preflight();
        assert!(!app.unlock_open);
        assert!(app.main_menu_tab == MainMenuTab::Settings);
        assert!(app.settings_tab == SettingsTab::AboutDevice);
    }

    #[test]
    fn start_submits_hid_work_and_cannot_be_dismissed_afterward() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.firmware = FirmwareProtocol::Vial;
        app.unlock_open = true;

        click_preflight_button(&mut app, &ctx, true);
        assert!(app.vial_unlock_session_started);
        assert!(app.vial_hid_task.is_some());
        app.dismiss_vial_unlock_preflight();
        assert!(app.unlock_open);

        app.finish_vial_unlock_start(false, vec![(0, 0)]);
        assert!(app.vial_unlock_polling);
        app.dismiss_vial_unlock_preflight();
        assert!(app.unlock_open);
    }
}
