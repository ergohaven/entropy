use super::*;

impl EntropyApp {
    pub(super) fn dismiss_text_expander_rule_editor_if_page_inactive(&mut self) {
        if self.text_expander_rule_editor.is_some()
            && (self.main_menu_tab != MainMenuTab::Advanced
                || self.settings_tab != SettingsTab::TextExpander)
        {
            self.flush_pending_text_expander_settings();
            self.text_expander_rule_editor = None;
            self.text_expander_rule_editor_focus_pending = false;
        }
    }

    pub(super) fn draw_text_expander_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        #[cfg(target_os = "linux")]
        self.poll_text_expander_probe(ui.ctx());
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(lang, "text_expander.title"))
                        .size(metrics.value(18.0))
                        .strong(),
                );
                ui.add_space(metrics.value(6.0));
                ui.add_sized(
                    Vec2::new(metrics.settings_content_width(), metrics.value(34.0)),
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(lang, "text_expander.description"))
                            .size(metrics.value(13.0))
                            .color(app_muted_text(dark)),
                    )
                    .wrap()
                    .halign(egui::Align::Center),
                );
                ui.add_sized(
                    Vec2::new(metrics.settings_content_width(), metrics.value(28.0)),
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(lang, "text_expander.quick_help"))
                            .size(metrics.value(11.5))
                            .color(app_muted_text(dark)),
                    )
                    .wrap()
                    .halign(egui::Align::Center),
                );
                ui.add_space(metrics.value(10.0));

                let rule_row_count = self.app_settings.text_expansion_rules.len().max(1);
                let row_count = text_expander_setup_row_count() + 4 + rule_row_count;
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "text_expander_settings",
                    metrics,
                    row_count,
                    metrics.value(44.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_text_expander_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
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

                let button_size = metrics.size(126.0, 34.0);
                let button_gap = metrics.value(10.0);
                let actions_rect = fixed_settings_action_bar_rect(
                    list.viewport,
                    metrics,
                    button_size,
                    2,
                    button_gap,
                );
                crate::ui_style::allocate_ui_at_rect(ui, actions_rect, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = button_gap;
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "text_expander.add_rule"),
                            button_size,
                            true,
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            lang,
                            "text_expander.add_rule_tooltip",
                        ))
                        .clicked()
                        {
                            self.app_settings
                                .text_expansion_rules
                                .push(crate::text_expander::TextExpansionRule::default());
                            self.save_text_expander_settings();
                        }

                        let restore_enabled = !self.text_expander_deleted_rules.is_empty();
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "text_expander.restore_deleted_rule"),
                            button_size,
                            restore_enabled,
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            lang,
                            "text_expander.restore_deleted_rule_tooltip",
                        ))
                        .clicked()
                            && restore_enabled
                        {
                            if let Some((rule_idx, rule)) = self.text_expander_deleted_rules.pop() {
                                let insert_idx =
                                    rule_idx.min(self.app_settings.text_expansion_rules.len());
                                self.app_settings
                                    .text_expansion_rules
                                    .insert(insert_idx, rule);
                                self.save_text_expander_settings();
                            }
                        }
                    });
                });
            });
        });
        self.draw_text_expander_rule_editor(ui.ctx(), metrics);
        #[cfg(target_os = "linux")]
        self.draw_text_expander_ibus_modal(ui.ctx(), metrics);
    }

    fn draw_text_expander_rule_editor(
        &mut self,
        ctx: &egui::Context,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        let Some((idx, focus_field)) = self.text_expander_rule_editor else {
            return;
        };
        let Some(original) = self.app_settings.text_expansion_rules.get(idx).cloned() else {
            self.text_expander_rule_editor = None;
            return;
        };
        let mut rule = original.clone();
        let focus_pending = std::mem::take(&mut self.text_expander_rule_editor_focus_pending);
        let lang = self.app_settings.language;
        let viewport = ctx.content_rect().size();
        let size = metrics.size(520.0, 370.0).min(viewport - Vec2::splat(32.0));
        let width = (size.x - metrics.value(42.0)).max(metrics.value(220.0));
        let replacement_height = (size.y - metrics.value(152.0)).max(metrics.value(100.0));
        let mut open = true;
        let mut done = false;
        crate::ui_style::centered_modal_window(
            ctx,
            crate::i18n::tr_catalog(lang, "text_expander.rule_editor_title"),
            egui::Id::new("text_expander_rule_editor"),
            &mut open,
            size,
        )
        .movable(false)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.label(crate::i18n::tr_catalog(lang, "text_expander.trigger_label"));
            ui.add_space(metrics.value(4.0));
            let trigger = crate::ui_style::modern_text_field_sized(
                ui,
                ui.make_persistent_id(("text_expander_trigger", idx)),
                &mut rule.trigger,
                width,
                metrics.settings_control_height(),
                crate::i18n::tr_catalog(lang, "text_expander.trigger_hint"),
                32,
                egui::Align::Min,
            );
            if focus_pending && focus_field == TextExpanderRuleField::Trigger {
                trigger.request_focus();
            }
            ui.add_space(metrics.value(12.0));
            ui.label(crate::i18n::tr_catalog(
                lang,
                "text_expander.replacement_label",
            ));
            ui.add_space(metrics.value(4.0));
            let replacement = crate::ui_style::modern_multiline_text_field_sized(
                ui,
                ui.make_persistent_id(("text_expander_replacement", idx)),
                &mut rule.replacement,
                egui::vec2(width, replacement_height),
                crate::i18n::tr_catalog(lang, "text_expander.replacement_hint"),
                480,
            );
            if focus_pending && focus_field == TextExpanderRuleField::Replacement {
                replacement.request_focus();
            }
            ui.add_space(metrics.value(14.0));
            ui.vertical_centered(|ui| {
                done = crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "text_expander.rule_editor_done"),
                    metrics.size(108.0, 32.0),
                    true,
                )
                .clicked();
            });
        });

        if rule != original {
            self.app_settings.text_expansion_rules[idx] = rule;
            self.queue_text_expander_settings_save(ctx.input(|input| input.time));
        }
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if !open || done || escape {
            if escape {
                ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            self.flush_pending_text_expander_settings();
            self.text_expander_rule_editor = None;
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub(super) fn draw_text_expander_backend_settings_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        suppress_tooltips: bool,
    ) {
        let button_size = metrics.size(148.0, 30.0);
        #[cfg(target_os = "linux")]
        let registration = self.ibus_registration.get();
        #[cfg(target_os = "linux")]
        ui.ctx()
            .request_repaint_after(crate::linux_setup::IBUS_REGISTRATION_MAX_AGE);
        #[cfg(target_os = "linux")]
        let action = text_expander_ibus_action(registration);
        #[cfg(target_os = "linux")]
        let setup_running = self.linux_setup_task.is_some() || self.pending_ibus_reload.is_some();
        #[cfg(target_os = "linux")]
        let delete_size = metrics.size(78.0, 30.0);
        #[cfg(target_os = "linux")]
        let button_gap = metrics.value(8.0);
        #[cfg(target_os = "linux")]
        let control_width = button_size.x + button_gap + delete_size.x;
        #[cfg(not(target_os = "linux"))]
        let control_width = button_size.x;

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, text_expander_backend_label_key()),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                text_expander_backend_hint_key(),
            )),
            control_width,
            |ui| {
                #[cfg(target_os = "linux")]
                {
                    ui.spacing_mut().item_spacing.x = button_gap;
                    let primary_key = text_expander_ibus_button_key(action, setup_running);
                    // The IBus NixOS runs never loads a copy installed for one
                    // user; only the NixOS module registers the engine there.
                    let install_blocked = action != TextExpanderIbusAction::Reload
                        && crate::linux_text_expander::nixos();
                    let primary_tooltip = match action {
                        _ if install_blocked => "text_expander.nixos_install_tooltip",
                        TextExpanderIbusAction::Install => text_expander_backend_hint_key(),
                        TextExpanderIbusAction::Reinstall => "text_expander.reinstall_ibus_tooltip",
                        TextExpanderIbusAction::Reload => {
                            "universal_symbols_setup.reload_ibus_tooltip"
                        }
                    };
                    if crate::ui_style::modern_button(
                        ui,
                        crate::i18n::tr_catalog(lang, primary_key),
                        button_size,
                        !setup_running && !install_blocked,
                    )
                    .on_hover_text(crate::i18n::tr_catalog(lang, primary_tooltip))
                    .clicked()
                    {
                        match action {
                            TextExpanderIbusAction::Install | TextExpanderIbusAction::Reinstall => {
                                self.run_linux_universal_symbols_setup(
                                    "linux/ibus/install-user.sh",
                                    "IBus",
                                );
                            }
                            TextExpanderIbusAction::Reload => self.start_linux_ibus_reload(),
                        }
                    }
                    if crate::ui_style::modern_button(
                        ui,
                        crate::i18n::tr_catalog(lang, "text_expander.delete_ibus_source"),
                        delete_size,
                        text_expander_ibus_delete_enabled(registration, setup_running),
                    )
                    .on_hover_text(crate::i18n::tr_catalog(
                        lang,
                        "universal_symbols_setup.remove_ibus_source_tooltip",
                    ))
                    .clicked()
                    {
                        self.run_linux_universal_symbols_setup(
                            "linux/ibus/uninstall-user.sh",
                            "IBus",
                        );
                    }
                }
                #[cfg(not(target_os = "linux"))]
                if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, text_expander_backend_button_key()),
                    button_size,
                    true,
                )
                .on_hover_text(crate::i18n::tr_catalog(
                    lang,
                    text_expander_backend_hint_key(),
                ))
                .clicked()
                {
                    self.open_text_expander_setup_page();
                }
            },
        );
    }

    /// The rows above the Text Expander switch, by index: the backend and, on
    /// Linux, where Text Expander stands and GNOME's input sources.
    #[cfg(not(target_os = "windows"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_text_expander_setup_row(
        &mut self,
        row: usize,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        suppress_tooltips: bool,
    ) {
        match row {
            0 => self.draw_text_expander_backend_settings_row(
                ui,
                content_width,
                row_height,
                metrics,
                lang,
                suppress_tooltips,
            ),
            #[cfg(target_os = "linux")]
            1 => self.draw_text_expander_status_row(
                ui,
                content_width,
                row_height,
                metrics,
                lang,
                suppress_tooltips,
            ),
            #[cfg(target_os = "linux")]
            _ => self.draw_text_expander_gnome_sources_row(
                ui,
                content_width,
                row_height,
                metrics,
                lang,
                suppress_tooltips,
            ),
            #[cfg(not(target_os = "linux"))]
            _ => {}
        }
    }

    /// Keeps the look at the session fresh while the page is open: taken on a
    /// worker thread every few seconds, and at once after an action of ours.
    #[cfg(target_os = "linux")]
    fn poll_text_expander_probe(&mut self, ctx: &egui::Context) {
        if let Some((started, receiver)) = &self.pending_text_expander_probe {
            match receiver.try_recv() {
                Ok(probe) => {
                    self.text_expander_probe = Some((*started, probe));
                    self.pending_text_expander_probe = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending_text_expander_probe = None;
                }
            }
        }
        let fresh = self
            .text_expander_probe
            .is_some_and(|(taken, _)| taken.elapsed() < TEXT_EXPANDER_PROBE_MAX_AGE);
        if !fresh && self.pending_text_expander_probe.is_none() {
            let (sender, receiver) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("entropy-text-expander-probe".to_owned())
                .spawn(move || {
                    let _ = sender.send(crate::linux_text_expander::probe());
                });
            if spawned.is_ok() {
                self.pending_text_expander_probe = Some((std::time::Instant::now(), receiver));
            }
        }
        ctx.request_repaint_after(TEXT_EXPANDER_PROBE_MAX_AGE);
    }

    /// Forgets the session as last seen, after an action of ours changed it.
    #[cfg(target_os = "linux")]
    pub(super) fn invalidate_text_expander_probe(&mut self) {
        self.text_expander_probe = None;
        self.pending_text_expander_probe = None;
    }

    /// The last look at the session, while it is recent enough to act on: a
    /// page reopened after a while shows "Checking…" until a new one is in.
    #[cfg(target_os = "linux")]
    fn current_text_expander_probe(&self) -> Option<crate::linux_text_expander::TextExpanderProbe> {
        self.text_expander_probe
            .filter(|(started, _)| started.elapsed() < TEXT_EXPANDER_PROBE_TRUSTED)
            .map(|(_, probe)| probe)
    }

    #[cfg(target_os = "linux")]
    fn draw_text_expander_status_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        suppress_tooltips: bool,
    ) {
        let registration = self.ibus_registration.get();
        let probe = self.current_text_expander_probe();
        let state = text_expander_setup_state(registration, probe.as_ref());
        let desktop = probe.map_or_else(crate::linux_text_expander::linux_desktop, |probe| {
            probe.desktop
        });
        let hint = text_expander_setup_hint_key(
            state,
            desktop,
            probe.and_then(|probe| probe.kwin_runs_ibus),
            crate::linux_text_expander::nixos() && !registration.system,
        );
        let color = if matches!(state, TextExpanderSetupState::Active(_)) {
            app_accent()
        } else {
            ui.visuals().text_color()
        };
        let steps_size = metrics.size(78.0, 30.0);
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "text_expander.ibus_status_label"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(lang, hint)),
            metrics.value(250.0),
            |ui| {
                ui.spacing_mut().item_spacing.x = metrics.value(8.0);
                let label = ui.add_sized(
                    egui::vec2(metrics.value(164.0), steps_size.y),
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(
                            lang,
                            text_expander_setup_state_key(state),
                        ))
                        .size(metrics.value(12.0))
                        .color(color),
                    )
                    .truncate(),
                );
                if !suppress_tooltips {
                    label.on_hover_text(crate::i18n::tr_catalog(lang, hint));
                }
                if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "text_expander.steps_button"),
                    steps_size,
                    true,
                )
                .clicked()
                {
                    self.text_expander_ibus_modal = Some(TextExpanderIbusModal::Steps);
                }
            },
        );
    }

    #[cfg(target_os = "linux")]
    fn draw_text_expander_gnome_sources_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        suppress_tooltips: bool,
    ) {
        let probe = self.current_text_expander_probe();
        let busy = self.linux_setup_task.is_some() || self.pending_ibus_reload.is_some();
        let buttons = gnome_sources_buttons(probe.as_ref(), busy);
        let button_size = metrics.size(120.0, 30.0);
        let restore_size = metrics.size(90.0, 30.0);
        let gap = metrics.value(8.0);
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "text_expander.gnome_sources_label"),
            true,
            (!suppress_tooltips).then_some(crate::i18n::tr_catalog(
                lang,
                "text_expander.gnome_sources_tooltip",
            )),
            button_size.x + gap + restore_size.x,
            |ui| {
                ui.spacing_mut().item_spacing.x = gap;
                if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "text_expander.gnome_sources_use"),
                    button_size,
                    buttons.use_blocked.is_none(),
                )
                .on_hover_text(crate::i18n::tr_catalog(
                    lang,
                    buttons
                        .use_blocked
                        .unwrap_or("text_expander.gnome_sources_tooltip"),
                ))
                .clicked()
                {
                    self.text_expander_ibus_modal = Some(TextExpanderIbusModal::UseGnomeSources);
                }
                if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "text_expander.gnome_sources_restore"),
                    restore_size,
                    buttons.restore,
                )
                .clicked()
                {
                    self.run_gnome_input_sources("restore");
                }
            },
        );
    }

    /// The setup steps for this desktop, or the question before GNOME's
    /// input sources change.
    #[cfg(target_os = "linux")]
    pub(super) fn draw_text_expander_ibus_modal(
        &mut self,
        ctx: &egui::Context,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        let Some(modal) = self.text_expander_ibus_modal else {
            return;
        };
        let lang = self.app_settings.language;
        let viewport = ctx.content_rect().size();
        let size = metrics.size(540.0, 380.0).min(viewport - Vec2::splat(32.0));
        let width = (size.x - metrics.value(42.0)).max(metrics.value(220.0));
        let title = match modal {
            TextExpanderIbusModal::Steps => "text_expander.steps_title",
            TextExpanderIbusModal::UseGnomeSources => "text_expander.gnome_sources_confirm_title",
        };
        let mut open = true;
        let mut close = false;
        let mut confirmed = false;
        crate::ui_style::centered_modal_window(
            ctx,
            crate::i18n::tr_catalog(lang, title),
            egui::Id::new("text_expander_ibus_modal"),
            &mut open,
            size,
        )
        .movable(false)
        .show(ctx, |ui| {
            ui.set_width(width);
            let paragraph = |ui: &mut egui::Ui, key: &'static str| {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(lang, key)).size(metrics.value(12.5)),
                    )
                    .wrap(),
                );
                ui.add_space(metrics.value(8.0));
            };
            match modal {
                TextExpanderIbusModal::Steps => {
                    let registration = self.ibus_registration.get();
                    let desktop = self
                        .current_text_expander_probe()
                        .map_or_else(crate::linux_text_expander::linux_desktop, |probe| {
                            probe.desktop
                        });
                    let nixos = crate::linux_text_expander::nixos() && !registration.system;
                    for (number, key) in text_expander_setup_steps(desktop, nixos)
                        .into_iter()
                        .enumerate()
                    {
                        ui.horizontal_top(|ui| {
                            ui.label(
                                RichText::new(format!("{}.", number + 1)).size(metrics.value(12.5)),
                            );
                            ui.vertical(|ui| paragraph(ui, key));
                        });
                    }
                    if desktop == crate::linux_text_expander::LinuxDesktop::Gnome {
                        paragraph(ui, "text_expander.steps_gnome_fallback");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(GNOME_SOURCES_FALLBACK_COMMAND)
                                        .monospace()
                                        .size(metrics.value(11.0)),
                                )
                                .wrap(),
                            );
                        });
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "text_expander.steps_copy"),
                            metrics.size(108.0, 30.0),
                            true,
                        )
                        .clicked()
                        {
                            ui.ctx()
                                .copy_text(GNOME_SOURCES_FALLBACK_COMMAND.to_owned());
                        }
                    }
                    ui.add_space(metrics.value(10.0));
                    ui.vertical_centered(|ui| {
                        close = crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "common.close"),
                            metrics.size(108.0, 32.0),
                            true,
                        )
                        .clicked();
                    });
                }
                TextExpanderIbusModal::UseGnomeSources => {
                    paragraph(ui, "text_expander.gnome_sources_confirm_body");
                    // The page behind stays usable, so what was true when the
                    // dialog opened is checked again before anything changes.
                    let blocked = gnome_sources_buttons(
                        self.current_text_expander_probe().as_ref(),
                        self.linux_setup_task.is_some() || self.pending_ibus_reload.is_some(),
                    )
                    .use_blocked;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = metrics.value(10.0);
                        confirmed = crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(
                                lang,
                                "text_expander.gnome_sources_confirm_button",
                            ),
                            metrics.size(190.0, 32.0),
                            blocked.is_none(),
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            lang,
                            blocked.unwrap_or("text_expander.gnome_sources_tooltip"),
                        ))
                        .clicked();
                        close = crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "unlock.cancel"),
                            metrics.size(108.0, 32.0),
                            true,
                        )
                        .clicked();
                    });
                }
            }
        });

        if confirmed {
            self.run_gnome_input_sources("use");
        }
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if escape {
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        if !open || close || confirmed || escape {
            self.text_expander_ibus_modal = None;
        }
    }
}

