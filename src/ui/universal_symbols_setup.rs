use super::*;

#[cfg(target_os = "macos")]
const MACOS_UNIVERSAL_SYMBOLS_SETUP_ROWS: usize = 11;

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum MacosPermissionStatusKind {
    Accessibility,
    InputMonitoring,
}

impl EntropyApp {
    pub(super) fn draw_universal_symbols_setup_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let content_width = metrics.settings_content_width();

        #[cfg(target_os = "macos")]
        {
            self.draw_macos_universal_symbols_setup_page(
                ui,
                content_rect,
                metrics,
                lang,
                dark,
                content_width,
            );
            return;
        }

        ui.allocate_ui_at_rect(content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.allocate_ui_with_layout(
                    Vec2::new(content_width, 0.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.add_sized(
                            Vec2::new(content_width, metrics.value(24.0)),
                            egui::Label::new(
                                RichText::new(crate::i18n::tr(
                                    lang,
                                    crate::i18n::Key::UniversalSymbolsSetupTitle,
                                ))
                                .size(metrics.value(18.0))
                                .strong(),
                            )
                            .halign(egui::Align::Center),
                        );
                        ui.add_space(metrics.value(6.0));
                        ui.add_sized(
                            Vec2::new(content_width, metrics.value(34.0)),
                            egui::Label::new(
                                RichText::new(crate::i18n::tr_catalog(
                                    lang,
                                    universal_symbols_intro_key(),
                                ))
                                .size(metrics.value(13.0))
                                .color(app_muted_text(dark)),
                            )
                            .wrap()
                            .halign(egui::Align::Center),
                        );
                        ui.add_space(metrics.value(18.0));

                        self.draw_universal_symbols_setup_rows(ui, metrics, lang, dark);

                        ui.add_space(metrics.value(18.0));
                        self.draw_universal_symbols_setup_actions(ui, metrics);
                        if !self.status_msg.is_empty() {
                            ui.add_space(metrics.value(12.0));
                            ui.add_sized(
                                Vec2::new(content_width, metrics.value(36.0)),
                                egui::Label::new(
                                    RichText::new(&self.status_msg)
                                        .size(metrics.value(11.5))
                                        .color(app_muted_text(dark)),
                                )
                                .wrap()
                                .halign(egui::Align::Center),
                            );
                        }
                    },
                );
            });
        });
    }

    #[cfg(target_os = "macos")]
    fn draw_macos_universal_symbols_setup_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        content_width: f32,
    ) {
        ui.allocate_ui_at_rect(content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.add_sized(
                    Vec2::new(content_width, metrics.value(24.0)),
                    egui::Label::new(
                        RichText::new(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::UniversalSymbolsSetupTitle,
                        ))
                        .size(metrics.value(18.0))
                        .strong(),
                    )
                    .halign(egui::Align::Center),
                );
                ui.add_space(metrics.value(6.0));
                ui.add_sized(
                    Vec2::new(content_width, metrics.value(34.0)),
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(lang, universal_symbols_intro_key()))
                            .size(metrics.value(13.0))
                            .color(app_muted_text(dark)),
                    )
                    .wrap()
                    .halign(egui::Align::Center),
                );
                ui.add_space(metrics.value(18.0));

                let bottom_reserve = if self.status_msg.is_empty() {
                    metrics.value(8.0)
                } else {
                    metrics.value(52.0)
                };
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "universal_symbols_setup_macos",
                    metrics,
                    MACOS_UNIVERSAL_SYMBOLS_SETUP_ROWS,
                    bottom_reserve,
                );

                ui.allocate_ui_at_rect(list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row_idx in list.first_visible_row..list.last_visible_row {
                        self.draw_macos_universal_symbols_setup_row(
                            ui,
                            row_idx,
                            list.row_content_width,
                            list.row_height,
                            metrics,
                            lang,
                            dark,
                            list.suppress_tooltips,
                        );
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

                if !self.status_msg.is_empty() {
                    ui.add_space(metrics.value(12.0));
                    ui.add_sized(
                        Vec2::new(content_width, metrics.value(36.0)),
                        egui::Label::new(
                            RichText::new(&self.status_msg)
                                .size(metrics.value(11.5))
                                .color(app_muted_text(dark)),
                        )
                        .wrap()
                        .halign(egui::Align::Center),
                    );
                }
            });
        });
    }

    #[cfg(target_os = "macos")]
    fn draw_macos_universal_symbols_setup_row(
        &mut self,
        ui: &mut egui::Ui,
        row_idx: usize,
        row_content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        suppress_tooltips: bool,
    ) {
        let tooltip =
            |key: &'static str| (!suppress_tooltips).then_some(crate::i18n::tr_catalog(lang, key));
        match row_idx {
            0 => crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_content_width,
                row_height,
                crate::i18n::tr_catalog(lang, "universal_symbols_setup.current_backend"),
                true,
                tooltip("universal_symbols_setup.current_backend_tooltip"),
                metrics.settings_control_width(),
                |ui| {
                    draw_universal_symbols_value(
                        ui,
                        metrics,
                        168.0,
                        crate::i18n::tr_catalog(lang, universal_symbols_backend_value_key()),
                        ui.visuals().text_color(),
                    );
                },
            ),
            1 => self.draw_macos_permission_status_row(
                ui,
                row_content_width,
                row_height,
                metrics,
                lang,
                dark,
                "universal_symbols_setup.accessibility_status",
                "universal_symbols_setup.accessibility_status_tooltip",
                MacosPermissionStatusKind::Accessibility,
                suppress_tooltips,
            ),
            2 => self.draw_macos_permission_status_row(
                ui,
                row_content_width,
                row_height,
                metrics,
                lang,
                dark,
                "universal_symbols_setup.input_monitoring_status",
                "universal_symbols_setup.input_monitoring_status_tooltip",
                MacosPermissionStatusKind::InputMonitoring,
                suppress_tooltips,
            ),
            3 => self.draw_macos_event_capture_status_row(
                ui,
                row_content_width,
                row_height,
                metrics,
                lang,
                dark,
                suppress_tooltips,
            ),
            4 => crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_content_width,
                row_height,
                crate::i18n::tr_catalog(lang, "universal_symbols_setup.recommended_setup"),
                true,
                tooltip("universal_symbols_setup.recommended_setup_tooltip"),
                metrics.settings_control_width(),
                |ui| self.draw_universal_symbols_recommended_control(ui, metrics, lang),
            ),
            5 => draw_universal_symbols_finish_step_row(
                ui,
                metrics,
                row_content_width,
                row_height,
                lang,
                dark,
                "universal_symbols_setup.finish_step_2",
                "universal_symbols_setup.finish_step_2_tooltip",
                universal_symbols_finish_step_2_key(),
                universal_symbols_finish_step_2_detail_key(),
            ),
            6 => draw_universal_symbols_finish_step_row(
                ui,
                metrics,
                row_content_width,
                row_height,
                lang,
                dark,
                "universal_symbols_setup.finish_step_3",
                "universal_symbols_setup.finish_step_3_tooltip",
                universal_symbols_finish_step_3_key(),
                universal_symbols_finish_step_3_detail_key(),
            ),
            7 => crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_content_width,
                row_height,
                crate::i18n::tr_catalog(lang, "universal_symbols_setup.text_expander"),
                true,
                tooltip("universal_symbols_setup.text_expander_tooltip"),
                metrics.value(220.0),
                |ui| {
                    draw_universal_symbols_value(
                        ui,
                        metrics,
                        220.0,
                        crate::i18n::tr_catalog(lang, universal_symbols_text_expander_key()),
                        app_muted_text(dark),
                    );
                },
            ),
            8 => {
                if draw_universal_symbols_action_row_with_tooltip_state(
                    ui,
                    metrics,
                    row_content_width,
                    row_height,
                    lang,
                    "universal_symbols_setup.open_accessibility_settings",
                    "universal_symbols_setup.open_accessibility_settings_tooltip",
                    "universal_symbols_setup.open_privacy_settings",
                    suppress_tooltips,
                ) {
                    self.open_macos_accessibility_settings(lang);
                }
            }
            9 => {
                if draw_universal_symbols_action_row_with_tooltip_state(
                    ui,
                    metrics,
                    row_content_width,
                    row_height,
                    lang,
                    "universal_symbols_setup.open_input_monitoring_settings",
                    "universal_symbols_setup.open_input_monitoring_settings_tooltip",
                    "universal_symbols_setup.open_privacy_settings",
                    suppress_tooltips,
                ) {
                    self.open_macos_input_monitoring_settings(lang);
                }
            }
            10
                if draw_universal_symbols_action_row_with_tooltip_state(
                    ui,
                    metrics,
                    row_content_width,
                    row_height,
                    lang,
                    "universal_symbols_setup.restart_event_tap",
                    "universal_symbols_setup.restart_event_tap_tooltip",
                    "universal_symbols_setup.restart_event_tap_button",
                    suppress_tooltips,
                ) => {
                    self.restart_macos_event_tap(lang);
                }
            _ => {}
        }
    }

    fn draw_universal_symbols_setup_rows(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
    ) {
        let row_height = metrics.settings_row_height();
        let row_content_width = metrics.settings_row_content_width();
        let tooltip = |key: &'static str| Some(crate::i18n::tr_catalog(lang, key));

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.current_backend"),
            true,
            tooltip("universal_symbols_setup.current_backend_tooltip"),
            metrics.settings_control_width(),
            |ui| {
                draw_universal_symbols_value(
                    ui,
                    metrics,
                    168.0,
                    crate::i18n::tr_catalog(lang, universal_symbols_backend_value_key()),
                    ui.visuals().text_color(),
                );
            },
        );

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.recommended_setup"),
            true,
            tooltip("universal_symbols_setup.recommended_setup_tooltip"),
            metrics.settings_control_width(),
            |ui| self.draw_universal_symbols_recommended_control(ui, metrics, lang),
        );

        draw_universal_symbols_section_label(
            ui,
            metrics,
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.next_step"),
        );

        draw_universal_symbols_finish_step_row(
            ui,
            metrics,
            row_content_width,
            row_height,
            lang,
            dark,
            "universal_symbols_setup.finish_step_2",
            "universal_symbols_setup.finish_step_2_tooltip",
            universal_symbols_finish_step_2_key(),
            universal_symbols_finish_step_2_detail_key(),
        );

        draw_universal_symbols_finish_step_row(
            ui,
            metrics,
            row_content_width,
            row_height,
            lang,
            dark,
            "universal_symbols_setup.finish_step_3",
            "universal_symbols_setup.finish_step_3_tooltip",
            universal_symbols_finish_step_3_key(),
            universal_symbols_finish_step_3_detail_key(),
        );

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.text_expander"),
            true,
            tooltip("universal_symbols_setup.text_expander_tooltip"),
            metrics.value(220.0),
            |ui| {
                draw_universal_symbols_value(
                    ui,
                    metrics,
                    220.0,
                    crate::i18n::tr_catalog(lang, universal_symbols_text_expander_key()),
                    app_muted_text(dark),
                );
            },
        );
    }

    fn draw_universal_symbols_recommended_control(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
    ) {
        #[cfg(target_os = "linux")]
        {
            match crate::smart_input::linux_recommended_input_backend() {
                crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                    draw_universal_symbols_value(
                        ui,
                        metrics,
                        168.0,
                        crate::i18n::tr_catalog(lang, "universal_symbols_setup.no_install_needed"),
                        ui.visuals().text_color(),
                    );
                }
                crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                    if crate::ui_style::modern_button(
                        ui,
                        crate::i18n::tr_catalog(lang, "universal_symbols_setup.setup_ibus"),
                        metrics.size(168.0, 34.0),
                        true,
                    )
                    .clicked()
                    {
                        self.run_linux_universal_symbols_setup(
                            "linux/ibus/install-user.sh",
                            "IBus",
                        );
                    }
                }
                crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                    if crate::ui_style::modern_button(
                        ui,
                        crate::i18n::tr_catalog(lang, "universal_symbols_setup.setup_fcitx5"),
                        metrics.size(168.0, 34.0),
                        true,
                    )
                    .clicked()
                    {
                        self.run_linux_universal_symbols_setup(
                            "linux/fcitx5/install-user.sh",
                            "Fcitx5",
                        );
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, "universal_symbols_setup.request_input_monitoring"),
                metrics.size(168.0, 34.0),
                true,
            )
            .clicked()
            {
                self.request_macos_input_monitoring_access(lang);
            }
        }

        #[cfg(target_os = "windows")]
        {
            draw_universal_symbols_value(
                ui,
                metrics,
                168.0,
                crate::i18n::tr_catalog(lang, "universal_symbols_setup.no_install_needed"),
                ui.visuals().text_color(),
            );
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            draw_universal_symbols_value(
                ui,
                metrics,
                168.0,
                crate::i18n::tr_catalog(lang, "universal_symbols_setup.unsupported"),
                app_muted_text(ui.visuals().dark_mode),
            );
        }
    }

    fn draw_universal_symbols_setup_actions(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        #[cfg(target_os = "windows")]
        {
            let _ = (ui, metrics);
        }

        #[cfg(target_os = "macos")]
        {
            let lang = self.app_settings.language;
            let row_height = metrics.settings_row_height();
            let row_content_width = metrics.settings_row_content_width();

            let accessibility_clicked = draw_universal_symbols_action_row(
                ui,
                metrics,
                row_content_width,
                row_height,
                lang,
                "universal_symbols_setup.open_accessibility_settings",
                "universal_symbols_setup.open_accessibility_settings_tooltip",
                "universal_symbols_setup.open_privacy_settings",
            );
            if accessibility_clicked {
                self.open_macos_accessibility_settings(lang);
            }

            let monitoring_clicked = draw_universal_symbols_action_row(
                ui,
                metrics,
                row_content_width,
                row_height,
                lang,
                "universal_symbols_setup.open_input_monitoring_settings",
                "universal_symbols_setup.open_input_monitoring_settings_tooltip",
                "universal_symbols_setup.open_privacy_settings",
            );
            if monitoring_clicked {
                self.open_macos_input_monitoring_settings(lang);
            }

            let restart_clicked = draw_universal_symbols_action_row(
                ui,
                metrics,
                row_content_width,
                row_height,
                lang,
                "universal_symbols_setup.restart_event_tap",
                "universal_symbols_setup.restart_event_tap_tooltip",
                "universal_symbols_setup.restart_event_tap_button",
            );
            if restart_clicked {
                self.restart_macos_event_tap(lang);
            }
        }

        #[cfg(target_os = "linux")]
        {
            let lang = self.app_settings.language;
            let row_height = metrics.settings_row_height();
            let row_content_width = metrics.settings_row_content_width();

            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        "universal_symbols_setup.advanced",
                    ))
                    .size(metrics.value(11.0))
                    .color(app_muted_text(ui.visuals().dark_mode)),
                );
            });
            ui.add_space(metrics.value(2.0));

            match crate::smart_input::linux_recommended_input_backend() {
                crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                    let ibus_clicked = draw_universal_symbols_action_row(
                        ui,
                        metrics,
                        row_content_width,
                        row_height,
                        lang,
                        "universal_symbols_setup.wayland_ibus",
                        "universal_symbols_setup.wayland_ibus_tooltip",
                        "universal_symbols_setup.setup_ibus",
                    );
                    if ibus_clicked {
                        self.run_linux_universal_symbols_setup(
                            "linux/ibus/install-user.sh",
                            "IBus",
                        );
                    }

                    let fcitx5_clicked = draw_universal_symbols_action_row(
                        ui,
                        metrics,
                        row_content_width,
                        row_height,
                        lang,
                        "universal_symbols_setup.wayland_fcitx5",
                        "universal_symbols_setup.wayland_fcitx5_tooltip",
                        "universal_symbols_setup.setup_fcitx5",
                    );
                    if fcitx5_clicked {
                        self.run_linux_universal_symbols_setup(
                            "linux/fcitx5/install-user.sh",
                            "Fcitx5",
                        );
                    }
                }
                crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                    let alternative_clicked = draw_universal_symbols_action_row(
                        ui,
                        metrics,
                        row_content_width,
                        row_height,
                        lang,
                        "universal_symbols_setup.alternative_backend",
                        "universal_symbols_setup.alternative_backend_tooltip",
                        "universal_symbols_setup.setup_fcitx5",
                    );
                    if alternative_clicked {
                        self.run_linux_universal_symbols_setup(
                            "linux/fcitx5/install-user.sh",
                            "Fcitx5",
                        );
                    }

                    let remove_clicked = draw_universal_symbols_action_row(
                        ui,
                        metrics,
                        row_content_width,
                        row_height,
                        lang,
                        "universal_symbols_setup.remove_ibus_source",
                        "universal_symbols_setup.remove_ibus_source_tooltip",
                        "universal_symbols_setup.remove_ibus",
                    );
                    if remove_clicked {
                        self.run_linux_universal_symbols_setup(
                            "linux/ibus/uninstall-user.sh",
                            "IBus",
                        );
                    }
                }
                crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                    let text_expansion_clicked = draw_universal_symbols_action_row(
                        ui,
                        metrics,
                        row_content_width,
                        row_height,
                        lang,
                        "universal_symbols_setup.text_expansion_backend",
                        "universal_symbols_setup.text_expansion_backend_tooltip",
                        "universal_symbols_setup.setup_ibus",
                    );
                    if text_expansion_clicked {
                        self.run_linux_universal_symbols_setup(
                            "linux/ibus/install-user.sh",
                            "IBus",
                        );
                    }

                    let remove_clicked = draw_universal_symbols_action_row(
                        ui,
                        metrics,
                        row_content_width,
                        row_height,
                        lang,
                        "universal_symbols_setup.remove_ibus_source",
                        "universal_symbols_setup.remove_ibus_source_tooltip",
                        "universal_symbols_setup.remove_ibus",
                    );
                    if remove_clicked {
                        self.run_linux_universal_symbols_setup(
                            "linux/ibus/uninstall-user.sh",
                            "IBus",
                        );
                    }
                }
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "universal_symbols_setup.no_setup_action",
                    ))
                    .size(11.0)
                    .color(app_muted_text(ui.visuals().dark_mode)),
                );
            });
        }
    }

    #[cfg(target_os = "linux")]
    fn run_linux_universal_symbols_setup(&mut self, script: &str, backend: &str) {
        let Some(script_path) = crate::linux_setup::setup_script_path(script) else {
            self.status_msg = format!("Could not find {script}; run it from the Entropy folder");
            return;
        };
        let output = std::process::Command::new("sh").arg(&script_path).output();
        self.status_msg = match output {
            Ok(output) if output.status.success() => crate::i18n::tr_catalog(
                self.app_settings.language,
                linux_setup_success_status_key(script, backend),
            )
            .to_owned(),
            Ok(output) => {
                let details = command_output_summary(&output.stderr, &output.stdout);
                let action = linux_setup_action_label(script);
                if details.is_empty() {
                    format!("{backend} {action} failed: {}", output.status)
                } else {
                    format!("{backend} {action} failed: {details}")
                }
            }
            Err(err) => format!("Could not run {}: {err}", script_path.display()),
        };
    }

    #[cfg(target_os = "macos")]
    fn draw_macos_permission_status_row(
        &mut self,
        ui: &mut egui::Ui,
        row_content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        label_key: &'static str,
        tooltip_key: &'static str,
        status_kind: MacosPermissionStatusKind,
        suppress_tooltips: bool,
    ) {
        let status = crate::smart_input::macos_universal_symbols_status();
        let granted = crate::i18n::tr_catalog(lang, "universal_symbols_setup.permission_granted");
        let denied = crate::i18n::tr_catalog(lang, "universal_symbols_setup.permission_denied");
        let is_granted = match status_kind {
            MacosPermissionStatusKind::Accessibility => status.accessibility_granted,
            MacosPermissionStatusKind::InputMonitoring => status.input_monitoring_granted,
        };

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_content_width,
            row_height,
            crate::i18n::tr_catalog(lang, label_key),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(lang, tooltip_key)),
            metrics.value(250.0),
            |ui| {
                draw_universal_symbols_value(
                    ui,
                    metrics,
                    250.0,
                    if is_granted { granted } else { denied },
                    macos_permission_color(is_granted, dark),
                );
            },
        );
    }

    #[cfg(target_os = "macos")]
    fn draw_macos_event_capture_status_row(
        &mut self,
        ui: &mut egui::Ui,
        row_content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        suppress_tooltips: bool,
    ) {
        let status = crate::smart_input::macos_universal_symbols_status();
        let active = crate::i18n::tr_catalog(lang, "universal_symbols_setup.event_tap_active");
        let inactive = crate::i18n::tr_catalog(lang, "universal_symbols_setup.event_tap_inactive");
        let capture_detail = macos_event_capture_detail(lang, &status);

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.event_capture_status"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                "universal_symbols_setup.event_capture_status_tooltip",
            )),
            metrics.value(250.0),
            |ui| {
                let status_label = if status.event_tap_active {
                    active
                } else {
                    inactive
                };
                if capture_detail.is_empty() {
                    draw_universal_symbols_value(
                        ui,
                        metrics,
                        250.0,
                        status_label,
                        macos_permission_color(status.event_tap_active, dark),
                    );
                } else {
                    draw_universal_symbols_two_line_value(
                        ui,
                        metrics,
                        250.0,
                        status_label,
                        &capture_detail,
                        dark,
                    );
                }
            },
        );
    }

    #[cfg(target_os = "macos")]
    fn request_macos_input_monitoring_access(&mut self, lang: crate::i18n::Language) {
        crate::smart_input::request_input_monitoring_access();
        self.status_msg = crate::i18n::tr_catalog(
            lang,
            "universal_symbols_setup.input_monitoring_requested_status",
        )
        .to_string();
        crate::smart_input::restart_event_tap();
    }

    #[cfg(target_os = "macos")]
    fn restart_macos_event_tap(&mut self, lang: crate::i18n::Language) {
        crate::smart_input::restart_event_tap();
        self.status_msg =
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.event_tap_restarted_status")
                .to_string();
    }

    #[cfg(target_os = "macos")]
    fn open_macos_accessibility_settings(&mut self, lang: crate::i18n::Language) {
        let result = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status();
        self.status_msg = if matches!(result, Ok(status) if status.success()) {
            crate::i18n::tr_catalog(lang, "universal_symbols_setup.macos_privacy_opened_status")
                .to_string()
        } else {
            crate::i18n::tr_catalog(
                lang,
                "universal_symbols_setup.macos_privacy_open_failed_status",
            )
            .to_string()
        };
    }

    #[cfg(target_os = "macos")]
    fn open_macos_input_monitoring_settings(&mut self, lang: crate::i18n::Language) {
        let result = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .status();
        self.status_msg = if matches!(result, Ok(status) if status.success()) {
            crate::i18n::tr_catalog(
                lang,
                "universal_symbols_setup.macos_input_monitoring_opened_status",
            )
            .to_string()
        } else {
            crate::i18n::tr_catalog(
                lang,
                "universal_symbols_setup.macos_privacy_open_failed_status",
            )
            .to_string()
        };
    }
}

