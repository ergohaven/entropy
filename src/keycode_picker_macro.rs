use super::*;

const SS_QMK_PREFIX: u8 = 1;
const SS_TAP_CODE: u8 = 1;
const SS_DOWN_CODE: u8 = 2;
const SS_UP_CODE: u8 = 3;
const SS_DELAY_CODE: u8 = 4;
const VIAL_MACRO_EXT_TAP: u8 = 5;
const VIAL_MACRO_EXT_DOWN: u8 = 6;
const VIAL_MACRO_EXT_UP: u8 = 7;

fn macro_settings_total_rows(action_rows: usize) -> usize {
    // Slot, name, description, all action rows, and one trailing selector
    // that creates the next action in place.
    4 + action_rows
}

fn append_macro_key_action(encoded: &mut Vec<u8>, basic_opcode: u8, ext_opcode: u8, keycode: u16) {
    encoded.push(SS_QMK_PREFIX);
    if keycode < 0x0100 {
        encoded.push(basic_opcode);
        encoded.push(keycode as u8);
        return;
    }

    encoded.push(ext_opcode);
    let wire_keycode = if keycode & 0x00FF == 0 {
        0xFF00 | (keycode >> 8)
    } else {
        keycode
    };
    encoded.extend_from_slice(&wire_keycode.to_le_bytes());
}

