use super::*;

type PopupChoice = (u16, String, String);
type PopupChoiceGroup = (&'static str, Vec<PopupChoice>);

pub(super) fn popup_group_i18n_key(title: &str) -> Option<&'static str> {
    match title {
        "Letters" => Some("key_picker.group_letters"),
        "Numbers" => Some("key_picker.group_numbers"),
        "Symbols" => Some("key_picker.group_symbols"),
        "Editing" => Some("key_picker.group_editing"),
        "Navigation" => Some("key_picker.group_navigation"),
        "Function keys" => Some("key_picker.group_function_keys"),
        "Numpad" => Some("key_picker.group_numpad"),
        "Media & system" => Some("key_picker.group_media_system"),
        "Mouse" => Some("key_picker.group_mouse"),
        "International" => Some("key_picker.group_international"),
        "Modifiers" => Some("key_picker.group_modifiers"),
        "Other keys" => Some("key_picker.group_other_keys"),
        _ => None,
    }
}

pub(super) fn popup_group_title(
    language: crate::i18n::Language,
    title: &'static str,
) -> &'static str {
    popup_group_i18n_key(title)
        .map(|key| tr_picker(language, key))
        .unwrap_or_else(|| crate::i18n::tr_catalog(language, title))
}

pub(super) fn popup_key_group_title(kc: &crate::keycode::Keycode) -> &'static str {
    match kc.value {
        0x0004..=0x001D => "Letters",
        0x001E..=0x0027 => "Numbers",
        0x002D..=0x0038 | 0x0064 | 0x021E..=0x0227 | 0x022D..=0x0238 => "Symbols",
        0x0028..=0x002C | 0x0039 | 0x0065 | 0x0076 => "Editing",
        _ if matches!(kc.category, KeycodeCategory::Navigation) => "Navigation",
        _ if matches!(kc.category, KeycodeCategory::Function) => "Function keys",
        _ if matches!(kc.category, KeycodeCategory::Numpad) => "Numpad",
        _ if matches!(kc.category, KeycodeCategory::Media) => "Media & system",
        _ if matches!(kc.category, KeycodeCategory::Mouse) => "Mouse",
        0x0087..=0x0094 => "International",
        _ if matches!(kc.category, KeycodeCategory::Modifier) => "Modifiers",
        _ => "Other keys",
    }
}

fn popup_order(value: u16, values: &[u16]) -> usize {
    values
        .iter()
        .position(|candidate| *candidate == value)
        .unwrap_or(usize::MAX)
}

fn popup_key_group_sort_key(kc: &crate::keycode::Keycode) -> usize {
    match popup_key_group_title(kc) {
        "Symbols" => popup_order(
            kc.value,
            &[
                0x0035, 0x002D, 0x002E, 0x002F, 0x0030, 0x0031, 0x0033, 0x0034, 0x0036, 0x0037,
                0x0038, 0x0064,
            ],
        ),
        "Editing" => popup_order(
            kc.value,
            &[0x0029, 0x002A, 0x002B, 0x0039, 0x0028, 0x002C, 0x0065],
        ),
        "Navigation" => popup_order(
            kc.value,
            &[
                0x0046, 0x0047, 0x0048, 0x0049, 0x004C, 0x004A, 0x004D, 0x004B, 0x004E, 0x0050,
                0x0052, 0x0051, 0x004F,
            ],
        ),
        "Media & system" => popup_order(
            kc.value,
            &[
                0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00AA, 0x00A9, 0x00AC, 0x00AB, 0x00AD, 0x00AE,
                0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8,
                0x00B9, 0x00BA, 0x00BC, 0x00BB, 0x00BE, 0x00BD, 0x00BF, 0x00C0,
            ],
        ),
        _ => kc.value as usize,
    }
}

pub(super) fn is_f13_to_f24(value: u16) -> bool {
    (0x0068..=0x0073).contains(&value)
}