/// How often the Text Expander page looks at the session again on its own.
/// A rebuild, a login or a choice made in the desktop's settings changes what
/// it shows without any input here.
#[cfg(target_os = "linux")]
const TEXT_EXPANDER_PROBE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(3);
/// How old a look at the session may be and still decide what the rows offer.
/// A refresh normally lands well within it; a page reopened after a while
/// waits for a new one.
#[cfg(target_os = "linux")]
const TEXT_EXPANDER_PROBE_TRUSTED: std::time::Duration = std::time::Duration::from_secs(10);

/// The last resort the steps offer on GNOME; it replaces the whole list.
#[cfg(target_os = "linux")]
const GNOME_SOURCES_FALLBACK_COMMAND: &str = "gsettings set org.gnome.desktop.input-sources sources \"[('ibus', 'entropy-universal-symbols-us'), ('ibus', 'entropy-universal-symbols-ru')]\"";

/// Rows above the Text Expander switch: none on Windows, the backend
/// elsewhere, plus on Linux the status and, on GNOME, its input sources.
pub(super) fn text_expander_setup_row_count() -> usize {
    #[cfg(target_os = "linux")]
    {
        2 + usize::from(
            crate::linux_text_expander::linux_desktop()
                == crate::linux_text_expander::LinuxDesktop::Gnome,
        )
    }
    #[cfg(target_os = "windows")]
    {
        0
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        1
    }
}

