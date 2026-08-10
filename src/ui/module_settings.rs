use super::*;

fn module_setting_catalog_keys(title: &str) -> Option<(&'static str, &'static str)> {
    match title.to_ascii_lowercase().as_str() {
        "module" => Some(("modules_settings.module", "modules_settings.module_tooltip")),
        "mode" => Some(("modules_settings.mode", "modules_settings.mode_tooltip")),
        "ball axis" => Some((
            "modules_settings.ball_axis",
            "modules_settings.ball_axis_tooltip",
        )),
        "touch axis" => Some((
            "modules_settings.touch_axis",
            "modules_settings.touch_axis_tooltip",
        )),
        "ball dpi" => Some((
            "modules_settings.ball_dpi",
            "modules_settings.ball_dpi_tooltip",
        )),
        "touch dpi" => Some((
            "modules_settings.touch_dpi",
            "modules_settings.touch_dpi_tooltip",
        )),
        "encoder interval" => Some((
            "modules_settings.encoder_interval",
            "modules_settings.encoder_interval_tooltip",
        )),
        "encoder steps" => Some((
            "modules_settings.encoder_steps",
            "modules_settings.encoder_steps_tooltip",
        )),
        "scroll sens" => Some((
            "modules_settings.scroll_sens",
            "modules_settings.scroll_sens_tooltip",
        )),
        "sniper sens" => Some((
            "modules_settings.sniper_sens",
            "modules_settings.sniper_sens_tooltip",
        )),
        "text sens" => Some((
            "modules_settings.text_sens",
            "modules_settings.text_sens_tooltip",
        )),
        "touch gestures" => Some((
            "modules_settings.touch_gestures",
            "modules_settings.touch_gestures_tooltip",
        )),
        "invert scroll" => Some((
            "modules_settings.invert_scroll",
            "modules_settings.invert_scroll_tooltip",
        )),
        "invert scroll vertical" => Some((
            "modules_settings.invert_scroll_vertical",
            "modules_settings.invert_scroll_vertical_tooltip",
        )),
        "invert scroll horizontal" => Some((
            "modules_settings.invert_scroll_horizontal",
            "modules_settings.invert_scroll_horizontal_tooltip",
        )),
        "invert text" => Some((
            "modules_settings.invert_text",
            "modules_settings.invert_text_tooltip",
        )),
        "invert text vertical" => Some((
            "modules_settings.invert_text_vertical",
            "modules_settings.invert_text_vertical_tooltip",
        )),
        "invert text horizontal" => Some((
            "modules_settings.invert_text_horizontal",
            "modules_settings.invert_text_horizontal_tooltip",
        )),
        "acceleration" => Some((
            "modules_settings.acceleration",
            "modules_settings.acceleration_tooltip",
        )),
        "sticky mode" => Some((
            "modules_settings.sticky_mode",
            "modules_settings.sticky_mode_tooltip",
        )),
        "led blinks" => Some((
            "modules_settings.led_blinks",
            "modules_settings.led_blinks_tooltip",
        )),
        "auto layer in normal" => Some((
            "modules_settings.auto_layer_in_normal",
            "modules_settings.auto_layer_normal_tooltip",
        )),
        "auto layer" => Some((
            "modules_settings.auto_layer",
            "modules_settings.auto_layer_tooltip",
        )),
        "auto layer in sniper" => Some((
            "modules_settings.auto_layer_in_sniper",
            "modules_settings.auto_layer_sniper_tooltip",
        )),
        "auto layer in scroll" => Some((
            "modules_settings.auto_layer_in_scroll",
            "modules_settings.auto_layer_scroll_tooltip",
        )),
        "auto layer in text" => Some((
            "modules_settings.auto_layer_in_text",
            "modules_settings.auto_layer_text_tooltip",
        )),
        "auto layer timeout" => Some((
            "modules_settings.auto_layer_timeout",
            "modules_settings.auto_layer_timeout_tooltip",
        )),
        "trackball enabled" => Some((
            "modules_settings.trackball_enabled",
            "modules_settings.trackball_enabled_tooltip",
        )),
        _ => None,
    }
}

