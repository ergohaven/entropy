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
            let dropdown_id = advanced_dropdown_open_id();
            let was_open = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(dropdown_id))
                .unwrap_or(false);
            let combo_supported = !self.combo_entries.is_empty();
            let macro_supported =
                self.keycode_picker.supports_macro && self.keycode_picker.macro_count > 0;
            let tap_dance_supported = self.keycode_picker.supports_tap_dance
                && !self.keycode_picker.tap_dance_entries.is_empty();
            let key_override_supported = !self.key_override_entries.is_empty();
            let auto_shift_supported =
                self.auto_shift_timeout.is_some() || self.supported_qmk_settings.contains(&4);
            let advanced_item_count = 2
                + macro_supported as usize
                + tap_dance_supported as usize
                + combo_supported as usize
                + auto_shift_supported as usize
                + key_override_supported as usize;
            let mut advanced_menu_labels =
                vec![crate::i18n::tr_catalog(lang, "text_expander.title")];
            advanced_menu_labels.push(crate::i18n::tr_catalog(lang, "typing_trainer.title"));
            if macro_supported {
                advanced_menu_labels.push(crate::i18n::tr_catalog(lang, "macro_editor.title"));
            }
            if tap_dance_supported {
                advanced_menu_labels.push(crate::i18n::tr_catalog(lang, "tap_dance_editor.title"));
            }
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
                adaptive_top_icon_dropdown_width(ui, advanced_menu_labels, 152.0);
            // Entropy tools work without a device. The rest is stored in firmware.
            let dividers = top_menu_dividers([2, advanced_item_count - 2]);
            let dropdown_rect = egui::Rect::from_min_size(
                egui::pos2(
                    advanced_rect.center().x - advanced_dropdown_width / 2.0,
                    advanced_rect.bottom() + 6.0,
                ),
                Vec2::new(
                    advanced_dropdown_width,
                    top_dropdown_height(advanced_item_count, dividers[0] as usize),
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
                let item_width = dropdown_rect.width() - 16.0;
                let (
                    text_expander_hovered,
                    typing_trainer_hovered,
                    macro_hovered,
                    tap_dance_hovered,
                    combo_hovered,
                    auto_shift_hovered,
                    key_override_hovered,
                    advanced_clicked,
                ) = show_top_dropdown(
                    ui.ctx(),
                    egui::Id::new("advanced_dropdown_area"),
                    dropdown_rect.min,
                    |ui| {
                        ui.set_min_width(item_width);
                        let text_expander_resp = top_dropdown_icon_item(
                            ui,
                            item_width,
                            TopMenuIcon::TextExpander,
                            crate::i18n::tr_catalog(lang, "text_expander.title"),
                            true,
                            self.main_menu_tab == MainMenuTab::Advanced
                                && self.settings_tab == SettingsTab::TextExpander,
                        );
                        let typing_trainer_resp = top_dropdown_icon_item(
                            ui,
                            item_width,
                            TopMenuIcon::TypingTrainer,
                            crate::i18n::tr_catalog(lang, "typing_trainer.title"),
                            true,
                            self.main_menu_tab == MainMenuTab::Advanced
                                && self.settings_tab == SettingsTab::TypingTrainer,
                        );
                        if dividers[0] {
                            top_dropdown_divider(ui, item_width);
                        }
                        let macro_resp = macro_supported.then(|| {
                            top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::Macros,
                                crate::i18n::tr_catalog(lang, "macro_editor.title"),
                                true,
                                self.main_menu_tab == MainMenuTab::Advanced
                                    && self.settings_tab == SettingsTab::Macros,
                            )
                        });
                        let tap_dance_resp = tap_dance_supported.then(|| {
                            top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::TapDance,
                                crate::i18n::tr_catalog(lang, "tap_dance_editor.title"),
                                true,
                                self.main_menu_tab == MainMenuTab::Advanced
                                    && self.settings_tab == SettingsTab::TapDance,
                            )
                        });
                        let combo_resp = combo_supported.then(|| {
                            top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::Combo,
                                crate::i18n::tr(lang, TrKey::ComboTitle),
                                true,
                                self.main_menu_tab == MainMenuTab::Advanced
                                    && self.settings_tab == SettingsTab::Combo,
                            )
                        });
                        let auto_shift_resp = auto_shift_supported.then(|| {
                            top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::AutoShift,
                                crate::i18n::tr(lang, TrKey::AutoShiftTitle),
                                true,
                                self.main_menu_tab == MainMenuTab::Advanced
                                    && self.settings_tab == SettingsTab::AutoShift,
                            )
                        });
                        let key_override_resp = key_override_supported.then(|| {
                            top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::KeyOverrides,
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
                        if macro_resp
                            .as_ref()
                            .is_some_and(|response| response.clicked())
                        {
                            self.close_top_dropdowns(ui.ctx());
                            self.open_macro_settings_page();
                        }
                        if tap_dance_resp
                            .as_ref()
                            .is_some_and(|response| response.clicked())
                        {
                            self.close_top_dropdowns(ui.ctx());
                            self.open_tap_dance_settings_page();
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
                            self.open_auto_shift_settings_page();
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
                            macro_resp.as_ref().map(|r| r.hovered()).unwrap_or(false),
                            tap_dance_resp
                                .as_ref()
                                .map(|r| r.hovered())
                                .unwrap_or(false),
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
                                || macro_resp.as_ref().map(|r| r.clicked()).unwrap_or(false)
                                || tap_dance_resp
                                    .as_ref()
                                    .map(|r| r.clicked())
                                    .unwrap_or(false)
                                || combo_resp.as_ref().map(|r| r.clicked()).unwrap_or(false)
                                || auto_shift_resp
                                    .as_ref()
                                    .map(|r| r.clicked())
                                    .unwrap_or(false)
                                || key_override_resp
                                    .as_ref()
                                    .map(|r| r.clicked())
                                    .unwrap_or(false),
                        )
                    },
                )
                .inner;
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        dropdown_id,
                        !advanced_clicked
                            && (advanced_tab_hovered
                                || text_expander_hovered
                                || typing_trainer_hovered
                                || macro_hovered
                                || tap_dance_hovered
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_vial_macro_menu_click_opens_unlock_preflight() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.firmware = FirmwareProtocol::Vial;
        app.layout = Some(
            KeyboardLayout::from_vial_json(&serde_json::json!({
                "name": "Test keyboard",
                "matrix": { "rows": 1, "cols": 1 },
                "layouts": { "keymap": [["0,0"]] }
            }))
            .unwrap(),
        );
        app.vial_unlocked = Some(false);
        app.keycode_picker.supports_macro = true;
        app.keycode_picker.macro_count = 1;
        let tab = egui::Rect::from_min_size(egui::pos2(400.0, 10.0), egui::vec2(100.0, 32.0));
        let draw = |app: &mut EntropyApp, events| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.draw_layout_advanced_dropdown(
                        ui,
                        crate::i18n::Language::English,
                        Some(tab),
                        false,
                        true,
                        false,
                    )
                },
            );
        };
        draw(&mut app, vec![]);
        // Two host-tool rows, then a divider, then the Macros row.
        let pos = egui::pos2(tab.center().x, tab.bottom() + 6.0 + 6.0 + 60.0 + 9.0 + 15.0);
        draw(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            draw(
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
        assert!(app.main_menu_tab == MainMenuTab::Advanced);
        assert!(app.settings_tab == SettingsTab::Macros);
        assert!(app.unlock_open);
        assert!(!app.vial_unlock_session_started);
    }
}