pub(crate) fn decode_macro_actions(bytes: &[u8]) -> Vec<MacroAction> {
    let mut actions = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == SS_QMK_PREFIX {
            if i + 1 >= bytes.len() {
                actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                break;
            }

            match bytes[i + 1] {
                SS_TAP_CODE => {
                    if i + 2 < bytes.len() {
                        let keycode = bytes[i + 2];
                        if is_send_string_keycode(keycode) {
                            actions.push(MacroAction::Tap(keycode as u16));
                        } else {
                            actions.push(MacroAction::Raw(bytes[i..i + 3].to_vec()));
                        }
                        i += 3;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                SS_DOWN_CODE => {
                    if i + 2 < bytes.len() {
                        let keycode = bytes[i + 2];
                        if is_send_string_keycode(keycode) {
                            actions.push(MacroAction::Down(keycode as u16));
                        } else {
                            actions.push(MacroAction::Raw(bytes[i..i + 3].to_vec()));
                        }
                        i += 3;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                SS_UP_CODE => {
                    if i + 2 < bytes.len() {
                        let keycode = bytes[i + 2];
                        if is_send_string_keycode(keycode) {
                            actions.push(MacroAction::Up(keycode as u16));
                        } else {
                            actions.push(MacroAction::Raw(bytes[i..i + 3].to_vec()));
                        }
                        i += 3;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                SS_DELAY_CODE => {
                    if i + 3 < bytes.len() && bytes[i + 2] != 0 && bytes[i + 3] != 0 {
                        let ms = (bytes[i + 2] as u16 - 1) + (bytes[i + 3] as u16 - 1) * 255;
                        actions.push(MacroAction::Delay(ms));
                        i += 4;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                VIAL_MACRO_EXT_TAP..=VIAL_MACRO_EXT_UP => {
                    if i + 3 < bytes.len() {
                        let mut keycode = u16::from_le_bytes([bytes[i + 2], bytes[i + 3]]);
                        // Matches Vial GUI's workaround for QMK decode_keycode():
                        // values whose low byte is zero are serialized as FFxx.
                        if keycode > 0xFF00 {
                            keycode = (keycode & 0xFF) << 8;
                        }
                        match bytes[i + 1] {
                            VIAL_MACRO_EXT_TAP => actions.push(MacroAction::Tap(keycode)),
                            VIAL_MACRO_EXT_DOWN => actions.push(MacroAction::Down(keycode)),
                            VIAL_MACRO_EXT_UP => actions.push(MacroAction::Up(keycode)),
                            _ => {}
                        }
                        i += 4;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                _ => {
                    let end = (i + 2).min(bytes.len());
                    actions.push(MacroAction::Raw(bytes[i..end].to_vec()));
                    i = end;
                }
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != SS_QMK_PREFIX {
                i += 1;
            }
            push_macro_text_or_raw(&mut actions, &bytes[start..i]);
        }
    }
    actions
}

fn is_send_string_keycode(keycode: u8) -> bool {
    matches!(keycode, 0x04..=0xC0 | 0xCD..=0xE7)
}

pub(crate) fn encode_macro_actions(actions: &[MacroAction]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for action in actions {
        match action {
            MacroAction::Text(s) => encoded.extend_from_slice(s.as_bytes()),
            MacroAction::Tap(kc) => {
                append_macro_key_action(&mut encoded, SS_TAP_CODE, VIAL_MACRO_EXT_TAP, *kc);
            }
            MacroAction::Down(kc) => {
                append_macro_key_action(&mut encoded, SS_DOWN_CODE, VIAL_MACRO_EXT_DOWN, *kc);
            }
            MacroAction::Up(kc) => {
                append_macro_key_action(&mut encoded, SS_UP_CODE, VIAL_MACRO_EXT_UP, *kc);
            }
            MacroAction::Delay(ms) => {
                let hi = (*ms / 255 + 1) as u8;
                let lo = (*ms % 255 + 1) as u8;
                encoded.push(SS_QMK_PREFIX);
                encoded.push(SS_DELAY_CODE);
                encoded.push(lo);
                encoded.push(hi);
            }
            MacroAction::Raw(bytes) => encoded.extend_from_slice(bytes),
        }
    }
    encoded
}

fn push_macro_text_or_raw(actions: &mut Vec<MacroAction>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        actions.push(MacroAction::Text(s.to_string()));
    } else {
        actions.push(MacroAction::Raw(bytes.to_vec()));
    }
}

fn format_raw_macro_bytes(bytes: &[u8]) -> String {
    let mut label = bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > 8 {
        label.push_str(" ...");
    }
    if label.is_empty() {
        "empty".to_string()
    } else {
        label
    }
}

fn limit_chars(value: &mut String, limit: usize) {
    let limited: String = value.chars().take(limit).collect();
    if limited.len() != value.len() {
        *value = limited;
    }
}

/// Distinct characters the Text warning names before it folds the rest into a
/// count, so a pasted paragraph of Cyrillic still fits under its step.
const MACRO_TEXT_UNTYPABLE_SHOWN: usize = 5;

/// Text steps are typed through the en-US ASCII table of QMK-Vial and RMK
/// alike: printable ASCII plus Tab, Enter, Backspace, Escape and Delete. Any
/// other character, UTF-8 bytes included, is typed as nothing.
fn macro_text_char_is_typable(c: char) -> bool {
    matches!(c, ' '..='~' | '\t' | '\n' | '\u{08}' | '\u{1B}' | '\u{7F}')
}

/// Plain-ASCII forms of the typography that editors and chat apps insert on
/// their own. The firmware cannot type the originals, while the ASCII form is
/// what the user meant; letters of other alphabets have no such form and stay.
fn ascii_typography_replacement(c: char) -> Option<&'static str> {
    Some(match c {
        // ‘ ’ ‚ ‛ ′ ‹ ›
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{2039}'
        | '\u{203A}' => "'",
        // “ ” „ ‟ « » ″
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{00AB}' | '\u{00BB}'
        | '\u{2033}' => "\"",
        // ‐ ‑ ‒ – — ― −
        '\u{2010}'..='\u{2015}' | '\u{2212}' => "-",
        '\u{2026}' => "...",
        // No-break, fixed-width, thin, hair and ideographic spaces.
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => " ",
        _ => return None,
    })
}

/// Returns the ASCII-normalized text and how many characters were replaced,
/// or `None` when there is nothing to replace.
fn normalize_macro_text_typography(text: &str) -> Option<(String, usize)> {
    let mut replaced = 0;
    let mut normalized = String::with_capacity(text.len());
    for c in text.chars() {
        match ascii_typography_replacement(c) {
            Some(ascii) => {
                normalized.push_str(ascii);
                replaced += 1;
            }
            None => normalized.push(c),
        }
    }
    (replaced > 0).then_some((normalized, replaced))
}

/// Where a character index of `original` lands after normalization: `…`
/// becomes three characters, and a cursor left at its old index would split
/// the replacement with the next keystroke.
fn typography_normalized_char_index(original: &str, index: usize) -> usize {
    original
        .chars()
        .take(index)
        .map(|c| ascii_typography_replacement(c).map_or(1, str::len))
        .sum()
}

fn move_text_cursor_past_typography(ctx: &egui::Context, id: egui::Id, original: &str) {
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
        return;
    };
    let Some(mut range) = state.cursor.char_range() else {
        return;
    };
    range.primary.index = typography_normalized_char_index(original, range.primary.index);
    range.secondary.index = typography_normalized_char_index(original, range.secondary.index);
    state.cursor.set_char_range(Some(range));
    egui::TextEdit::store_state(ctx, id, state);
}

/// Distinct untypable characters in order of first use, joined for display,
/// and how many more distinct ones did not fit.
fn macro_text_untypable_summary(text: &str) -> Option<(String, usize)> {
    let mut distinct = Vec::new();
    for c in text.chars().filter(|c| !macro_text_char_is_typable(*c)) {
        if !distinct.contains(&c) {
            distinct.push(c);
        }
    }
    if distinct.is_empty() {
        return None;
    }
    let shown = distinct
        .iter()
        .take(MACRO_TEXT_UNTYPABLE_SHOWN)
        .map(|c| {
            // An invisible character would leave a blank between the commas.
            if c.is_control()
                || c.is_whitespace()
                || matches!(
                    c,
                    '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'
                )
            {
                format!("U+{:04X}", *c as u32)
            } else {
                c.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some((
        shown,
        distinct.len().saturating_sub(MACRO_TEXT_UNTYPABLE_SHOWN),
    ))
}

fn macro_text_untypable_warning(language: crate::i18n::Language, text: &str) -> Option<String> {
    let (chars, more) = macro_text_untypable_summary(text)?;
    Some(if more == 0 {
        crate::i18n::tr_catalog_format(
            language,
            "macro_editor.text_will_not_be_typed",
            &[("chars", &chars)],
        )
    } else {
        crate::i18n::tr_catalog_format(
            language,
            "macro_editor.text_will_not_be_typed_more",
            &[("chars", &chars), ("count", &more.to_string())],
        )
    })
}

/// A settings list row after slot, name and description. A Text step the
/// firmware cannot fully type gets a warning row right under it, so the row
/// of a step is no longer a fixed offset from its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MacroActionRow {
    Action(usize),
    UntypableText(usize),
}

fn macro_action_rows(actions: &[MacroAction]) -> Vec<MacroActionRow> {
    let mut rows = Vec::with_capacity(actions.len());
    for (idx, action) in actions.iter().enumerate() {
        rows.push(MacroActionRow::Action(idx));
        if matches!(action, MacroAction::Text(text) if !text.chars().all(macro_text_char_is_typable))
        {
            rows.push(MacroActionRow::UntypableText(idx));
        }
    }
    rows
}

impl KeycodePicker {
    pub(super) fn assign_macro_slot(&mut self, slot: u8) {
        if (slot as usize) >= self.macro_count {
            return;
        }
        self.result = Some((0x7700 + slot as u16).into());
        self.macro_inline_selected = Some(slot);
        self.open = false;
    }

    pub(super) fn show_macro_slot_grid(
        &mut self,
        ui: &mut egui::Ui,
        selected_macro: u8,
        grid_id: &'static str,
    ) -> Option<u8> {
        let slot_count = self.macro_count.min(u8::MAX as usize + 1);
        if slot_count == 0 {
            return None;
        }

        let columns = ((ui.available_width() + 4.0) / 52.0)
            .floor()
            .clamp(4.0, 16.0) as usize;
        let mut picked = None;
        egui::Frame::NONE.show(ui, |ui| {
            let rows = slot_count.div_ceil(columns);
            let slot_scroll_height =
                (rows.min(2) as f32 * 43.0 + 4.0) * responsive_picker_element_scale(ui.ctx());
            ui.set_max_height(slot_scroll_height);
            egui::ScrollArea::vertical()
                .max_height(slot_scroll_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(grid_id)
                        .num_columns(columns)
                        .spacing([4.0, 4.0])
                        .show(ui, |ui| {
                            for i in 0..slot_count {
                                let i = i as u8;
                                let is_active = i == selected_macro;
                                let has_content = self.macro_has_content(i as usize);
                                let display_name = self.macro_display_name(i as usize);
                                let description = self.macro_description(i as usize);
                                let id_text = format!("M{}", i);
                                let mut resp = picker_slot_button(
                                    ui,
                                    &id_text,
                                    &display_name,
                                    is_active,
                                    has_content,
                                );
                                if let Some(description) = description {
                                    resp = resp
                                        .on_hover_text(format!("{display_name}\n{description}"));
                                } else if display_name != id_text {
                                    resp = resp.on_hover_text(display_name.clone());
                                }
                                if resp.clicked() {
                                    self.ensure_macro_meta_len(i as usize);
                                    picked = Some(i);
                                }
                                if (i as usize + 1).is_multiple_of(columns) {
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
        picked
    }

    /// `memory_use` is `(used, capacity)` of the keyboard's macro memory, when
    /// the board reported it.
    pub(crate) fn show_macro_settings_editor(
        &mut self,
        ui: &mut egui::Ui,
        raw_n: u8,
        grid_id: &'static str,
        memory_use: Option<(usize, usize)>,
    ) -> u8 {
        let slot_count = self.macro_count.min(u8::MAX as usize + 1);
        if slot_count == 0 {
            crate::ui_style::modal_empty_state(
                ui,
                crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.no_macro_slots_available_on_this_keyboard",
                ),
                None,
            );
            return 254;
        }

        let mut selected_macro = if (raw_n as usize) < slot_count {
            raw_n
        } else {
            0
        };
        let n = selected_macro as usize;
        self.ensure_macro_meta_len(n);

        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let scale = metrics.scale;
        let content_width = metrics.settings_content_width();
        let row_content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let control_width = metrics.settings_control_width();
        let control_height = metrics.settings_control_height();
        let control_font_size = metrics.settings_control_font_size();
        let action_control_width = metrics.value(300.0);
        let language = self.language;
        let custom_pairs = self.custom_keycode_pairs();
        let original_actions = self.macro_actions[n].clone();
        let mut edited_actions = original_actions.clone();
        let original_name = self.macro_names[n].clone();
        let original_description = self.macro_descriptions[n].clone();
        let mut edited_name = original_name.clone();
        let mut edited_description = original_description.clone();
        let mut picked_slot = None;
        let mut remove_idx = None;
        let mut move_up = None;
        let mut move_down = None;
        let mut undo_applied = false;
        let mut ascii_replaced = 0;

        if let Some(notice) = self.macro_ext_keycodes_notice(language) {
            crate::ui_style::modal_hint(ui, notice);
            ui.add_space(metrics.value(4.0));
        }
        if let Some((used, capacity)) = memory_use {
            let color = if used > capacity {
                Color32::from_rgb(220, 92, 76)
            } else {
                crate::ui_style::muted_text(ui.visuals().dark_mode)
            };
            ui.label(
                RichText::new(crate::i18n::tr_catalog_format(
                    language,
                    "macro_editor.memory_use",
                    &[
                        ("used", &used.to_string()),
                        ("capacity", &capacity.to_string()),
                    ],
                ))
                .size(11.0)
                .color(color),
            )
            .on_hover_text(crate::i18n::tr_catalog(
                language,
                "macro_editor.memory_use_shared_by_all_macros",
            ));
            ui.add_space(metrics.value(4.0));
        }
        if self
            .macro_text_ascii_replaced
            .is_some_and(|(macro_idx, _)| macro_idx != n)
        {
            self.macro_text_ascii_replaced = None;
        }
        if let Some((_, replaced)) = self.macro_text_ascii_replaced {
            crate::ui_style::modal_hint(
                ui,
                &crate::i18n::tr_catalog_format(
                    language,
                    "macro_editor.text_replaced_with_ascii",
                    &[("count", &replaced.to_string())],
                ),
            );
            ui.add_space(metrics.value(4.0));
        }

        let action_rows = macro_action_rows(&edited_actions);
        let total_rows = macro_settings_total_rows(action_rows.len());
        crate::ui_style::modal_content(
            ui,
            crate::ui_style::ModalLayout::new(content_width).with_top_padding(metrics.value(4.0)),
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let list = crate::app::allocate_adaptive_settings_list_viewport(
                    ui,
                    "macro_settings",
                    metrics,
                    total_rows,
                    metrics.value(94.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;

                    for row_idx in list.first_visible_row..list.last_visible_row {
                        match row_idx {
                            0 => {
                                let labels = (0..slot_count)
                                    .map(|idx| {
                                        let name = self
                                            .macro_names
                                            .get(idx)
                                            .map(|name| name.trim())
                                            .filter(|name| !name.is_empty());
                                        match name {
                                            Some(name) => format!("M{idx}: {name}"),
                                            None => format!("M{idx}"),
                                        }
                                    })
                                    .collect::<Vec<_>>();
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(language, "macro_editor.picker_item"),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.select_macro_slot",
                                    )),
                                    control_width,
                                    |ui| {
                                        let dropdown_id =
                                            ui.make_persistent_id(("macro_settings_slot", grid_id));
                                        let (_, picked) =
                                            crate::ui_style::modern_dropdown_select_sized(
                                                ui,
                                                dropdown_id,
                                                &labels,
                                                n,
                                                control_width,
                                                control_height,
                                                control_font_size,
                                            );
                                        picked_slot = picked.map(|idx| idx as u8);
                                    },
                                );
                            }
                            1 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(language, "macro_editor.macro_name"),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.local_name_for_this_macro",
                                    )),
                                    control_width,
                                    |ui| {
                                        let response = crate::ui_style::modern_text_field_sized(
                                            ui,
                                            ui.make_persistent_id(("macro_name", grid_id, n)),
                                            &mut edited_name,
                                            control_width,
                                            control_height,
                                            crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.macro_name",
                                            ),
                                            MACRO_NAME_CHAR_LIMIT,
                                            egui::Align::Center,
                                        );
                                        if response.changed() {
                                            limit_chars(&mut edited_name, MACRO_NAME_CHAR_LIMIT);
                                        }
                                    },
                                );
                            }
                            2 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.macro_description",
                                    ),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.optional_description_for_this_macro",
                                    )),
                                    control_width,
                                    |ui| {
                                        let response = crate::ui_style::modern_text_field_sized(
                                            ui,
                                            ui.make_persistent_id((
                                                "macro_description",
                                                grid_id,
                                                n,
                                            )),
                                            &mut edited_description,
                                            control_width,
                                            control_height,
                                            crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.macro_description",
                                            ),
                                            MACRO_DESCRIPTION_CHAR_LIMIT,
                                            egui::Align::Min,
                                        );
                                        if response.changed() {
                                            limit_chars(
                                                &mut edited_description,
                                                MACRO_DESCRIPTION_CHAR_LIMIT,
                                            );
                                        }
                                    },
                                );
                            }
                            action_row => {
                                let action_idx = match action_rows.get(action_row - 3) {
                                    Some(&MacroActionRow::Action(idx)) => idx,
                                    Some(&MacroActionRow::UntypableText(idx)) => {
                                        let warning = match edited_actions.get(idx) {
                                            Some(MacroAction::Text(text)) => {
                                                macro_text_untypable_warning(language, text)
                                            }
                                            _ => None,
                                        };
                                        crate::ui_style::settings_list_row(
                                            ui,
                                            row_content_width,
                                            row_height,
                                            "",
                                            false,
                                            action_control_width,
                                            |ui| {
                                                if let Some(warning) = warning {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(warning)
                                                                .size(metrics.value(11.0))
                                                                .color(Color32::from_rgb(
                                                                    220, 92, 76,
                                                                )),
                                                        )
                                                        .wrap(),
                                                    );
                                                }
                                            },
                                        );
                                        continue;
                                    }
                                    None => edited_actions.len(),
                                };
                                if action_idx == edited_actions.len() {
                                    let mut added_action = None;
                                    crate::ui_style::settings_list_row_with_tooltip(
                                        ui,
                                        row_content_width,
                                        row_height,
                                        crate::i18n::tr_catalog(language, "macro_editor.actions"),
                                        true,
                                        (!list.suppress_tooltips).then_some(
                                            crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.add_actions_below",
                                            ),
                                        ),
                                        action_control_width,
                                        |ui| {
                                            ui.spacing_mut().item_spacing.x = metrics.value(4.0);
                                            let add_width =
                                                (action_control_width - metrics.value(16.0)) / 5.0;
                                            let add_size = egui::vec2(add_width, control_height);
                                            for (label_key, tooltip_key, action) in [
                                                (
                                                    "macro_editor.plus_text",
                                                    "macro_editor.type_characters",
                                                    MacroAction::Text(String::new()),
                                                ),
                                                (
                                                    "macro_editor.plus_tap",
                                                    "macro_editor.press_and_release_a_key",
                                                    MacroAction::Tap(0x04),
                                                ),
                                                (
                                                    "macro_editor.plus_down",
                                                    "macro_editor.hold_a_key",
                                                    MacroAction::Down(0x04),
                                                ),
                                                (
                                                    "macro_editor.plus_up",
                                                    "macro_editor.release_a_key",
                                                    MacroAction::Up(0x04),
                                                ),
                                                (
                                                    "macro_editor.plus_delay",
                                                    "macro_editor.pause_in_milliseconds",
                                                    MacroAction::Delay(100),
                                                ),
                                            ] {
                                                if crate::ui_style::modern_button_with_font(
                                                    ui,
                                                    crate::i18n::tr_catalog(language, label_key),
                                                    add_size,
                                                    metrics.value(10.5),
                                                    true,
                                                )
                                                .on_hover_text(crate::i18n::tr_catalog(
                                                    language,
                                                    tooltip_key,
                                                ))
                                                .clicked()
                                                {
                                                    added_action = Some(action);
                                                }
                                            }
                                        },
                                    );
                                    if let Some(action) = added_action {
                                        let opens_key_picker = matches!(
                                            action,
                                            MacroAction::Tap(_)
                                                | MacroAction::Down(_)
                                                | MacroAction::Up(_)
                                        );
                                        edited_actions.push(action);
                                        if opens_key_picker {
                                            self.macro_key_pick =
                                                Some((n, edited_actions.len() - 1));
                                        }
                                    }
                                    continue;
                                }

                                let action_count = edited_actions.len();
                                let action = &mut edited_actions[action_idx];
                                let (label, tooltip) = match action {
                                    MacroAction::Text(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.text"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.types_text_characters_one_by_one",
                                        ),
                                    ),
                                    MacroAction::Tap(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.tap"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.press_and_release_a_key",
                                        ),
                                    ),
                                    MacroAction::Down(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.down"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.press_a_key_hold_until_up",
                                        ),
                                    ),
                                    MacroAction::Up(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.up"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.release_a_previously_pressed_key",
                                        ),
                                    ),
                                    MacroAction::Delay(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.delay"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.wait_before_next_action",
                                        ),
                                    ),
                                    MacroAction::Raw(_) => {
                                        ("Raw", "Raw macro bytes preserved for compatibility")
                                    }
                                };

                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    label,
                                    true,
                                    (!list.suppress_tooltips).then_some(tooltip),
                                    action_control_width,
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = metrics.value(4.0);
                                        let small_size = metrics.size(26.0, control_height / scale);
                                        let up_response = crate::ui_style::modern_button_with_font(
                                            ui,
                                            "↑",
                                            small_size,
                                            control_font_size,
                                            action_idx > 0,
                                        )
                                        .on_hover_text(crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.move_up",
                                        ));
                                        if up_response.clicked() && action_idx > 0 {
                                            move_up = Some(action_idx);
                                        }
                                        let down_response =
                                            crate::ui_style::modern_button_with_font(
                                                ui,
                                                "↓",
                                                small_size,
                                                control_font_size,
                                                action_idx + 1 < action_count,
                                            )
                                            .on_hover_text(crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.move_down",
                                            ));
                                        if down_response.clicked() && action_idx + 1 < action_count
                                        {
                                            move_down = Some(action_idx);
                                        }

                                        let editor_width = metrics.value(200.0);
                                        match action {
                                            MacroAction::Text(text) => {
                                                let text_id = ui.make_persistent_id((
                                                    "macro_text_action",
                                                    grid_id,
                                                    n,
                                                    action_idx,
                                                ));
                                                let response =
                                                    crate::ui_style::modern_text_field_sized(
                                                        ui,
                                                        text_id,
                                                        text,
                                                        editor_width,
                                                        control_height,
                                                        crate::i18n::tr_catalog(
                                                            language,
                                                            "macro_editor.type_text_here",
                                                        ),
                                                        // The keyboard's shared macro memory
                                                        // is the only limit; see the memory line.
                                                        usize::MAX,
                                                        egui::Align::Min,
                                                    )
                                                    .on_hover_text(crate::i18n::tr_catalog(
                                                        language,
                                                        "macro_editor.characters_to_type_when_this_macro_runs",
                                                    ));
                                                // Only an edit rewrites a step, so
                                                // text read from the keyboard stays
                                                // as read until the user touches it.
                                                if response.changed() {
                                                    if let Some((normalized, replaced)) =
                                                        normalize_macro_text_typography(text)
                                                    {
                                                        move_text_cursor_past_typography(
                                                            ui.ctx(),
                                                            text_id,
                                                            text,
                                                        );
                                                        *text = normalized;
                                                        ascii_replaced += replaced;
                                                    }
                                                    // The warning row and the notice
                                                    // follow the edit on the next frame.
                                                    ui.ctx().request_repaint();
                                                }
                                            }
                                            MacroAction::Tap(keycode)
                                            | MacroAction::Down(keycode)
                                            | MacroAction::Up(keycode) => {
                                                let button_label =
                                                    keycode_label_with_names_and_layout(
                                                        *keycode,
                                                        &custom_pairs,
                                                        &self.layer_names,
                                                        self.key_legend_layout,
                                                    );
                                                let response =
                                                    crate::ui_style::modern_button_with_font(
                                                        ui,
                                                        &button_label,
                                                        egui::vec2(editor_width, control_height),
                                                        control_font_size,
                                                        true,
                                                    )
                                                    .on_hover_text(tooltip);
                                                if response.clicked() {
                                                    self.macro_key_pick = Some((n, action_idx));
                                                }
                                            }
                                            MacroAction::Delay(ms) => {
                                                let mut value = ms.to_string();
                                                if crate::ui_style::modern_text_field_sized(
                                                    ui,
                                                    ui.make_persistent_id((
                                                        "macro_delay",
                                                        grid_id,
                                                        n,
                                                        action_idx,
                                                    )),
                                                    &mut value,
                                                    metrics.value(82.0),
                                                    control_height,
                                                    "",
                                                    5,
                                                    egui::Align::Center,
                                                )
                                                .on_hover_text(crate::i18n::tr_catalog(
                                                    language,
                                                    "macro_editor.delay_is_in_milliseconds",
                                                ))
                                                .changed()
                                                {
                                                    if let Ok(parsed) = value.parse::<u16>() {
                                                        *ms = parsed;
                                                    }
                                                }
                                            }
                                            MacroAction::Raw(bytes) => {
                                                ui.add_sized(
                                                    egui::vec2(
                                                        editor_width,
                                                        control_height,
                                                    ),
                                                    egui::Label::new(
                                                        RichText::new(format_raw_macro_bytes(bytes))
                                                            .size(control_font_size)
                                                            .color(crate::ui_style::muted_text(
                                                                ui.visuals().dark_mode,
                                                            )),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(
                                                    "Entropy cannot edit these macro bytes yet, but will save them unchanged",
                                                );
                                            }
                                        }

                                        let remove_response =
                                            crate::ui_style::modern_button_with_font(
                                                ui,
                                                "×",
                                                small_size,
                                                control_font_size,
                                                true,
                                            )
                                            .on_hover_text(crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.remove_this_action",
                                            ));
                                        if remove_response.clicked() {
                                            remove_idx = Some(action_idx);
                                        }
                                    },
                                );
                            }
                        }
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

                let action_size = crate::ui_style::modal_action_button_size() * scale;
                let action_gap = metrics.value(8.0);
                let actions_rect = crate::app::fixed_settings_action_bar_rect(
                    list.viewport,
                    metrics,
                    action_size,
                    2,
                    action_gap,
                );
                crate::ui_style::allocate_ui_at_rect(ui, actions_rect, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = action_gap;
                        let can_clear = !edited_actions.is_empty()
                            || !edited_name.trim().is_empty()
                            || !edited_description.trim().is_empty();
                        let clear_response = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(language, "alt_repeat_editor.clear"),
                            action_size,
                            control_font_size,
                            can_clear,
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            language,
                            "key_picker_text.remove_all_actions_from_this_macro",
                        ));
                        if clear_response.clicked() && can_clear {
                            edited_actions.clear();
                            edited_name.clear();
                            edited_description.clear();
                        }

                        let undo_position = self
                            .macro_undo_stack
                            .iter()
                            .rposition(|(macro_idx, _)| *macro_idx == n);
                        let undo_response = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(language, "alt_repeat_editor.undo"),
                            action_size,
                            control_font_size,
                            undo_position.is_some(),
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            language,
                            "key_picker_text.undo_last_change",
                        ));
                        if undo_response.clicked() {
                            if let Some(position) = undo_position {
                                let (_, previous) = self.macro_undo_stack.remove(position);
                                edited_actions = previous;
                                undo_applied = true;
                            }
                        }
                    });
                });
            },
        );

        if let Some(idx) = remove_idx {
            if idx < edited_actions.len() {
                edited_actions.remove(idx);
                if self
                    .macro_key_pick
                    .is_some_and(|(macro_idx, action_idx)| macro_idx == n && action_idx >= idx)
                {
                    self.macro_key_pick = None;
                }
            }
        }
        if let Some(idx) = move_up {
            if idx > 0 && idx < edited_actions.len() {
                edited_actions.swap(idx, idx - 1);
            }
        }
        if let Some(idx) = move_down {
            if idx + 1 < edited_actions.len() {
                edited_actions.swap(idx, idx + 1);
            }
        }

        if ascii_replaced > 0 {
            let previous = self.macro_text_ascii_replaced.map_or(0, |(_, count)| count);
            self.macro_text_ascii_replaced = Some((n, previous + ascii_replaced));
        }
        if edited_actions != original_actions {
            if !undo_applied {
                self.macro_undo_stack.push((n, original_actions));
                if self.macro_undo_stack.len() > 64 {
                    self.macro_undo_stack.remove(0);
                }
            }
            self.macro_actions[n] = edited_actions;
            self.encode_macro(n);
            self.mark_macros_dirty();
        }
        if edited_name != original_name || edited_description != original_description {
            self.macro_names[n] = edited_name;
            self.macro_descriptions[n] = edited_description;
            self.macro_metadata_dirty = true;
        }
        if let Some(slot) = picked_slot {
            selected_macro = slot;
            self.macro_inline_selected = Some(slot);
        }

        selected_macro
    }

    fn macro_has_content(&self, n: usize) -> bool {
        self.macro_actions
            .get(n)
            .map(|a| !a.is_empty())
            .unwrap_or(false)
            || self
                .macro_texts
                .get(n)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
    }

    fn ensure_macro_meta_len(&mut self, n: usize) {
        while self.macro_texts.len() <= n {
            self.macro_texts.push(Vec::new());
        }
        while self.macro_names.len() <= n {
            self.macro_names.push(String::new());
        }
        while self.macro_descriptions.len() <= n {
            self.macro_descriptions.push(String::new());
        }
        while self.macro_actions.len() <= n {
            self.macro_actions.push(vec![]);
        }
    }

    pub(super) fn macro_display_name(&self, n: usize) -> String {
        match self.macro_names.get(n) {
            Some(name) if !name.trim().is_empty() => name.clone(),
            _ => format!("M{}", n),
        }
    }

    pub(super) fn macro_description(&self, n: usize) -> Option<String> {
        self.macro_descriptions
            .get(n)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub(super) fn encode_macro(&mut self, n: usize) -> bool {
        while self.macro_texts.len() <= n {
            self.macro_texts.push(Vec::new());
        }
        while self.macro_actions.len() <= n {
            self.macro_actions.push(vec![]);
        }
        let encoded = encode_macro_actions(&self.macro_actions[n]);
        let changed = self.macro_texts[n] != encoded;
        if changed {
            self.macro_texts[n] = encoded;
        }
        changed
    }

    pub(super) fn show_vial_macros(&mut self, ui: &mut egui::Ui) {
        if self.macro_count == 0 {
            self.macro_inline_selected = None;
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.no_macro_slots_available_on_this_keyboard",
                ))
                .size(16.0)
                .color(Color32::from_gray(140)),
            );
            return;
        }

        let Some(selected) = self.macro_inline_selected else {
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.picker_intro",
                ))
                .size(11.0)
                .color(Color32::from_gray(150)),
            );
            ui.add_space(4.0);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.picker_item"),
                Self::picker_key_size(ui.ctx()),
                true,
                false,
            )
            .clicked()
            {
                self.macro_inline_selected = Some(0);
            }
            return;
        };

        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.choose_macro",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        if let Some(picked) = self.show_macro_slot_grid(ui, selected, "macro_grid_picker") {
            self.assign_macro_slot(picked);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macro_picker_frame(
        picker: &mut KeycodePicker,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_100.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |_ui| {
            picker.show(
                ctx,
                DeferredPickerDataState::Ready,
                DeferredPickerDataState::Ready,
            );
        })
    }

    fn macro_picker_text_position(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
        fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|shape| {
            find(&shape.shape, label).filter(|pos| shape.clip_rect.contains(*pos))
        })
    }

    fn macro_picker_click(picker: &mut KeycodePicker, ctx: &egui::Context, pos: egui::Pos2) {
        macro_picker_click_button(picker, ctx, pos, egui::PointerButton::Primary);
    }

    fn macro_picker_click_button(
        picker: &mut KeycodePicker,
        ctx: &egui::Context,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) {
        for pressed in [true, false] {
            let _ = macro_picker_frame(
                picker,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn macro_modifier_picker_mouse_selection_encodes_hyper_f16_for_each_action_kind() {
        for (action, expected, opcode) in [
            (MacroAction::Tap(0x04), MacroAction::Tap(0x0F6B), 5),
            (MacroAction::Down(0x04), MacroAction::Down(0x0F6B), 6),
            (MacroAction::Up(0x04), MacroAction::Up(0x0F6B), 7),
        ] {
            let ctx = egui::Context::default();
            let mut picker = KeycodePicker {
                language: crate::i18n::Language::English,
                popup_view_mode: PickerViewMode::Layout,
                macro_key_pick: Some((0, 0)),
                macro_actions: vec![vec![action]],
                ..Default::default()
            };
            let _ = macro_picker_frame(&mut picker, &ctx, vec![]);
            let output = macro_picker_frame(&mut picker, &ctx, vec![]);
            assert!(macro_picker_text_position(&output, "Hyper").is_none());
            let pos = macro_picker_text_position(&output, "List").expect("List view tab");
            macro_picker_click(&mut picker, &ctx, pos);
            assert_eq!(picker.popup_view_mode, PickerViewMode::List);
            let _ = macro_picker_frame(&mut picker, &ctx, vec![]);
            let output = macro_picker_frame(&mut picker, &ctx, vec![]);
            let pos = macro_picker_text_position(&output, "Hyper").expect("Hyper choice");
            macro_picker_click(&mut picker, &ctx, pos);
            assert_eq!(picker.macro_mod_key_pick, Some(0x0F00));
            assert!(!picker.macros_dirty);

            let _ = macro_picker_frame(&mut picker, &ctx, vec![]);
            let mut output = macro_picker_frame(&mut picker, &ctx, vec![]);
            let mut f16 = macro_picker_text_position(&output, "F16");
            for _ in 0..12 {
                if f16.is_some() {
                    break;
                }
                output = macro_picker_frame(
                    &mut picker,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(550.0, 550.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -180.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                );
                f16 = macro_picker_text_position(&output, "F16");
            }
            macro_picker_click(&mut picker, &ctx, f16.expect("visible F16 choice"));
            assert_eq!(picker.macro_actions[0], [expected.clone()]);
            assert_eq!(picker.macro_texts[0], [1, opcode, 0x6B, 0x0F]);
            assert_eq!(decode_macro_actions(&picker.macro_texts[0]), [expected]);
            assert!(picker.macros_dirty);
            assert!(picker.result.is_none());
            assert!(picker.macro_key_pick.is_none());
            assert!(picker.macro_mod_key_pick.is_none());
        }
    }

    #[test]
    fn macro_modifier_picker_escape_cancels_modifier_then_picker_without_changes() {
        let ctx = egui::Context::default();
        let mut picker = KeycodePicker {
            macro_key_pick: Some((0, 0)),
            macro_mod_key_pick: Some(0x0F00),
            macro_actions: vec![vec![MacroAction::Tap(0x04)]],
            ..Default::default()
        };
        let escape = |pressed| egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = macro_picker_frame(&mut picker, &ctx, vec![escape(true)]);
        assert!(picker.macro_mod_key_pick.is_none());
        assert_eq!(picker.macro_key_pick, Some((0, 0)));
        let _ = macro_picker_frame(&mut picker, &ctx, vec![escape(false)]);
        let _ = macro_picker_frame(&mut picker, &ctx, vec![escape(true)]);
        assert!(picker.macro_key_pick.is_none());
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x04)]);
        assert!(!picker.macros_dirty);
        assert!(picker.result.is_none());
    }

    #[test]
    fn macro_modifier_picker_physical_key_rejects_nested_modifiers_and_accepts_first_base() {
        let ctx = egui::Context::default();
        let mut picker = KeycodePicker {
            macro_key_pick: Some((0, 0)),
            macro_mod_key_pick: Some(0x0F00),
            macro_actions: vec![vec![MacroAction::Tap(0x04)]],
            macro_texts: vec![vec![1, 1, 0x04]],
            ..Default::default()
        };
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        let _ = macro_picker_frame(&mut picker, &ctx, vec![key(Key::A, egui::Modifiers::CTRL)]);
        assert_eq!(picker.macro_mod_key_pick, Some(0x0F00));
        assert_eq!(picker.macro_key_pick, Some((0, 0)));
        assert_eq!(picker.macro_texts[0], [1, 1, 0x04]);
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x04)]);
        assert!(!picker.macros_dirty);

        let _ = macro_picker_frame(
            &mut picker,
            &ctx,
            vec![
                key(Key::F16, egui::Modifiers::NONE),
                key(Key::B, egui::Modifiers::NONE),
            ],
        );
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x0F6B)]);
        assert_eq!(picker.macro_texts[0], [1, 5, 0x6B, 0x0F]);
        assert!(picker.macro_key_pick.is_none());
        assert!(picker.macro_mod_key_pick.is_none());
        assert!(picker.macros_dirty);
        assert!(picker.result.is_none());
    }

    #[test]
    fn macro_modifier_picker_secondary_click_selects_right_modifier() {
        let ctx = egui::Context::default();
        let mut picker = KeycodePicker {
            language: crate::i18n::Language::English,
            macro_key_pick: Some((0, 0)),
            macro_actions: vec![vec![MacroAction::Tap(0x04)]],
            ..Default::default()
        };
        let _ = macro_picker_frame(&mut picker, &ctx, vec![]);
        let output = macro_picker_frame(&mut picker, &ctx, vec![]);
        let pos = macro_picker_text_position(&output, "Ctrl").expect("Ctrl modifier choice");
        macro_picker_click_button(&mut picker, &ctx, pos, egui::PointerButton::Secondary);
        assert_eq!(picker.macro_mod_key_pick, Some(0x1100));
        let _ = macro_picker_frame(
            &mut picker,
            &ctx,
            vec![egui::Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x1104)]);
        assert_eq!(picker.macro_texts[0], [1, 5, 0x04, 0x11]);
        assert!(picker.macros_dirty);
        assert!(picker.macro_key_pick.is_none());
        assert!(picker.macro_mod_key_pick.is_none());
        assert!(picker.result.is_none());
    }

    #[test]
    fn macro_modifier_picker_cancel_button_preserves_action_and_keeps_picker_open() {
        let ctx = egui::Context::default();
        let mut picker = KeycodePicker {
            language: crate::i18n::Language::English,
            macro_key_pick: Some((0, 0)),
            macro_mod_key_pick: Some(0x0F00),
            macro_actions: vec![vec![MacroAction::Tap(0x04)]],
            macro_texts: vec![vec![1, 1, 0x04]],
            ..Default::default()
        };
        let _ = macro_picker_frame(&mut picker, &ctx, vec![]);
        let output = macro_picker_frame(&mut picker, &ctx, vec![]);
        let pos = macro_picker_text_position(&output, "✕ Cancel").expect("Cancel button");
        macro_picker_click(&mut picker, &ctx, pos);
        assert!(picker.macro_mod_key_pick.is_none());
        assert_eq!(picker.macro_key_pick, Some((0, 0)));
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x04)]);
        assert_eq!(picker.macro_texts[0], [1, 1, 0x04]);
        assert!(!picker.macros_dirty);
        assert!(picker.result.is_none());
    }

    #[test]
    fn macro_modifier_picker_rejects_extended_values_without_firmware_support() {
        let mut picker = KeycodePicker {
            supports_macro_ext_keycodes: false,
            macro_key_pick: Some((0, 0)),
            macro_actions: vec![vec![MacroAction::Tap(0x04)]],
            ..Default::default()
        };
        picker.set_macro_key_pick_value(0, 0, 0x0F6B);
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x04)]);
        assert_eq!(picker.macro_key_pick, Some((0, 0)));
        assert!(!picker.macros_dirty);
        picker.set_macro_key_pick_value(0, 0, 0x006B);
        assert_eq!(picker.macro_actions[0], [MacroAction::Tap(0x006B)]);
        assert_eq!(picker.macro_texts[0], [1, 1, 0x6B]);
    }

    #[test]
    fn macro_modifier_picker_offers_combos_in_supported_list_view() {
        for view in PickerViewMode::ALL {
            for supported in [true, false] {
                let ctx = egui::Context::default();
                let mut picker = KeycodePicker {
                    language: crate::i18n::Language::English,
                    popup_view_mode: view,
                    supports_macro_ext_keycodes: supported,
                    macro_key_pick: Some((0, 0)),
                    macro_actions: vec![vec![MacroAction::Tap(0x04)]],
                    ..Default::default()
                };
                let _ = macro_picker_frame(&mut picker, &ctx, vec![]);
                let output = macro_picker_frame(&mut picker, &ctx, vec![]);
                assert_eq!(
                    macro_picker_text_position(&output, "Hyper").is_some(),
                    supported && view == PickerViewMode::List,
                    "{view:?}"
                );
                assert!(!picker.macros_dirty);
                assert!(picker.result.is_none());
            }
        }
    }

    #[test]
    fn macro_editor_keeps_one_trailing_action_selector_row() {
        assert_eq!(macro_settings_total_rows(0), 4);
        assert_eq!(macro_settings_total_rows(3), 7);
    }

    fn macro_settings_frame(
        picker: &mut KeycodePicker,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_100.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            picker.show_macro_settings_editor(ui, 0, "macro_settings_test", None);
        })
    }

    #[test]
    fn macro_text_typable_matches_firmware_ascii_table() {
        for c in [' ', 'a', '~', '\t', '\n', '\u{08}', '\u{1B}', '\u{7F}'] {
            assert!(macro_text_char_is_typable(c), "{c:?}");
        }
        for c in [
            '\u{00}', '\u{1F}', '\r', '\u{80}', '\u{A0}', 'ж', 'Ё', '—', '😀',
        ] {
            assert!(!macro_text_char_is_typable(c), "{c:?}");
        }
    }

    #[test]
    fn macro_text_untypable_summary_lists_distinct_characters_then_count() {
        assert_eq!(macro_text_untypable_summary("plain ASCII ~\t\n"), None);
        assert_eq!(
            macro_text_untypable_summary("ж-ж ё № ж"),
            Some(("ж, ё, №".to_owned(), 0))
        );
        assert_eq!(
            macro_text_untypable_summary("абвгдеёаб"),
            Some(("а, б, в, г, д".to_owned(), 2))
        );
        assert_eq!(
            macro_text_untypable_summary("a\u{00A0}b\u{200B}c\r"),
            Some(("U+00A0, U+200B, U+000D".to_owned(), 0))
        );
    }

    #[test]
    fn macro_text_untypable_warning_names_characters_in_both_languages() {
        use crate::i18n::Language;

        assert_eq!(macro_text_untypable_warning(Language::English, "ok"), None);
        assert_eq!(
            macro_text_untypable_warning(Language::English, "№ 5 😀").as_deref(),
            Some("These characters will not be typed: №, 😀")
        );
        assert_eq!(
            macro_text_untypable_warning(Language::English, "привет").as_deref(),
            Some("These characters will not be typed: п, р, и, в, е and 1 more")
        );
        assert_eq!(
            macro_text_untypable_warning(Language::Russian, "привет").as_deref(),
            Some("Эти символы макрос не введёт: п, р, и, в, е и ещё 1")
        );
    }

    #[test]
    fn macro_text_typography_has_an_ascii_form_for_each_listed_character() {
        for (from, to) in [
            ('‘', "'"),
            ('’', "'"),
            ('‚', "'"),
            ('‛', "'"),
            ('′', "'"),
            ('‹', "'"),
            ('›', "'"),
            ('“', "\""),
            ('”', "\""),
            ('„', "\""),
            ('‟', "\""),
            ('«', "\""),
            ('»', "\""),
            ('″', "\""),
            ('‐', "-"),
            ('‑', "-"),
            ('‒', "-"),
            ('–', "-"),
            ('—', "-"),
            ('―', "-"),
            ('−', "-"),
            ('…', "..."),
            ('\u{00A0}', " "),
            ('\u{2007}', " "),
            ('\u{2009}', " "),
            ('\u{202F}', " "),
            ('\u{3000}', " "),
        ] {
            assert_eq!(ascii_typography_replacement(from), Some(to), "{from:?}");
            assert!(to.chars().all(macro_text_char_is_typable), "{from:?}");
        }
        for c in ['a', '"', 'ж', '№', '\u{200B}', '\u{00AD}', '😀'] {
            assert_eq!(ascii_typography_replacement(c), None, "{c:?}");
        }
    }

    #[test]
    fn macro_text_typography_normalization_counts_replacements_and_skips_the_rest() {
        assert_eq!(
            normalize_macro_text_typography("«Hi» — it’s\u{00A0}done…"),
            Some(("\"Hi\" - it's done...".to_owned(), 6))
        );
        assert_eq!(
            normalize_macro_text_typography("привет — мир"),
            Some(("привет - мир".to_owned(), 1))
        );
        assert_eq!(
            normalize_macro_text_typography("plain \"ASCII\" - it's ..."),
            None
        );
        assert_eq!(normalize_macro_text_typography("привет, мир"), None);
        assert_eq!(normalize_macro_text_typography(""), None);
    }

    #[test]
    fn macro_text_typography_moves_char_index_past_an_expanded_ellipsis() {
        let original = "a…b—c";
        let normalized = normalize_macro_text_typography(original).unwrap().0;
        assert_eq!(normalized, "a...b-c");
        for (index, expected) in [(0, 0), (1, 1), (2, 4), (3, 5), (5, 7)] {
            assert_eq!(typography_normalized_char_index(original, index), expected);
        }
    }

    #[test]
    fn macro_editor_puts_a_warning_row_under_untypable_text_steps() {
        let actions = [
            MacroAction::Text("ok".into()),
            MacroAction::Text("ок".into()),
            MacroAction::Tap(0x04),
            MacroAction::Text("tab\there".into()),
        ];

        assert_eq!(
            macro_action_rows(&actions),
            [
                MacroActionRow::Action(0),
                MacroActionRow::Action(1),
                MacroActionRow::UntypableText(1),
                MacroActionRow::Action(2),
                MacroActionRow::Action(3),
            ]
        );
    }

    #[test]
    fn macro_editor_warns_about_text_read_from_keyboard_without_rewriting_it() {
        let ctx = egui::Context::default();
        let text = "Привет — мир".to_owned();
        let mut picker = KeycodePicker {
            language: crate::i18n::Language::English,
            macro_actions: vec![vec![MacroAction::Text(text.clone())]],
            macro_texts: vec![text.as_bytes().to_vec()],
            ..Default::default()
        };

        let _ = macro_settings_frame(&mut picker, &ctx, vec![]);
        let output = macro_settings_frame(&mut picker, &ctx, vec![]);

        assert!(macro_picker_text_position(
            &output,
            "These characters will not be typed: П, р, и, в, е and 3 more"
        )
        .is_some());
        assert_eq!(picker.macro_actions[0], [MacroAction::Text(text.clone())]);
        assert_eq!(picker.macro_texts[0], text.as_bytes());
        assert!(picker.macro_text_ascii_replaced.is_none());
        assert!(!picker.macros_dirty);
    }

    #[test]
    fn macro_text_paste_replaces_typography_and_says_how_many_characters() {
        let ctx = egui::Context::default();
        let mut picker = KeycodePicker {
            language: crate::i18n::Language::English,
            macro_actions: vec![vec![MacroAction::Text(String::new())]],
            ..Default::default()
        };
        let _ = macro_settings_frame(&mut picker, &ctx, vec![]);
        let output = macro_settings_frame(&mut picker, &ctx, vec![]);
        // The Text field ends right before the step's remove button.
        let remove = macro_picker_text_position(&output, "×").expect("remove button");
        let field = egui::pos2(remove.x - 100.0, remove.y);
        for pressed in [true, false] {
            let _ = macro_settings_frame(
                &mut picker,
                &ctx,
                vec![
                    egui::Event::PointerMoved(field),
                    egui::Event::PointerButton {
                        pos: field,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }

        let _ = macro_settings_frame(&mut picker, &ctx, vec![egui::Event::Paste("«Go»…".into())]);
        assert_eq!(
            picker.macro_actions[0],
            [MacroAction::Text("\"Go\"...".into())]
        );
        assert_eq!(picker.macro_texts[0], b"\"Go\"...");
        assert_eq!(picker.macro_text_ascii_replaced, Some((0, 3)));
        assert!(picker.macros_dirty);

        // The cursor follows the expanded ellipsis instead of landing in it.
        let _ = macro_settings_frame(&mut picker, &ctx, vec![egui::Event::Text("!".into())]);
        assert_eq!(
            picker.macro_actions[0],
            [MacroAction::Text("\"Go\"...!".into())]
        );
        assert_eq!(picker.macro_text_ascii_replaced, Some((0, 3)));
        let output = macro_settings_frame(&mut picker, &ctx, vec![]);
        assert!(macro_picker_text_position(
            &output,
            "Typographic characters replaced with ASCII: 3"
        )
        .is_some());
    }

    #[test]
    fn explicit_slot_pick_assigns_macro_without_marking_contents_dirty() {
        let mut picker = KeycodePicker {
            open: true,
            macro_count: 8,
            ..Default::default()
        };

        picker.assign_macro_slot(4);

        assert_eq!(
            picker.result.map(|binding| binding.vial_keycode()),
            Some(0x7704)
        );
        assert_eq!(picker.macro_inline_selected, Some(4));
        assert!(!picker.open);
        assert!(!picker.macros_dirty);
    }

    #[test]
    fn encodes_basic_macro_tap_as_vial_short_action() {
        let mut picker = KeycodePicker {
            macro_actions: vec![vec![MacroAction::Tap(0x0006)]],
            ..Default::default()
        };

        picker.encode_macro(0);

        assert_eq!(picker.macro_texts[0].as_slice(), &[0x01, 0x01, 0x06]);
    }

    #[test]
    fn encodes_modified_macro_tap_as_vial_extended_action() {
        let mut picker = KeycodePicker {
            macro_actions: vec![vec![MacroAction::Tap(0x0106)]],
            ..Default::default()
        };

        picker.encode_macro(0);

        assert_eq!(picker.macro_texts[0].as_slice(), &[0x01, 0x05, 0x06, 0x01]);
    }

    #[test]
    fn encodes_zero_low_byte_extended_keycode_with_vial_escape() {
        let mut encoded = Vec::new();

        append_macro_key_action(&mut encoded, SS_TAP_CODE, VIAL_MACRO_EXT_TAP, 0xA000);

        assert_eq!(encoded, [0x01, 0x05, 0xA0, 0xFF]);
    }

    #[test]
    fn preserves_raw_macro_bytes_that_cannot_be_decoded() {
        let raw_macro = [
            0xEF,
            SS_QMK_PREFIX,
            0x99,
            0xAA,
            SS_QMK_PREFIX,
            SS_DELAY_CODE,
            0x10,
        ];

        let actions = decode_macro_actions(&raw_macro);

        assert!(matches!(actions[0], MacroAction::Raw(_)));
        assert_eq!(encode_macro_actions(&actions), raw_macro);
    }

    #[test]
    fn preserves_unsupported_send_string_keycodes_as_raw() {
        let raw_macro = [
            SS_QMK_PREFIX,
            SS_DOWN_CODE,
            0xEF,
            SS_QMK_PREFIX,
            SS_UP_CODE,
            0xEF,
        ];

        let actions = decode_macro_actions(&raw_macro);

        assert_eq!(
            actions,
            vec![
                MacroAction::Raw(vec![SS_QMK_PREFIX, SS_DOWN_CODE, 0xEF]),
                MacroAction::Raw(vec![SS_QMK_PREFIX, SS_UP_CODE, 0xEF]),
            ]
        );
        assert_eq!(encode_macro_actions(&actions), raw_macro);
    }

    #[test]
    fn decodes_supported_send_string_modifiers() {
        let raw_macro = [
            SS_QMK_PREFIX,
            SS_DOWN_CODE,
            0xE0,
            SS_QMK_PREFIX,
            SS_UP_CODE,
            0xE0,
        ];

        let actions = decode_macro_actions(&raw_macro);

        assert_eq!(
            actions,
            vec![MacroAction::Down(0xE0), MacroAction::Up(0xE0)]
        );
        assert_eq!(encode_macro_actions(&actions), raw_macro);
    }
}