/// How far Text Expander is from working in this session.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextExpanderSetupState {
    NotInstalled,
    Checking,
    IbusOff,
    EngineMissing,
    /// GNOME lists no Entropy source, so it never switches to one.
    NotInSources,
    NotSelected,
    /// With the base layout of the source in use: 0 English, 1 Russian.
    Active(Option<u8>),
}

#[cfg(target_os = "linux")]
fn text_expander_setup_state(
    registration: crate::linux_setup::IbusRegistration,
    probe: Option<&crate::linux_text_expander::TextExpanderProbe>,
) -> TextExpanderSetupState {
    let registered = registration.user || registration.system;
    let Some(probe) = probe else {
        return if registered {
            TextExpanderSetupState::Checking
        } else {
            TextExpanderSetupState::NotInstalled
        };
    };
    // What the daemon says outranks the component files: it may serve an
    // engine registered where this app does not look.
    match probe.ibus {
        Some(ibus) if ibus.active.is_some() => {
            TextExpanderSetupState::Active(ibus.active.flatten())
        }
        Some(ibus) if ibus.engine_listed => {
            if probe.gnome_sources.is_some_and(|sources| !sources.entropy) {
                TextExpanderSetupState::NotInSources
            } else {
                TextExpanderSetupState::NotSelected
            }
        }
        _ if !registered => TextExpanderSetupState::NotInstalled,
        Some(_) => TextExpanderSetupState::EngineMissing,
        None => TextExpanderSetupState::IbusOff,
    }
}

