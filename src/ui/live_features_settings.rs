use super::*;

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    fn selected_live_features_path_and_mode(
        &self,
    ) -> Option<(String, crate::qmk_hid_host::HostDataMode)> {
        let selected = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))?;
        if selected.firmware != FirmwareProtocol::Vial {
            return None;
        }

        let mut mode = crate::qmk_hid_host::HostDataMode::default();
        if let Some(layout) = self.layout.as_ref() {
            mode = Self::qmk_hid_host_mode_for(layout, self.layout_options_value);
        }
        if Self::device_uses_automatic_display_host_data(selected) {
            mode.clock_volume = true;
            mode.media = true;
        }

        (!mode.is_empty()).then_some((selected.path.clone(), mode))
    }

    #[cfg(target_arch = "wasm32")]
    fn selected_live_features_path_and_mode(
        &self,
    ) -> Option<(String, crate::qmk_hid_host::HostDataMode)> {
        None
    }

    pub(super) fn live_features_available_for_selected_device(&self) -> bool {
        self.selected_live_features_path_and_mode().is_some()
    }

    fn draw_live_feature_row(
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        label: &str,
        status: &str,
        ok: bool,
        hint: Option<&str>,
    ) {
        let dark = ui.visuals().dark_mode;
        let status_color = if ok {
            if dark {
                Color32::from_rgb(205, 210, 205)
            } else {
                Color32::from_rgb(65, 70, 65)
            }
        } else if dark {
            Color32::from_rgb(230, 188, 150)
        } else {
            Color32::from_rgb(150, 82, 44)
        };
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            metrics.settings_row_content_width(),
            metrics.settings_row_height(),
            label,
            true,
            hint,
            metrics.settings_control_width(),
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(status)
                            .size(metrics.value(12.0))
                            .color(status_color),
                    );
                });
            },
        );
    }

    pub(super) fn draw_live_features_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let content_width = metrics.settings_content_width();
        let path_and_mode = self.selected_live_features_path_and_mode();
        let bridge_active = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                path_and_mode
                    .as_ref()
                    .map(|(path, _)| self.qmk_hid_hosts.contains_key(path))
                    .unwrap_or(false)
            }
            #[cfg(target_arch = "wasm32")]
            {
                false
            }
        };

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(crate::i18n::tr(
                        self.app_settings.language,
                        crate::i18n::Key::LiveFeaturesTitle,
                    ))
                    .size(metrics.value(18.0))
                    .strong(),
                );
                ui.add_space(metrics.value(6.0));
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::LiveFeaturesDescription,
                    ))
                    .size(metrics.value(13.0))
                    .color(app_muted_text(dark)),
                );
                ui.add_space(metrics.value(24.0));

                let Some((_, mode)) = path_and_mode else {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::LiveFeaturesInactive),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::LiveFeaturesSelectHint,
                        )),
                    );
                    return;
                };

                ui.set_width(content_width);
                let status = if bridge_active {
                    crate::i18n::tr_catalog(self.app_settings.language, "live_features.active")
                } else {
                    crate::i18n::tr_catalog(self.app_settings.language, "live_features.starting")
                };
                Self::draw_live_feature_row(
                    ui,
                    metrics,
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "live_features.entropy_background",
                    ),
                    status,
                    bridge_active,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "live_features.keep_entropy_running_in_the_background_for_live_firmware_data",
                    )),
                );
                if mode.clock_volume {
                    Self::draw_live_feature_row(
                        ui,
                        metrics,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "live_features.time_sync",
                        ),
                        crate::i18n::tr_catalog(self.app_settings.language, "live_features.ready"),
                        true,
                        Some(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "live_features.uses_the_local_system_clock",
                        )),
                    );
                    let volume = crate::qmk_hid_host::volume_check();
                    Self::draw_live_feature_row(
                        ui,
                        metrics,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "live_features.volume_sync",
                        ),
                        if volume.ok {
                            crate::i18n::tr_catalog(self.app_settings.language, volume.label)
                        } else {
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "live_features.needs_setup",
                            )
                        },
                        volume.ok,
                        Some(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            volume.hint,
                        )),
                    );
                }
                if mode.media {
                    let media = crate::qmk_hid_host::media_check();
                    Self::draw_live_feature_row(
                        ui,
                        metrics,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "live_features.media_info",
                        ),
                        if media.ok {
                            crate::i18n::tr_catalog(self.app_settings.language, media.label)
                        } else {
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "live_features.needs_setup",
                            )
                        },
                        media.ok,
                        Some(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            media.hint,
                        )),
                    );
                }

                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::LiveFeaturesReadyNote,
                    ))
                    .size(metrics.value(12.0))
                    .color(app_muted_text(dark)),
                );
            });
        });
    }
}
