use super::*;

impl KeycodePicker {
    pub(super) fn custom_keycode_pairs(&self) -> Vec<crate::keyboard::CustomKeycode> {
        self.custom_keycodes
            .iter()
            .map(|(name, label, title, _)| crate::keyboard::CustomKeycode {
                name: name.clone(),
                label: label.clone(),
                title: title.clone(),
            })
            .collect()
    }

    pub(super) fn has_visible_custom_keycodes(&self) -> bool {
        self.custom_keycodes
            .iter()
            .any(|(_, label, _, _)| !label.trim().is_empty())
    }

    pub(super) fn show_custom_keycode_choice_section(&self, ui: &mut egui::Ui) -> Option<u16> {
        if !self.has_visible_custom_keycodes() {
            return None;
        }

        let custom_keycodes = self.custom_keycodes.clone();
        let mut selected = None;
        ui.add_space(2.0);
        ui.label(
            RichText::new(tr_picker(
                self.language,
                "key_picker.section_custom_keycodes",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (name, label, title, value) in custom_keycodes {
                if label.trim().is_empty() {
                    continue;
                }
                let tip = if title.trim().is_empty() {
                    name.as_str()
                } else {
                    title.as_str()
                };
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, &label);
                if resp.clicked() {
                    selected = Some(value);
                }
                resp.on_hover_text(crate::i18n::tr_text(self.language, tip));
            }
        });
        ui.add_space(8.0);

        selected
    }

