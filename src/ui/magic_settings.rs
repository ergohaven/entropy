use super::*;

impl EntropyApp {
    pub(super) fn draw_magic_settings_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
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
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::MagicTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::MagicDescription))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.magic_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::MagicUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::MagicEnableHint)),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::MagicConnect),
                        None,
                    );
                    return;
                }

                const TOTAL_ROWS: usize = 10;
                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "magic_settings",
                    metrics,
                    TOTAL_ROWS,
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_magic_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
                        list.suppress_tooltips,
                    );
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
            });
        });
    }

    fn draw_magic_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let switch_width = (row_height / 54.0).clamp(1.0, 1.12) * 46.0;
        let switch_size = egui::vec2(switch_width, (row_height / 54.0).clamp(1.0, 1.12) * 24.0);
        let gui = crate::keycode::gui_mod_name();
        for row_idx in row_range {
            let (bit, label, tooltip) = match row_idx {
                0 => (
                    0,
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "advanced_settings.swap_caps_lock_and_left_control",
                    )
                    .to_owned(),
                    crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.caps_lock_sends_left_control_and_left_control_sends_caps_lock")
                    .to_owned(),
                ),
                1 => (
                    1,
                    crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.treat_caps_lock_as_control")
                    .to_owned(),
                    crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.caps_lock_sends_control_without_swapping_left_control")
                    .to_owned(),
                ),
                2 => (
                    2,
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Поменять Left Alt и {gui}")
                    } else {
                        format!("Swap Left Alt and {gui}")
                    },
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Left Alt отправляет {gui}, а Left {gui} — Alt")
                    } else {
                        format!("Left Alt sends {gui} and Left {gui} sends Alt")
                    },
                ),
                3 => (
                    3,
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Поменять Right Alt и {gui}")
                    } else {
                        format!("Swap Right Alt and {gui}")
                    },
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Right Alt отправляет {gui}, а Right {gui} — Alt")
                    } else {
                        format!("Right Alt sends {gui} and Right {gui} sends Alt")
                    },
                ),
                4 => (
                    4,
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Отключить клавиши {gui}")
                    } else {
                        format!("Disable {gui} keys")
                    },
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Игнорировать обе клавиши {gui}, пока опция включена")
                    } else {
                        format!("Ignore both {gui} keys while this option is enabled")
                    },
                ),
                5 => (
                    5,
                    crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.swap_grave_and_escape")
                    .to_owned(),
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "advanced_settings.grave_sends_escape_and_escape_sends_grave",
                    )
                    .to_owned(),
                ),
                6 => (
                    6,
                    crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.swap_backslash_and_backspace")
                    .to_owned(),
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "advanced_settings.backslash_sends_backspace_and_backspace_sends_backslash",
                    )
                    .to_owned(),
                ),
                7 => (
                    7,
                    crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.enable_n_key_rollover")
                    .to_owned(),
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "advanced_settings.allow_more_simultaneous_key_presses_when_the_keyboard_supports_it",
                    )
                    .to_owned(),
                ),
                8 => (
                    8,
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Поменять Left Control и {gui}")
                    } else {
                        format!("Swap Left Control and {gui}")
                    },
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Left Control отправляет {gui}, а Left {gui} — Control")
                    } else {
                        format!("Left Control sends {gui} and Left {gui} sends Control")
                    },
                ),
                9 => (
                    9,
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Поменять Right Control и {gui}")
                    } else {
                        format!("Swap Right Control and {gui}")
                    },
                    if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                        format!("Right Control отправляет {gui}, а Right {gui} — Control")
                    } else {
                        format!("Right Control sends {gui} and Right {gui} sends Control")
                    },
                ),
                _ => continue,
            };
            let mut value = self.magic_settings.bit(bit);
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                crate::ui_style::SettingsListRowLayout::new(
                    content_width,
                    row_height,
                    switch_width,
                ),
                label.as_str(),
                true,
                if suppress_tooltips {
                    None
                } else {
                    Some(tooltip.as_str())
                },
                |ui| {
                    let resp = crate::ui_style::settings_switch_sized_stable(
                        ui,
                        ("magic_settings", bit),
                        &mut value,
                        switch_size,
                    );
                    if resp.changed() {
                        self.magic_settings.set_bit(bit, value);
                        self.write_magic_settings();
                    }
                },
            );
        }
    }

    fn write_magic_settings(&mut self) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        if let Err(e) = hid.set_qmk_setting_u16(21, self.magic_settings.bits) {
            self.status_msg = format!("Failed to save Magic settings: {}", e);
            log::warn!("set_qmk_setting_u16(magic qsid 21) failed: {e}");
        }
    }
}
