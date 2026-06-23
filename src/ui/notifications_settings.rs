use super::*;

impl EntropyApp {
    pub(super) fn draw_notifications_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(lang, "notifications.title"))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(lang, "notifications.description"))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                let layer_count = layout.layers.len().max(1);
                let total_rows = 6 + layer_count;
                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "notifications_settings",
                    metrics,
                    total_rows,
                    0.0,
                );
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(list.content_rect), |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_notifications_editor_content(
                        ui,
                        layout,
                        list.first_visible_row..list.last_visible_row,
                        metrics,
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

    fn draw_notifications_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        row_range: std::ops::Range<usize>,
        metrics: crate::ui_style::ResponsiveMetrics,
        suppress_tooltips: bool,
    ) {
        let content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let scale = (row_height / 54.0).clamp(1.0, 1.12);
        let switch_width = 46.0 * scale;
        let switch_size = egui::vec2(46.0 * scale, 24.0 * scale);
        let control_height = metrics.settings_control_height();

        for row_idx in row_range {
            match row_idx {
                0 => self.draw_notifications_enabled_row(
                    ui,
                    content_width,
                    row_height,
                    switch_width,
                    switch_size,
                    suppress_tooltips,
                ),
                1 => self.draw_notifications_timeout_row(
                    ui,
                    content_width,
                    row_height,
                    metrics.value(96.0),
                    control_height,
                    suppress_tooltips,
                ),
                2 => self.draw_notifications_theme_row(
                    ui,
                    content_width,
                    row_height,
                    metrics.settings_control_width(),
                    control_height,
                    metrics.settings_control_font_size(),
                    suppress_tooltips,
                ),
                3 => self.draw_notifications_size_row(
                    ui,
                    content_width,
                    row_height,
                    metrics.settings_control_width(),
                    control_height,
                    metrics.settings_control_font_size(),
                    suppress_tooltips,
                ),
                4 => self.draw_notifications_position_row(
                    ui,
                    content_width,
                    row_height,
                    metrics.settings_control_width(),
                    control_height,
                    metrics.settings_control_font_size(),
                    suppress_tooltips,
                ),
                5 => self.draw_notifications_opacity_row(
                    ui,
                    content_width,
                    row_height,
                    metrics.settings_control_width(),
                    control_height,
                    metrics.settings_control_font_size(),
                    suppress_tooltips,
                ),
                _ => {
                    let layer_idx = row_idx - 6;
                    if layer_idx < layout.layers.len().max(1) {
                        self.draw_notifications_layer_row(
                            ui,
                            layer_idx,
                            content_width,
                            row_height,
                            switch_width,
                            switch_size,
                            suppress_tooltips,
                        );
                    }
                }
            }
        }
    }

    fn draw_notifications_enabled_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        switch_width: f32,
        switch_size: egui::Vec2,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let mut enabled = self.app_settings.layer_key_osd;
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "notifications.layer_keys_label"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                "notifications.layer_keys_tooltip",
            )),
            switch_width,
            |ui| {
                let _ = crate::ui_style::settings_switch_sized_stable(
                    ui,
                    "notifications_layer_keys_enabled",
                    &mut enabled,
                    switch_size,
                );
            },
        );
        if enabled != self.app_settings.layer_key_osd {
            self.app_settings.layer_key_osd = enabled;
            if !enabled {
                self.layer_key_osd_until = None;
            }
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_notifications_timeout_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        field_width: f32,
        control_height: f32,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let current = clamp_notification_timeout_ms(self.app_settings.layer_key_osd_timeout_ms);
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "notifications.timeout_label"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                "notifications.timeout_tooltip",
            )),
            field_width,
            |ui| {
                let edit_id = egui::Id::new("notifications_timeout_ms");
                let mut text = ui.ctx().data_mut(|d| {
                    d.get_temp::<String>(edit_id)
                        .unwrap_or_else(|| current.to_string())
                });
                if text.parse::<u32>().ok() != Some(current) && !ui.memory(|m| m.has_focus(edit_id))
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
                let resp = settings_field_unit_tooltip(
                    resp,
                    lang,
                    suppress_tooltips,
                    SettingsFieldUnit::Milliseconds,
                );
                if resp.changed() {
                    let filtered: String =
                        text.chars().filter(|c: &char| c.is_ascii_digit()).collect();
                    if filtered != text {
                        text = filtered;
                    }
                }
                let commit = resp.lost_focus()
                    || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if commit {
                    match text.trim().parse::<u32>() {
                        Ok(value) => {
                            let clamped = clamp_notification_timeout_ms(value);
                            if clamped != self.app_settings.layer_key_osd_timeout_ms {
                                self.app_settings.layer_key_osd_timeout_ms = clamped;
                                save_app_settings(&self.app_settings);
                            }
                            text = clamped.to_string();
                        }
                        Err(_) => text = current.to_string(),
                    }
                }
                ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
            },
        );
    }

    fn draw_notifications_theme_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        dropdown_width: f32,
        control_height: f32,
        font_size: f32,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let mut selected = self.app_settings.notifications_theme;
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "notifications.theme_label"),
            true,
            (!suppress_tooltips)
                .then_some(crate::i18n::tr_catalog(lang, "notifications.theme_tooltip")),
            dropdown_width,
            |ui| {
                draw_notifications_dropdown_control(
                    ui,
                    lang,
                    "notifications_theme_dropdown",
                    &mut selected,
                    &[NotificationTheme::Dark, NotificationTheme::Light],
                    notification_theme_label,
                    dropdown_width,
                    control_height,
                    font_size,
                );
            },
        );
        if selected != self.app_settings.notifications_theme {
            self.app_settings.notifications_theme = selected;
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_notifications_size_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        dropdown_width: f32,
        control_height: f32,
        font_size: f32,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let mut selected = self.app_settings.notifications_size;
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "notifications.size_label"),
            true,
            (!suppress_tooltips)
                .then_some(crate::i18n::tr_catalog(lang, "notifications.size_tooltip")),
            dropdown_width,
            |ui| {
                draw_notifications_dropdown_control(
                    ui,
                    lang,
                    "notifications_size_dropdown",
                    &mut selected,
                    &[
                        NotificationSize::Small,
                        NotificationSize::Medium,
                        NotificationSize::Large,
                    ],
                    notification_size_label,
                    dropdown_width,
                    control_height,
                    font_size,
                );
            },
        );
        if selected != self.app_settings.notifications_size {
            self.app_settings.notifications_size = selected;
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_notifications_position_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        dropdown_width: f32,
        control_height: f32,
        font_size: f32,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let mut selected = self.app_settings.notifications_position;
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "notifications.position_label"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                "notifications.position_tooltip",
            )),
            dropdown_width,
            |ui| {
                draw_notifications_dropdown_control(
                    ui,
                    lang,
                    "notifications_position_dropdown",
                    &mut selected,
                    &[
                        NotificationPosition::TopLeft,
                        NotificationPosition::TopCenter,
                        NotificationPosition::TopRight,
                        NotificationPosition::CenterLeft,
                        NotificationPosition::Center,
                        NotificationPosition::CenterRight,
                        NotificationPosition::BottomLeft,
                        NotificationPosition::BottomCenter,
                        NotificationPosition::BottomRight,
                    ],
                    notification_position_label,
                    dropdown_width,
                    control_height,
                    font_size,
                );
            },
        );
        if selected != self.app_settings.notifications_position {
            self.app_settings.notifications_position = selected;
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_notifications_opacity_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        dropdown_width: f32,
        control_height: f32,
        font_size: f32,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let mut selected = clamp_notification_opacity(self.app_settings.notifications_opacity);
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "notifications.opacity_label"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                "notifications.opacity_tooltip",
            )),
            dropdown_width,
            |ui| {
                draw_notifications_opacity_dropdown_control(
                    ui,
                    lang,
                    "notifications_opacity_dropdown",
                    &mut selected,
                    dropdown_width,
                    control_height,
                    font_size,
                );
            },
        );
        if (selected - self.app_settings.notifications_opacity).abs() > f32::EPSILON {
            self.app_settings.notifications_opacity = selected;
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_notifications_layer_row(
        &mut self,
        ui: &mut egui::Ui,
        layer_idx: usize,
        content_width: f32,
        row_height: f32,
        switch_width: f32,
        switch_size: egui::Vec2,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let mut enabled = self
            .app_settings
            .layer_key_osd_layers
            .get(layer_idx)
            .copied()
            .unwrap_or(true);
        let label = notification_layer_label(lang, layer_idx, self.layer_names.get(layer_idx));
        let tooltip = crate::i18n::tr_catalog_format(
            lang,
            "notifications.layer_tooltip",
            &[("layer", label.as_str())],
        );

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            label.as_str(),
            true,
            (!suppress_tooltips).then_some(tooltip.as_str()),
            switch_width,
            |ui| {
                let _ = crate::ui_style::settings_switch_sized_stable(
                    ui,
                    ("notifications_layer", layer_idx),
                    &mut enabled,
                    switch_size,
                );
            },
        );
        let current = self
            .app_settings
            .layer_key_osd_layers
            .get(layer_idx)
            .copied()
            .unwrap_or(true);
        if enabled != current {
            if self.app_settings.layer_key_osd_layers.len() <= layer_idx {
                self.app_settings
                    .layer_key_osd_layers
                    .resize(layer_idx + 1, true);
            }
            self.app_settings.layer_key_osd_layers[layer_idx] = enabled;
            save_app_settings(&self.app_settings);
        }
    }
}

