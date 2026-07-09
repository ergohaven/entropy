use super::*;

impl EntropyApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_layout_advanced_dropdown(
        &mut self,
        ui: &mut egui::Ui,
        lang: crate::i18n::Language,
        advanced_tab_rect: Option<egui::Rect>,
        device_tab_hovered: bool,
        advanced_tab_hovered: bool,
        settings_tab_hovered: bool,
    ) {
        use crate::i18n::Key as TrKey;

        if let Some(advanced_rect) = advanced_tab_rect {
            let dropdown_id = ui.make_persistent_id("advanced_dropdown_open");
            let was_open = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(dropdown_id))
                .unwrap_or(false);
            let combo_supported = !self.combo_entries.is_empty();
            let key_override_supported = !self.key_override_entries.is_empty();
            let auto_shift_supported = self.auto_shift_timeout.is_some();
            let advanced_item_count = 2
                + combo_supported as usize
                + auto_shift_supported as usize
                + key_override_supported as usize;
            let mut advanced_menu_labels =
                vec![crate::i18n::tr_catalog(lang, "text_expander.title")];
            advanced_menu_labels.push(crate::i18n::tr_catalog(lang, "typing_trainer.title"));
            if combo_supported {
                advanced_menu_labels.push(crate::i18n::tr(lang, TrKey::ComboTitle));
            }
            if auto_shift_supported {
                advanced_menu_labels.push(crate::i18n::tr(lang, TrKey::AutoShiftTitle));
            }
            if key_override_supported {
                advanced_menu_labels.push(crate::i18n::tr(lang, TrKey::KeyOverridesTitle));
            }
            let advanced_dropdown_width =
                adaptive_top_dropdown_width(ui, advanced_menu_labels, 152.0);
            let dropdown_rect = egui::Rect::from_min_size(
                egui::pos2(
                    advanced_rect.center().x - advanced_dropdown_width / 2.0,
                    advanced_rect.bottom() + 6.0,
                ),
                Vec2::new(
                    advanced_dropdown_width,
                    (advanced_item_count.max(1) as f32) * 28.0 + 22.0,
                ),
            );
            let hover_bridge_rect = advanced_rect.union(dropdown_rect).expand(3.0);
            let pointer_over_bridge = ui
                .ctx()
                .input(|i| i.pointer.hover_pos())
                .map(|pos| hover_bridge_rect.contains(pos))
                .unwrap_or(false);
            let show_dropdown = advanced_item_count > 0
                && !device_tab_hovered
                && !settings_tab_hovered
                && (advanced_tab_hovered || (was_open && pointer_over_bridge));

            if show_dropdown {
                let dark = ui.visuals().dark_mode;
                let item_width = dropdown_rect.width() - 16.0;
                let (
                    text_expander_hovered,
                    typing_trainer_hovered,
                    combo_hovered,
                    auto_shift_hovered,
                    key_override_hovered,
                    advanced_clicked,
                ) = egui::Area::new(egui::Id::new("advanced_dropdown_area"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(dropdown_rect.min)
                    .show(ui.ctx(), |ui| {
                        top_dropdown_frame(dark)
                            .show(ui, |ui| {
                                ui.set_min_width(item_width);
                                let text_expander_resp = top_dropdown_item(
                                    ui,
                                    item_width,
                                    crate::i18n::tr_catalog(lang, "text_expander.title"),
                                    true,
                                    self.main_menu_tab == MainMenuTab::Advanced
                                        && self.settings_tab == SettingsTab::TextExpander,
                                );
                                let typing_trainer_resp = top_dropdown_item(
                                    ui,
                                    item_width,
                                    crate::i18n::tr_catalog(lang, "typing_trainer.title"),
                                    true,
                                    self.main_menu_tab == MainMenuTab::Advanced
                                        && self.settings_tab == SettingsTab::TypingTrainer,
                                );
                                let combo_resp = combo_supported.then(|| {
                                    top_dropdown_item(
                                        ui,
                                        item_width,
                                        crate::i18n::tr(lang, TrKey::ComboTitle),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Advanced
                                            && self.settings_tab == SettingsTab::Combo,
                                    )
                                });
                                let auto_shift_resp = auto_shift_supported.then(|| {
                                    top_dropdown_item(
                                        ui,
                                        item_width,
                                        crate::i18n::tr(lang, TrKey::AutoShiftTitle),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Advanced
                                            && self.settings_tab == SettingsTab::AutoShift,
                                    )
                                });
                                let key_override_resp = key_override_supported.then(|| {
                                    top_dropdown_item(
                                        ui,
                                        item_width,
                                        crate::i18n::tr(lang, TrKey::KeyOverridesTitle),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Advanced
                                            && self.settings_tab == SettingsTab::KeyOverrides,
                                    )
                                });
                                if text_expander_resp.clicked() {
                                    self.close_top_dropdowns(ui.ctx());
                                    self.open_text_expander_settings_page();
                                }
                                if typing_trainer_resp.clicked() {
                                    self.close_top_dropdowns(ui.ctx());
                                    self.open_typing_trainer_page();
                                }

                                if combo_resp.as_ref().map(|r| r.clicked()).unwrap_or(false) {
                                    self.close_top_dropdowns(ui.ctx());
                                    self.settings_tab = SettingsTab::Combo;
                                    self.main_menu_tab = MainMenuTab::Advanced;
                                    if self.combo_visible_count == 0 {
                                        self.combo_visible_count = 1;
                                    }
                                }
                                if auto_shift_resp
                                    .as_ref()
                                    .map(|r| r.clicked())
                                    .unwrap_or(false)
                                {
                                    self.close_top_dropdowns(ui.ctx());
                                    self.settings_tab = SettingsTab::AutoShift;
                                    self.main_menu_tab = MainMenuTab::Advanced;
                                    if self.is_vial_locked() {
                                        self.unlock_open = true;
                                        self.status_msg = format!(
                                            "{} — {}",
                                            crate::i18n::tr(
                                                self.app_settings.language,
                                                TrKey::KeyboardLocked,
                                            ),
                                            crate::i18n::tr(
                                                self.app_settings.language,
                                                TrKey::AutoShiftUnlockHint,
                                            ),
                                        );
                                    }
                                }
                                if key_override_resp
                                    .as_ref()
                                    .map(|r| r.clicked())
                                    .unwrap_or(false)
                                {
                                    self.close_top_dropdowns(ui.ctx());
                                    self.settings_tab = SettingsTab::KeyOverrides;
                                    self.main_menu_tab = MainMenuTab::Advanced;
                                }
                                (
                                    text_expander_resp.hovered(),
                                    typing_trainer_resp.hovered(),
                                    combo_resp.as_ref().map(|r| r.hovered()).unwrap_or(false),
                                    auto_shift_resp
                                        .as_ref()
                                        .map(|r| r.hovered())
                                        .unwrap_or(false),
                                    key_override_resp
                                        .as_ref()
                                        .map(|r| r.hovered())
                                        .unwrap_or(false),
                                    text_expander_resp.clicked()
                                        || typing_trainer_resp.clicked()
                                        || combo_resp
                                            .as_ref()
                                            .map(|r| r.clicked())
                                            .unwrap_or(false)
                                        || auto_shift_resp
                                            .as_ref()
                                            .map(|r| r.clicked())
                                            .unwrap_or(false)
                                        || key_override_resp
                                            .as_ref()
                                            .map(|r| r.clicked())
                                            .unwrap_or(false),
                                )
                            })
                            .inner
                    })
                    .inner;
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        dropdown_id,
                        !advanced_clicked
                            && (advanced_tab_hovered
                                || text_expander_hovered
                                || typing_trainer_hovered
                                || combo_hovered
                                || auto_shift_hovered
                                || key_override_hovered
                                || pointer_over_bridge),
                    )
                });
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(dropdown_id, false));
            }
        }
    }
}