#[cfg(target_os = "macos")]
fn macos_permission_color(granted: bool, dark: bool) -> Color32 {
    if granted {
        app_accent()
    } else {
        app_muted_text(dark)
    }
}

#[cfg(target_os = "macos")]
fn macos_event_capture_detail(
    lang: crate::i18n::Language,
    status: &crate::smart_input::MacosUniversalSymbolsStatus,
) -> String {
    if let Some(reason) = &status.failure_reason {
        return reason.clone();
    }
    if let Some(ms) = status.last_event_ms_ago {
        if ms < 30_000 {
            return crate::i18n::tr_catalog(lang, "universal_symbols_setup.last_key_event_recent")
                .replace("{seconds}", &format!("{:.1}", ms as f64 / 1000.0));
        }
    }
    if status.event_tap_active {
        return crate::i18n::tr_catalog(lang, "universal_symbols_setup.waiting_for_key_event")
            .to_string();
    }
    String::new()
}

fn draw_universal_symbols_value(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    width: f32,
    text: &str,
    color: Color32,
) {
    ui.allocate_ui_with_layout(
        metrics.size(width, 44.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.add_sized(
                metrics.size(width, 44.0),
                egui::Label::new(RichText::new(text).size(metrics.value(12.0)).color(color))
                    .wrap()
                    .halign(egui::Align::RIGHT),
            );
        },
    );
}

