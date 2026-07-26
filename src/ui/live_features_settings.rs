use super::*;

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    fn selected_live_features_path_and_supported_mode(
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
            mode = Self::qmk_hid_host_supported_mode_for(layout);
        }
        if Self::device_uses_automatic_display_host_data(selected) {
            mode.time = true;
            mode.volume = true;
            mode.media = true;
        }

        (!mode.is_empty()).then_some((selected.path.clone(), mode))
    }

    #[cfg(target_arch = "wasm32")]
    fn selected_live_features_path_and_supported_mode(
        &self,
    ) -> Option<(String, crate::qmk_hid_host::HostDataMode)> {
        None
    }

    pub(super) fn live_features_available_for_selected_device(&self) -> bool {
        self.selected_live_features_path_and_supported_mode()
            .is_some()
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

    fn draw_layout_sync_row(
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        enabled: &mut bool,
        check: crate::qmk_hid_host::FeatureCheck,
    ) -> bool {
        let before = *enabled;
        let dark = ui.visuals().dark_mode;
        let status_color = if check.ok {
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
        let switch_size = metrics.size(46.0, 24.0);
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            metrics.settings_row_content_width(),
            metrics.settings_row_height(),
            crate::i18n::tr_catalog(lang, "live_features.layout_sync"),
            true,
            Some(crate::i18n::tr_catalog(
                lang,
                "live_features.layout_sync_tooltip",
            )),
            metrics.settings_control_width(),
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let _ = crate::ui_style::settings_switch_sized_stable(
                        ui,
                        "live_features_layout_sync_enabled",
                        enabled,
                        switch_size,
                    );
                    ui.add_space(metrics.value(8.0));
                    ui.label(
                        RichText::new(if check.ok {
                            crate::i18n::tr_catalog(lang, "live_features.ready")
                        } else {
                            crate::i18n::tr_catalog(lang, "live_features.needs_setup")
                        })
                        .size(metrics.value(12.0))
                        .color(status_color),
                    );
                });
            },
        );
        before != *enabled
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
        let supported_path_and_mode = self.selected_live_features_path_and_supported_mode();

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

                let Some((_, supported_mode)) = supported_path_and_mode else {
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
                Self::draw_live_feature_row(
                    ui,
                    metrics,
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "live_features.entropy_background",
                    ),
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "live_features.required",
                    ),
                    true,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "live_features.keep_entropy_running_in_the_background_for_live_firmware_data",
                    )),
                );
                if supported_mode.layout {
                    let layout = crate::qmk_hid_host::layout_check();
                    let mut layout_sync_enabled = self.app_settings.layout_sync_enabled;
                    if Self::draw_layout_sync_row(
                        ui,
                        metrics,
                        lang,
                        &mut layout_sync_enabled,
                        layout,
                    ) {
                        self.app_settings.layout_sync_enabled = layout_sync_enabled;
                        save_app_settings(&self.app_settings);
                        self.sync_qmk_hid_host_bridges();
                    }
                }
                if supported_mode.time {
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
                }
                if supported_mode.volume {
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
                if supported_mode.media {
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn collect_text(shape: &egui::Shape, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text_shape) => {
                text.push(text_shape.galley.job.text.clone());
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, text);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn supported_qmk_live_features_are_described_with_a_static_preset() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.app_settings.language = crate::i18n::Language::English;
        app.device_manager
            .replace_devices(vec![crate::device::Device {
                name: "Test QMK Keyboard".to_owned(),
                vendor_id: 0x1209,
                product_id: 0x2327,
                manufacturer: "Entropy".to_owned(),
                serial_number: "test".to_owned(),
                bus_type: "Usb".to_owned(),
                path: "test-live-features".to_owned(),
                firmware: FirmwareProtocol::Vial,
            }]);
        app.selected_device = Some(0);
        app.layout = Some(KeyboardLayout {
            name: "Test".to_owned(),
            rows: 0,
            cols: 0,
            keys: Vec::new(),
            encoders: Vec::new(),
            layers: Vec::new(),
            encoder_layers: Vec::new(),
            layer_names: Vec::new(),
            custom_keycodes: vec![crate::keyboard::CustomKeycode {
                name: "LG_SYNC".to_owned(),
                label: "RuEn\nSync".to_owned(),
                title: "Sync language".to_owned(),
            }],
            layout_options: vec![LayoutOption {
                label: "OLED Master".to_owned(),
                choices: vec![
                    "Status (classic)".to_owned(),
                    "Clock & Volume (qmk-hid-host)".to_owned(),
                    "Media (qmk-hid-host)".to_owned(),
                    "Disabled".to_owned(),
                ],
            }],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        });
        app.layout_options_value = Some(0);
        app.app_settings.layout_sync_enabled = false;

        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        ));
        let output = ctx.run_ui(input, |ui| {
            app.draw_live_features_settings_page(ui, ui.max_rect());
        });
        let mut text = Vec::new();
        for clipped_shape in &output.shapes {
            collect_text(&clipped_shape.shape, &mut text);
        }

        assert!(text.iter().any(|value| value == "Time sync"));
        assert!(text.iter().any(|value| value == "Volume sync"));
        assert!(text.iter().any(|value| value == "Media info"));
        assert!(text.iter().any(|value| value == "Layout sync"));
        assert!(text.iter().any(|value| value == "Entropy background"));
        assert!(text.iter().any(|value| value == "required"));
        assert!(!text
            .iter()
            .any(|value| value == "Live Features are not active for this device"));
    }
}
