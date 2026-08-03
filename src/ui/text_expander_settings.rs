use super::*;

impl EntropyApp {
    pub(super) fn draw_text_expander_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
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
                #[cfg(not(target_os = "windows"))]
                self.draw_text_expander_backend_hint(ui, metrics, lang, dark);
                ui.add_space(metrics.value(10.0));

                let rule_row_count = self.app_settings.text_expansion_rules.len().max(1);
                let row_count = 4 + rule_row_count;
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
                let actions_width = button_size.x * 2.0 + button_gap;
                let actions_rect = egui::Rect::from_center_size(
                    egui::pos2(
                        list.viewport.center().x,
                        list.viewport.bottom() + metrics.value(26.0),
                    ),
                    egui::vec2(actions_width, button_size.y),
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
    }

    #[cfg(not(target_os = "windows"))]
    fn draw_text_expander_backend_hint(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
    ) {
        let content_width = metrics.settings_content_width();
        let button_size = metrics.size(148.0, 30.0);
        let button_gap = metrics.value(8.0);
        let hint_height = metrics.value(42.0);
        ui.add_space(metrics.value(4.0));
        let (hint_rect, _) =
            ui.allocate_exact_size(Vec2::new(content_width, hint_height), egui::Sense::hover());
        let label_rect = egui::Rect::from_min_max(
            hint_rect.left_top(),
            egui::pos2(
                hint_rect.right() - button_size.x - button_gap,
                hint_rect.bottom(),
            ),
        );
        let button_rect = egui::Rect::from_center_size(
            egui::pos2(
                hint_rect.right() - button_size.x / 2.0,
                hint_rect.center().y,
            ),
            button_size,
        );
        crate::ui_style::allocate_ui_at_rect(ui, label_rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add_sized(
                    label_rect.size(),
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(
                            lang,
                            text_expander_backend_hint_key(),
                        ))
                        .size(metrics.value(11.0))
                        .color(app_muted_text(dark)),
                    )
                    .wrap(),
                );
            });
        });
        crate::ui_style::allocate_ui_at_rect(ui, button_rect, |ui| {
            if crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, "text_expander.open_backend_setup"),
                button_size,
                true,
            )
            .clicked()
            {
                self.open_text_expander_setup_page();
            }
        });
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