fn module_setting_variant_label(language: crate::i18n::Language, variant: &str) -> String {
    let key = match variant.to_ascii_lowercase().as_str() {
        "none" => Some("modules_settings.none"),
        "normal" => Some("modules_settings.normal"),
        "sniper" => Some("modules_settings.sniper"),
        "scroll" => Some("modules_settings.scroll"),
        "text" => Some("modules_settings.text"),
        "trackball" => Some("modules_settings.trackball"),
        "touchpad" => Some("modules_settings.touchpad"),
        "ball" => Some("modules_settings.ball"),
        "touch" => Some("modules_settings.touch"),
        "encoder" => Some("modules_settings.encoder"),
        _ => None,
    };
    key.map(|key| crate::i18n::tr_catalog(language, key).to_owned())
        .unwrap_or_else(|| crate::i18n::tr_text(language, variant))
}

#[derive(Clone, Copy)]
enum ModuleSettingsRow {
    SideSelector,
    Section(usize),
    Field { group_idx: usize, field_idx: usize },
}

impl EntropyApp {
    pub(super) fn module_settings_title_key(&self) -> &'static str {
        if self.module_settings.is_trackball_page() {
            "modules_settings.trackball_title"
        } else {
            "modules_settings.title"
        }
    }

    fn module_settings_description_key(&self) -> &'static str {
        if self.module_settings.is_trackball_page() {
            "modules_settings.trackball_description"
        } else {
            "modules_settings.description"
        }
    }

    fn module_setting_label(&self, group_kind: ModuleSettingsGroupKind, title: &str) -> String {
        let lang = self.app_settings.language;
        let display_title = group_kind.field_base_title(title);
        module_setting_catalog_keys(display_title)
            .map(|(label_key, _)| crate::i18n::tr_catalog(lang, label_key).to_owned())
            .unwrap_or_else(|| crate::i18n::tr_text(lang, display_title))
    }

    fn module_setting_tooltip(
        &self,
        group_kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> String {
        let lang = self.app_settings.language;
        let display_title = group_kind.field_base_title(&field.title);
        let key = module_setting_catalog_keys(display_title)
            .map(|(_, tooltip_key)| tooltip_key)
            .unwrap_or("modules_settings.generic_tooltip");
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
        let group = self.module_settings.groups.get(group_idx);
        let group_title = group
            .map(|group| group.title.clone())
            .unwrap_or_else(|| "Modules".to_owned());
        let group_kind = group
            .map(|group| group.kind)
            .unwrap_or(ModuleSettingsGroupKind::Other);
        let field_title = field.title.clone();
        let display_label = self.module_setting_label(group_kind, &field_title);
        let old_value = self.module_settings.value(field.qsid);
        let requested = Self::module_setting_transport_value(field, value);

        self.queue_module_setting_write(
            group_title,
            field_title,
            display_label,
            field.qsid,
            field.width,
            old_value,
            requested,
        );
        self.sync_firmware_managed_layout_options();
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
        let raw_value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
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
                    .map(|variant| {
                        module_setting_variant_label(self.app_settings.language, variant)
                    })
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
        let selected_module =
            selected_side_group.and_then(|group_idx| self.selected_module_kind(group_idx));
        let selected_mode =
            selected_side_group.and_then(|group_idx| self.selected_pointer_mode(group_idx));
        if side_groups.len() > 1 {
            rows.push(ModuleSettingsRow::SideSelector);
        }
        if let Some(group_idx) = selected_side_group {
            rows.extend(self.module_settings_field_rows(group_idx, selected_module, selected_mode));
        }
        for (group_idx, group) in self.module_settings.groups.iter().enumerate() {
            if matches!(
                group.kind,
                ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
            ) {
                continue;
            }
            if group.kind == ModuleSettingsGroupKind::AutoLayer
                && matches!(
                    selected_module,
                    Some(ModuleDeviceKind::None | ModuleDeviceKind::Encoder)
                )
            {
                continue;
            }
            rows.push(ModuleSettingsRow::Section(group_idx));
            rows.extend(self.module_settings_field_rows(group_idx, selected_module, selected_mode));
        }
        rows
    }

    fn selected_module_kind(&self, group_idx: usize) -> Option<ModuleDeviceKind> {
        let group = self.module_settings.groups.get(group_idx)?;
        let field = group.module_selector_field()?;
        let value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
        group.selected_module_kind(value)
    }

    fn selected_pointer_mode(&self, group_idx: usize) -> Option<PointerModeKind> {
        let group = self.module_settings.groups.get(group_idx)?;
        let field = group.mode_selector_field()?;
        let value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
        group.selected_pointer_mode(value)
    }

    fn module_settings_field_rows(
        &self,
        group_idx: usize,
        selected_module: Option<ModuleDeviceKind>,
        selected_mode: Option<PointerModeKind>,
    ) -> Vec<ModuleSettingsRow> {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return Vec::new();
        };
        group
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                group.field_visible_for_selection(field, selected_module, selected_mode)
            })
            .map(|(field_idx, _)| ModuleSettingsRow::Field {
                group_idx,
                field_idx,
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

    pub(super) fn module_settings_include_encoder_visibility(
        &self,
        layout: &KeyboardLayout,
    ) -> bool {
        self.module_settings.supported
            && self
                .module_settings_side_group_indices()
                .into_iter()
                .any(|group_idx| {
                    let Some(group) = self.module_settings.groups.get(group_idx) else {
                        return false;
                    };
                    group.supports_module_kind(ModuleDeviceKind::Encoder)
                        && Self::encoder_visibility_entry_for_module_group(layout, group.kind)
                            .is_some()
                })
    }

    pub(super) fn module_encoder_selectors_loaded(&self, layout: &KeyboardLayout) -> bool {
        self.module_settings
            .groups
            .iter()
            .filter(|group| {
                group.supports_module_kind(ModuleDeviceKind::Encoder)
                    && Self::encoder_visibility_entry_for_module_group(layout, group.kind).is_some()
            })
            .filter_map(ModuleSettingsGroup::module_selector_field)
            .all(|field| self.module_settings.values.contains_key(&field.qsid))
    }

    pub(super) fn hide_modular_encoders_by_default(&self, layout: &KeyboardLayout) -> bool {
        self.module_settings_include_encoder_visibility(layout)
            && !self.module_encoder_selectors_loaded(layout)
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
            egui::Stroke::new(1.0_f32, separator),
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
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        self.module_settings_title_key(),
                    ))
                    .size(18.0)
                    .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        self.module_settings_description_key(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

    fn test_module_field() -> ModuleSettingField {
        ModuleSettingField {
            title: "Mode".to_owned(),
            qsid: 134,
            kind: ModuleSettingKind::Select,
            bit: 0,
            layout_option: None,
            width: 1,
            min: 0,
            max: 3,
            variants: vec![
                "Normal".to_owned(),
                "Sniper".to_owned(),
                "Scroll".to_owned(),
                "Text".to_owned(),
            ],
        }
    }

    fn add_test_module_group(app: &mut EntropyApp, field: &ModuleSettingField) {
        app.module_settings.groups.push(ModuleSettingsGroup {
            title: "Left Modules".to_owned(),
            kind: ModuleSettingsGroupKind::Left,
            fields: vec![field.clone()],
        });
    }

    fn module_filter_field(title: &str, qsid: u16) -> ModuleSettingField {
        ModuleSettingField {
            title: title.to_owned(),
            qsid,
            kind: ModuleSettingKind::Select,
            bit: 0,
            layout_option: None,
            width: 1,
            min: 0,
            max: 3,
            variants: vec!["Off".to_owned(), "On".to_owned()],
        }
    }

    fn add_filterable_module_groups(app: &mut EntropyApp) {
        let mut selector = module_filter_field("Module", 149);
        selector.variants = vec![
            "None".to_owned(),
            "Encoder".to_owned(),
            "Trackball".to_owned(),
            "Touchpad".to_owned(),
        ];
        let mut mode = module_filter_field("Mode", 134);
        mode.variants = vec![
            "Normal".to_owned(),
            "Sniper".to_owned(),
            "Scroll".to_owned(),
            "Text".to_owned(),
            "Experimental".to_owned(),
        ];
        mode.max = 4;
        app.module_settings.groups = vec![
            ModuleSettingsGroup {
                title: "Left Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Left,
                fields: vec![
                    selector,
                    module_filter_field("Encoder interval", 325),
                    module_filter_field("Encoder steps", 332),
                    mode,
                    module_filter_field("Ball axis", 130),
                    module_filter_field("Touch axis", 132),
                    module_filter_field("Ball DPI", 120),
                    module_filter_field("Touch DPI", 122),
                    module_filter_field("Scroll sens", 125),
                    module_filter_field("Sniper sens", 124),
                    module_filter_field("Text sens", 126),
                    module_filter_field("Touch gestures", 151),
                    module_filter_field("Invert scroll vertical", 136),
                    module_filter_field("Invert scroll horizontal", 327),
                    module_filter_field("Invert text vertical", 147),
                    module_filter_field("Invert text horizontal", 329),
                    module_filter_field("Acceleration", 137),
                    module_filter_field("Sticky mode", 140),
                ],
            },
            ModuleSettingsGroup {
                title: "Auto Layer".to_owned(),
                kind: ModuleSettingsGroupKind::AutoLayer,
                fields: vec![
                    module_filter_field("Auto layer", 143),
                    module_filter_field("Auto layer in Normal", 142),
                    module_filter_field("Auto layer in Sniper", 144),
                    module_filter_field("Auto layer in Scroll", 145),
                    module_filter_field("Auto layer in Text", 146),
                    module_filter_field("Auto layer timeout", 324),
                ],
            },
        ];
        app.module_settings.values.insert(149, 0);
        app.module_settings.values.insert(134, 0);
    }

    fn encoder_visibility_layout(left_label: &str, right_label: &str) -> KeyboardLayout {
        KeyboardLayout {
            name: "Modular keyboard".to_owned(),
            rows: 1,
            cols: 1,
            keys: Vec::new(),
            encoders: [0, 1]
                .into_iter()
                .map(|encoder_idx| PhysicalEncoder {
                    x: encoder_idx as f32,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    label: String::new(),
                    encoder_idx,
                    direction: 0,
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            layers: Vec::new(),
            encoder_layers: Vec::new(),
            layer_names: Vec::new(),
            custom_keycodes: Vec::new(),
            layout_options: vec![
                LayoutOption {
                    label: left_label.to_owned(),
                    choices: Vec::new(),
                },
                LayoutOption {
                    label: right_label.to_owned(),
                    choices: Vec::new(),
                },
            ],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn add_encoder_module_groups(app: &mut EntropyApp) {
        let mut left_selector = module_filter_field("Module", 149);
        left_selector.variants = vec!["Encoder".to_owned(), "Trackball".to_owned()];
        let mut right_selector = module_filter_field("Module", 150);
        right_selector.variants = left_selector.variants.clone();
        app.module_settings.groups = vec![
            ModuleSettingsGroup {
                title: "Left Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Left,
                fields: vec![left_selector],
            },
            ModuleSettingsGroup {
                title: "Right Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Right,
                fields: vec![right_selector],
            },
        ];
        app.module_settings.values.insert(149, 0);
        app.module_settings.values.insert(150, 0);
        app.module_settings.supported = true;
    }

    fn encoder_module_settings_json() -> serde_json::Value {
        serde_json::json!({
            "settings": [
                {
                    "name": "Left modules",
                    "fields": [{
                        "type": "select",
                        "title": "Module",
                        "qsid": 149,
                        "variants": ["None", "Encoder", "Trackball", "Touchpad"]
                    }]
                },
                {
                    "name": "Right modules",
                    "fields": [{
                        "type": "select",
                        "title": "Module",
                        "qsid": 150,
                        "variants": ["None", "Encoder", "Trackball", "Touchpad"]
                    }]
                }
            ]
        })
    }

    fn visible_module_qsids(app: &EntropyApp) -> Vec<u16> {
        app.module_settings_rows()
            .into_iter()
            .filter_map(|row| match row {
                ModuleSettingsRow::Field {
                    group_idx,
                    field_idx,
                } => app
                    .module_settings
                    .groups
                    .get(group_idx)
                    .and_then(|group| group.fields.get(field_idx))
                    .map(|field| field.qsid),
                ModuleSettingsRow::SideSelector | ModuleSettingsRow::Section(_) => None,
            })
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drain_hid_writes(app: &mut EntropyApp, ctx: &egui::Context) {
        for _ in 0..200 {
            app.poll_combo_write(ctx);
            app.poll_settings_write(ctx);
            if !app.hid_write_task_active() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("HID writes did not drain");
    }

    #[test]
    fn modular_encoder_visibility_is_automatic() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder module",
            "Hide right encoder module",
        ));

        assert!(
            app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
        assert_eq!(visible_module_qsids(&app), vec![149]);

        app.module_settings.set_selected_module_group(1);
        assert_eq!(visible_module_qsids(&app), vec![150]);
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn deferred_module_definition_owns_encoder_visibility_before_values_load() {
        let json = encoder_module_settings_json();
        let mut app = test_app();
        app.module_settings = EntropyApp::module_settings_from_definition(&json, &[149, 150]);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder module",
            "Hide right encoder module",
        ));

        assert!(app.module_settings.supported);
        assert!(app.module_settings.values.is_empty());
        assert!(
            app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
        assert!(
            app.hide_modular_encoders_by_default(app.layout.as_ref().expect("layout")),
            "unknown module selectors must not guess that an encoder is installed"
        );
    }

    #[test]
    fn loaded_module_selectors_show_installed_encoders_by_default() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        let layout =
            encoder_visibility_layout("Hide left encoder module", "Hide right encoder module");

        assert!(app.module_encoder_selectors_loaded(&layout));
        assert!(!app.hide_modular_encoders_by_default(&layout));
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(
                &layout,
                Some(0),
                None,
                app.hide_modular_encoders_by_default(&layout),
            ),
            vec![true, true]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn staged_module_selector_reader_preloads_layout_critical_values() {
        let json = encoder_module_settings_json();
        let (hid, _) = crate::hid::HidDevice::test_device();
        hid.set_qmk_setting_u8(149, 1).unwrap();
        hid.set_qmk_setting_u8(150, 2).unwrap();
        let mut settings = EntropyApp::module_settings_from_definition(&json, &[149, 150]);

        EntropyApp::read_initial_module_values(&mut settings, &hid);

        assert_eq!(settings.values.get(&149), Some(&1));
        assert_eq!(settings.values.get(&150), Some(&2));
    }

    #[test]
    fn phenom_encoder_labels_are_owned_by_module_settings() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder",
            "Hide right encoder",
        ));

        assert!(
            app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn encoder_visibility_control_is_absent_for_selected_pointing_module() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder",
            "Hide right encoder",
        ));
        app.module_settings.values.insert(149, 1);

        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn keyboards_without_module_settings_do_not_get_encoder_visibility_page() {
        let mut app = test_app();
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder",
            "Hide right encoder",
        ));

        assert!(
            !app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn k03_and_imperial44_keep_their_encoder_page() {
        let mut app = test_app();
        for name in ["K:03", "Imperial44"] {
            app.layout = Some(encoder_visibility_layout(
                "Hide left encoder",
                "Hide right encoder",
            ));
            app.layout.as_mut().expect("layout").name = name.to_owned();

            assert!(
                app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
            );
        }
    }

    #[test]
    fn all_firmware_module_fields_have_catalog_entries_case_insensitively() {
        for title in [
            "Module",
            "Encoder interval",
            "Encoder steps",
            "Sticky mode",
            "Invert scroll vertical",
            "invert scroll horizontal",
            "Invert text vertical",
            "invert text horizontal",
            "Auto layer in Normal",
            "auto layer in sniper",
            "Auto layer in Scroll",
            "auto layer in text",
            "Auto layer timeout",
        ] {
            assert!(
                module_setting_catalog_keys(title).is_some(),
                "missing module translation for {title}"
            );
        }
    }

    #[test]
    fn lowercase_module_variants_are_localized_and_capitalized() {
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "trackball"),
            "Трекбол"
        );
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "normal"),
            "Обычный"
        );
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "none"),
            "Нет"
        );
    }

    #[test]
    fn module_selector_keeps_none_and_transport_values_aligned() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        let group = &app.module_settings.groups[0];
        let field = &group.fields[0];

        assert_eq!(
            field.variants,
            vec!["None", "Encoder", "Trackball", "Touchpad"]
        );
    }

    #[test]
    fn stored_none_selects_none() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        let group = &app.module_settings.groups[0];

        assert_eq!(group.selected_module_kind(0), Some(ModuleDeviceKind::None));
    }

    #[test]
    fn selected_module_filters_rows_to_relevant_settings() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);

        assert_eq!(visible_module_qsids(&app), vec![149]);

        app.module_settings.set_value(149, 1);
        assert_eq!(visible_module_qsids(&app), vec![149, 325, 332]);

        app.module_settings.set_value(149, 2);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 137, 140, 143, 142, 324]
        );

        app.module_settings.set_value(149, 3);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 132, 122, 151, 137, 140, 143, 142, 324]
        );
    }

    #[test]
    fn selected_pointer_mode_filters_rows_to_relevant_settings() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 2);

        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 137, 140, 143, 142, 324]
        );

        app.module_settings.set_value(134, 1);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 124, 137, 140, 143, 144, 324]
        );

        app.module_settings.set_value(134, 2);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 125, 136, 327, 137, 140, 143, 145, 324]
        );

        app.module_settings.set_value(134, 3);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 126, 147, 329, 137, 140, 143, 146, 324]
        );
    }

    #[test]
    fn unknown_pointer_mode_keeps_all_mode_fields_visible() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 2);
        app.module_settings.set_value(134, 4);

        assert_eq!(
            visible_module_qsids(&app),
            vec![
                149, 134, 130, 120, 125, 124, 126, 136, 327, 147, 329, 137, 140, 143, 142, 144,
                145, 146, 324
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_module_selection_updates_visible_rows_immediately() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        add_filterable_module_groups(&mut app);
        let selector = app.module_settings.groups[0].fields[0].clone();

        app.write_module_setting_value(0, &selector, 2);

        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 137, 140, 143, 142, 324]
        );

        drain_hid_writes(&mut app, &ctx);
        assert_eq!(app.module_settings.value(149), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_pointer_mode_updates_visible_rows_immediately() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 3);
        let mode = app.module_settings.groups[0].fields[3].clone();

        app.write_module_setting_value(0, &mode, 2);

        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 132, 122, 125, 151, 136, 327, 137, 140, 143, 145, 324]
        );

        drain_hid_writes(&mut app, &ctx);
        assert_eq!(app.module_settings.value(134), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_setting_write_matches_vial_set_without_readback_or_debounce() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        let field = test_module_field();
        add_test_module_group(&mut app, &field);
        app.status_msg = "unchanged".to_owned();

        app.write_module_setting_value(0, &field, 2);

        assert!(app.settings_write_task.is_some());
        assert_eq!(app.pending_settings_write_value(field.qsid), Some(2));
        assert_eq!(app.pending_qmk_settings_write_value(field.qsid), None);
        assert_eq!(app.status_msg, "unchanged");

        drain_hid_writes(&mut app, &ctx);

        assert_eq!(app.module_settings.value(field.qsid), 2);
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(app.status_msg, "unchanged");
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|request| {
                    request[2] == field.qsid as u8 && request[3] == (field.qsid >> 8) as u8
                })
                .count(),
            1
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_setting_write_waits_for_busy_hid_owner() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        app.combo_entries = vec![ComboEntry {
            keys: [0x0004, 0x0005, 0, 0],
            output: 0x0006.into(),
        }];
        app.combo_synced_entries = vec![ComboEntry::default()];
        app.mark_combo_dirty();
        app.maybe_start_combo_write(&ctx);
        assert!(app.combo_write_task.is_some());
        assert!(app.hid_device.is_none());

        let field = test_module_field();
        add_test_module_group(&mut app, &field);
        app.write_module_setting_value(0, &field, 3);

        assert!(app.settings_write_task.is_none());
        assert_eq!(app.pending_settings_write_value(field.qsid), Some(3));
        assert_eq!(app.pending_qmk_settings_write_value(field.qsid), None);

        drain_hid_writes(&mut app, &ctx);

        assert_eq!(app.module_settings.value(field.qsid), 3);
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|request| {
                    request[2] == field.qsid as u8 && request[3] == (field.qsid >> 8) as u8
                })
                .count(),
            1
        );
    }

    #[test]
    fn dedicated_trackball_groups_use_trackball_page_title() {
        let mut app = test_app();
        app.module_settings.groups = vec![
            ModuleSettingsGroup {
                title: "Trackball".to_owned(),
                kind: ModuleSettingsGroupKind::Other,
                fields: vec![test_module_field()],
            },
            ModuleSettingsGroup {
                title: "Auto layer".to_owned(),
                kind: ModuleSettingsGroupKind::AutoLayer,
                fields: vec![module_filter_field("Auto layer timeout", 324)],
            },
        ];

        assert!(app.module_settings.is_trackball_page());
        assert_eq!(
            app.module_settings_title_key(),
            "modules_settings.trackball_title"
        );
    }

    #[test]
    fn russian_inversion_labels_use_direction_words_without_arrows() {
        let mut app = test_app();
        app.app_settings.language = crate::i18n::Language::Russian;

        for (title, direction) in [
            ("Invert scroll vertical", "вертикали"),
            ("Invert scroll horizontal", "горизонтали"),
            ("Invert text vertical", "вертикали"),
            ("Invert text horizontal", "горизонтали"),
        ] {
            let label = app.module_setting_label(ModuleSettingsGroupKind::Left, title);
            assert!(label.contains(direction), "{label}");
            assert!(
                !label
                    .chars()
                    .any(|character| matches!(character, '↑' | '↓' | '←' | '→')),
                "{label}"
            );
        }
    }
}
