use super::*;

#[derive(Clone, Debug, PartialEq)]
struct OneShotModifierChoice {
    label: String,
    left_value: u16,
    right_value: Option<u16>,
    mod_name: String,
}

fn mod_tap_choices(lgui: &str) -> Vec<(String, u16, Option<u16>, String)> {
    vec![
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
            picker_mod_tap_label(0x2900),
            0x2900,
            None,
            format!("Ctrl+{lgui}"),
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
            picker_mod_tap_label(0x2A00),
            0x2A00,
            None,
            format!("Shift+{lgui}"),
        ),
        (
            picker_mod_tap_label(0x2F00),
            0x2F00,
            None,
            format!("Hyper (Ctrl+Shift+Alt+{})", gui_mod_name()),
        ),
    ]
}

fn one_shot_modifier_choices(gui_label: &str, gui_mod_name: &str) -> Vec<OneShotModifierChoice> {
    vec![
        OneShotModifierChoice {
            label: "OSM\nCtrl".into(),
            left_value: 0x52A1,
            right_value: Some(0x52B1),
            mod_name: "Ctrl".into(),
        },
        OneShotModifierChoice {
            label: "OSM\nShift".into(),
            left_value: 0x52A2,
            right_value: Some(0x52B2),
            mod_name: "Shift".into(),
        },
        OneShotModifierChoice {
            label: "OSM\nAlt".into(),
            left_value: 0x52A4,
            right_value: Some(0x52B4),
            mod_name: "Alt".into(),
        },
        OneShotModifierChoice {
            label: format!("OSM\n{gui_label}"),
            left_value: 0x52A8,
            right_value: Some(0x52B8),
            mod_name: gui_label.to_owned(),
        },
        OneShotModifierChoice {
            label: "OSM\nC+S".into(),
            left_value: 0x52A3,
            right_value: Some(0x52B3),
            mod_name: "Ctrl+Shift".into(),
        },
        OneShotModifierChoice {
            label: "OSM\nC+A".into(),
            left_value: 0x52A5,
            right_value: Some(0x52B5),
            mod_name: "Ctrl+Alt".into(),
        },
        OneShotModifierChoice {
            label: "OSM\nS+A".into(),
            left_value: 0x52A6,
            right_value: Some(0x52B6),
            mod_name: "Shift+Alt".into(),
        },
        OneShotModifierChoice {
            label: format!("OSM\nS+{gui_label}"),
            left_value: 0x52AA,
            right_value: Some(0x52BA),
            mod_name: format!("Shift+{gui_label}"),
        },
        OneShotModifierChoice {
            label: "OSM\nMeh".into(),
            left_value: 0x52A7,
            right_value: None,
            mod_name: "Meh (Ctrl+Shift+Alt)".into(),
        },
        OneShotModifierChoice {
            label: "OSM\nHyper".into(),
            left_value: 0x52AF,
            right_value: None,
            mod_name: format!("Hyper (Ctrl+Shift+Alt+{gui_mod_name})"),
        },
    ]
}

/// Universal Symbols controls and punctuation (native RMK actions).
pub(super) fn universal_symbol_rows(language: crate::i18n::Language) -> Vec<PickerRow> {
    let tab = KeycodeTab::UniversalSymbols;
    let mut rows = Vec::new();
    for control in crate::universal_symbols::CONTROLS {
        let label = crate::universal_symbols::label_for_user_id(control.user_id)
            .expect("universal symbol control should have a display label");
        rows.push(
            PickerRow::new(
                tab,
                "",
                label,
                crate::i18n::tr_text(language, control.name),
                PickerAction::Assign(crate::universal_symbols::binding(control.user_id)),
            )
            .with_aliases(control.action_label),
        );
    }
    for symbol in crate::universal_symbols::SYMBOLS {
        let label = crate::universal_symbols::label_for_user_id(symbol.user_id)
            .expect("universal symbol should have a display label");
        let tooltip = format!(
            "Universal Symbols: firmware types {} in English and Russian layouts",
            symbol.symbol
        );
        rows.push(
            PickerRow::new(
                tab,
                "",
                label,
                crate::i18n::tr_text(language, &tooltip),
                PickerAction::Assign(crate::universal_symbols::binding(symbol.user_id)),
            )
            .with_aliases(symbol.symbol.to_string()),
        );
    }
    rows
}

