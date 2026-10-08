use super::*;

impl KeycodePicker {
    pub(super) fn picker_key_size(ctx: &egui::Context) -> Vec2 {
        responsive_picker_key_size(ctx)
    }

    pub(super) fn key_grid_width(ui: &egui::Ui, cols: usize, spacing: f32) -> f32 {
        let key_size = Self::picker_key_size(ui.ctx());
        key_size.x * cols as f32 + spacing * cols.saturating_sub(1) as f32
    }

    pub(super) fn slot_grid_width(cols: usize, spacing: f32) -> f32 {
        48.0 * cols as f32 + spacing * cols.saturating_sub(1) as f32
    }

    pub(super) fn tab_content_width(&self, ui: &egui::Ui) -> f32 {
        let spacing = ui.spacing().item_spacing.x;
        let width = match self.selected_tab {
            KeycodeTab::Symbols
            | KeycodeTab::UniversalSymbols
            | KeycodeTab::Special
            | KeycodeTab::Rgb
            | KeycodeTab::Custom
            | KeycodeTab::Bluetooth => Self::key_grid_width(ui, 13, spacing),
            KeycodeTab::Modifiers => Self::key_grid_width(ui, 13, spacing),
            KeycodeTab::Macro | KeycodeTab::TapDance => Self::slot_grid_width(8, 8.0),
            _ => 840.0,
        };
        width.min(ui.available_width())
    }

    pub(super) fn paint_compact_picker_label(ui: &egui::Ui, resp: &egui::Response, label: &str) {
        let visuals = ui.style().interact(resp);
        let painter = ui.painter();
        let dark = ui.visuals().dark_mode;
        let (top, bottom) = if label.contains('\n') {
            let mut parts = label.splitn(2, '\n');
            let t = parts.next().unwrap_or("");
            let b = parts.next().unwrap_or(label);
            (Some(t), b)
        } else if let Some(pos) = label
            .find('/')
            .filter(|pos| *pos > 0 && *pos + 1 < label.len())
        {
            (Some(&label[..pos]), &label[pos + 1..])
        } else {
            (None, label)
        };
        let label_scale = (resp.rect.height() / 54.0).clamp(1.0, 1.22);
        let (top_size, bottom_size) = key_label_font_sizes(label);
        let top_size = top_size.map(|size| size * label_scale);
        let bottom_size = bottom_size * label_scale;
        let top_color = if dark {
            Color32::from_rgb(130, 130, 145)
        } else {
            Color32::from_rgb(130, 130, 150)
        };
        let main_color = if resp.enabled() {
            if dark {
                Color32::from_rgb(239, 233, 232)
            } else {
                Color32::from_rgb(26, 26, 30)
            }
        } else {
            visuals.fg_stroke.color
        };

        if let Some(top_str) = top {
            let center = resp.rect.center();
            painter.text(
                egui::pos2(center.x, center.y - 7.0 * label_scale),
                egui::Align2::CENTER_CENTER,
                top_str,
                egui::FontId::proportional(top_size.unwrap_or(9.0)),
                top_color,
            );
            painter.text(
                egui::pos2(center.x, center.y + 6.0 * label_scale),
                egui::Align2::CENTER_CENTER,
                bottom,
                egui::FontId::proportional(bottom_size),
                main_color,
            );
        } else {
            let font_size = if bottom == "↵" {
                16.0 * label_scale
            } else {
                bottom_size
            };
            painter.text(
                resp.rect.center(),
                egui::Align2::CENTER_CENTER,
                bottom,
                egui::FontId::proportional(font_size),
                main_color,
            );
        }
    }