pub(super) fn is_8bit_tap_key_choice(kc: &crate::keycode::Keycode) -> bool {
    kc.value != 0
        && kc.value != 0x0001
        && kc.value < 0x0100
        && !is_f13_to_f24(kc.value)
        && !matches!(kc.value, 0x0087..=0x0094)
        && matches!(
            kc.category,
            KeycodeCategory::Basic
                | KeycodeCategory::Function
                | KeycodeCategory::Navigation
                | KeycodeCategory::Numpad
                | KeycodeCategory::Media
                | KeycodeCategory::Mouse
                | KeycodeCategory::Modifier
        )
}

pub(super) fn popup_key_button_label(
    kc: &crate::keycode::Keycode,
    layer_names: &[String],
    friendly_mods: bool,
    key_legend_layout: KeyLegendLayout,
) -> String {
    if matches!(kc.category, KeycodeCategory::Numpad) {
        return match kc.name {
            "KC_NUMLOCK" => "Num\nLock",
            "KC_KP_SLASH" => "Num\n÷",
            "KC_KP_ASTERISK" => "Num\n×",
            "KC_KP_MINUS" => "Num\n−",
            "KC_KP_PLUS" => "Num\n+",
            "KC_KP_ENTER" => "Num\nEnter",
            "KC_KP_1" => "Num\n1",
            "KC_KP_2" => "Num\n2",
            "KC_KP_3" => "Num\n3",
            "KC_KP_4" => "Num\n4",
            "KC_KP_5" => "Num\n5",
            "KC_KP_6" => "Num\n6",
            "KC_KP_7" => "Num\n7",
            "KC_KP_8" => "Num\n8",
            "KC_KP_9" => "Num\n9",
            "KC_KP_0" => "Num\n0",
            "KC_KP_DOT" => "Num\n.",
            "KC_KP_COMMA" => "Num\n,",
            "KC_KP_EQUAL" => "Num\n=",
            _ => kc.label,
        }
        .into();
    }
    if friendly_mods {
        let gui = crate::keycode::gui_mod_name();
        match kc.value {
            0x00E0 => return "Left\nCtrl".into(),
            0x00E4 => return "Right\nCtrl".into(),
            0x00E1 => return "Left\nShift".into(),
            0x00E5 => return "Right\nShift".into(),
            0x00E2 => return "Left\nAlt".into(),
            0x00E6 => return "Right\nAlt".into(),
            0x00E3 => return format!("Left\n{}", gui),
            0x00E7 => return format!("Right\n{}", gui),
            _ => {}
        }
    }
    keycode_label_with_names_and_layout(kc.value, &[], layer_names, key_legend_layout)
}

fn friendly_plain_modifier_entry(value: u16) -> Option<(String, u16, u16, String)> {
    match value {
        0x00E0 => Some(("Ctrl".to_owned(), 0x00E0, 0x00E4, "Ctrl".to_owned())),
        0x00E1 => Some(("Shift".to_owned(), 0x00E1, 0x00E5, "Shift".to_owned())),
        0x00E2 => Some(("Alt".to_owned(), 0x00E2, 0x00E6, "Alt".to_owned())),
        0x00E3 => Some((
            crate::keycode::gui_label(false).to_string(),
            0x00E3,
            0x00E7,
            crate::keycode::gui_mod_name().to_string(),
        )),
        _ => None,
    }
}

fn is_right_plain_modifier(value: u16) -> bool {
    matches!(value, 0x00E4..=0x00E7)
}

pub(super) fn picker_shifted_number_label(
    value: u16,
    key_legend_layout: KeyLegendLayout,
) -> Option<String> {
    let (digit, english, russian) = match value {
        0x001E => ("1", "!", "!"),
        0x001F => ("2", "@", "\""),
        0x0020 => ("3", "#", "№"),
        0x0021 => ("4", "$", ";"),
        0x0022 => ("5", "%", "%"),
        0x0023 => ("6", "^", ":"),
        0x0024 => ("7", "&", "?"),
        0x0025 => ("8", "*", "*"),
        0x0026 => ("9", "(", "("),
        0x0027 => ("0", ")", ")"),
        _ => return None,
    };
    let shifted = match key_legend_layout {
        KeyLegendLayout::English => english.to_string(),
        KeyLegendLayout::Russian if english == russian => english.to_string(),
        KeyLegendLayout::Russian => format!("{}  {}", english, russian),
        KeyLegendLayout::RussianPrimary if english == russian => russian.to_string(),
        KeyLegendLayout::RussianPrimary => format!("{}  {}", russian, english),
    };
    Some(format!("{}\n{}", shifted, digit))
}