/// Universal Russian letters, attributed to the tab and section showing them.
pub(super) fn universal_russian_letter_rows(
    language: crate::i18n::Language,
    tab: KeycodeTab,
    section: &'static str,
) -> Vec<PickerRow> {
    crate::universal_symbols::RUSSIAN_LETTERS
        .iter()
        .map(|letter| {
            let binding = crate::universal_symbols::binding(letter.user_id);
            let label = crate::universal_symbols::label_for_user_id(letter.user_id)
                .expect("universal Russian letter should have a display label");
            let tooltip = binding
                .rmk_action()
                .and_then(crate::universal_symbols::tooltip)
                .unwrap_or_default();
            PickerRow::new(
                tab,
                section,
                label,
                crate::i18n::tr_text(language, &tooltip),
                PickerAction::Assign(binding),
            )
            .with_aliases(letter.letter.to_string())
        })
        .collect()
}

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
        self.custom_keycodes.iter().any(|(name, label, title, _)| {
            !label.trim().is_empty() && !is_bluetooth_custom_keycode(name, label, title)
        })
    }

    pub(super) fn has_visible_bluetooth_keycodes(&self) -> bool {
        self.custom_keycodes.iter().any(|(name, label, title, _)| {
            !label.trim().is_empty() && is_bluetooth_custom_keycode(name, label, title)
        })
    }

    /// Device-defined keycodes: Bluetooth controls on their own tab, the
    /// rest on the Custom tab.
    pub(super) fn custom_keycode_rows(&self, bluetooth: bool) -> Vec<PickerRow> {
        let tab = if bluetooth {
            KeycodeTab::Bluetooth
        } else {
            KeycodeTab::Custom
        };
        self.custom_keycodes
            .iter()
            .filter(|(name, label, title, _)| {
                !label.trim().is_empty()
                    && is_bluetooth_custom_keycode(name, label, title) == bluetooth
            })
            .map(|(name, label, title, value)| {
                let tip = if title.trim().is_empty() {
                    name.as_str()
                } else {
                    title.as_str()
                };
                PickerRow::new(
                    tab,
                    "",
                    label.clone(),
                    crate::i18n::tr_text(self.language, tip),
                    PickerAction::Assign(crate::keyboard::KeyBinding::Vial(*value)),
                )
                .with_aliases(name)
            })
            .collect()
    }

    /// Custom keycodes as a section inside another chooser (e.g. the Tap
    /// Dance key picker). Returns the picked keycode.
    pub(super) fn show_custom_keycode_choice_section(&self, ui: &mut egui::Ui) -> Option<u16> {
        let rows = self.custom_keycode_rows(false);
        if rows.is_empty() {
            return None;
        }
        ui.add_space(2.0);
        show_section_heading(
            ui,
            tr_picker(self.language, "key_picker.section_custom_keycodes"),
        );
        let picked = show_row_grid(ui, &rows);
        ui.add_space(8.0);
        match picked {
            Some(PickerAction::Assign(crate::keyboard::KeyBinding::Vial(value))) => Some(value),
            _ => None,
        }
    }

    /// Keycodes from the shared table that belong to the tab.
    pub(super) fn keycode_table_rows(&self, tab: KeycodeTab) -> Vec<PickerRow> {
        KEYCODES
            .iter()
            .filter(|kc| tab.vial_matches(kc) && self.vial_keycode_supported(kc))
            .map(|kc| self.table_keycode_row(tab, "", kc.value))
            .collect()
    }

    /// Layer actions. Each opens the layer chooser for its operation.
    pub(super) fn layer_rows(&self, tab: KeycodeTab) -> Vec<PickerRow> {
        let section = tr_picker(self.language, "key_picker.section_layers");
        let ops: [(u16, &'static str, &'static str); 6] = [
            (0x5220, "MO", "Hold to activate, release to return"),
            (0x5260, "TG", "Tap to toggle on/off"),
            (0x5280, "OSL", "Active for next keypress only"),
            (0x52C0, "TT", "Hold = MO, tap = toggle"),
            (0x5200, "TO", "Switch and stay on this layer"),
            (0x5240, "DF", "Set as permanent base layer"),
        ];
        let mut rows: Vec<PickerRow> = ops
            .iter()
            .map(|(base, op, hint)| {
                PickerRow::new(
                    tab,
                    section,
                    format!("Layer\n{op}"),
                    crate::i18n::tr_catalog(self.language, hint),
                    PickerAction::PickLayer(*base),
                )
                .with_aliases(*op)
            })
            .collect();
        rows.push(
            PickerRow::new(
                tab,
                section,
                "Layer\nLT",
                crate::i18n::tr_catalog(
                    self.language,
                    "key_picker_text.hold_activate_layer_tap_keycode_set_key_via_right_click_afterwards",
                ),
                PickerAction::PickLayer(0x4000),
            )
            .with_aliases("LT layer tap"),
        );
        rows
    }

    /// The Mods tab: plain modifiers, layer actions, Mod+Key chords,
    /// Mod-Taps and One-Shot modifiers. Right click picks the right-hand
    /// variant where the firmware has one.
    pub(super) fn modifier_rows(&self) -> Vec<PickerRow> {
        let tab = KeycodeTab::Modifiers;
        let lang = self.language;
        let gui = gui_label(false);
        let mut rows = Vec::new();

        let plain_section = tr_picker(lang, "key_picker.section_plain_modifiers");
        for (label, left_value, right_value, mod_name) in [
            ("Ctrl", 0x00E0_u16, 0x00E4_u16, "Ctrl"),
            ("Shift", 0x00E1, 0x00E5, "Shift"),
            ("Alt", 0x00E2, 0x00E6, "Alt"),
            (gui, 0x00E3, 0x00E7, gui),
        ] {
            if !self.picker_value_supported(left_value) && !self.picker_value_supported(right_value)
            {
                continue;
            }
            let right_name = crate::keycode::find_keycode(right_value)
                .map(|kc| kc.name)
                .unwrap_or_default();
            rows.push(
                self.vial_row(
                    tab,
                    plain_section,
                    left_value,
                    label,
                    crate::i18n::tr_text(lang, &plain_modifier_tooltip(mod_name)),
                )
                .with_aliases(right_name)
                .with_secondary(PickerAction::Assign(
                    crate::keyboard::KeyBinding::Vial(right_value),
                )),
            );
        }

        rows.extend(self.layer_rows(tab));

        let mod_key_section = tr_picker(lang, "key_picker.section_mod_key");
        for choice in mod_key_choices(false) {
            let tooltip = mod_combo_tooltip(&choice.mod_name, choice.right_value.is_some());
            let mut row = PickerRow::new(
                tab,
                mod_key_section,
                choice.label,
                crate::i18n::tr_text(lang, &tooltip),
                PickerAction::PickModKey(choice.left_value),
            )
            .with_aliases(format!("Mod+Key {}", choice.mod_name));
            if let Some(right_value) = choice.right_value {
                row = row.with_secondary(PickerAction::PickModKey(right_value));
            }
            rows.push(row);
        }

        let mod_tap_section = tr_picker(lang, "key_picker.section_mod_tap");
        for (label, left_value, right_value, mod_name) in mod_tap_choices(gui) {
            let tooltip = mod_tap_tooltip(&mod_name, right_value.is_some());
            let mut row = PickerRow::new(
                tab,
                mod_tap_section,
                label,
                crate::i18n::tr_text(lang, &tooltip),
                PickerAction::PickModTap(left_value),
            )
            .with_aliases(format!("Mod-Tap MT {mod_name}"));
            if let Some(right_value) = right_value {
                row = row.with_secondary(PickerAction::PickModTap(right_value));
            }
            rows.push(row);
        }

        let one_shot_section = tr_picker(lang, "key_picker.section_one_shot_mod");
        for choice in one_shot_modifier_choices(gui, gui_mod_name()) {
            let tooltip = one_shot_modifier_tooltip(&choice.mod_name, choice.right_value.is_some());
            let mut row = self
                .vial_row(
                    tab,
                    one_shot_section,
                    choice.left_value,
                    choice.label,
                    crate::i18n::tr_text(lang, &tooltip),
                )
                .with_aliases(format!("One-Shot OSM {}", choice.mod_name));
            if let Some(right_value) = choice.right_value {
                row = row.with_secondary(PickerAction::Assign(crate::keyboard::KeyBinding::Vial(
                    right_value,
                )));
            }
            rows.push(row);
        }

        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_modifier_choices_include_shift_gui_chord() {
        let choices = one_shot_modifier_choices("GUI", "GUI");
        let shift_gui = choices
            .iter()
            .find(|choice| choice.left_value == 0x52AA)
            .expect("OS_LSG should be exposed as a one-shot modifier chord");

        assert_eq!(shift_gui.right_value, Some(0x52BA));
        assert_eq!(shift_gui.label, "OSM\nS+GUI");
        assert_eq!(shift_gui.mod_name, "Shift+GUI");
    }

    #[test]
    fn mod_tap_choices_include_gui_chords() {
        let choices = mod_tap_choices(crate::keycode::gui_label(false));
        for (value, modifier) in [(0x2900, "Ctrl"), (0x2A00, "Shift")] {
            let (label, _, right_value, mod_name) = choices
                .iter()
                .find(|(_, left_value, _, _)| *left_value == value)
                .expect("GUI Mod-Tap chord should be exposed in the picker");

            assert_eq!(*right_value, None);
            assert_eq!(
                label,
                &format!("Hold {modifier}+{}/key", crate::keycode::gui_sym())
            );
            assert_eq!(
                mod_name,
                &format!("{modifier}+{}", crate::keycode::gui_label(false))
            );
        }
    }

    #[test]
    fn one_shot_modifier_choices_cover_mod_key_chords() {
        let values: Vec<u16> = one_shot_modifier_choices("GUI", "GUI")
            .iter()
            .map(|choice| choice.left_value)
            .collect();

        for value in [0x52A3, 0x52A5, 0x52A6, 0x52AA, 0x52A7, 0x52AF] {
            assert!(
                values.contains(&value),
                "missing one-shot modifier chord {value:#06X}"
            );
        }
    }
}