#[cfg(target_os = "linux")]
fn text_expander_setup_state_key(state: TextExpanderSetupState) -> &'static str {
    match state {
        TextExpanderSetupState::NotInstalled => "text_expander.ibus_state_not_installed",
        TextExpanderSetupState::Checking => "text_expander.ibus_state_checking",
        TextExpanderSetupState::IbusOff => "text_expander.ibus_state_not_running",
        TextExpanderSetupState::EngineMissing => "text_expander.ibus_state_engine_missing",
        TextExpanderSetupState::NotInSources => "text_expander.ibus_state_not_in_sources",
        TextExpanderSetupState::NotSelected => "text_expander.ibus_state_not_selected",
        TextExpanderSetupState::Active(Some(0)) => "text_expander.ibus_state_active_en",
        TextExpanderSetupState::Active(Some(1)) => "text_expander.ibus_state_active_ru",
        TextExpanderSetupState::Active(_) => "text_expander.ibus_state_active",
    }
}

/// The next step, said for the desktop at hand. `nixos_unregistered` is a
/// NixOS system without the engine registered by its module, where a copy
/// installed for one user is never loaded.
#[cfg(target_os = "linux")]
fn text_expander_setup_hint_key(
    state: TextExpanderSetupState,
    desktop: crate::linux_text_expander::LinuxDesktop,
    kwin_runs_ibus: Option<bool>,
    nixos_unregistered: bool,
) -> &'static str {
    use crate::linux_text_expander::LinuxDesktop;

    match state {
        TextExpanderSetupState::NotInstalled | TextExpanderSetupState::EngineMissing
            if nixos_unregistered =>
        {
            "text_expander.nixos_install_tooltip"
        }
        TextExpanderSetupState::NotInstalled | TextExpanderSetupState::Checking => {
            "text_expander.ibus_hint_not_installed"
        }
        TextExpanderSetupState::IbusOff => match desktop {
            LinuxDesktop::KdeWayland if kwin_runs_ibus == Some(true) => {
                "text_expander.ibus_hint_not_running_kde_chosen"
            }
            LinuxDesktop::KdeWayland => "text_expander.ibus_hint_not_running_kde",
            LinuxDesktop::Gnome => "text_expander.ibus_hint_not_running_gnome",
            LinuxDesktop::Other => "text_expander.ibus_hint_not_running_other",
        },
        TextExpanderSetupState::EngineMissing => "text_expander.ibus_hint_engine_missing",
        TextExpanderSetupState::NotInSources => "text_expander.ibus_hint_not_in_sources",
        TextExpanderSetupState::NotSelected if desktop == LinuxDesktop::Gnome => {
            "text_expander.ibus_hint_not_selected_gnome"
        }
        TextExpanderSetupState::NotSelected => "text_expander.ibus_hint_not_selected_other",
        TextExpanderSetupState::Active(_)
            if desktop == LinuxDesktop::KdeWayland && kwin_runs_ibus == Some(false) =>
        {
            "text_expander.ibus_hint_active_kde_outside_kwin"
        }
        TextExpanderSetupState::Active(_) => "text_expander.ibus_hint_active",
    }
}