    // Curated Special-tab entries: visible caption, keycode, tooltip.
    // The search index reuses these exact strings so everything the user
    // sees on the Special tab is findable by the same words.
    pub(super) fn vial_special_key_entries(&self) -> Vec<(String, u16, String)> {
        vec![
            (
                "✕
None"
                    .into(),
                0x0000,
                "KC_NO — disables this key completely, it sends nothing when pressed".into(),
            ),
            (
                "▽
Inherit"
                    .into(),
                0x0001,
                "KC_TRNS — inherits the key from the layer below".into(),
            ),
            (
                "Esc
~"
                .into(),
                0x7C16,
                format!(
                    "Grave/Escape — sends Esc normally, ` when Shift or {} is held",
                    gui_mod_name()
                ),
            ),
            (
                "⚡
Boot"
                    .into(),
                0x7C00,
                "QK_BOOT — put keyboard into flash mode".into(),
            ),
            (
                "🐛
Debug"
                    .into(),
                0x7C02,
                "DB_TOGG — toggle debug mode".into(),
            ),
            (
                "🔒
Lock"
                    .into(),
                0x7800,
                "QK_LOCK — hold to lock remaining keys until pressed again".into(),
            ),
            (
                "Auto
Shift"
                    .into(),
                0x7C15,
                "Toggles the state of the Auto Shift feature".into(),
            ),
            (
                "Combo
Toggle"
                    .into(),
                0x7C52,
                "Toggles Combo feature on and off".into(),
            ),
            (
                "Caps
Word"
                    .into(),
                0x7C73,
                "Capitalizes until end of current word".into(),
            ),
            (
                "Repeat".into(),
                0x7C79,
                "Repeats the last pressed key".into(),
            ),
            (
                "Alt
Repeat"
                    .into(),
                0x7C7A,
                "Alt repeats the last pressed key".into(),
            ),
        ]
    }

    /// Every section of the Special tab, in display order.
    pub(super) fn special_rows(&self) -> Vec<PickerRow> {
        let lang = self.language;
        let tab = KeycodeTab::Special;
        let mut rows = Vec::new();

        // Special keys: the slot choosers first, then the curated QMK keys.
        let special = crate::i18n::tr_catalog(lang, "key_picker_text.special_qmk_keys");
        if self.supports_macro {
            rows.push(PickerRow::new(
                tab,
                special,
                compact_caption(tr_picker(lang, "macro_editor.picker_item")),
                tr_picker(lang, "key_picker.advanced_macro_tooltip"),
                PickerAction::ChooseSlot(AdvancedSlotKind::Macro),
            ));
        }
        if self.supports_tap_dance {
            rows.push(PickerRow::new(
                tab,
                special,
                compact_caption(tr_picker(lang, "tap_dance_editor.picker_item")),
                tr_picker(lang, "key_picker.advanced_tap_dance_tooltip"),
                PickerAction::ChooseSlot(AdvancedSlotKind::TapDance),
            ));
        }
        for (label, value, tip) in self.vial_special_key_entries() {
            if !self.picker_value_supported(value) {
                continue;
            }
            if let Some(kc) = crate::keycode::find_keycode(value) {
                if !self.vial_keycode_supported(kc) {
                    continue;
                }
            }
            rows.push(self.vial_row(tab, special, value, label, crate::i18n::tr_text(lang, &tip)));
        }

        if self.supports_mouse_keys {
            let mouse = crate::i18n::tr_catalog(lang, "key_picker_text.mouse");
            rows.extend(
                KEYCODES
                    .iter()
                    .filter(|kc| {
                        matches!(kc.category, KeycodeCategory::Mouse)
                            && self.picker_value_supported(kc.value)
                    })
                    .map(|kc| self.table_keycode_row(tab, mouse, kc.value)),
            );
        }

        let media = crate::i18n::tr_catalog(lang, "key_picker_text.media_apps_system");
        rows.extend(
            MEDIA_KEYS
                .iter()
                .map(|(_, value)| *value)
                .filter(|value| self.picker_value_supported(*value))
                .map(|value| self.table_keycode_row(tab, media, value)),
        );

        let shortcuts = crate::i18n::tr_catalog(lang, "key_picker_text.os_edit_shortcuts");
        for (os, text, value, tip) in OS_EDIT_SHORTCUTS {
            if !self.picker_value_supported(*value) {
                continue;
            }
            rows.push(
                PickerRow::new(
                    tab,
                    shortcuts,
                    *text,
                    crate::i18n::tr_text(lang, tip),
                    PickerAction::Assign(crate::keyboard::KeyBinding::Vial(*value)),
                )
                .with_aliases(*os),
            );
        }

        let numpad = crate::i18n::tr_catalog(lang, "key_picker_text.numpad");
        for kc in KEYCODES.iter().filter(|kc| {
            matches!(kc.category, KeycodeCategory::Numpad) && self.picker_value_supported(kc.value)
        }) {
            rows.push(
                self.vial_row(
                    tab,
                    numpad,
                    kc.value,
                    format!("Num\n{}", numpad_display(kc)),
                    crate::i18n::tr_text(lang, &self.picker_keycode_tooltip(kc.value, &[])),
                )
                .with_aliases(kc.label),
            );
        }

        let function = crate::i18n::tr_catalog(lang, "key_picker_text.function_keys");
        for kc in KEYCODES.iter().filter(|kc| {
            is_extended_function_key(kc.value) && self.picker_value_supported(kc.value)
        }) {
            rows.push(self.vial_row(
                tab,
                function,
                kc.value,
                kc.label,
                crate::i18n::tr_text(lang, &self.picker_keycode_tooltip(kc.value, &[])),
            ));
        }

        let magic = crate::i18n::tr_catalog(lang, "ui.magic_title");
        for value in MAGIC_KEYS
            .iter()
            .copied()
            .filter(|value| self.picker_value_supported(*value))
        {
            rows.push(self.vial_row(
                tab,
                magic,
                value,
                crate::keycode::keycode_label(value),
                crate::i18n::tr_text(lang, &self.picker_keycode_tooltip(value, &[])),
            ));
        }

        let cadet = crate::i18n::tr_catalog(lang, "key_picker_text.space_cadet");
        for (top, bottom, value, tip) in SPACE_CADET_KEYS {
            if !self.picker_value_supported(*value) {
                continue;
            }
            rows.push(self.vial_row(
                tab,
                cadet,
                *value,
                format!("{top}\n{bottom}"),
                crate::i18n::tr_text(lang, tip),
            ));
        }

        let international = crate::i18n::tr_catalog(lang, "key_picker_text.international");
        if self.universal_russian_letters_available() {
            rows.extend(universal_russian_letter_rows(lang, tab, international));
        }
        for (top, bottom, value, tip) in INTERNATIONAL_KEYS {
            if !self.picker_value_supported(*value) {
                continue;
            }
            rows.push(self.vial_row(
                tab,
                international,
                *value,
                format!("{top}\n{bottom}"),
                crate::i18n::tr_text(lang, tip),
            ));
        }

        rows
    }

