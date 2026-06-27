use super::*;

impl EntropyApp {
    pub(super) fn draw_grave_escape_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
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
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::GraveEscapeTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::GraveEscapeDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.grave_escape_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::GraveEscapeUnavailable),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::GraveEscapeEnableHint,
                        )),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::GraveEscapeConnect),
                        None,
                    );
                    return;
                }

                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let content_width = metrics.settings_content_width();
                let row_height = metrics.settings_row_height();
                let row_content_width = metrics.settings_row_content_width();
                let switch_width = metrics.value(46.0);
                let switch_size = metrics.size(46.0, 24.0);
                let gui_name = crate::keycode::gui_mod_name();
                let rows: Vec<(u8, String, String)> = vec![
                    (
                        0,
                        crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.alt_forces_esc")
                            .to_string(),
                        crate::i18n::tr_catalog(self.app_settings.language,
                            "advanced_settings.when_alt_is_held_grave_escape_sends_esc_instead_of_grave_or_tilde",
                        )
                        .to_string(),
                    ),
                    (
                        1,
                        crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.control_forces_esc")
                            .to_string(),
                        crate::i18n::tr_catalog(self.app_settings.language,
                            "advanced_settings.when_control_is_held_grave_escape_sends_esc_instead_of_grave_or_tilde",
                        )
                        .to_string(),
                    ),
                    (
                        2,
                        if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                            format!("{gui_name} отправляет Esc")
                        } else {
                            format!("{gui_name} forces Esc")
                        },
                        if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                            format!("При удержании {gui_name} Grave Escape отправляет Esc вместо ` или ~")
                        } else {
                            format!(
                                "When {gui_name} is held, Grave Escape sends Esc instead of ` or ~"
                            )
                        },
                    ),
                    (
                        3,
                        crate::i18n::tr_catalog(self.app_settings.language, "advanced_settings.shift_forces_esc")
                            .to_string(),
                        crate::i18n::tr_catalog(self.app_settings.language,
                            "advanced_settings.when_shift_is_held_grave_escape_sends_esc_instead_of_grave_or_tilde",
                        )
                        .to_string(),
                    ),
                ];

                crate::ui_style::modal_content(
                    ui,
                    crate::ui_style::ModalLayout::new(content_width).with_top_padding(0.0),
                    |ui| {
                        for (bit, label, tooltip) in rows {
                            let mut value = self.grave_escape_settings.bit(bit);
                            crate::ui_style::settings_list_row_with_tooltip(
                                ui,
                                row_content_width,
                                row_height,
                                &label,
                                true,
                                Some(&tooltip),
                                switch_width,
                                |ui| {
                                    let resp = crate::ui_style::settings_switch_sized(
                                        ui,
                                        &mut value,
                                        switch_size,
                                    );
                                    if resp.changed() {
                                        self.grave_escape_settings.set_bit(bit, value);
                                        self.write_grave_escape_settings();
                                    }
                                },
                            );
                        }
                    },
                );
            });
        });
    }

    fn write_grave_escape_settings(&mut self) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        if let Err(e) = hid.set_qmk_setting_u8(1, self.grave_escape_settings.bits) {
            self.status_msg = format!("Failed to save Grave Escape settings: {}", e);
            log::warn!("set_qmk_setting_u8(grave_escape qsid 1) failed: {e}");
        }
    }
}
