use super::*;

impl EntropyApp {
    pub(super) fn draw_mouse_keys_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::MouseKeysTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::MouseKeysDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.mouse_keys_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::MouseKeysUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::MouseKeysEnableHint)),
                    );
                    return;
                }

                const TOTAL_MOUSE_KEY_ROWS: usize = 9;
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "mouse_keys_settings",
                    metrics,
                    TOTAL_MOUSE_KEY_ROWS,
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_mouse_keys_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
                        metrics.value(86.0),
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

    fn write_mouse_keys_setting(&mut self, qsid: u16, value: u16) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let value = value.min(u8::MAX as u16) as u8;
        if let Err(e) = hid.set_qmk_setting_u8(qsid, value) {
            self.status_msg = format!("Failed to save Mouse keys setting (qsid {qsid}): {}", e);
            log::warn!("set_qmk_setting_u8(mouse_keys qsid {qsid}) failed: {e}");
        }
    }

    fn draw_mouse_keys_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        content_width: f32,
        row_height: f32,
        field_width: f32,
        suppress_tooltips: bool,
    ) {
        // Limits match Vial GUI qmk_settings.json.
        let lang = self.app_settings.language;
        let rows: [(u16, &'static str, &'static str, SettingsFieldUnit, u32); 9] = [
            (
                9,
                "mouse_keys_settings.delay_label",
                "mouse_keys_settings.delay_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                10,
                "mouse_keys_settings.interval_label",
                "mouse_keys_settings.interval_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                11,
                "mouse_keys_settings.move_delta_label",
                "mouse_keys_settings.move_delta_tooltip",
                SettingsFieldUnit::CursorSteps,
                1000,
            ),
            (
                12,
                "mouse_keys_settings.max_speed_label",
                "mouse_keys_settings.max_speed_tooltip",
                SettingsFieldUnit::SpeedSteps,
                1000,
            ),
            (
                13,
                "mouse_keys_settings.time_to_max_label",
                "mouse_keys_settings.time_to_max_tooltip",
                SettingsFieldUnit::Milliseconds,
                1000,
            ),
            (
                14,
                "mouse_keys_settings.wheel_delay_label",
                "mouse_keys_settings.wheel_delay_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                15,
                "mouse_keys_settings.wheel_interval_label",
                "mouse_keys_settings.wheel_interval_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                16,
                "mouse_keys_settings.wheel_max_speed_label",
                "mouse_keys_settings.wheel_max_speed_tooltip",
                SettingsFieldUnit::SpeedSteps,
                1000,
            ),
            (
                17,
                "mouse_keys_settings.wheel_time_to_max_label",
                "mouse_keys_settings.wheel_time_to_max_tooltip",
                SettingsFieldUnit::Milliseconds,
                1000,
            ),
        ];
        let control_height = (row_height / 54.0).clamp(1.0, 1.12) * 32.0;

        for row_idx in row_range {
            let Some((qsid, label, tooltip, unit, max)) = rows.get(row_idx).copied() else {
                continue;
            };
            let current = match qsid {
                9 => self.mouse_keys_settings.delay,
                10 => self.mouse_keys_settings.interval,
                11 => self.mouse_keys_settings.move_delta,
                12 => self.mouse_keys_settings.max_speed,
                13 => self.mouse_keys_settings.time_to_max,
                14 => self.mouse_keys_settings.wheel_delay,
                15 => self.mouse_keys_settings.wheel_interval,
                16 => self.mouse_keys_settings.wheel_max_speed,
                17 => self.mouse_keys_settings.wheel_time_to_max,
                _ => continue,
            };

            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                crate::ui_style::SettingsListRowLayout::new(content_width, row_height, field_width),
                crate::i18n::tr_catalog(lang, label),
                true,
                if suppress_tooltips {
                    None
                } else {
                    Some(crate::i18n::tr_catalog(lang, tooltip))
                },
                |ui| {
                    let edit_id = egui::Id::new(("mouse_keys_edit", qsid));
                    let mut text = ui.ctx().data_mut(|d| {
                        d.get_temp::<String>(edit_id)
                            .unwrap_or_else(|| current.to_string())
                    });
                    if text.parse::<u16>().ok() != Some(current)
                        && !ui.memory(|m| m.has_focus(edit_id))
                    {
                        text = current.to_string();
                    }

                    let resp = crate::ui_style::modern_text_field_sized(
                        ui,
                        edit_id,
                        &mut text,
                        field_width,
                        control_height,
                        "",
                        5,
                        egui::Align::RIGHT,
                    );
                    let resp = settings_field_unit_tooltip(resp, lang, suppress_tooltips, unit);

                    if resp.changed() {
                        let filtered: String =
                            text.chars().filter(|c: &char| c.is_ascii_digit()).collect();
                        let parsed = filtered.parse::<u32>().unwrap_or(0).min(max);
                        let new_value = parsed as u16;
                        if new_value != current {
                            match qsid {
                                9 => self.mouse_keys_settings.delay = new_value,
                                10 => self.mouse_keys_settings.interval = new_value,
                                11 => self.mouse_keys_settings.move_delta = new_value,
                                12 => self.mouse_keys_settings.max_speed = new_value,
                                13 => self.mouse_keys_settings.time_to_max = new_value,
                                14 => self.mouse_keys_settings.wheel_delay = new_value,
                                15 => self.mouse_keys_settings.wheel_interval = new_value,
                                16 => self.mouse_keys_settings.wheel_max_speed = new_value,
                                17 => self.mouse_keys_settings.wheel_time_to_max = new_value,
                                _ => {}
                            }
                            self.write_mouse_keys_setting(qsid, new_value);
                        }
                        text = filtered;
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                },
            );
        }
    }
}