    pub(super) fn show_vial_special(
        &mut self,
        ui: &mut egui::Ui,
        macro_data_state: DeferredPickerDataState,
        tap_dance_data_state: DeferredPickerDataState,
    ) {
        let rows = self.cached_tab_rows(KeycodeTab::Special);
        self.show_picker_rows(
            ui,
            &rows,
            SlotDataStates {
                macro_state: macro_data_state,
                tap_dance_state: tap_dance_data_state,
            },
        );
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn bluetooth_and_custom_keycaps_use_the_same_horizontal_insets() {
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| {
                let mut picker = KeycodePicker::default();
                for tab in [KeycodeTab::Custom, KeycodeTab::Bluetooth] {
                    picker.selected_tab = tab;
                    widths.push(picker.tab_content_width(ui));
                }
                assert!(ui.available_width() > widths[0]);
                assert_eq!(
                    widths[0],
                    KeycodePicker::key_grid_width(ui, 13, ui.spacing().item_spacing.x)
                );
            },
        );
        assert_eq!(widths[0], widths[1]);
    }
}

/// Slot chooser caption on two keycap lines ("Tap Dance" -> "Tap\nDance").
fn compact_caption(caption: &str) -> String {
    caption.replace(' ', "\n")
}

/// Short bottom line of a numpad keycap; the top line always says "Num".
fn numpad_display(kc: &crate::keycode::Keycode) -> &str {
    match kc.name {
        "KC_NUMLOCK" => "Lock",
        "KC_KP_SLASH" => "÷",
        "KC_KP_ASTERISK" => "×",
        "KC_KP_MINUS" => "−",
        "KC_KP_PLUS" => "+",
        "KC_KP_ENTER" => "Enter",
        "KC_KP_1" => "1",
        "KC_KP_2" => "2",
        "KC_KP_3" => "3",
        "KC_KP_4" => "4",
        "KC_KP_5" => "5",
        "KC_KP_6" => "6",
        "KC_KP_7" => "7",
        "KC_KP_8" => "8",
        "KC_KP_9" => "9",
        "KC_KP_0" => "0",
        "KC_KP_DOT" => ".",
        "KC_KP_COMMA" => ",",
        "KC_KP_EQUAL" => "=",
        _ => kc
            .label
            .strip_prefix("Num ")
            .or_else(|| kc.label.strip_prefix("Numpad "))
            .or_else(|| kc.label.strip_prefix("Num"))
            .unwrap_or(kc.label),
    }
}