#[cfg(target_os = "linux")]
fn text_expander_setup_steps(
    desktop: crate::linux_text_expander::LinuxDesktop,
    nixos_unregistered: bool,
) -> Vec<&'static str> {
    use crate::linux_text_expander::LinuxDesktop;

    let mut steps = Vec::new();
    if nixos_unregistered {
        steps.push("text_expander.steps_nixos");
    }
    steps.extend(match desktop {
        LinuxDesktop::Gnome => [
            "text_expander.steps_gnome_use",
            "text_expander.steps_gnome_by_hand",
            "text_expander.steps_gnome_switch",
        ]
        .as_slice(),
        LinuxDesktop::KdeWayland => [
            "text_expander.steps_kde_virtual_keyboard",
            "text_expander.steps_kde_variables",
            "text_expander.steps_kde_add",
            "text_expander.steps_kde_system_layout",
            "text_expander.steps_kde_switch",
        ]
        .as_slice(),
        LinuxDesktop::Other => [
            "text_expander.steps_other_input_method",
            "text_expander.steps_other_add",
        ]
        .as_slice(),
    });
    steps
}

/// What the GNOME input sources row lets the user do.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GnomeSourcesButtons {
    /// Why Use Entropy is off, as a catalog key; `None` when it is on.
    use_blocked: Option<&'static str>,
    restore: bool,
}

