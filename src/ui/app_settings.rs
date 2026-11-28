use super::*;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod notification_row_tests {
    use super::*;

    #[test]
    fn notification_rows_keep_distinct_labels_and_settings() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for language in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
            for dark in [false, true] {
                for app_enabled in [false, true] {
                    for firmware_enabled in [false, true] {
                        let mut app = EntropyApp::new_inert_for_test();
                        app.app_settings.language = language;
                        app.app_settings.app_update_notifications = app_enabled;
                        app.app_settings.firmware_update_notifications = firmware_enabled;
                        let before = serde_json::to_value(&app.app_settings).unwrap();
                        let ctx = egui::Context::default();
                        ctx.set_visuals(if dark {
                            egui::Visuals::dark()
                        } else {
                            egui::Visuals::light()
                        });
                        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 2400.0));
                        let output = crate::app::discard_texture_deltas(ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(rect),
                                ..Default::default()
                            },
                            |ui| {
                                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                                // Exercise the real row owner on unsupported hosts too,
                                // without enabling or calling a native notification backend.
                                let rows = total_app_settings_rows()
                                    + if crate::app::update_notifications::SUPPORTED {
                                        0
                                    } else {
                                        2
                                    };
                                let list = allocate_adaptive_settings_list_viewport(
                                    ui,
                                    "notification_rows_regression",
                                    metrics,
                                    rows,
                                    0.0,
                                );
                                assert_eq!(list.first_visible_row, 0);
                                assert_eq!(list.last_visible_row, rows);
                                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                                    ui.set_clip_rect(list.viewport);
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    app.draw_app_settings_editor_content(ui, &list, metrics, dark);
                                });
                            },
                        ));
                        fn collect(shape: &egui::Shape, texts: &mut Vec<String>) {
                            match shape {
                                egui::Shape::Text(text) => texts.push(text.galley.text().to_owned()),
                                egui::Shape::Vec(shapes) => {
                                    for shape in shapes {
                                        collect(shape, texts);
                                    }
                                }
                                _ => {}
                            }
                        }
                        let mut texts = Vec::new();
                        for shape in &output.shapes {
                            collect(&shape.shape, &mut texts);
                        }
                        for key in ["notifications.app_label", "notifications.firmware_label"] {
                            let label = crate::i18n::tr_catalog(language, key);
                            assert_eq!(
                                texts.iter().filter(|text| text.as_str() == label).count(),
                                1,
                                "notification row missing or duplicated: {key}; {texts:?}"
                            );
                        }
                        assert_eq!(
                            serde_json::to_value(&app.app_settings).unwrap(),
                            before,
                            "painting notification rows changed saved settings"
                        );
                        output.drop_without_applying_deltas();
                    }
                }
            }
        }
    }
}

