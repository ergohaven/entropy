use super::*;

#[derive(Clone, Copy)]
enum BluetoothRow {
    ChargeIndicator,
    ProfileColor(usize),
}

impl EntropyApp {
    pub(super) fn draw_bluetooth_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let hid_ready = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.qmk_setting_transport_available()
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
                    RichText::new(crate::i18n::tr_catalog(lang, "bluetooth_settings.title"))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        "bluetooth_settings.description",
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.bluetooth_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "bluetooth_settings.unavailable"),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::QmkSettingsEnableHint,
                        )),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "bluetooth_settings.connect"),
                        None,
                    );
                    return;
                }

                let rows = self.bluetooth_rows();
                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "bluetooth_settings",
                    metrics,
                    rows.len(),
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_bluetooth_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        metrics,
                        list.suppress_tooltips,
                        &rows,
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

    fn bluetooth_rows(&self) -> Vec<BluetoothRow> {
        let mut rows = Vec::with_capacity(self.bluetooth_settings.row_count());
        if self.bluetooth_settings.charge_indicator.is_some() {
            rows.push(BluetoothRow::ChargeIndicator);
        }
        rows.extend(
            (0..self.bluetooth_settings.profile_colors.len()).map(BluetoothRow::ProfileColor),
        );
        rows
    }

    fn draw_bluetooth_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        metrics: crate::ui_style::ResponsiveMetrics,
        suppress_tooltips: bool,
        rows: &[BluetoothRow],
    ) {
        let content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let dropdown_width = metrics.value(156.0);
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);

        for row_idx in row_range {
            let Some(row) = rows.get(row_idx).copied() else {
                continue;
            };
            match row {
                BluetoothRow::ChargeIndicator => {
                    let Some(setting) = self.bluetooth_settings.charge_indicator else {
                        continue;
                    };
                    let mut enabled = setting.value;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "bluetooth_settings.charge_indicator",
                        ),
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "bluetooth_settings.charge_indicator_tooltip",
                            ))
                        },
                        switch_width,
                        |ui| {
                            let response = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                ("bluetooth_settings", "charge_indicator", setting.qsid),
                                &mut enabled,
                                switch_size,
                            );
                            if response.changed() {
                                self.write_bluetooth_charge_indicator(setting, enabled);
                            }
                        },
                    );
                }
                BluetoothRow::ProfileColor(idx) => {
                    let Some(profile) = self.bluetooth_settings.profile_colors.get(idx).cloned()
                    else {
                        continue;
                    };
                    let variants = self.bluetooth_variant_labels(&profile.setting.variants);
                    let profile_text = profile.profile.to_string();
                    self.draw_bluetooth_select_row(
                        ui,
                        content_width,
                        row_height,
                        dropdown_width,
                        crate::i18n::tr_catalog_format(
                            self.app_settings.language,
                            "bluetooth_settings.profile_color",
                            &[("profile", profile_text.as_str())],
                        ),
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog_format(
                                self.app_settings.language,
                                "bluetooth_settings.profile_color_tooltip",
                                &[("profile", profile_text.as_str())],
                            ))
                        },
                        profile.setting,
                        variants,
                    );
                }
            }
        }
    }

    fn bluetooth_variant_labels(&self, variants: &[String]) -> Vec<String> {
        variants
            .iter()
            .map(|variant| crate::i18n::tr_text(self.app_settings.language, variant))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_bluetooth_select_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        dropdown_width: f32,
        label: String,
        tooltip: Option<String>,
        setting: BluetoothSelectSetting,
        variants: Vec<String>,
    ) {
        let dark = ui.visuals().dark_mode;
        let selected_idx = (setting.value as usize).min(variants.len().saturating_sub(1));
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            label.as_str(),
            true,
            tooltip.as_deref(),
            dropdown_width,
            |ui| {
                let dropdown_id =
                    ui.make_persistent_id(("bluetooth_setting_dropdown", setting.qsid));
                let (_, picked) = Self::draw_touchpad_select_control(
                    ui,
                    dark,
                    dropdown_id,
                    selected_idx,
                    &variants,
                    dropdown_width,
                );
                if let Some(picked) = picked {
                    self.write_bluetooth_select_setting(&setting, picked as u16);
                }
            },
        );
    }

    fn write_bluetooth_select_setting(&mut self, setting: &BluetoothSelectSetting, value: u16) {
        let value = value.min(setting.variants.len().saturating_sub(1) as u16);
        if let Some(current) = self
            .bluetooth_settings
            .profile_colors
            .iter_mut()
            .find(|profile| profile.setting.qsid == setting.qsid)
        {
            current.setting.value = value;
        }

        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = if setting.width > 1 {
            hid.set_qmk_setting_u16(setting.qsid, value)
        } else {
            hid.set_qmk_setting_u8(setting.qsid, value.min(u8::MAX as u16) as u8)
        };
        if let Err(e) = result {
            self.status_msg = format!(
                "Failed to save Bluetooth setting (qsid {}): {}",
                setting.qsid, e
            );
            log::warn!(
                "set_qmk_setting(bluetooth qsid {}) failed: {e}",
                setting.qsid
            );
        }
    }

    fn write_bluetooth_charge_indicator(
        &mut self,
        setting: BluetoothBooleanSetting,
        enabled: bool,
    ) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        match hid.set_qmk_setting_u8(setting.qsid, u8::from(enabled)) {
            Ok(()) => {
                if let Some(current) = &mut self.bluetooth_settings.charge_indicator {
                    if current.qsid == setting.qsid {
                        current.value = enabled;
                    }
                }
            }
            Err(e) => {
                self.status_msg = format!(
                    "Failed to save Bluetooth setting (qsid {}): {}",
                    setting.qsid, e
                );
                log::warn!(
                    "set_qmk_setting(bluetooth charge indicator qsid {}) failed: {e}",
                    setting.qsid
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_state::{BluetoothBooleanSetting, BluetoothSettingsState};
    #[cfg(not(target_arch = "wasm32"))]
    use crate::app::vial_hid_task::{VialHidOperation, VialHidTaskStart};
    use crate::i18n::Language;

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
    fn charge_indicator_adds_one_firmware_gated_row() {
        let mut settings = BluetoothSettingsState::default();
        assert_eq!(settings.row_count(), 0);

        settings.charge_indicator = Some(BluetoothBooleanSetting {
            qsid: 331,
            value: true,
        });
        assert_eq!(settings.row_count(), 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn background_hid_work_keeps_bluetooth_settings_visible() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, _) = crate::hid::HidDevice::test_device();
        app.app_settings.language = Language::English;
        app.bluetooth_settings = BluetoothSettingsState {
            charge_indicator: Some(BluetoothBooleanSetting {
                qsid: 331,
                value: true,
            }),
            supported: true,
            ..BluetoothSettingsState::default()
        };
        app.hid_device = Some(hid);

        assert_eq!(
            app.start_vial_hid_operation(&ctx, VialHidOperation::BatteryRefresh),
            VialHidTaskStart::Started
        );
        assert!(app.hid_device.is_none());
        assert!(app.vial_hid_task_active());

        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        ));
        let output = ctx.run_ui(input, |ui| {
            app.draw_bluetooth_settings_page(ui, ui.max_rect());
        });
        let mut text = Vec::new();
        for clipped_shape in &output.shapes {
            collect_text(&clipped_shape.shape, &mut text);
        }

        assert!(text.iter().any(|value| value == "Charging indicator"));
        assert!(!text.iter().any(|value| {
            value == crate::i18n::tr_catalog(Language::English, "bluetooth_settings.connect")
        }));
    }
}