// Media, application and system keys, in display order: (name, keycode).
const MEDIA_KEYS: &[(&str, u16)] = &[
    ("Power", 0x00A5),
    ("Sleep", 0x00A6),
    ("Wake", 0x00A7),
    ("Mute", 0x00A8),
    ("Vol-", 0x00AA),
    ("Vol+", 0x00A9),
    ("Prev", 0x00AC),
    ("Next", 0x00AB),
    ("Stop", 0x00AD),
    ("Play", 0x00AE),
    ("Media", 0x00AF),
    ("Eject", 0x00B0),
    ("Mail", 0x00B1),
    ("Calc", 0x00B2),
    ("Files", 0x00B3),
    ("Search", 0x00B4),
    ("Home", 0x00B5),
    ("Back", 0x00B6),
    ("Fwd", 0x00B7),
    ("Web", 0x00B8),
    ("Reload", 0x00B9),
    ("Favs", 0x00BA),
    ("Rewind", 0x00BC),
    ("Fast+", 0x00BB),
    ("Bright-", 0x00BE),
    ("Bright+", 0x00BD),
    ("Mission", 0x00BF),
    ("Launch", 0x00C0),
];

// Editing shortcuts of the host OS: (OS, caption, keycode, tooltip).
#[cfg(target_os = "macos")]
const OS_EDIT_SHORTCUTS: &[(&str, &str, u16, &str)] = &[
    ("macOS", "Undo", 0x0800 | 0x001D, "Command + Z"),
    ("macOS", "Redo", 0x0A00 | 0x001D, "Command + Shift + Z"),
    ("macOS", "Cut", 0x0800 | 0x001B, "Command + X"),
    ("macOS", "Copy", 0x0800 | 0x0006, "Command + C"),
    ("macOS", "Paste", 0x0800 | 0x0019, "Command + V"),
    ("macOS", "Find", 0x0800 | 0x0009, "Command + F"),
    ("macOS", "Select\nAll", 0x0800 | 0x0004, "Command + A"),
    ("macOS", "Save", 0x0800 | 0x0016, "Command + S"),
    ("macOS", "New", 0x0800 | 0x0011, "Command + N"),
    ("macOS", "Open", 0x0800 | 0x0012, "Command + O"),
    ("macOS", "Close", 0x0800 | 0x001A, "Command + W"),
    ("macOS", "New\nTab", 0x0800 | 0x0017, "Command + T"),
    (
        "macOS",
        "Prev\nWord",
        0x0400 | 0x0050,
        "Option + Left Arrow",
    ),
    (
        "macOS",
        "Next\nWord",
        0x0400 | 0x004F,
        "Option + Right Arrow",
    ),
    (
        "macOS",
        "Prev\nApp",
        0x0A00 | 0x002B,
        "Shift + Command + Tab",
    ),
    ("macOS", "Next\nApp", 0x0800 | 0x002B, "Command + Tab"),
];
#[cfg(target_os = "windows")]
const OS_EDIT_SHORTCUTS: &[(&str, &str, u16, &str)] = &[
    ("Windows", "Undo", 0x0100 | 0x001D, "Ctrl + Z"),
    ("Windows", "Redo", 0x0100 | 0x001C, "Ctrl + Y"),
    ("Windows", "Cut", 0x0100 | 0x001B, "Ctrl + X"),
    ("Windows", "Copy", 0x0100 | 0x0006, "Ctrl + C"),
    ("Windows", "Paste", 0x0100 | 0x0019, "Ctrl + V"),
    ("Windows", "Find", 0x0100 | 0x0009, "Ctrl + F"),
    ("Windows", "Select\nAll", 0x0100 | 0x0004, "Ctrl + A"),
    ("Windows", "Save", 0x0100 | 0x0016, "Ctrl + S"),
    ("Windows", "New", 0x0100 | 0x0011, "Ctrl + N"),
    ("Windows", "Open", 0x0100 | 0x0012, "Ctrl + O"),
    ("Windows", "Close", 0x0100 | 0x001A, "Ctrl + W"),
    ("Windows", "New\nTab", 0x0100 | 0x0017, "Ctrl + T"),
    (
        "Windows",
        "Prev\nWord",
        0x0100 | 0x0050,
        "Ctrl + Left Arrow",
    ),
    (
        "Windows",
        "Next\nWord",
        0x0100 | 0x004F,
        "Ctrl + Right Arrow",
    ),
    ("Windows", "Prev\nApp", 0x0600 | 0x002B, "Shift + Alt + Tab"),
    ("Windows", "Next\nApp", 0x0400 | 0x002B, "Alt + Tab"),
];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const OS_EDIT_SHORTCUTS: &[(&str, &str, u16, &str)] = &[
    ("Linux", "Undo", 0x0100 | 0x001D, "Ctrl + Z"),
    ("Linux", "Redo", 0x0100 | 0x001C, "Ctrl + Y"),
    ("Linux", "Cut", 0x0100 | 0x001B, "Ctrl + X"),
    ("Linux", "Copy", 0x0100 | 0x0006, "Ctrl + C"),
    ("Linux", "Paste", 0x0100 | 0x0019, "Ctrl + V"),
    ("Linux", "Find", 0x0100 | 0x0009, "Ctrl + F"),
    ("Linux", "Select\nAll", 0x0100 | 0x0004, "Ctrl + A"),
    ("Linux", "Save", 0x0100 | 0x0016, "Ctrl + S"),
    ("Linux", "New", 0x0100 | 0x0011, "Ctrl + N"),
    ("Linux", "Open", 0x0100 | 0x0012, "Ctrl + O"),
    ("Linux", "Close", 0x0100 | 0x001A, "Ctrl + W"),
    ("Linux", "New\nTab", 0x0100 | 0x0017, "Ctrl + T"),
    ("Linux", "Prev\nWord", 0x0100 | 0x0050, "Ctrl + Left Arrow"),
    ("Linux", "Next\nWord", 0x0100 | 0x004F, "Ctrl + Right Arrow"),
    ("Linux", "Prev\nApp", 0x0600 | 0x002B, "Shift + Alt + Tab"),
    ("Linux", "Next\nApp", 0x0400 | 0x002B, "Alt + Tab"),
];