#[cfg(target_os = "linux")]
fn gnome_sources_buttons(
    probe: Option<&crate::linux_text_expander::TextExpanderProbe>,
    busy: bool,
) -> GnomeSourcesButtons {
    let sources = probe.and_then(|probe| probe.gnome_sources);
    let writable = sources.is_some_and(|sources| sources.writable);
    let engine_listed = probe
        .and_then(|probe| probe.ibus)
        .is_some_and(|ibus| ibus.engine_listed);
    let use_blocked = match sources {
        None => Some("text_expander.gnome_sources_unavailable_tooltip"),
        Some(sources) if !sources.writable => Some("text_expander.gnome_sources_locked_tooltip"),
        // GNOME drops a source whose engine IBus does not list, so the plain
        // layouts must not give way to the Entropy ones before IBus has them.
        Some(_) if !engine_listed => Some("text_expander.gnome_sources_engine_missing_tooltip"),
        Some(sources) if !sources.replaceable && sources.entropy => {
            Some("text_expander.gnome_sources_in_use_tooltip")
        }
        Some(sources) if !sources.replaceable => {
            Some("text_expander.gnome_sources_nothing_tooltip")
        }
        Some(_) => None,
    };
    GnomeSourcesButtons {
        use_blocked: use_blocked.or(busy.then_some("text_expander.ibus_working")),
        restore: !busy && writable && sources.is_some_and(|sources| sources.entropy),
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextExpanderIbusAction {
    Install,
    Reinstall,
    Reload,
}

#[cfg(target_os = "linux")]
fn text_expander_ibus_action(
    registration: crate::linux_setup::IbusRegistration,
) -> TextExpanderIbusAction {
    if registration.system {
        TextExpanderIbusAction::Reload
    } else if registration.user {
        TextExpanderIbusAction::Reinstall
    } else {
        TextExpanderIbusAction::Install
    }
}

#[cfg(target_os = "linux")]
fn text_expander_ibus_delete_enabled(
    registration: crate::linux_setup::IbusRegistration,
    setup_running: bool,
) -> bool {
    registration.user && !setup_running
}

#[cfg(target_os = "linux")]
fn text_expander_ibus_button_key(
    action: TextExpanderIbusAction,
    setup_running: bool,
) -> &'static str {
    if setup_running {
        "text_expander.ibus_working"
    } else {
        match action {
            TextExpanderIbusAction::Install => "universal_symbols_setup.install_ibus",
            TextExpanderIbusAction::Reinstall => "text_expander.reinstall_ibus_source",
            TextExpanderIbusAction::Reload => "universal_symbols_setup.reload_ibus_button",
        }
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "linux")))]
fn text_expander_backend_button_key() -> &'static str {
    "text_expander.open_backend_setup"
}

#[cfg(not(target_os = "windows"))]
fn text_expander_backend_label_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "universal_symbols_setup.wayland_ibus"
    }
    #[cfg(not(target_os = "linux"))]
    {
        "universal_symbols_setup.current_backend"
    }
}

#[cfg(not(target_os = "windows"))]
fn text_expander_backend_hint_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "text_expander.backend_hint_linux_ibus"
    }
    #[cfg(target_os = "macos")]
    {
        "text_expander.backend_hint_macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "text_expander.backend_hint_unsupported"
    }
}

#[cfg(all(test, target_os = "linux"))]
mod setup_state_tests {
    use super::*;
    use crate::linux_setup::IbusRegistration;
    use crate::linux_text_expander::{GnomeSources, IbusSession, LinuxDesktop, TextExpanderProbe};

    fn probe(desktop: LinuxDesktop, ibus: Option<IbusSession>) -> TextExpanderProbe {
        TextExpanderProbe {
            desktop,
            ibus,
            gnome_sources: None,
            kwin_runs_ibus: None,
        }
    }

    fn session(engine_listed: bool, active: Option<Option<u8>>) -> Option<IbusSession> {
        Some(IbusSession {
            engine_listed,
            active,
        })
    }

    const USER: IbusRegistration = IbusRegistration {
        user: true,
        system: false,
    };
    const NONE: IbusRegistration = IbusRegistration {
        user: false,
        system: false,
    };

    #[test]
    fn the_state_follows_what_the_running_ibus_says() {
        use TextExpanderSetupState as State;

        let kde = |ibus| probe(LinuxDesktop::KdeWayland, ibus);
        assert_eq!(text_expander_setup_state(NONE, None), State::NotInstalled);
        assert_eq!(text_expander_setup_state(USER, None), State::Checking);
        assert_eq!(
            text_expander_setup_state(USER, Some(&kde(None))),
            State::IbusOff
        );
        assert_eq!(
            text_expander_setup_state(NONE, Some(&kde(None))),
            State::NotInstalled
        );
        assert_eq!(
            text_expander_setup_state(USER, Some(&kde(session(false, None)))),
            State::EngineMissing
        );
        assert_eq!(
            text_expander_setup_state(USER, Some(&kde(session(true, None)))),
            State::NotSelected
        );
        assert_eq!(
            text_expander_setup_state(USER, Some(&kde(session(true, Some(Some(1)))))),
            State::Active(Some(1))
        );
        // An engine the daemon serves counts even where no component file
        // of ours was found.
        assert_eq!(
            text_expander_setup_state(NONE, Some(&kde(session(true, None)))),
            State::NotSelected
        );

        let mut gnome = probe(LinuxDesktop::Gnome, session(true, None));
        gnome.gnome_sources = Some(GnomeSources {
            entropy: false,
            replaceable: true,
            writable: true,
        });
        assert_eq!(
            text_expander_setup_state(USER, Some(&gnome)),
            State::NotInSources
        );
        gnome.gnome_sources = Some(GnomeSources {
            entropy: true,
            ..gnome.gnome_sources.unwrap()
        });
        assert_eq!(
            text_expander_setup_state(USER, Some(&gnome)),
            State::NotSelected
        );
    }