fn draw_notifications_dropdown_control<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    id_source: &'static str,
    selected: &mut T,
    options: &[T],
    label_fn: fn(crate::i18n::Language, T) -> &'static str,
    dropdown_width: f32,
    control_height: f32,
    font_size: f32,
) {
    let dropdown_id = ui.make_persistent_id(id_source);
    let selected_text = label_fn(lang, *selected);
    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
        ui,
        dropdown_id,
        selected_text,
        ui.visuals().text_color(),
        dropdown_width,
        control_height,
        font_size,
    );
    egui::popup_below_widget(
        ui,
        dropdown_id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(dropdown_width);
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            egui::ScrollArea::vertical()
                .id_salt((id_source, "scroll"))
                .max_height(170.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for option in options {
                        let option_text = label_fn(lang, *option);
                        let selected_option = *option == *selected;
                        let (option_rect, option_resp) = ui
                            .allocate_exact_size(egui::vec2(dropdown_width, 28.0), Sense::click());
                        if option_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let option_fill = if selected_option {
                            if ui.visuals().dark_mode {
                                Color32::from_rgb(58, 58, 61)
                            } else {
                                Color32::from_rgb(236, 236, 238)
                            }
                        } else if option_resp.hovered() {
                            crate::ui_style::hover_fill(ui.visuals().dark_mode)
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(option_rect, 7.0, option_fill);
                        ui.painter().text(
                            egui::pos2(option_rect.left() + 10.0, option_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            option_text,
                            FontId::proportional(12.0),
                            if selected_option {
                                ui.visuals().text_color()
                            } else {
                                app_muted_text(ui.visuals().dark_mode)
                            },
                        );
                        if option_resp.clicked() {
                            *selected = *option;
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                });
        },
    );
}

fn notification_opacity_label(lang: crate::i18n::Language, opacity: f32) -> String {
    let label_prefix = crate::i18n::tr_catalog(lang, "ui.sticky_layout_transparency_short");
    format!("{} {}%", label_prefix, (opacity * 100.0).round() as i32)
}

fn draw_notifications_opacity_dropdown_control(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    id_source: &'static str,
    selected: &mut f32,
    dropdown_width: f32,
    control_height: f32,
    font_size: f32,
) {
    const OPACITY_VALUES: [f32; 6] = [1.0, 0.90, 0.80, 0.70, 0.60, 0.50];

    let current = clamp_notification_opacity(*selected);
    let selected_idx = OPACITY_VALUES
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - current)
                .abs()
                .partial_cmp(&(*b - current).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    let dropdown_id = ui.make_persistent_id(id_source);
    let selected_text = notification_opacity_label(lang, OPACITY_VALUES[selected_idx]);
    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
        ui,
        dropdown_id,
        &selected_text,
        ui.visuals().text_color(),
        dropdown_width,
        control_height,
        font_size,
    );
    egui::popup_below_widget(
        ui,
        dropdown_id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(dropdown_width);
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            for (idx, value) in OPACITY_VALUES.iter().copied().enumerate() {
                let option_text = notification_opacity_label(lang, value);
                let selected_option = idx == selected_idx;
                let (option_rect, option_resp) =
                    ui.allocate_exact_size(egui::vec2(dropdown_width, 28.0), Sense::click());
                if option_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let option_fill = if selected_option {
                    if ui.visuals().dark_mode {
                        Color32::from_rgb(58, 58, 61)
                    } else {
                        Color32::from_rgb(236, 236, 238)
                    }
                } else if option_resp.hovered() {
                    crate::ui_style::hover_fill(ui.visuals().dark_mode)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(option_rect, 7.0, option_fill);
                ui.painter().text(
                    egui::pos2(option_rect.left() + 10.0, option_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    option_text,
                    FontId::proportional(12.0),
                    if selected_option {
                        ui.visuals().text_color()
                    } else {
                        app_muted_text(ui.visuals().dark_mode)
                    },
                );
                if option_resp.clicked() {
                    *selected = value;
                    ui.memory_mut(|m| m.close_popup());
                }
            }
        },
    );
}

fn notification_theme_label(lang: crate::i18n::Language, theme: NotificationTheme) -> &'static str {
    match theme {
        NotificationTheme::Dark => crate::i18n::tr_catalog(lang, "notifications.theme_dark"),
        NotificationTheme::Light => crate::i18n::tr_catalog(lang, "notifications.theme_light"),
    }
}