impl EntropyApp {
    fn draw_startup_device_setting(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        let lang = self.app_settings.language;
        let startup_screen = crate::i18n::tr_catalog(lang, "ui.startup_device_screen");
        let mut preference = self.app_settings.preferred_startup_device.clone();
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            width,
            height,
            crate::i18n::tr_catalog(lang, "ui.startup_device_label"),
            true,
            Some(crate::i18n::tr_catalog(lang, "ui.startup_device_tooltip")),
            metrics.settings_control_width(),
            |ui| {
                let id = ui.make_persistent_id("startup_device_dropdown");
                let response = crate::ui_style::modern_dropdown_button_sized(
                    ui,
                    id,
                    preference
                        .as_ref()
                        .map_or(startup_screen, |p| p.name.as_str()),
                    ui.visuals().text_color(),
                    metrics.settings_control_width(),
                    metrics.settings_control_height(),
                    metrics.settings_control_font_size(),
                );
                crate::ui_style::popup_below_widget(
                    ui,
                    id,
                    &response,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_max_width(metrics.settings_control_width());
                        if ui
                            .selectable_label(preference.is_none(), startup_screen)
                            .clicked()
                        {
                            preference = None;
                            ui.close();
                        }
                        for device in self.device_manager.devices() {
                            let candidate = StartupDevicePreference::from_device(device);
                            let unique = self
                                .device_manager
                                .devices()
                                .iter()
                                .filter(|d| candidate.identity.matches(d))
                                .count()
                                == 1;
                            if ui
                                .add_enabled(
                                    unique,
                                    egui::Button::selectable(
                                        preference.as_ref() == Some(&candidate),
                                        &candidate.name,
                                    ),
                                )
                                .on_disabled_hover_text(crate::i18n::tr_catalog(
                                    lang,
                                    "ui.startup_device_ambiguous",
                                ))
                                .on_hover_text(format!(
                                    "{:04x}:{:04x} {}",
                                    device.vendor_id, device.product_id, device.serial_number
                                ))
                                .clicked()
                            {
                                preference = Some(candidate);
                                ui.close();
                            }
                        }
                    },
                );
            },
        );
        if preference != self.app_settings.preferred_startup_device {
            self.app_settings.preferred_startup_device = preference;
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_app_layout_setting(
        &mut self,
        ui: &mut egui::Ui,
        row: usize,
        width: f32,
        height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let current = self.app_settings.key_legend_layout;
        let (label_key, tooltip_key, choices, labels, selected, enabled) = match row {
            1 => (
                "ui.primary_layout_label",
                "ui.primary_layout_tooltip",
                vec![KeyLegendLayout::English],
                vec![crate::i18n::tr_catalog(lang, "ui.key_legends_english").to_owned()],
                0,
                false,
            ),
            2 => {
                let choices = KeyLegendLayout::secondary_layouts();
                let labels = choices
                    .iter()
                    .map(|choice| choice.secondary_label(lang))
                    .collect();
                let selected = choices
                    .iter()
                    .position(|choice| choice.same_family(current))
                    .unwrap_or(0);
                (
                    "ui.secondary_layout_label",
                    "ui.secondary_layout_tooltip",
                    choices,
                    labels,
                    selected,
                    true,
                )
            }
            3 => {
                let variants = current.variants();
                let enabled = !variants.is_empty();
                let choices = if enabled {
                    variants
                } else {
                    vec![KeyLegendLayout::English]
                };
                let labels = choices
                    .iter()
                    .map(|choice| choice.variant_label(lang))
                    .collect();
                let selected = choices
                    .iter()
                    .position(|choice| choice.variant_id() == current.variant_id())
                    .unwrap_or(0);
                (
                    "ui.layout_variant_label",
                    "ui.layout_variant_tooltip",
                    choices,
                    labels,
                    selected,
                    enabled,
                )
            }
            _ => return,
        };
        let mut picked = None;
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            width,
            height,
            crate::i18n::tr_catalog(lang, label_key),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(lang, tooltip_key)),
            metrics.settings_control_width(),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let id = ui.make_persistent_id(("app_layout_dropdown", row));
                    picked = crate::ui_style::modern_dropdown_select_sized(
                        ui,
                        id,
                        &labels,
                        selected,
                        metrics.settings_control_width(),
                        metrics.settings_control_height(),
                        metrics.settings_control_font_size(),
                    )
                    .1;
                });
            },
        );
        if let Some(target) = picked.and_then(|index| choices.get(index)).copied() {
            let updated = match row {
                2 => current.select_secondary(target),
                3 => current.select_variant(target),
                _ => current,
            };
            if updated != current {
                self.app_settings.key_legend_layout = updated;
                save_app_settings(&self.app_settings);
            }
        }
    }

    pub(super) fn draw_app_settings_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        use crate::i18n::Key as TrKey;

        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let lang = self.app_settings.language;
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(crate::i18n::tr(lang, TrKey::AppSettingsTitle))
                        .size(metrics.value(18.0))
                        .strong(),
                );
                ui.add_space(metrics.value(6.0));
                ui.label(
                    RichText::new(crate::i18n::tr(lang, TrKey::AppSettingsDescription))
                        .size(metrics.value(13.0))
                        .color(app_muted_text(dark)),
                );
                ui.add_space(metrics.value(24.0));

                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "app_settings",
                    metrics,
                    total_app_settings_rows(),
                    metrics.value(44.0),
                );

                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_app_settings_editor_content(ui, &list, metrics, dark);
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

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let button_size = metrics.size(168.0, 32.0);
                    let button_gap = metrics.value(10.0);
                    let actions_width = button_size.x * 2.0 + button_gap;
                    let actions_rect = egui::Rect::from_center_size(
                        egui::pos2(
                            list.viewport.center().x,
                            list.viewport.bottom() + metrics.value(34.0),
                        ),
                        egui::vec2(actions_width, button_size.y),
                    );
                    crate::ui_style::allocate_ui_at_rect(ui, actions_rect, |ui| {
                        ui.set_min_size(actions_rect.size());
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = button_gap;
                            if crate::ui_style::modern_button(
                                ui,
                                crate::i18n::tr_catalog(lang, "ui.import_app_settings"),
                                button_size,
                                true,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                lang,
                                "ui.import_app_settings_tooltip",
                            ))
                            .clicked()
                            {
                                self.import_entsettings_dialog();
                            }
                            if crate::ui_style::modern_button(
                                ui,
                                crate::i18n::tr_catalog(lang, "ui.export_app_settings"),
                                button_size,
                                true,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                lang,
                                "ui.export_app_settings_tooltip",
                            ))
                            .clicked()
                            {
                                self.export_entsettings_dialog();
                            }
                        });
                    });
                }
            });
        });
    }

    fn draw_app_settings_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        metrics: crate::ui_style::ResponsiveMetrics,
        dark: bool,
    ) {
        use crate::i18n::Key as TrKey;

        let row_range = list.first_visible_row..list.last_visible_row;
        let content_width = list.row_content_width;
        let row_height = list.row_height;
        let suppress_tooltips = list.suppress_tooltips;
        let lang = self.app_settings.language;
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);
        let tooltip = |text: &'static str| (!suppress_tooltips).then_some(text);

        for row_idx in row_range {
            #[cfg(target_os = "linux")]
            if row_idx == 7 {
                self.draw_linux_device_udev_rules_row(
                    ui,
                    content_width,
                    row_height,
                    metrics,
                    dark,
                    suppress_tooltips,
                );
                continue;
            }

            let row_idx = app_settings_base_row_index(row_idx);
            if row_idx == 7 {
                self.draw_startup_device_setting(ui, content_width, row_height, metrics);
                continue;
            }
            let row_idx = if row_idx > 7 { row_idx - 1 } else { row_idx };
            match row_idx {
                0 => {
                    let mut selected_language = self.app_settings.language;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr(lang, TrKey::LanguageLabel),
                        true,
                        tooltip(crate::i18n::tr(lang, TrKey::LanguageTooltip)),
                        metrics.settings_control_width(),
                        |ui| {
                            let dropdown_id = ui.make_persistent_id("app_language_dropdown");
                            let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
                                ui,
                                dropdown_id,
                                selected_language.native_name(),
                                ui.visuals().text_color(),
                                metrics.settings_control_width(),
                                metrics.settings_control_height(),
                                metrics.settings_control_font_size(),
                            );
                            crate::ui_style::popup_below_widget(
                                ui,
                                dropdown_id,
                                &dropdown_resp,
                                egui::PopupCloseBehavior::CloseOnClickOutside,
                                |ui| {
                                    ui.set_min_width(metrics.settings_control_width());
                                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                                    for language in crate::i18n::Language::ALL {
                                        let selected = language == selected_language;
                                        let (option_rect, option_resp) = ui.allocate_exact_size(
                                            metrics.size(168.0, 28.0),
                                            Sense::click(),
                                        );
                                        if option_resp.hovered() {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        let option_fill = if selected {
                                            if dark {
                                                Color32::from_rgb(58, 58, 61)
                                            } else {
                                                Color32::from_rgb(236, 236, 238)
                                            }
                                        } else if option_resp.hovered() {
                                            crate::ui_style::hover_fill(dark)
                                        } else {
                                            Color32::TRANSPARENT
                                        };
                                        ui.painter().rect_filled(option_rect, 7.0, option_fill);
                                        ui.painter().text(
                                            egui::pos2(
                                                option_rect.left() + metrics.value(10.0),
                                                option_rect.center().y,
                                            ),
                                            egui::Align2::LEFT_CENTER,
                                            language.native_name(),
                                            FontId::proportional(metrics.value(12.0)),
                                            if selected {
                                                ui.visuals().text_color()
                                            } else {
                                                app_muted_text(dark)
                                            },
                                        );
                                        if option_resp.clicked() {
                                            selected_language = language;
                                            egui::Popup::close_all(ui.ctx());
                                        }
                                    }
                                },
                            );
                        },
                    );
                    if selected_language != self.app_settings.language {
                        self.app_settings.language = selected_language;
                        save_app_settings(&self.app_settings);
                    }
                }
                1 | 2 | 3 => {
                    self.draw_app_layout_setting(
                        ui,
                        row_idx,
                        content_width,
                        row_height,
                        metrics,
                        suppress_tooltips,
                    );
                }
                4 => {
                    let close_in_background_before = self.app_settings.minimize_to_tray_on_close
                        || matches!(
                            self.app_settings.close_to_tray_behavior,
                            CloseToTrayBehavior::Tray
                        );
                    let mut minimize_to_tray = close_in_background_before;
                    #[cfg(target_os = "linux")]
                    let (close_behavior_label, close_behavior_tooltip) = (
                        crate::i18n::tr(lang, TrKey::RunInBackgroundLabel),
                        crate::i18n::tr(lang, TrKey::RunInBackgroundTooltip),
                    );
                    #[cfg(not(target_os = "linux"))]
                    let (close_behavior_label, close_behavior_tooltip) = (
                        crate::i18n::tr(lang, TrKey::CloseToTrayLabel),
                        crate::i18n::tr(lang, TrKey::CloseToTrayTooltip),
                    );
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        close_behavior_label,
                        true,
                        tooltip(close_behavior_tooltip),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_minimize_to_tray",
                                &mut minimize_to_tray,
                                switch_size,
                            );
                        },
                    );
                    if minimize_to_tray != close_in_background_before {
                        self.app_settings.close_to_tray_behavior = if minimize_to_tray {
                            CloseToTrayBehavior::Tray
                        } else {
                            CloseToTrayBehavior::Ask
                        };
                        self.app_settings.minimize_to_tray_on_close = minimize_to_tray;
                        if !minimize_to_tray {
                            #[cfg(target_os = "windows")]
                            {
                                self.tray_icon = None;
                            }
                            #[cfg(target_os = "linux")]
                            {
                                self.linux_tray = None;
                            }
                        }
                        save_app_settings(&self.app_settings);
                    }
                }
                5 => {
                    let mut launch_at_startup = self.app_settings.launch_at_startup;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr(lang, TrKey::LaunchAtStartupLabel),
                        true,
                        tooltip(crate::i18n::tr(lang, TrKey::LaunchAtStartupTooltip)),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_launch_at_startup",
                                &mut launch_at_startup,
                                switch_size,
                            );
                        },
                    );
                    if launch_at_startup != self.app_settings.launch_at_startup {
                        if self.set_launch_at_startup(
                            launch_at_startup,
                            self.app_settings.launch_minimized,
                        ) {
                            self.app_settings.launch_at_startup = launch_at_startup;
                            save_app_settings(&self.app_settings);
                        } else {
                            self.status_msg = "Failed to update startup setting".into();
                        }
                    }
                }
                6 => {
                    let mut launch_minimized = self.app_settings.launch_minimized;
                    #[cfg(target_os = "windows")]
                    let (launch_background_label, launch_background_tooltip) = (
                        crate::i18n::tr(lang, TrKey::LaunchInTrayLabel),
                        crate::i18n::tr(lang, TrKey::LaunchInTrayTooltip),
                    );
                    #[cfg(not(target_os = "windows"))]
                    let (launch_background_label, launch_background_tooltip) = (
                        crate::i18n::tr(lang, TrKey::LaunchMinimizedLabel),
                        crate::i18n::tr(lang, TrKey::LaunchMinimizedTooltip),
                    );
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        launch_background_label,
                        true,
                        tooltip(launch_background_tooltip),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_launch_minimized",
                                &mut launch_minimized,
                                switch_size,
                            );
                        },
                    );
                    if launch_minimized != self.app_settings.launch_minimized {
                        let registration_updated = !self.app_settings.launch_at_startup
                            || self.set_launch_at_startup(true, launch_minimized);
                        if registration_updated {
                            self.app_settings.launch_minimized = launch_minimized;
                            save_app_settings(&self.app_settings);
                        } else {
                            self.status_msg = "Failed to update startup setting".into();
                        }
                    }
                }
                7 => {
                    let mut show_shifted_symbols = self.app_settings.show_shifted_number_symbols;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr(lang, TrKey::ShiftedNumberSymbolsLabel),
                        true,
                        tooltip(crate::i18n::tr(lang, TrKey::ShiftedNumberSymbolsTooltip)),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_shifted_symbols",
                                &mut show_shifted_symbols,
                                switch_size,
                            );
                        },
                    );
                    if show_shifted_symbols != self.app_settings.show_shifted_number_symbols {
                        self.app_settings.show_shifted_number_symbols = show_shifted_symbols;
                        save_app_settings(&self.app_settings);
                    }
                }
                8 => {
                    let mut layer_hover_preview = self.app_settings.layer_hover_preview;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr(lang, TrKey::LayerHoverPreviewLabel),
                        true,
                        tooltip(crate::i18n::tr(lang, TrKey::LayerHoverPreviewTooltip)),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_layer_hover_preview",
                                &mut layer_hover_preview,
                                switch_size,
                            );
                        },
                    );
                    if layer_hover_preview != self.app_settings.layer_hover_preview {
                        self.app_settings.layer_hover_preview = layer_hover_preview;
                        if !layer_hover_preview {
                            self.hover_layer = None;
                        }
                        save_app_settings(&self.app_settings);
                    }
                }
                9 => {
                    let mut encoder_hover_enlarge = self.app_settings.encoder_hover_enlarge;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr(lang, TrKey::EncoderHoverZoomLabel),
                        true,
                        tooltip(crate::i18n::tr(lang, TrKey::EncoderHoverZoomTooltip)),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_encoder_hover_enlarge",
                                &mut encoder_hover_enlarge,
                                switch_size,
                            );
                        },
                    );
                    if encoder_hover_enlarge != self.app_settings.encoder_hover_enlarge {
                        self.app_settings.encoder_hover_enlarge = encoder_hover_enlarge;
                        save_app_settings(&self.app_settings);
                    }
                }
                10 => {
                    let mut selected_accent = self.app_settings.accent_color;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr(lang, TrKey::AccentColorLabel),
                        true,
                        tooltip(crate::i18n::tr(lang, TrKey::AccentColorTooltip)),
                        metrics.value(218.0),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 7.0;
                                for accent in AppAccentColor::ALL {
                                    let color = accent.color();
                                    let selected = accent == selected_accent;
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(26.0, 26.0),
                                        egui::Sense::click(),
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() {
                                        selected_accent = accent;
                                    }
                                    let stroke = if selected {
                                        Stroke::new(2.0_f32, color)
                                    } else {
                                        crate::ui_style::modal_outline_stroke(dark)
                                    };
                                    ui.painter().circle_filled(rect.center(), 8.5, color);
                                    ui.painter().circle_stroke(rect.center(), 11.0, stroke);
                                    if !suppress_tooltips {
                                        resp.on_hover_text(crate::i18n::tr_catalog(
                                            lang,
                                            accent.name(),
                                        ));
                                    }
                                }
                            });
                        },
                    );
                    if selected_accent != self.app_settings.accent_color {
                        self.app_settings.accent_color = selected_accent;
                        crate::ui_style::set_accent(selected_accent.color());
                        #[cfg(target_os = "windows")]
                        {
                            self.tray_icon = None;
                        }
                        save_app_settings(&self.app_settings);
                    }
                }
                11 => {
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(lang, "onboarding_tour.settings_row_label"),
                        true,
                        tooltip(crate::i18n::tr_catalog(
                            lang,
                            "onboarding_tour.settings_row_tooltip",
                        )),
                        metrics.settings_control_width(),
                        |ui| {
                            if crate::ui_style::modern_button(
                                ui,
                                crate::i18n::tr_catalog(lang, "onboarding_tour.show_again"),
                                metrics.size(168.0, 32.0),
                                true,
                            )
                            .clicked()
                            {
                                self.start_onboarding_tour(ui.ctx());
                            }
                        },
                    );
                }
                12 => {
                    let mut diagnostics_enabled = self.app_settings.diagnostics_enabled;
                    let diagnostics_log_path = crate::diagnostics::active_log_path_display();
                    let diagnostics_tooltip = crate::i18n::tr_catalog_format(
                        lang,
                        "ui.diagnostics_mode_tooltip",
                        &[("path", diagnostics_log_path.as_str())],
                    );
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(lang, "ui.diagnostics_mode_label"),
                        true,
                        (!suppress_tooltips).then_some(diagnostics_tooltip.as_str()),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_diagnostics_enabled",
                                &mut diagnostics_enabled,
                                switch_size,
                            );
                        },
                    );
                    if diagnostics_enabled != self.app_settings.diagnostics_enabled {
                        self.app_settings.diagnostics_enabled = diagnostics_enabled;
                        crate::diagnostics::set_enabled(diagnostics_enabled);
                        self.status_msg = if diagnostics_enabled {
                            crate::i18n::tr_catalog(lang, "ui.diagnostics_enabled_status").into()
                        } else {
                            crate::i18n::tr_catalog(lang, "ui.diagnostics_disabled_status").into()
                        };
                        save_app_settings(&self.app_settings);
                    }
                }
                13 => {
                    let mut show_signature = self.app_settings.show_made_by_signature;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(lang, "ui.show_made_by_signature_label"),
                        true,
                        tooltip(crate::i18n::tr_catalog(
                            lang,
                            "ui.show_made_by_signature_tooltip",
                        )),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_show_made_by_signature",
                                &mut show_signature,
                                switch_size,
                            );
                        },
                    );
                    if show_signature != self.app_settings.show_made_by_signature {
                        self.app_settings.show_made_by_signature = show_signature;
                        save_app_settings(&self.app_settings);
                    }
                }
                14 => {
                    let mut assign_transparent = self.app_settings.middle_click_assigns_transparent;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(lang, "ui.middle_click_transparent_label"),
                        true,
                        tooltip(crate::i18n::tr_catalog(
                            lang,
                            "ui.middle_click_transparent_tooltip",
                        )),
                        switch_width,
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "app_settings_middle_click_transparent",
                                &mut assign_transparent,
                                switch_size,
                            );
                        },
                    );
                    if assign_transparent != self.app_settings.middle_click_assigns_transparent {
                        self.app_settings.middle_click_assigns_transparent = assign_transparent;
                        save_app_settings(&self.app_settings);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                15 | 16 => {
                    let firmware = row_idx == 16;
                    let setting = if firmware {
                        &mut self.app_settings.firmware_update_notifications
                    } else {
                        &mut self.app_settings.app_update_notifications
                    };
                    let before = *setting;
                    let key = if firmware {
                        "notifications.firmware_label"
                    } else {
                        "notifications.app_label"
                    };
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(lang, key),
                        true,
                        tooltip(crate::i18n::tr_catalog(lang, "notifications.tooltip")),
                        switch_width,
                        |ui| {
                            crate::ui_style::settings_switch_sized_stable(
                                ui,
                                key,
                                setting,
                                switch_size,
                            );
                        },
                    );
                    if *setting != before {
                        self.authorize_notification_setting(firmware, before);
                        save_app_settings(&self.app_settings);
                    }
                }
                _ => {}
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn draw_linux_device_udev_rules_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        dark: bool,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let tooltip = (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
            lang,
            "ui.device_udev_rules_tooltip",
        ));

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "ui.device_udev_rules_label"),
            true,
            tooltip,
            metrics.settings_control_width(),
            |ui| {
                if super::device_scan::udev_rule_covers(
                    self.device_manager.devices(),
                    linux_udev_access(),
                ) {
                    draw_app_settings_value(
                        ui,
                        metrics,
                        crate::i18n::tr_catalog(lang, "ui.device_udev_rules_installed"),
                        app_muted_text(dark),
                    );
                } else if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "ui.install_device_udev_rules"),
                    metrics.size(168.0, 34.0),
                    true,
                )
                .clicked()
                {
                    self.run_linux_device_udev_rules_install();
                }
            },
        );
    }

    #[cfg(target_os = "linux")]
    pub(super) fn run_linux_device_udev_rules_install(&mut self) {
        let script = "linux/udev/install-vial-rules.sh";
        let Some(script_path) = crate::linux_setup::setup_script_path(script) else {
            self.status_msg = format!("Could not find {script}; run it from the Entropy folder");
            return;
        };

        let output = std::process::Command::new("sh").arg(&script_path).output();
        self.status_msg = match output {
            Ok(output) if output.status.success() => crate::i18n::tr_catalog(
                self.app_settings.language,
                "ui.device_udev_rules_installed_status",
            )
            .to_owned(),
            Ok(output) => {
                let details = command_output_summary(&output.stderr, &output.stdout);
                if details.is_empty() {
                    format!(
                        "Entropy device access rules install failed: {}",
                        output.status
                    )
                } else {
                    format!("Entropy device access rules install failed: {details}")
                }
            }
            Err(err) => format!("Could not run {}: {err}", script_path.display()),
        };
    }
}