    #[test]
    fn the_hint_names_the_next_step_on_this_desktop() {
        use TextExpanderSetupState as State;

        let hint =
            |state, desktop, kwin, nixos| text_expander_setup_hint_key(state, desktop, kwin, nixos);
        assert_eq!(
            hint(State::IbusOff, LinuxDesktop::KdeWayland, Some(false), false),
            "text_expander.ibus_hint_not_running_kde"
        );
        assert_eq!(
            hint(State::IbusOff, LinuxDesktop::KdeWayland, Some(true), false),
            "text_expander.ibus_hint_not_running_kde_chosen"
        );
        assert_eq!(
            hint(State::IbusOff, LinuxDesktop::Gnome, None, false),
            "text_expander.ibus_hint_not_running_gnome"
        );
        // An IBus that KWin does not run reaches only apps that call it directly.
        assert_eq!(
            hint(
                State::Active(Some(0)),
                LinuxDesktop::KdeWayland,
                Some(false),
                false
            ),
            "text_expander.ibus_hint_active_kde_outside_kwin"
        );
        assert_eq!(
            hint(
                State::Active(Some(0)),
                LinuxDesktop::KdeWayland,
                Some(true),
                false
            ),
            "text_expander.ibus_hint_active"
        );
        assert_eq!(
            hint(State::NotInstalled, LinuxDesktop::KdeWayland, None, true),
            "text_expander.nixos_install_tooltip"
        );
        assert_eq!(
            hint(State::EngineMissing, LinuxDesktop::Other, None, true),
            "text_expander.nixos_install_tooltip"
        );
        assert_eq!(
            hint(State::NotSelected, LinuxDesktop::Gnome, None, false),
            "text_expander.ibus_hint_not_selected_gnome"
        );
        assert_eq!(
            hint(
                State::NotSelected,
                LinuxDesktop::KdeWayland,
                Some(true),
                false
            ),
            "text_expander.ibus_hint_not_selected_other"
        );
    }

    #[test]
    fn the_steps_fit_the_desktop_and_start_with_the_module_on_nixos() {
        let steps = text_expander_setup_steps(LinuxDesktop::KdeWayland, false);
        assert_eq!(
            steps.first(),
            Some(&"text_expander.steps_kde_virtual_keyboard")
        );
        assert!(steps
            .iter()
            .all(|step| step.starts_with("text_expander.steps_kde_")));
        assert!(steps.contains(&"text_expander.steps_kde_system_layout"));
        let steps = text_expander_setup_steps(LinuxDesktop::Gnome, true);
        assert_eq!(steps.first(), Some(&"text_expander.steps_nixos"));
        assert!(steps.contains(&"text_expander.steps_gnome_use"));
        assert_eq!(
            text_expander_setup_steps(LinuxDesktop::Other, false).len(),
            2
        );
    }

    #[test]
    fn gnome_sources_change_only_once_ibus_lists_the_engine() {
        let mut gnome = probe(LinuxDesktop::Gnome, session(true, None));
        let sources = |entropy, replaceable, writable| {
            Some(GnomeSources {
                entropy,
                replaceable,
                writable,
            })
        };
        let buttons = |probe: &TextExpanderProbe, busy| gnome_sources_buttons(Some(probe), busy);

        gnome.gnome_sources = sources(false, true, true);
        assert_eq!(
            buttons(&gnome, false),
            GnomeSourcesButtons {
                use_blocked: None,
                restore: false
            }
        );
        assert_eq!(
            buttons(&gnome, true).use_blocked,
            Some("text_expander.ibus_working")
        );
        gnome.gnome_sources = sources(true, true, true);
        assert!(buttons(&gnome, false).restore);
        assert!(!buttons(&gnome, true).restore);
        gnome.gnome_sources = sources(true, false, true);
        assert_eq!(
            buttons(&gnome, false).use_blocked,
            Some("text_expander.gnome_sources_in_use_tooltip")
        );
        gnome.gnome_sources = sources(false, false, true);
        assert_eq!(
            buttons(&gnome, false).use_blocked,
            Some("text_expander.gnome_sources_nothing_tooltip")
        );
        gnome.gnome_sources = sources(true, true, false);
        assert_eq!(
            buttons(&gnome, false),
            GnomeSourcesButtons {
                use_blocked: Some("text_expander.gnome_sources_locked_tooltip"),
                restore: false
            }
        );

        // GNOME would drop sources whose engine IBus does not list.
        gnome.gnome_sources = sources(false, true, true);
        gnome.ibus = session(false, None);
        assert_eq!(
            buttons(&gnome, false).use_blocked,
            Some("text_expander.gnome_sources_engine_missing_tooltip")
        );
        gnome.ibus = None;
        assert_eq!(
            buttons(&gnome, false).use_blocked,
            Some("text_expander.gnome_sources_engine_missing_tooltip")
        );
        assert_eq!(
            gnome_sources_buttons(None, false).use_blocked,
            Some("text_expander.gnome_sources_unavailable_tooltip")
        );
    }