fn draw_universal_symbols_two_line_value(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    width: f32,
    primary: &str,
    detail: &str,
    dark: bool,
) {
    let (rect, _) = ui.allocate_exact_size(metrics.size(width, 44.0), egui::Sense::hover());
    let x = rect.right();
    ui.painter().text(
        egui::pos2(x, rect.center().y - metrics.value(7.0)),
        egui::Align2::RIGHT_CENTER,
        primary,
        egui::FontId::proportional(metrics.value(12.0)),
        ui.visuals().text_color(),
    );
    ui.painter().text(
        egui::pos2(x, rect.center().y + metrics.value(8.5)),
        egui::Align2::RIGHT_CENTER,
        detail,
        egui::FontId::proportional(metrics.value(10.5)),
        app_muted_text(dark),
    );
}

fn draw_universal_symbols_section_label(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    label: &str,
) {
    ui.add_space(metrics.value(8.0));
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(label)
                .size(metrics.value(11.0))
                .color(app_muted_text(ui.visuals().dark_mode)),
        );
    });
    ui.add_space(metrics.value(2.0));
}

fn draw_universal_symbols_finish_step_row(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    row_content_width: f32,
    row_height: f32,
    lang: crate::i18n::Language,
    dark: bool,
    label_key: &'static str,
    tooltip_key: &'static str,
    value_key: &'static str,
    detail_key: Option<&'static str>,
) {
    crate::ui_style::settings_list_row_with_tooltip(
        ui,
        row_content_width,
        row_height,
        crate::i18n::tr_catalog(lang, label_key),
        true,
        Some(crate::i18n::tr_catalog(lang, tooltip_key)),
        metrics.value(250.0),
        |ui| {
            if let Some(detail_key) = detail_key {
                draw_universal_symbols_two_line_value(
                    ui,
                    metrics,
                    250.0,
                    crate::i18n::tr_catalog(lang, value_key),
                    crate::i18n::tr_catalog(lang, detail_key),
                    dark,
                );
            } else {
                draw_universal_symbols_value(
                    ui,
                    metrics,
                    250.0,
                    crate::i18n::tr_catalog(lang, value_key),
                    app_muted_text(dark),
                );
            }
        },
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn draw_universal_symbols_action_row(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    row_content_width: f32,
    row_height: f32,
    lang: crate::i18n::Language,
    label_key: &'static str,
    tooltip_key: &'static str,
    button_key: &'static str,
) -> bool {
    draw_universal_symbols_action_row_with_tooltip_state(
        ui,
        metrics,
        row_content_width,
        row_height,
        lang,
        label_key,
        tooltip_key,
        button_key,
        false,
    )
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn draw_universal_symbols_action_row_with_tooltip_state(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    row_content_width: f32,
    row_height: f32,
    lang: crate::i18n::Language,
    label_key: &'static str,
    tooltip_key: &'static str,
    button_key: &'static str,
    suppress_tooltips: bool,
) -> bool {
    let mut clicked = false;
    crate::ui_style::settings_list_row_with_tooltip(
        ui,
        row_content_width,
        row_height,
        crate::i18n::tr_catalog(lang, label_key),
        true,
        (!suppress_tooltips).then_some(crate::i18n::tr_catalog(lang, tooltip_key)),
        metrics.settings_control_width(),
        |ui| {
            clicked = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, button_key),
                metrics.size(168.0, 34.0),
                true,
            )
            .clicked();
        },
    );
    clicked
}

fn universal_symbols_intro_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                "universal_symbols_setup.intro_linux_x11"
            }
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                "universal_symbols_setup.intro_linux_ibus"
            }
            crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                "universal_symbols_setup.intro_linux_fcitx5"
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        "universal_symbols_setup.intro_windows"
    }
    #[cfg(target_os = "macos")]
    {
        "universal_symbols_setup.intro_macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "universal_symbols_setup.intro_unsupported"
    }
}

