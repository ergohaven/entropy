use super::*;

const AUTO_SHIFT_TIMEOUT_MIN_MS: u16 = 1;
const AUTO_SHIFT_TIMEOUT_MAX_MS: u16 = 1000;

impl EntropyApp {
    pub(super) fn draw_auto_shift_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        dark: bool,
    ) {
        let lang = self.app_settings.language;
        let hid_ready = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.hid_device.is_some()
            }
            #[cfg(target_arch = "wasm32")]
            {
                false
            }
        };

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::AutoShiftTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::AutoShiftDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if self.auto_shift_timeout.is_none() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::AutoShiftUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::AutoShiftEnableHint)),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(self.app_settings.language, "auto_shift_settings.connect_a_vial_keyboard_to_edit_auto_shift_settings"),
                        None,
                    );
                    return;
                }

                if self.is_vial_locked() {
                    self.unlock_open = true;
                    self.status_msg = format!(
                        "{} — {}",
                        crate::i18n::tr(lang, crate::i18n::Key::KeyboardLocked),
                        crate::i18n::tr(lang, crate::i18n::Key::AutoShiftUnlockHint),
                    );
                    ui.ctx().request_repaint();
                    return;
                }

                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                self.draw_auto_shift_editor_content(ui, dark, metrics);
            });
        });
    }

    fn draw_auto_shift_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        dark: bool,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        const TOTAL_ROWS: usize = 8;
        let field_width = metrics.value(86.0);
        let list = allocate_adaptive_settings_list_viewport(
            ui,
            "auto_shift_settings",
            metrics,
            TOTAL_ROWS,
            0.0,
        );

        crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
            ui.set_clip_rect(list.viewport);
            ui.set_min_size(list.content_rect.size());
            ui.spacing_mut().item_spacing.y = 0.0;
            for row_idx in list.first_visible_row..list.last_visible_row {
                self.draw_auto_shift_row(
                    ui,
                    row_idx,
                    list.row_content_width,
                    list.row_height,
                    field_width,
                    dark,
                    list.suppress_tooltips,
                );
            }
        });

        if list.has_scrollbar {
            crate::ui_style::paint_floating_scrollbar_handle(
                ui,
                list.track_rect,
                list.handle_height,
                list.scroll_ratio,
                list.track_hovered,
            );
        }
    }

    fn draw_auto_shift_row(
        &mut self,
        ui: &mut egui::Ui,
        row_idx: usize,
        content_width: f32,
        row_height: f32,
        field_width: f32,
        _dark: bool,
        suppress_tooltips: bool,
    ) {
        let row = match row_idx {
            0 => (
                crate::i18n::tr_catalog(self.app_settings.language, "common.enable"),
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.enable_tooltip"),
                true,
            ),
            1 => (
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.enable_for_modifiers",
                ),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.enable_for_modifiers_tooltip",
                ),
                true,
            ),
            2 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.no_special_keys"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.no_special_keys_tooltip",
                ),
                true,
            ),
            3 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.no_numeric_keys"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.no_numeric_keys_tooltip",
                ),
                true,
            ),
            4 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.no_alpha_keys"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.no_alpha_keys_tooltip",
                ),
                true,
            ),
            5 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.enable_keyrepeat"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.enable_keyrepeat_tooltip",
                ),
                true,
            ),
            6 => (
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.stop_repeat_after_timeout",
                ),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.stop_repeat_after_timeout_tooltip",
                ),
                true,
            ),
            7 => (
                crate::i18n::tr_catalog(self.app_settings.language, "common.timeout"),
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.timeout_tooltip"),
                false,
            ),
            _ => return,
        };
        let enabled = row_idx == 0 || self.auto_shift_options.enabled;
        let tooltip = if suppress_tooltips { None } else { Some(row.1) };
        if row.2 {
            let mut value = match row_idx {
                0 => self.auto_shift_options.enabled,
                1 => self.auto_shift_options.enable_for_modifiers,
                2 => self.auto_shift_options.no_special,
                3 => self.auto_shift_options.no_numeric,
                4 => self.auto_shift_options.no_alpha,
                5 => self.auto_shift_options.enable_keyrepeat,
                6 => self.auto_shift_options.disable_keyrepeat_timeout,
                _ => false,
            };
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                content_width,
                row_height,
                row.0,
                enabled,
                tooltip,
                46.0,
                |ui| {
                    let resp = crate::ui_style::settings_switch_sized_stable_interactive(
                        ui,
                        ("auto_shift_settings", row_idx),
                        &mut value,
                        egui::vec2(46.0, 24.0),
                        enabled,
                    );
                    if resp.changed() {
                        match row_idx {
                            0 => self.auto_shift_options.enabled = value,
                            1 => self.auto_shift_options.enable_for_modifiers = value,
                            2 => self.auto_shift_options.no_special = value,
                            3 => self.auto_shift_options.no_numeric = value,
                            4 => self.auto_shift_options.no_alpha = value,
                            5 => self.auto_shift_options.enable_keyrepeat = value,
                            6 => self.auto_shift_options.disable_keyrepeat_timeout = value,
                            _ => {}
                        }
                        self.write_auto_shift_flags();
                    }
                },
            );
        } else {
            let timeout_value = self.auto_shift_timeout.unwrap_or(175);
            if self.auto_shift_timeout_text.is_empty() {
                self.auto_shift_timeout_text = timeout_value.to_string();
            }
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                content_width,
                row_height,
                row.0,
                enabled,
                tooltip,
                field_width,
                |ui| {
                    let edit_id = egui::Id::new("auto_shift_timeout");
                    let resp = crate::ui_style::modern_text_field_interactive(
                        ui,
                        edit_id,
                        &mut self.auto_shift_timeout_text,
                        field_width,
                        "",
                        5,
                        egui::Align::RIGHT,
                        enabled,
                    );
                    let resp = settings_field_unit_tooltip(
                        resp,
                        self.app_settings.language,
                        suppress_tooltips,
                        SettingsFieldUnit::Milliseconds,
                    );
                    if resp.changed() {
                        let filtered: String = self
                            .auto_shift_timeout_text
                            .chars()
                            .filter(|c: &char| c.is_ascii_digit())
                            .collect();
                        if filtered != self.auto_shift_timeout_text {
                            self.auto_shift_timeout_text = filtered.clone();
                        }
                        if let Ok(parsed) = filtered.parse::<u32>() {
                            let timeout_value = parsed.clamp(
                                AUTO_SHIFT_TIMEOUT_MIN_MS as u32,
                                AUTO_SHIFT_TIMEOUT_MAX_MS as u32,
                            ) as u16;
                            self.auto_shift_timeout = Some(timeout_value);
                            self.auto_shift_timeout_text = timeout_value.to_string();
                            self.write_auto_shift_timeout();
                        }
                    }
                },
            );
        }
    }

    fn write_auto_shift_flags(&mut self) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        if let Err(e) = hid.set_qmk_setting_u8(3, self.auto_shift_options.bits()) {
            self.status_msg = format!("Failed to save Auto Shift flags: {}", e);
            log::warn!("set_qmk_setting_u8(auto_shift_flags) failed: {e}");
        }
    }

    fn write_auto_shift_timeout(&mut self) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let Some(timeout) = self.auto_shift_timeout else {
            return;
        };
        if let Err(e) = hid.set_qmk_setting_u16(4, timeout) {
            self.status_msg = format!("Failed to save Auto Shift timeout: {}", e);
            log::warn!("set_qmk_setting_u16(auto_shift_timeout) failed: {e}");
        }
    }
}
