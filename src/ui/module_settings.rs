use super::*;

#[derive(Clone, Copy)]
enum ModuleSettingsRow {
    SideSelector,
    Section(usize),
    Field { group_idx: usize, field_idx: usize },
}

const MODULE_SETTING_WRITEBACK_DELAY: std::time::Duration = std::time::Duration::from_millis(20);

impl EntropyApp {
    fn module_setting_display_title<'a>(
        &self,
        group_kind: ModuleSettingsGroupKind,
        title: &'a str,
    ) -> &'a str {
        if !matches!(
            group_kind,
            ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
        ) {
            return title;
        }
        title
            .strip_prefix("Left ")
            .or_else(|| title.strip_prefix("Right "))
            .unwrap_or(title)
    }

    fn module_setting_label(&self, group_kind: ModuleSettingsGroupKind, title: &str) -> String {
        let lang = self.app_settings.language;
        let display_title = self.module_setting_display_title(group_kind, title);
        match display_title.to_ascii_lowercase().as_str() {
            "module" => crate::i18n::tr_catalog(lang, "modules_settings.module").to_owned(),
            "mode" => crate::i18n::tr_catalog(lang, "modules_settings.mode").to_owned(),
            "ball axis" => crate::i18n::tr_catalog(lang, "modules_settings.ball_axis").to_owned(),
            "touch axis" => crate::i18n::tr_catalog(lang, "modules_settings.touch_axis").to_owned(),
            "ball dpi" => crate::i18n::tr_catalog(lang, "modules_settings.ball_dpi").to_owned(),
            "touch dpi" => crate::i18n::tr_catalog(lang, "modules_settings.touch_dpi").to_owned(),
            "scroll sens" => {
                crate::i18n::tr_catalog(lang, "modules_settings.scroll_sens").to_owned()
            }
            "sniper sens" => {
                crate::i18n::tr_catalog(lang, "modules_settings.sniper_sens").to_owned()
            }
            "text sens" => crate::i18n::tr_catalog(lang, "modules_settings.text_sens").to_owned(),
            "touch gestures" => {
                crate::i18n::tr_catalog(lang, "modules_settings.touch_gestures").to_owned()
            }
            "invert scroll" => {
                crate::i18n::tr_catalog(lang, "modules_settings.invert_scroll").to_owned()
            }
            "invert text" => {
                crate::i18n::tr_catalog(lang, "modules_settings.invert_text").to_owned()
            }
            "acceleration" => {
                crate::i18n::tr_catalog(lang, "modules_settings.acceleration").to_owned()
            }
            title => crate::i18n::tr_text(lang, title),
        }
    }

    fn module_setting_tooltip(
        &self,
        group_kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> String {
        let lang = self.app_settings.language;
        let display_title = self.module_setting_display_title(group_kind, &field.title);
        let key = match display_title.to_ascii_lowercase().as_str() {
            "module" => "modules_settings.module_tooltip",
            "mode" => "modules_settings.mode_tooltip",
            "ball axis" => "modules_settings.ball_axis_tooltip",
            "touch axis" => "modules_settings.touch_axis_tooltip",
            "ball dpi" => "modules_settings.ball_dpi_tooltip",
            "touch dpi" => "modules_settings.touch_dpi_tooltip",
            "scroll sens" => "modules_settings.scroll_sens_tooltip",
            "sniper sens" => "modules_settings.sniper_sens_tooltip",
            "text sens" => "modules_settings.text_sens_tooltip",
            "touch gestures" => "modules_settings.touch_gestures_tooltip",
            "invert scroll" => "modules_settings.invert_scroll_tooltip",
            "invert text" => "modules_settings.invert_text_tooltip",
            "acceleration" => "modules_settings.acceleration_tooltip",
            "sticky mode" => "modules_settings.sticky_mode_tooltip",
            "led blinks" => "modules_settings.led_blinks_tooltip",
            "auto layer in normal" => "modules_settings.auto_layer_normal_tooltip",
            "auto layer" => "modules_settings.auto_layer_tooltip",
            "auto layer in sniper" => "modules_settings.auto_layer_sniper_tooltip",
            "auto layer in scroll" => "modules_settings.auto_layer_scroll_tooltip",
            "auto layer in text" => "modules_settings.auto_layer_text_tooltip",
            "auto layer timeout" => "modules_settings.auto_layer_timeout_tooltip",
            _ => "modules_settings.generic_tooltip",
        };
        let field_label = self.module_setting_label(group_kind, &field.title);
        crate::i18n::tr_catalog_format(lang, key, &[("field", field_label.as_str())])
    }

    fn module_setting_transport_value(field: &ModuleSettingField, value: u16) -> u16 {
        if field.width > 1 {
            value
        } else {
            value.min(u8::MAX as u16)
        }
    }

    fn write_module_setting_value(
        &mut self,
        group_idx: usize,
        field: &ModuleSettingField,
        value: u16,
    ) {
        let group_title = self
            .module_settings
            .groups
            .get(group_idx)
            .map(|group| group.title.clone())
            .unwrap_or_else(|| "Modules".to_owned());
        let field_title = field.title.clone();
        let old_value = self.module_settings.value(field.qsid);
        let requested = Self::module_setting_transport_value(field, value);

        let Some(hid) = self.hid_device.as_ref() else {
            self.status_msg = format!(
                "Failed to save module setting (qsid {}): device is not connected",
                field.qsid
            );
            log::warn!(
                "module setting write skipped: group={group_title:?} field={field_title:?} qsid={} old={} requested={} readback=unavailable error=device not connected",
                field.qsid,
                old_value,
                requested,
            );
            return;
        };

        let result = self.module_settings.write_verified_value(
            field.qsid,
            requested,
            || {
                if field.width > 1 {
                    hid.set_qmk_setting_u16(field.qsid, requested)
                } else {
                    hid.set_qmk_setting_u8(field.qsid, requested as u8)
                }
                .map_err(|error| error.to_string())
            },
            || {
                std::thread::sleep(MODULE_SETTING_WRITEBACK_DELAY);
                if field.width > 1 {
                    hid.get_qmk_setting_u16(field.qsid)
                } else {
                    hid.get_qmk_setting_u8(field.qsid)
                        .map(|readback| readback as u16)
                }
                .map_err(|error| error.to_string())
            },
        );

        if let Err(error) = result {
            let readback = match &error {
                ModuleSettingWritebackError::ReadbackMismatch { actual, .. } => actual.to_string(),
                _ => "unavailable".to_owned(),
            };
            self.status_msg = format!(
                "Failed to save module setting {} (qsid {}): {}",
                field_title, field.qsid, error
            );
            log::warn!(
                "module setting writeback failed: group={group_title:?} field={field_title:?} qsid={} old={} requested={} readback={} error={}",
                field.qsid,
                old_value,
                requested,
                readback,
                error,
            );
        }
    }

    fn draw_module_settings_field_row(
        &mut self,
        ui: &mut egui::Ui,
        group_idx: usize,
        field_idx: usize,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return;
        };
        let group_kind = group.kind;
        let Some(field) = group.fields.get(field_idx).cloned() else {
            return;
        };
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let dark = ui.visuals().dark_mode;
        let label = self.module_setting_label(group_kind, &field.title);
        let tooltip = if suppress_tooltips {
            None
        } else {
            Some(self.module_setting_tooltip(group_kind, &field))
        };
        let raw_value = self.module_settings.value(field.qsid);
        match field.kind {
            ModuleSettingKind::Boolean => {
                let switch_width = metrics.value(46.0);
                let switch_size = metrics.size(46.0, 24.0);
                let mask = 1u16 << field.bit;
                let mut checked = raw_value & mask != 0;
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    switch_width,
                    |ui| {
                        let resp = crate::ui_style::settings_switch_sized_stable(
                            ui,
                            ("module_settings", group_idx, field.qsid, field.bit),
                            &mut checked,
                            switch_size,
                        );
                        if resp.changed() {
                            let new_value = if checked {
                                raw_value | mask
                            } else {
                                raw_value & !mask
                            };
                            self.write_module_setting_value(group_idx, &field, new_value);
                        }
                    },
                );
            }
            ModuleSettingKind::Integer => {
                let field_width = metrics.value(86.0);
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    field_width,
                    |ui| {
                        let edit_id = egui::Id::new(("module_setting_edit", group_idx, field.qsid));
                        let current = raw_value.clamp(field.min, field.max);
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
                            metrics.settings_control_height(),
                            "",
                            5,
                            egui::Align::Center,
                        );
                        let commit = resp.lost_focus()
                            || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if commit {
                            match text.trim().parse::<u16>() {
                                Ok(value) => {
                                    let value = value.clamp(field.min, field.max);
                                    if value != raw_value {
                                        self.write_module_setting_value(group_idx, &field, value);
                                    }
                                    text = value.to_string();
                                }
                                Err(_) => text = current.to_string(),
                            }
                        }
                        ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                    },
                );
            }
            ModuleSettingKind::Select => {
                let dropdown_width = metrics.value(120.0);
                let selected_idx = (raw_value as usize).min(field.variants.len().saturating_sub(1));
                let variants = field
                    .variants
                    .iter()
                    .map(|variant| crate::i18n::tr_text(self.app_settings.language, variant))
                    .collect::<Vec<_>>();
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    dropdown_width,
                    |ui| {
                        let dropdown_id = ui.make_persistent_id((
                            "module_setting_dropdown",
                            group_idx,
                            field.qsid,
                        ));
                        let (_, picked) = Self::draw_touchpad_select_control(
                            ui,
                            dark,
                            dropdown_id,
                            selected_idx,
                            &variants,
                            dropdown_width,
                        );
                        if let Some(picked) = picked {
                            self.write_module_setting_value(group_idx, &field, picked as u16);
                        }
                    },
                );
            }
        }
    }

    fn module_settings_rows(&self) -> Vec<ModuleSettingsRow> {
        let mut rows = Vec::new();
        let side_groups = self.module_settings_side_group_indices();
        let selected_side_group = self.module_settings.selected_module_group();
        if side_groups.len() > 1 {
            rows.push(ModuleSettingsRow::SideSelector);
        }
        if let Some(group_idx) = selected_side_group {
            rows.extend(self.module_settings_field_rows(group_idx));
        }
        for (group_idx, group) in self.module_settings.groups.iter().enumerate() {
            if matches!(
                group.kind,
                ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
            ) {
                continue;
            }
            rows.push(ModuleSettingsRow::Section(group_idx));
            rows.extend(self.module_settings_field_rows(group_idx));
        }
        rows
    }

    fn module_settings_field_rows(&self, group_idx: usize) -> Vec<ModuleSettingsRow> {
        self.module_settings
            .groups
            .get(group_idx)
            .into_iter()
            .flat_map(move |group| {
                (0..group.fields.len()).map(move |field_idx| ModuleSettingsRow::Field {
                    group_idx,
                    field_idx,
                })
            })
            .collect()
    }

    fn module_settings_side_group_indices(&self) -> Vec<usize> {
        self.module_settings
            .groups
            .iter()
            .enumerate()
            .filter_map(|(idx, group)| {
                matches!(
                    group.kind,
                    ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
                )
                .then_some(idx)
            })
            .collect()
    }

    fn module_settings_group_label(&self, group: &ModuleSettingsGroup) -> String {
        let lang = self.app_settings.language;
        match group.kind {
            ModuleSettingsGroupKind::Left => {
                crate::i18n::tr_catalog(lang, "modules_settings.left_half").to_owned()
            }
            ModuleSettingsGroupKind::Right => {
                crate::i18n::tr_catalog(lang, "modules_settings.right_half").to_owned()
            }
            ModuleSettingsGroupKind::AutoLayer => {
                crate::i18n::tr_catalog(lang, "modules_settings.auto_layer_section").to_owned()
            }
            ModuleSettingsGroupKind::Other => crate::i18n::tr_text(lang, &group.title),
        }
    }

    fn draw_module_settings_section(
        &self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        group_idx: usize,
    ) {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return;
        };
        let dark = ui.visuals().dark_mode;
        let title = self.module_settings_group_label(group);
        let (row_rect, _) =
            ui.allocate_exact_size(egui::vec2(content_width, row_height), egui::Sense::hover());
        let separator =
            crate::ui_style::border_color(dark).gamma_multiply(if dark { 0.72 } else { 0.9 });
        ui.painter().line_segment(
            [row_rect.left_bottom(), row_rect.right_bottom()],
            egui::Stroke::new(1.0, separator),
        );
        ui.painter().text(
            row_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.5),
            app_muted_text(dark),
        );
    }

    fn draw_module_settings_side_selector(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let side_groups = self.module_settings_side_group_indices();
        if side_groups.len() <= 1 {
            return;
        }
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let dark = ui.visuals().dark_mode;
        let lang = self.app_settings.language;
        let selected_group = self
            .module_settings
            .selected_module_group()
            .unwrap_or(side_groups[0]);
        let selected_idx = side_groups
            .iter()
            .position(|group_idx| *group_idx == selected_group)
            .unwrap_or(0);
        let labels = side_groups
            .iter()
            .filter_map(|group_idx| self.module_settings.groups.get(*group_idx))
            .map(|group| self.module_settings_group_label(group))
            .collect::<Vec<_>>();
        let dropdown_width = metrics.value(156.0);
        let tooltip = (!suppress_tooltips).then(|| {
            crate::i18n::tr_catalog(lang, "modules_settings.module_side_tooltip").to_owned()
        });
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "modules_settings.module_side"),
            true,
            tooltip.as_deref(),
            dropdown_width,
            |ui| {
                let dropdown_id = ui.make_persistent_id("module_settings_side_dropdown");
                let (_, picked) = Self::draw_touchpad_select_control(
                    ui,
                    dark,
                    dropdown_id,
                    selected_idx,
                    &labels,
                    dropdown_width,
                );
                if let Some(picked) = picked.and_then(|idx| side_groups.get(idx).copied()) {
                    self.module_settings.set_selected_module_group(picked);
                }
            },
        );
    }

    fn draw_module_settings_row_entry(
        &mut self,
        ui: &mut egui::Ui,
        row: ModuleSettingsRow,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        match row {
            ModuleSettingsRow::SideSelector => self.draw_module_settings_side_selector(
                ui,
                content_width,
                row_height,
                suppress_tooltips,
            ),
            ModuleSettingsRow::Section(group_idx) => {
                self.draw_module_settings_section(ui, content_width, row_height, group_idx)
            }
            ModuleSettingsRow::Field {
                group_idx,
                field_idx,
            } => self.draw_module_settings_field_row(
                ui,
                group_idx,
                field_idx,
                content_width,
                row_height,
                suppress_tooltips,
            ),
        }
    }

    pub(super) fn draw_module_settings_page(
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
                    RichText::new(crate::i18n::tr_catalog(lang, "modules_settings.title"))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        "modules_settings.description",
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.module_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "modules_settings.unavailable"),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::QmkSettingsEnableHint,
                        )),
                    );
                    return;
                }

                let rows = self.module_settings_rows();
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "module_settings",
                    metrics,
                    rows.len(),
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row_idx in list.first_visible_row..list.last_visible_row {
                        if let Some(row) = rows.get(row_idx).copied() {
                            self.draw_module_settings_row_entry(
                                ui,
                                row,
                                list.row_content_width,
                                list.row_height,
                                list.suppress_tooltips,
                            );
                        }
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
            });
        });
    }
}
