use super::*;

fn plural_catalog_key(
    value: u32,
    one: &'static str,
    few: &'static str,
    many: &'static str,
) -> &'static str {
    let last_two = value % 100;
    let last = value % 10;
    if last == 1 && last_two != 11 {
        one
    } else if (2..=4).contains(&last) && !(12..=14).contains(&last_two) {
        few
    } else {
        many
    }
}

fn bluetooth_timeout_variant_label(language: crate::i18n::Language, variant: &str) -> String {
    let normalized = variant.trim().to_ascii_lowercase();
    if normalized == "never" {
        return crate::i18n::tr_catalog(language, "bluetooth_settings.timeout_never").to_owned();
    }

    let mut parts = normalized.split_whitespace();
    let value = parts.next().and_then(|value| value.parse::<u32>().ok());
    let unit = parts.next();
    if parts.next().is_none() {
        if let (Some(value), Some(unit)) = (value, unit) {
            let key = match unit {
                "minute" | "minutes" => Some(plural_catalog_key(
                    value,
                    "bluetooth_settings.timeout_minutes_one",
                    "bluetooth_settings.timeout_minutes_few",
                    "bluetooth_settings.timeout_minutes_many",
                )),
                "hour" | "hours" => Some(plural_catalog_key(
                    value,
                    "bluetooth_settings.timeout_hours_one",
                    "bluetooth_settings.timeout_hours_few",
                    "bluetooth_settings.timeout_hours_many",
                )),
                _ => None,
            };
            if let Some(key) = key {
                let value_text = value.to_string();
                return crate::i18n::tr_catalog_format(
                    language,
                    key,
                    &[("value", value_text.as_str())],
                );
            }
        }
    }

    crate::i18n::tr_text(language, variant)
}

#[derive(Clone, Copy)]
enum BluetoothRow {
    SleepTimeout,
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
        if self.bluetooth_settings.sleep_timeout.is_some() {
            rows.push(BluetoothRow::SleepTimeout);
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

        for row_idx in row_range {
            let Some(row) = rows.get(row_idx).copied() else {
                continue;
            };
            match row {
                BluetoothRow::SleepTimeout => {
                    let Some(setting) = self.bluetooth_settings.sleep_timeout.clone() else {
                        continue;
                    };
                    let variants = setting
                        .variants
                        .iter()
                        .map(|variant| {
                            bluetooth_timeout_variant_label(self.app_settings.language, variant)
                        })
                        .collect();
                    self.draw_bluetooth_select_row(
                        ui,
                        content_width,
                        row_height,
                        dropdown_width,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "bluetooth_settings.sleep_timeout",
                        )
                        .to_owned(),
                        if suppress_tooltips {
                            None
                        } else {
                            Some(
                                crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "bluetooth_settings.sleep_timeout_tooltip",
                                )
                                .to_owned(),
                            )
                        },
                        setting,
                        variants,
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
        if let Some(current) = &mut self.bluetooth_settings.sleep_timeout {
            if current.qsid == setting.qsid {
                current.value = value;
            }
        }
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
}

#[cfg(test)]
mod tests {
    use super::bluetooth_timeout_variant_label;
    use crate::i18n::Language;

    #[test]
    fn bluetooth_sleep_timeout_options_are_localized_in_russian() {
        for (source, expected) in [
            ("Never", "Никогда"),
            ("10 minutes", "10 минут"),
            ("45 minutes", "45 минут"),
            ("1 hour", "1 час"),
            ("2 hours", "2 часа"),
            ("5 hours", "5 часов"),
        ] {
            assert_eq!(
                bluetooth_timeout_variant_label(Language::Russian, source),
                expected
            );
        }
    }

    #[test]
    fn unknown_bluetooth_option_keeps_catalog_fallback() {
        assert_eq!(
            bluetooth_timeout_variant_label(Language::Russian, "Firmware default"),
            "Firmware default"
        );
    }
}