fn universal_symbols_backend_value_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                "universal_symbols_setup.backend_linux_x11"
            }
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                "universal_symbols_setup.backend_linux_ibus"
            }
            crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                "universal_symbols_setup.backend_linux_fcitx5"
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        "universal_symbols_setup.backend_windows"
    }
    #[cfg(target_os = "macos")]
    {
        "universal_symbols_setup.backend_macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "universal_symbols_setup.unsupported"
    }
}

fn universal_symbols_finish_step_2_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                "universal_symbols_setup.finish_step_2_x11"
            }
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                "universal_symbols_setup.finish_step_2_ibus"
            }
            crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                "universal_symbols_setup.finish_step_2_fcitx5"
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        "universal_symbols_setup.finish_step_2_windows"
    }
    #[cfg(target_os = "macos")]
    {
        "universal_symbols_setup.finish_step_2_macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "universal_symbols_setup.unsupported"
    }
}

fn universal_symbols_finish_step_2_detail_key() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                Some("universal_symbols_setup.finish_step_2_ibus_detail")
            }
            _ => None,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn universal_symbols_finish_step_3_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                "universal_symbols_setup.finish_step_3_x11"
            }
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                "universal_symbols_setup.finish_step_3_ibus"
            }
            crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                "universal_symbols_setup.finish_step_3_fcitx5"
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        "universal_symbols_setup.finish_step_3_windows"
    }
    #[cfg(target_os = "macos")]
    {
        "universal_symbols_setup.finish_step_3_macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "universal_symbols_setup.unsupported"
    }
}