fn total_app_settings_rows() -> usize {
    let rows = 16;
    #[cfg(not(target_arch = "wasm32"))]
    let rows = rows
        + if super::update_notifications::SUPPORTED {
            2
        } else {
            0
        };
    #[cfg(target_os = "linux")]
    {
        rows + 1
    }
    #[cfg(not(target_os = "linux"))]
    {
        rows
    }
}

fn app_settings_base_row_index(row_idx: usize) -> usize {
    #[cfg(target_os = "linux")]
    {
        if row_idx > 7 {
            return row_idx - 1;
        }
    }
    row_idx
}

#[cfg(target_os = "linux")]
fn draw_app_settings_value(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    text: &str,
    color: Color32,
) {
    let width = metrics.settings_control_width();
    ui.allocate_ui_with_layout(
        metrics.size(width, 34.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.add_sized(
                metrics.size(width, 34.0),
                egui::Label::new(RichText::new(text).size(metrics.value(12.0)).color(color))
                    .wrap()
                    .halign(egui::Align::RIGHT),
            );
        },
    );
}

/// What the installed udev rule lets the local session open.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct UdevAccess {
    /// Vial boards' hidraw nodes, on USB and over kernel Bluetooth HID.
    pub(super) vial: bool,
    /// Rynk boards' USB devices.
    pub(super) rynk: bool,
}