// QMK Magic keycodes, in display order.
const MAGIC_KEYS: &[u16] = &[
    0x7000, 0x7001, 0x7002, 0x7004, 0x7003, 0x7020, 0x7021, 0x7022, 0x7017, 0x7018, 0x7019, 0x701A,
    0x701B, 0x701C, 0x701D, 0x7005, 0x7006, 0x7007, 0x7008, 0x7014, 0x7015, 0x7016, 0x700A, 0x7009,
    0x700B, 0x700C, 0x700D, 0x700E, 0x700F, 0x7010, 0x7011, 0x7012, 0x7013, 0x701E, 0x701F,
];

// Space Cadet keys: (modifier, tapped output, keycode, tooltip).
const SPACE_CADET_KEYS: &[(&str, &str, u16, &str)] = &[
    (
        "LCtrl",
        "(",
        0x7C18,
        "Left Control when held, ( when tapped",
    ),
    (
        "RCtrl",
        ")",
        0x7C19,
        "Right Control when held, ) when tapped",
    ),
    ("LShift", "(", 0x7C1A, "Left Shift when held, ( when tapped"),
    (
        "RShift",
        ")",
        0x7C1B,
        "Right Shift when held, ) when tapped",
    ),
    ("LAlt", "(", 0x7C1C, "Left Alt when held, ( when tapped"),
    ("RAlt", ")", 0x7C1D, "Right Alt when held, ) when tapped"),
    (
        "RShift",
        "Enter",
        0x7C1E,
        "Right Shift when held, Enter when tapped",
    ),
];

// JIS and Hangul keys: (layout, caption, keycode, tooltip).
const INTERNATIONAL_KEYS: &[(&str, &str, u16, &str)] = &[
    ("JIS", "\\ _", 0x0087, "JIS \\ and _"),
    ("JIS", "Kana", 0x0088, "JIS Katakana/Hiragana"),
    ("JIS", "¥ |", 0x0089, "JIS ¥ and |"),
    ("JIS", "Henkan", 0x008A, "JIS Henkan"),
    ("JIS", "Muhenk", 0x008B, "JIS Muhenkan"),
    ("JIS", "Num ,", 0x008C, "JIS Numpad ,"),
    ("Hangul", "Eng", 0x0090, "Hangul/English"),
    ("Hangul", "Hanja", 0x0091, "Hanja"),
    ("JIS", "Katak", 0x0092, "JIS Katakana"),
    ("JIS", "Hirag", 0x0093, "JIS Hiragana"),
    ("JIS", "ZenHan", 0x0094, "JIS Zenkaku/Hankaku"),
];
