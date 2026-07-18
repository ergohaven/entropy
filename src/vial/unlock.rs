#[cfg(not(target_arch = "wasm32"))]
use super::vial_hid_task::VialHidTaskStart;
use super::*;

const VIAL_UNLOCK_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
const VIAL_UNLOCK_PROGRESS_ANIMATION_TIME: f32 = 0.16;

fn should_start_vial_unlock(
    unlock_open: bool,
    unlock_polling: bool,
    start_requested: bool,
) -> bool {
    unlock_open && !unlock_polling && start_requested
}

impl EntropyApp {
    pub(super) fn stop_vial_unlock_with_status(&mut self, status: impl Into<String>) {
        self.status_msg = status.into();
        self.unlock_open = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.vial_unlock_reconnect_after_completion = false;
        self.vial_unlock_counter = self.vial_unlock_total;
        self.vial_unlock_best = self.vial_unlock_total;
        self.pending_layout_indicator_open_after_unlock = false;
    }

    fn complete_vial_unlock(&mut self) {
        let reconnect_after_completion = self.vial_unlock_reconnect_after_completion;
        self.vial_unlocked = Some(true);
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            "status_messages.device_unlocked",
        )
        .into();
        self.unlock_open = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.vial_unlock_reconnect_after_completion = false;
        self.macro_auto_unlock_cancelled = false;
        if self.pending_layout_indicator_open_after_unlock {
            self.pending_layout_indicator_open_after_unlock = false;
            self.app_settings.sticky_layout_window = true;
            self.sticky_layout_last_size = None;
            save_app_settings(&self.app_settings);
        }
        if reconnect_after_completion {
            if let Some(device_idx) = self.selected_device {
                log::info!("Reloading device state after Vial unlock recovery");
                self.start_connect(device_idx);
            } else {
                self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.unlock_cancelled_disconnected",
                ));
            }
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
        if unlocked {
            self.complete_vial_unlock();
        } else if !in_progress {
            log::warn!("Vial unlock poll reported session ended before unlock; reconnecting");
            self.vial_unlocked = Some(false);
            self.unlock_open = false;
            self.vial_unlock_polling = false;
            self.vial_unlock_last_poll = None;
            self.vial_unlock_reconnect_after_completion = false;
            if let Some(device_idx) = self.selected_device {
                self.start_connect(device_idx);
            } else {
                self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.unlock_cancelled_disconnected",
                ));
            }
        } else {
            self.vial_unlocked = Some(false);
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
            let waiting_for_start = !self.vial_unlock_polling;
            let mut start_requested = false;
            let mut cancel_requested = false;

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
                    ui.interact(
                        screen,
                        egui::Id::new("unlock_overlay_blocker"),
                        egui::Sense::click_and_drag(),
                    );
                    ui.painter().rect_filled(screen, 0.0, screen_bg);

                    let center_x = screen.center().x;
                    let top_y = screen.min.y + 40.0;

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

                    let subtitle_key = if waiting_for_start {
                        "unlock.start_hint"
                    } else {
                        "unlock.highlighted_keys_hint"
                    };
                    ui.painter().text(
                        egui::pos2(center_x, top_y + 30.0),
                        egui::Align2::CENTER_CENTER,
                        crate::i18n::tr_catalog(self.app_settings.language, subtitle_key),
                        FontId::proportional(14.0),
                        subtitle_color,
                    );

                    if waiting_for_start {
                        let controls_rect = egui::Rect::from_center_size(
                            egui::pos2(center_x, top_y + 78.0),
                            egui::vec2(320.0, 40.0),
                        );
                        crate::ui_style::allocate_ui_at_rect(ui, controls_rect, |ui| {
                            ui.horizontal_centered(|ui| {
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
                                    cancel_requested = true;
                                }
                                ui.add_space(12.0);
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
                                    start_requested = true;
                                }
                            });
                        });
                    } else {
                        // Progress bar
                        let target_progress = if total > 0 {
                            1.0 - (counter as f32 / total as f32)
                        } else {
                            0.0
                        };
                        let progress = ui.ctx().animate_value_with_time(
                            egui::Id::new((
                                "vial_unlock_progress",
                                self.vial_unlock_animation_nonce,
                            )),
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
                    }

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

            if cancel_requested {
                self.cancel_vial_unlock(true);
                ctx.request_repaint();
                return;
            }
            if should_start_vial_unlock(self.unlock_open, self.vial_unlock_polling, start_requested)
            {
                match self.start_vial_unlock(ctx) {
                    VialHidTaskStart::Started | VialHidTaskStart::Busy => {}
                    VialHidTaskStart::NoDevice => {
                        self.stop_vial_unlock_with_status(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "status_messages.unlock_cancelled_disconnected",
                        ));
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_waits_for_explicit_start_request() {
        assert!(!should_start_vial_unlock(true, false, false));
        assert!(should_start_vial_unlock(true, false, true));
        assert!(!should_start_vial_unlock(false, false, true));
        assert!(!should_start_vial_unlock(true, true, true));
    }
}