    #[test]
    fn the_setup_rows_and_dialogs_draw_in_both_languages() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let mut gnome = probe(LinuxDesktop::Gnome, session(true, None));
        gnome.gnome_sources = Some(GnomeSources {
            entropy: false,
            replaceable: true,
            writable: true,
        });
        app.text_expander_probe = Some((std::time::Instant::now(), gnome));
        for lang in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            app.app_settings.language = lang;
            for modal in [
                None,
                Some(TextExpanderIbusModal::Steps),
                Some(TextExpanderIbusModal::UseGnomeSources),
            ] {
                app.text_expander_ibus_modal = modal;
                let _ = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                        for row in 0..3 {
                            app.draw_text_expander_setup_row(
                                row, ui, 452.0, 54.0, metrics, lang, false,
                            );
                        }
                        app.draw_text_expander_ibus_modal(ui.ctx(), metrics);
                    },
                );
                assert_eq!(
                    app.text_expander_ibus_modal, modal,
                    "drawing alone keeps it open"
                );
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod ibus_action_tests {
    use super::*;
    use crate::linux_setup::IbusRegistration;

    #[test]
    fn installed_user_source_offers_reinstall_and_delete() {
        let missing = IbusRegistration {
            user: false,
            system: false,
        };
        let user = IbusRegistration {
            user: true,
            system: false,
        };
        let system = IbusRegistration {
            user: false,
            system: true,
        };
        let both = IbusRegistration {
            user: true,
            system: true,
        };

        assert_eq!(
            text_expander_ibus_action(missing),
            TextExpanderIbusAction::Install
        );
        assert_eq!(
            text_expander_ibus_action(user),
            TextExpanderIbusAction::Reinstall
        );
        assert_eq!(
            text_expander_ibus_action(system),
            TextExpanderIbusAction::Reload
        );
        assert_eq!(
            text_expander_ibus_action(both),
            TextExpanderIbusAction::Reload
        );
        assert!(!text_expander_ibus_delete_enabled(missing, false));
        assert!(text_expander_ibus_delete_enabled(user, false));
        assert!(!text_expander_ibus_delete_enabled(system, false));
        assert!(text_expander_ibus_delete_enabled(both, false));
        assert!(!text_expander_ibus_delete_enabled(user, true));
        assert_eq!(
            text_expander_ibus_button_key(TextExpanderIbusAction::Reinstall, false),
            "text_expander.reinstall_ibus_source"
        );
        assert_eq!(
            text_expander_ibus_button_key(TextExpanderIbusAction::Reinstall, true),
            "text_expander.ibus_working"
        );
    }

    #[test]
    fn source_management_labels_are_localized() {
        for language in crate::i18n::Language::ALL {
            for key in [
                "text_expander.reinstall_ibus_source",
                "text_expander.delete_ibus_source",
                "text_expander.reinstall_ibus_tooltip",
                "text_expander.ibus_working",
            ] {
                assert_ne!(crate::i18n::tr_catalog(language, key), key);
            }
        }
    }
}

#[cfg(test)]
mod rule_editor_tests {
    use super::*;

    #[test]
    fn clicking_either_rule_preview_opens_the_same_editor() {
        for (x, field) in [
            (190.0, TextExpanderRuleField::Trigger),
            (285.0, TextExpanderRuleField::Replacement),
        ] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            app.app_settings
                .text_expansion_rules
                .push(crate::text_expander::TextExpansionRule::default());
            let pos = egui::pos2(x, 27.0);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 420.0));
            let frame = |app: &mut EntropyApp, events| {
                let _ = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                        let first_rule_row = text_expander_setup_row_count() + 4;
                        app.draw_text_expander_editor_content(
                            ui,
                            first_rule_row..first_rule_row + 1,
                            452.0,
                            54.0,
                            metrics,
                            false,
                        );
                    },
                );
            };
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(app.text_expander_rule_editor, Some((0, field)));
            assert!(app.text_expander_rule_editor_focus_pending);
        }
    }

    #[test]
    fn expanded_rule_editor_fits_small_window_and_closes_with_escape() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings
            .text_expansion_rules
            .push(crate::text_expander::TextExpansionRule::default());
        app.text_expander_rule_editor = Some((0, TextExpanderRuleField::Replacement));
        app.text_expander_rule_editor_focus_pending = true;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 480.0));
        let frame = |app: &mut EntropyApp, events| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                    app.draw_text_expander_rule_editor(ui.ctx(), metrics);
                },
            );
        };
        frame(&mut app, vec![]);
        let rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("text_expander_rule_editor")))
            .expect("rule editor is visible");
        assert!(screen.contains_rect(rect), "modal exceeds screen: {rect:?}");
        assert!(rect.width() >= 400.0 && rect.height() >= 300.0);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.text_expander_rule_editor.is_none());
    }

    #[test]
    fn expanded_replacement_accepts_multiline_text_in_the_rule_model() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings
            .text_expansion_rules
            .push(crate::text_expander::TextExpansionRule::default());
        app.text_expander_rule_editor = Some((0, TextExpanderRuleField::Replacement));
        app.text_expander_rule_editor_focus_pending = true;
        let frame = |app: &mut EntropyApp, events| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                    app.draw_text_expander_rule_editor(ui.ctx(), metrics);
                },
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::Text("First".into())]);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(&mut app, vec![egui::Event::Text("Second".into())]);
        assert_eq!(
            app.app_settings.text_expansion_rules[0].replacement,
            "First\nSecond"
        );
        assert!(app.text_expander_settings_save_pending);
    }

    #[test]
    fn leaving_text_expander_dismisses_editor_without_changing_rule() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings
            .text_expansion_rules
            .push(crate::text_expander::TextExpansionRule::default());
        let rule = app.app_settings.text_expansion_rules[0].clone();
        app.main_menu_tab = MainMenuTab::Advanced;
        app.settings_tab = SettingsTab::TextExpander;
        app.text_expander_rule_editor = Some((0, TextExpanderRuleField::Trigger));
        app.dismiss_text_expander_rule_editor_if_page_inactive();
        assert!(app.text_expander_rule_editor.is_some());
        app.settings_tab = SettingsTab::TypingTrainer;
        app.dismiss_text_expander_rule_editor_if_page_inactive();
        assert!(app.text_expander_rule_editor.is_none());
        assert_eq!(app.app_settings.text_expansion_rules[0], rule);
    }
}