    pub(super) fn show_vial_symbols(&mut self, ui: &mut egui::Ui) {
        let custom_pairs = self.custom_keycode_pairs();

        if let Some(value) = show_universal_symbol_section(
            ui,
            self.language,
            "key_picker.section_universal_symbols",
            UNIVERSAL_MAIN_SYMBOL_ORDER,
            true,
        ) {
            self.assign_keycode_value(value);
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr_picker(
                self.language,
                "key_picker.section_layout_symbols",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for kc in KEYCODES.iter() {
                if !self.selected_tab.vial_matches(kc) || !self.vial_keycode_supported(kc) {
                    continue;
                }
                let label = keycode_label_with_names_and_layout(
                    kc.value,
                    &custom_pairs,
                    &self.layer_names,
                    self.key_legend_layout,
                );
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, &label);
                if resp.clicked() {
                    self.assign_keycode_value(kc.value);
                }
                if resp.hovered() {
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &self.picker_keycode_tooltip(kc.value, &custom_pairs),
                    ));
                }
            }
        });

        ui.add_space(10.0);
        if let Some(value) = show_universal_symbol_section(
            ui,
            self.language,
            "key_picker.section_extra_universal_symbols",
            UNIVERSAL_EXTRA_SYMBOL_ORDER,
            false,
        ) {
            self.assign_keycode_value(value);
        }
    }

    pub(super) fn show_vial_generic(&mut self, ui: &mut egui::Ui) {
        let custom_pairs = self.custom_keycode_pairs();
        ui.horizontal_wrapped(|ui| {
            for kc in KEYCODES.iter() {
                if !self.selected_tab.vial_matches(kc) || !self.vial_keycode_supported(kc) {
                    continue;
                }
                let label = keycode_label_with_names_and_layout(
                    kc.value,
                    &custom_pairs,
                    &self.layer_names,
                    self.key_legend_layout,
                );
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, &label);
                if resp.clicked() {
                    self.assign_keycode_value(kc.value);
                }
                if resp.hovered() {
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &self.picker_keycode_tooltip(kc.value, &custom_pairs),
                    ));
                }
            }
        });
    }

    pub(super) fn show_vial_custom(&mut self, ui: &mut egui::Ui) {
        if let Some(value) = self.show_custom_keycode_choice_section(ui) {
            self.assign_keycode_value(value);
        }
    }

    pub(super) fn show_vial_layers(&mut self, ui: &mut egui::Ui) {
        let ops: &[(u16, &str, &str)] = &[
            (0x5220, "Layer\nMO", "Hold to activate, release to return"),
            (0x5260, "Layer\nTG", "Tap to toggle on/off"),
            (0x5280, "Layer\nOSL", "Active for next keypress only"),
            (0x52C0, "Layer\nTT", "Hold = MO, tap = toggle"),
            (0x5200, "Layer\nTO", "Switch and stay on this layer"),
            (0x5240, "Layer\nDF", "Set as permanent base layer"),
        ];

        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_layers"))
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (base, label, hint) in ops {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked() {
                    self.vial_layer_pending = Some(*base);
                }
                resp.on_hover_text(crate::i18n::tr_catalog(self.language, hint));
            }
            let lt_resp = ui
                .add(egui::Button::new("").min_size(Self::picker_key_size(ui.ctx())))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            Self::paint_compact_picker_label(ui, &lt_resp, "Layer\nLT");
            if lt_resp.clicked() {
                self.vial_layer_pending = Some(0x4000);
            }
            lt_resp.on_hover_text(crate::i18n::tr_catalog(self.language, "key_picker_text.hold_activate_layer_tap_keycode_set_key_via_right_click_afterwards"));
        });
    }

    pub(super) fn show_vial_modifiers(&mut self, ui: &mut egui::Ui) {
        let gui = gui_label(false);
        let lgui = gui_label(false);

        ui.label(
            RichText::new(tr_picker(
                self.language,
                "key_picker.section_plain_modifiers",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let plain: Vec<(String, u16, u16, String)> = vec![
            ("Ctrl".into(), 0x00E0, 0x00E4, "Ctrl".into()),
            ("Shift".into(), 0x00E1, 0x00E5, "Shift".into()),
            ("Alt".into(), 0x00E2, 0x00E6, "Alt".into()),
            (gui.into(), 0x00E3, 0x00E7, lgui.to_string()),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, left_value, right_value, mod_name) in &plain {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.assign_keycode_value(*left_value);
                }
                if resp.clicked_by(egui::PointerButton::Secondary) {
                    self.assign_keycode_value(*right_value);
                }
                resp.on_hover_text(crate::i18n::tr_text(
                    self.language,
                    &plain_modifier_tooltip(mod_name),
                ));
            }
        });

        ui.add_space(10.0);
        self.show_vial_layers(ui);

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_mod_key"))
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let mk: Vec<(String, u16, Option<u16>, String)> = vec![
            (
                picker_mod_key_label(0x0100),
                0x0100,
                Some(0x1100),
                "Ctrl".into(),
            ),
            (
                picker_mod_key_label(0x0200),
                0x0200,
                Some(0x1200),
                "Shift".into(),
            ),
            (
                picker_mod_key_label(0x0400),
                0x0400,
                Some(0x1400),
                "Alt".into(),
            ),
            (
                picker_mod_key_label(0x0800),
                0x0800,
                Some(0x1800),
                lgui.to_string(),
            ),
            (
                picker_mod_key_label(0x0300),
                0x0300,
                None,
                "Ctrl+Shift".into(),
            ),
            (
                picker_mod_key_label(0x0500),
                0x0500,
                None,
                "Ctrl+Alt".into(),
            ),
            (
                picker_mod_key_label(0x0600),
                0x0600,
                None,
                "Shift+Alt (LSA)".into(),
            ),
            (
                picker_mod_key_label(0x0700),
                0x0700,
                None,
                "Ctrl+Shift+Alt".into(),
            ),
            (
                picker_mod_key_label(0x0A00),
                0x0A00,
                None,
                format!("{}+Shift", lgui),
            ),
            (
                picker_mod_key_label(0x0F00),
                0x0F00,
                None,
                format!("Ctrl+Shift+Alt+{}", gui_mod_name()),
            ),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, left_value, right_value, mod_name) in &mk {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.vial_quantum_pending_mod = Some(*left_value);
                }
                if let Some(right_value) = right_value {
                    if resp.clicked_by(egui::PointerButton::Secondary) {
                        self.vial_quantum_pending_mod = Some(*right_value);
                    }
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &mod_combo_tooltip(mod_name, true),
                    ));
                } else {
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &mod_combo_tooltip(mod_name, false),
                    ));
                }
            }
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_mod_tap"))
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let mt: Vec<(String, u16, Option<u16>, String)> = vec![
            (
                picker_mod_tap_label(0x2100),
                0x2100,
                Some(0x3100),
                "Ctrl".into(),
            ),
            (
                picker_mod_tap_label(0x2200),
                0x2200,
                Some(0x3200),
                "Shift".into(),
            ),
            (
                picker_mod_tap_label(0x2400),
                0x2400,
                Some(0x3400),
                "Alt".into(),
            ),
            (
                picker_mod_tap_label(0x2800),
                0x2800,
                Some(0x3800),
                lgui.to_string(),
            ),
            (
                picker_mod_tap_label(0x2300),
                0x2300,
                None,
                "Ctrl+Shift".into(),
            ),
            (
                picker_mod_tap_label(0x2500),
                0x2500,
                None,
                "Ctrl+Alt".into(),
            ),
            (
                picker_mod_tap_label(0x2600),
                0x2600,
                None,
                "Shift+Alt (LSA)".into(),
            ),
            (
                picker_mod_tap_label(0x2700),
                0x2700,
                None,
                "Meh (Ctrl+Shift+Alt)".into(),
            ),
            (
                picker_mod_tap_label(0x2F00),
                0x2F00,
                None,
                format!("Hyper (Ctrl+Shift+Alt+{})", gui_mod_name()),
            ),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, left_value, right_value, mod_name) in &mt {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.vial_quantum_pending_mt = Some(*left_value);
                }
                if let Some(right_value) = right_value {
                    if resp.clicked_by(egui::PointerButton::Secondary) {
                        self.vial_quantum_pending_mt = Some(*right_value);
                    }
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &mod_tap_tooltip(mod_name, true),
                    ));
                } else {
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &mod_tap_tooltip(mod_name, false),
                    ));
                }
            }
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_one_shot_mod"))
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let osm: Vec<(String, u16, Option<u16>, String)> = vec![
            ("OSM\nCtrl".into(), 0x52A1, Some(0x52B1), "Ctrl".into()),
            ("OSM\nShift".into(), 0x52A2, Some(0x52B2), "Shift".into()),
            ("OSM\nAlt".into(), 0x52A4, Some(0x52B4), "Alt".into()),
            (
                format!("OSM\n{lgui}"),
                0x52A8,
                Some(0x52B8),
                lgui.to_string(),
            ),
            (
                "OSM\nMeh".into(),
                0x52A7,
                None,
                "Meh (Ctrl+Shift+Alt)".into(),
            ),
            (
                "OSM\nHyper".into(),
                0x52AF,
                None,
                format!("Hyper (Ctrl+Shift+Alt+{})", gui_mod_name()),
            ),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, left_value, right_value, mod_name) in &osm {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.assign_keycode_value(*left_value);
                }
                if let Some(right_value) = right_value {
                    if resp.clicked_by(egui::PointerButton::Secondary) {
                        self.assign_keycode_value(*right_value);
                    }
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &one_shot_modifier_tooltip(mod_name, true),
                    ));
                } else {
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &one_shot_modifier_tooltip(mod_name, false),
                    ));
                }
            }
        });
    }
}