pub(super) fn show_grouped_popup_key_buttons(
    ui: &mut egui::Ui,
    keys: Vec<&'static crate::keycode::Keycode>,
    layer_names: &[String],
    friendly_mods: bool,
    language: crate::i18n::Language,
    key_legend_layout: KeyLegendLayout,
) -> Option<u16> {
    let default_group_order = [
        "Letters",
        "Numbers",
        "Symbols",
        "Editing",
        "Navigation",
        "Function keys",
        "Numpad",
        "Media & system",
        "Mouse",
        "Modifiers",
    ];
    let friendly_modifier_group_order = [
        "Modifiers",
        "Letters",
        "Numbers",
        "Symbols",
        "Editing",
        "Navigation",
        "Function keys",
        "Numpad",
        "Media & system",
        "Mouse",
    ];
    let group_order = if friendly_mods {
        friendly_modifier_group_order
    } else {
        default_group_order
    };
    let mut selected = None;

    for title in group_order {
        let mut group: Vec<&'static crate::keycode::Keycode> = keys
            .iter()
            .copied()
            .filter(|kc| popup_key_group_title(kc) == title)
            .filter(|kc| !friendly_mods || !is_right_plain_modifier(kc.value))
            .collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by_key(|kc| popup_key_group_sort_key(kc));

        ui.add_space(2.0);
        ui.label(
            RichText::new(popup_group_title(language, title))
                .size(11.0)
                .color(Color32::from_gray(150))
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for kc in &group {
                if friendly_mods {
                    if let Some((label, left_value, right_value, mod_name)) =
                        friendly_plain_modifier_entry(kc.value)
                    {
                        let size = popup_key_button_size(ui, &label);
                        let resp = picker_keycap_button(ui, &label, size, true, false);
                        if resp.clicked_by(egui::PointerButton::Primary) {
                            selected = Some(left_value);
                        }
                        if resp.clicked_by(egui::PointerButton::Secondary) {
                            selected = Some(right_value);
                        }
                        if resp.hovered() {
                            resp.on_hover_text(crate::i18n::tr_text(
                                language,
                                &plain_modifier_tooltip(&mod_name),
                            ));
                        }
                        continue;
                    }
                }

                let label =
                    popup_key_button_label(kc, layer_names, friendly_mods, key_legend_layout);
                let size = popup_key_button_size(ui, &label);
                let resp = picker_keycap_button(ui, &label, size, true, false);
                if resp.clicked() {
                    selected = Some(kc.value);
                }
                if resp.hovered() {
                    resp.on_hover_text(crate::i18n::tr_text(
                        language,
                        &keycode_tooltip(kc.value, &[], layer_names),
                    ));
                }
            }
        });
        ui.add_space(8.0);
    }

    selected
}

pub(super) fn show_grouped_popup_choice_buttons(
    ui: &mut egui::Ui,
    groups: Vec<PopupChoiceGroup>,
    language: crate::i18n::Language,
) -> Option<u16> {
    let mut selected = None;

    for (title, choices) in groups {
        if choices.is_empty() {
            continue;
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new(popup_group_title(language, title))
                .size(11.0)
                .color(Color32::from_gray(150))
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (value, label, tooltip) in choices {
                let size = popup_key_button_size(ui, &label);
                let resp = picker_keycap_button(ui, &label, size, true, false);
                if resp.clicked() {
                    selected = Some(value);
                }
                if resp.hovered() {
                    resp.on_hover_text(crate::i18n::tr_text(language, &tooltip));
                }
            }
        });
        ui.add_space(8.0);
    }

    selected
}
