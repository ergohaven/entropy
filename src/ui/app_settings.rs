use super::*;

impl EntropyApp {
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
            if row_idx == 5 {
                self.draw_linux_vial_udev_rules_row(
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
                1 => {
                    let mut selected_key_legend_layout = self.app_settings.key_legend_layout;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(lang, "ui.key_legends_label"),
                        true,
                        tooltip(crate::i18n::tr_catalog(lang, "ui.key_legends_tooltip")),
                        metrics.settings_control_width(),
                        |ui| {
                            let dropdown_id = ui.make_persistent_id("app_key_legends_dropdown");
                            let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
                                ui,
                                dropdown_id,
                                crate::i18n::tr_catalog(
                                    lang,
                                    selected_key_legend_layout.i18n_key(),
                                ),
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
                                    for key_legend_layout in KeyLegendLayout::ALL {
                                        let selected =
                                            key_legend_layout == selected_key_legend_layout;
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
                                            crate::i18n::tr_catalog(
                                                lang,
                                                key_legend_layout.i18n_key(),
                                            ),
                                            FontId::proportional(metrics.value(12.0)),
                                            if selected {
                                                ui.visuals().text_color()
                                            } else {
                                                app_muted_text(dark)
                                            },
                                        );
                                        if option_resp.clicked() {
                                            selected_key_legend_layout = key_legend_layout;
                                            egui::Popup::close_all(ui.ctx());
                                        }
                                    }
                                },
                            );
                        },
                    );
                    if selected_key_legend_layout != self.app_settings.key_legend_layout {
                        self.app_settings.key_legend_layout = selected_key_legend_layout;
                        save_app_settings(&self.app_settings);
                    }
                }
                2 => {
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
                        }
                        save_app_settings(&self.app_settings);
                    }
                }
                3 => {
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
                4 => {
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
                5 => {
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
                6 => {
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
                7 => {
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
                8 => {
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
                9 => {
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
                10 => {
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
                11 => {
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
                12 => {
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
                _ => {}
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn draw_linux_vial_udev_rules_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        dark: bool,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let tooltip = (!suppress_tooltips)
            .then_some(crate::i18n::tr_catalog(lang, "ui.vial_udev_rules_tooltip"));

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "ui.vial_udev_rules_label"),
            true,
            tooltip,
            metrics.settings_control_width(),
            |ui| {
                if linux_vial_udev_rules_installed() {
                    draw_app_settings_value(
                        ui,
                        metrics,
                        crate::i18n::tr_catalog(lang, "ui.vial_udev_rules_installed"),
                        app_muted_text(dark),
                    );
                } else if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "ui.install_vial_udev_rules"),
                    metrics.size(168.0, 34.0),
                    true,
                )
                .clicked()
                {
                    self.run_linux_vial_udev_rules_install();
                }
            },
        );
    }

    #[cfg(target_os = "linux")]
    pub(super) fn run_linux_vial_udev_rules_install(&mut self) {
        let script = "linux/udev/install-vial-rules.sh";
        let Some(script_path) = crate::linux_setup::setup_script_path(script) else {
            self.status_msg = format!("Could not find {script}; run it from the Entropy folder");
            return;
        };

        let output = std::process::Command::new("sh").arg(&script_path).output();
        self.status_msg = match output {
            Ok(output) if output.status.success() => crate::i18n::tr_catalog(
                self.app_settings.language,
                "ui.vial_udev_rules_installed_status",
            )
            .to_owned(),
            Ok(output) => {
                let details = command_output_summary(&output.stderr, &output.stdout);
                if details.is_empty() {
                    format!("Vial udev rules install failed: {}", output.status)
                } else {
                    format!("Vial udev rules install failed: {details}")
                }
            }
            Err(err) => format!("Could not run {}: {err}", script_path.display()),
        };
    }
}

fn total_app_settings_rows() -> usize {
    let rows = 13;
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
        if row_idx > 5 {
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

#[cfg(target_os = "linux")]
pub(super) fn linux_vial_udev_rules_installed() -> bool {
    const RULE_PATHS: [&str; 4] = [
        "/etc/udev/rules.d/59-vial.rules",
        "/run/udev/rules.d/59-vial.rules",
        "/usr/lib/udev/rules.d/59-vial.rules",
        "/lib/udev/rules.d/59-vial.rules",
    ];

    RULE_PATHS.iter().any(|path| {
        std::fs::read_to_string(path)
            .map(|contents| vial_udev_rule_is_current(&contents))
            .unwrap_or(false)
    })
}

#[cfg(target_os = "linux")]
fn vial_udev_rule_is_current(contents: &str) -> bool {
    const VERSION_MARKER: &str = "# Entropy Vial hidraw access v2";
    const USB_RULE_MARKER: &str = r#"ATTRS{serial}=="*vial:f64c2b3c*""#;
    const ERGOHAVEN_BLUETOOTH_RULE_MARKER: &str = r#"KERNELS=="0005:E126:*""#;

    contents.contains(VERSION_MARKER)
        && contents.contains(USB_RULE_MARKER)
        && contents.contains(ERGOHAVEN_BLUETOOTH_RULE_MARKER)
        && contents.contains("SUBSYSTEM==\"hidraw\"")
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
    use super::vial_udev_rule_is_current;

    const OLD_RULE: &str = r#"KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{serial}=="*vial:f64c2b3c*", MODE="0660", GROUP="1000", TAG+="uaccess""#;
    const CURRENT_RULE: &str = r#"# Entropy Vial hidraw access v2
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{serial}=="*vial:f64c2b3c*", MODE="0660", GROUP="1000", TAG+="uaccess"
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", KERNELS=="0005:E126:*", MODE="0660", GROUP="1000", TAG+="uaccess""#;

    #[test]
    fn old_usb_only_rule_requires_upgrade() {
        assert!(!vial_udev_rule_is_current(OLD_RULE));
    }

    #[test]
    fn current_rule_covers_usb_and_ergohaven_bluetooth() {
        assert!(vial_udev_rule_is_current(CURRENT_RULE));
    }

    #[test]
    fn bundled_installer_writes_the_current_rule() {
        let installer = include_str!("../../linux/udev/install-vial-rules.sh");

        assert!(installer.contains("# Entropy Vial hidraw access v2"));
        assert!(installer.contains(r#"KERNELS==\"0005:E126:*\""#));
    }
}