fn universal_symbols_finish_step_3_detail_key() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                Some("universal_symbols_setup.finish_step_3_ibus_detail")
            }
            _ => None,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn universal_symbols_text_expander_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        match crate::smart_input::linux_recommended_input_backend() {
            crate::smart_input::LinuxRecommendedInputBackend::X11Native => {
                "universal_symbols_setup.text_expander_x11"
            }
            crate::smart_input::LinuxRecommendedInputBackend::IBus => {
                "universal_symbols_setup.text_expander_ibus"
            }
            crate::smart_input::LinuxRecommendedInputBackend::Fcitx5 => {
                "universal_symbols_setup.text_expander_fcitx5"
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        "universal_symbols_setup.text_expander_native"
    }
    #[cfg(target_os = "macos")]
    {
        "universal_symbols_setup.text_expander_native"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "universal_symbols_setup.unsupported"
    }
}

#[cfg(target_os = "linux")]
fn linux_setup_success_status_key(script: &str, backend: &str) -> &'static str {
    if script.contains("uninstall") {
        "universal_symbols_setup.ibus_uninstalled_status"
    } else if backend == "Fcitx5" {
        "universal_symbols_setup.fcitx5_installed_status"
    } else {
        "universal_symbols_setup.ibus_installed_status"
    }
}

#[cfg(target_os = "linux")]
fn linux_setup_action_label(script: &str) -> &'static str {
    if script.contains("uninstall") {
        "uninstall"
    } else {
        "install"
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