#[cfg(target_os = "linux")]
pub(super) fn linux_udev_access() -> UdevAccess {
    const RULE_PATHS: [&str; 4] = [
        "/etc/udev/rules.d/59-vial.rules",
        "/run/udev/rules.d/59-vial.rules",
        "/usr/lib/udev/rules.d/59-vial.rules",
        "/lib/udev/rules.d/59-vial.rules",
    ];

    RULE_PATHS
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .fold(UdevAccess::default(), |access, contents| {
            let found = udev_rule_access(&contents);
            UdevAccess {
                vial: access.vial || found.vial,
                rynk: access.rynk || found.rynk,
            }
        })
}

#[cfg(target_os = "linux")]
fn udev_rule_access(contents: &str) -> UdevAccess {
    // A v2 rule already has both hidraw lines, so a Vial user who installed it
    // is not asked again; only Rynk boards need what v3 adds.
    const VIAL_VERSION_MARKERS: [&str; 2] = [
        "# Entropy Vial hidraw access v2",
        "# Entropy keyboard access v3",
    ];
    const RYNK_VERSION_MARKER: &str = "# Entropy keyboard access v3";
    const USB_RULE_MARKER: &str = r#"ATTRS{serial}=="*vial:f64c2b3c*""#;
    const ERGOHAVEN_BLUETOOTH_RULE_MARKER: &str = r#"KERNELS=="0005:E126:*""#;
    // Rynk boards are claimed through usbfs, so their rule grants the USB
    // device itself rather than a hidraw node.
    const RYNK_USB_RULE_MARKER: &str = r#"ATTR{serial}=="rynk:*""#;

    UdevAccess {
        vial: VIAL_VERSION_MARKERS
            .iter()
            .any(|marker| contents.contains(marker))
            && contents.contains(USB_RULE_MARKER)
            && contents.contains(ERGOHAVEN_BLUETOOTH_RULE_MARKER)
            && contents.contains("SUBSYSTEM==\"hidraw\""),
        rynk: contents.contains(RYNK_VERSION_MARKER)
            && contents.contains(RYNK_USB_RULE_MARKER)
            && contents.contains(r#"ENV{DEVTYPE}=="usb_device""#),
    }
}

#[cfg(target_os = "linux")]
fn command_output_summary(primary: &[u8], fallback: &[u8]) -> String {
    let text = if primary.is_empty() {
        fallback
    } else {
        primary
    };
    String::from_utf8_lossy(text)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(all(test, target_os = "linux"))]
mod linux_udev_tests {
    use super::{udev_rule_access, UdevAccess};

    const OLD_RULE: &str = r#"KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{serial}=="*vial:f64c2b3c*", MODE="0660", GROUP="1000", TAG+="uaccess""#;
    const VIAL_ONLY_RULE: &str = r#"# Entropy Vial hidraw access v2
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{serial}=="*vial:f64c2b3c*", MODE="0660", GROUP="1000", TAG+="uaccess"
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", KERNELS=="0005:E126:*", MODE="0660", GROUP="1000", TAG+="uaccess""#;
    const CURRENT_RULE: &str = r#"# Entropy keyboard access v3
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{serial}=="*vial:f64c2b3c*", MODE="0660", GROUP="1000", TAG+="uaccess"
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", KERNELS=="0005:E126:*", MODE="0660", GROUP="1000", TAG+="uaccess"
SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="e126", ATTR{serial}=="rynk:*", MODE="0660", GROUP="1000", TAG+="uaccess""#;

    #[test]
    fn old_usb_only_rule_requires_upgrade() {
        assert_eq!(udev_rule_access(OLD_RULE), UdevAccess::default());
    }

    #[test]
    fn a_v2_rule_still_reaches_vial_boards_but_not_rynk_ones() {
        assert_eq!(
            udev_rule_access(VIAL_ONLY_RULE),
            UdevAccess {
                vial: true,
                rynk: false
            }
        );
    }

    #[test]
    fn current_rule_covers_usb_ergohaven_bluetooth_and_rynk() {
        assert_eq!(
            udev_rule_access(CURRENT_RULE),
            UdevAccess {
                vial: true,
                rynk: true
            }
        );
    }

    #[test]
    fn bundled_installer_writes_the_current_rule() {
        let installer = include_str!("../../linux/udev/install-vial-rules.sh");

        assert!(installer.contains("# Entropy keyboard access v3"));
        assert!(installer.contains(r#"KERNELS==\"0005:E126:*\""#));
        assert!(installer.contains(r#"ATTR{serial}==\"rynk:*\""#));
    }

    #[test]
    fn packaged_udev_rule_is_accepted() {
        let packaged = include_str!("../../packaging/linux/59-vial.rules");

        assert_eq!(
            udev_rule_access(packaged),
            UdevAccess {
                vial: true,
                rynk: true
            }
        );
    }
}

#[cfg(test)]
mod startup_device_ui_tests {
    use super::*;

    fn frame(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        width: f32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1400.0));
        crate::app::discard_texture_deltas(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            },
            |ui| {
                app.draw_app_settings_page(ui, rect);
            },
        ))
    }

    fn position(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    let p = t.pos + t.galley.size() / 2.0;
                    shape.clip_rect.contains(p).then_some(p)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible text: {text}"))
    }

    fn click(app: &mut EntropyApp, ctx: &egui::Context, width: f32, p: egui::Pos2) {
        frame(
            app,
            ctx,
            width,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(app, ctx, width, vec![]);
    }

    #[test]
    fn startup_screen_choice_waits_for_manual_pick_with_multiple_devices_after_restart() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let device = Device {
            name: "Phenom Mini".into(),
            vendor_id: 0xe126,
            product_id: 0x71,
            manufacturer: "Test".into(),
            serial_number: "123".into(),
            bus_type: "Usb".into(),
            path: "/test/phenom".into(),
            instance_token: String::new(),
            firmware: FirmwareProtocol::Vial,
        };
        app.device_manager.replace_devices(vec![device.clone()]);
        app.app_settings.preferred_startup_device =
            Some(StartupDevicePreference::from_device(&device));
        save_app_settings(&app.app_settings);
        frame(&mut app, &ctx, 800.0, vec![]);
        let output = frame(&mut app, &ctx, 800.0, vec![]);
        position(&output, "Multiple devices");
        let device_label = StartupDevicePreference::from_device(&device).name;
        click(&mut app, &ctx, 800.0, position(&output, &device_label));
        let output = frame(&mut app, &ctx, 800.0, vec![]);
        click(&mut app, &ctx, 800.0, position(&output, "Startup screen"));
        let settings = load_app_settings();
        assert!(settings.preferred_startup_device.is_none());
        let mut restarted = EntropyApp::new_inert_for_test();
        let mut other = device.clone();
        other.name = "Macropad".into();
        other.serial_number = "other-board".into();
        other.path = "/test/other".into();
        let (tx, requests) = mpsc::channel();
        restarted.test_connect_requests = Some(tx);
        #[cfg(target_os = "linux")]
        {
            restarted.test_device_scan_udev_access = Some(UdevAccess {
                vial: true,
                rynk: true,
            });
        }
        restarted.apply_device_scan_result(vec![]);
        restarted.apply_device_scan_result(vec![device.clone(), other.clone()]);
        restarted.apply_device_scan_result(vec![device.clone(), other]);
        assert!(
            requests.try_recv().is_err(),
            "multiple-device startup screen must wait for a manual pick"
        );
        assert!(restarted.selected_device.is_none());
        restarted.pick_device(0);
        assert_eq!(
            requests.try_recv().unwrap().0.stable_identity(),
            device.stable_identity()
        );
        assert!(restarted.startup_device.is_none());
        assert!(load_app_settings().preferred_startup_device.is_none());
    }

    #[test]
    fn startup_device_dropdown_saves_and_clears_preference_in_both_languages() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            for width in [800.0, 1200.0] {
                let ctx = egui::Context::default();
                let mut app = EntropyApp::new_inert_for_test();
                app.app_settings.language = language;
                let device = Device {
                    name: "Phenom Mini".into(),
                    vendor_id: 0xe126,
                    product_id: 0x71,
                    manufacturer: "Test".into(),
                    serial_number: "123".into(),
                    bus_type: "Usb".into(),
                    path: "/test/phenom".into(),
                    instance_token: String::new(),
                    firmware: FirmwareProtocol::Vial,
                };
                let preference = StartupDevicePreference::from_device(&device);
                let mut other = device.clone();
                other.name = "Macropad".into();
                other.serial_number = "456".into();
                other.path = "/test/macropad".into();
                let other_preference = StartupDevicePreference::from_device(&other);
                app.device_manager.replace_devices(vec![device, other]);
                let startup_screen = crate::i18n::tr_catalog(language, "ui.startup_device_screen");
                frame(&mut app, &ctx, width, vec![]);
                let output = frame(&mut app, &ctx, width, vec![]);
                position(
                    &output,
                    crate::i18n::tr_catalog(language, "ui.startup_device_label"),
                );
                click(&mut app, &ctx, width, position(&output, startup_screen));
                let output = frame(&mut app, &ctx, width, vec![]);
                position(&output, &other_preference.name);
                assert!(
                    output.shapes.iter().all(|shape| !matches!(
                        &shape.shape, egui::Shape::Text(t)
                            if ["Automatic", "Автоматически"].contains(&t.galley.text())
                    )),
                    "Automatic must not appear in the startup dropdown"
                );
                click(&mut app, &ctx, width, position(&output, &preference.name));
                assert_eq!(
                    load_app_settings().preferred_startup_device,
                    Some(preference.clone())
                );
                assert!(
                    matches!(app.startup_device, Some(StartupDeviceTarget::Screen)),
                    "settings apply on the next launch"
                );
                let output = frame(&mut app, &ctx, width, vec![]);
                click(&mut app, &ctx, width, position(&output, &preference.name));
                let output = frame(&mut app, &ctx, width, vec![]);
                click(&mut app, &ctx, width, position(&output, startup_screen));
                assert!(load_app_settings().preferred_startup_device.is_none());
            }
        }
    }
}