fn notification_size_label(lang: crate::i18n::Language, size: NotificationSize) -> &'static str {
    match size {
        NotificationSize::Small => crate::i18n::tr_catalog(lang, "notifications.size_small"),
        NotificationSize::Medium => crate::i18n::tr_catalog(lang, "notifications.size_medium"),
        NotificationSize::Large => crate::i18n::tr_catalog(lang, "notifications.size_large"),
    }
}

fn notification_position_label(
    lang: crate::i18n::Language,
    position: NotificationPosition,
) -> &'static str {
    match position {
        NotificationPosition::TopLeft => {
            crate::i18n::tr_catalog(lang, "notifications.position_top_left")
        }
        NotificationPosition::TopCenter => {
            crate::i18n::tr_catalog(lang, "notifications.position_top_center")
        }
        NotificationPosition::TopRight => {
            crate::i18n::tr_catalog(lang, "notifications.position_top_right")
        }
        NotificationPosition::CenterLeft => {
            crate::i18n::tr_catalog(lang, "notifications.position_center_left")
        }
        NotificationPosition::Center => {
            crate::i18n::tr_catalog(lang, "notifications.position_center")
        }
        NotificationPosition::CenterRight => {
            crate::i18n::tr_catalog(lang, "notifications.position_center_right")
        }
        NotificationPosition::BottomLeft => {
            crate::i18n::tr_catalog(lang, "notifications.position_bottom_left")
        }
        NotificationPosition::BottomCenter => {
            crate::i18n::tr_catalog(lang, "notifications.position_bottom_center")
        }
        NotificationPosition::BottomRight => {
            crate::i18n::tr_catalog(lang, "notifications.position_bottom_right")
        }
    }
}

fn notification_layer_label(
    lang: crate::i18n::Language,
    layer_idx: usize,
    layer_name: Option<&String>,
) -> String {
    let fallback = crate::i18n::tr_catalog_format(
        lang,
        "notifications.layer_label",
        &[("layer", &layer_idx.to_string())],
    );
    match layer_name.map(|name| name.trim()) {
        Some(name) if !name.is_empty() && name != layer_idx.to_string().as_str() => {
            format!("{fallback} - {name}")
        }
        _ => fallback,
    }
}
