/// Keycode picker modal for Vial/QMK keycodes.
use crate::app::MacroExtKeycodesDisabledReason;
use crate::keycode::{
    gui_label, gui_mod_name, gui_sym, key_label_font_sizes, keycode_label_with_names_and_layout,
    keycode_tooltip, modifier_label_from_bits, KeyLegendLayout, KeycodeCategory, KEYCODES,
};
use crate::popup_state::{PopupKey, PopupState};
use egui::{Color32, Key, RichText, Vec2};

#[path = "keycode_picker_keyboard.rs"]
mod keycode_picker_keyboard;
pub use keycode_picker_keyboard::egui_key_to_qmk;
#[path = "keycode_picker_model.rs"]
mod keycode_picker_model;
pub use keycode_picker_model::{BasicPickerLayout, KeycodeTab, PickerViewMode};
#[path = "keycode_picker_ui.rs"]
mod keycode_picker_ui;
use keycode_picker_ui::*;
#[path = "keycode_picker_popups.rs"]
mod keycode_picker_popups;
use keycode_picker_popups::*;
#[path = "keycode_picker_basic.rs"]
mod keycode_picker_basic;
#[path = "keycode_picker_lighting_quantum.rs"]
mod keycode_picker_lighting_quantum;
#[path = "keycode_picker_macro.rs"]
mod keycode_picker_macro;
pub(crate) use keycode_picker_macro::decode_macro_actions;
#[path = "keycode_picker_special.rs"]
mod keycode_picker_special;
#[path = "keycode_picker_tabs.rs"]
mod keycode_picker_tabs;
#[path = "keycode_picker_tap_dance.rs"]
mod keycode_picker_tap_dance;
#[path = "keycode_picker_tap_dance_picker.rs"]
mod keycode_picker_tap_dance_picker;

fn plain_modifier_tooltip(mod_name: &str) -> String {
    format!(
        "Use {mod_name} by itself as a held modifier\nLeft click assigns Left {mod_name}\nRight click assigns Right {mod_name}"
    )
}

fn one_sided_modifier_tooltip(mod_name: &str, side: &str) -> String {
    format!("Use {side} {mod_name} by itself as a held modifier")
}

fn mod_combo_tooltip(mod_name: &str, has_right_side: bool) -> String {
    if has_right_side {
        format!(
            "Hold {mod_name} together with another key\nLeft click starts a Left {mod_name}+key binding\nRight click starts a Right {mod_name}+key binding\nThen choose the key part"
        )
    } else {
        format!("Hold {mod_name} together with another key\nClick to choose the key part")
    }
}

fn mod_tap_tooltip(mod_name: &str, has_right_side: bool) -> String {
    if has_right_side {
        format!(
            "Dual-role key: hold for {mod_name}, tap for another key\nLeft click uses Left {mod_name}\nRight click uses Right {mod_name}\nThen choose the tap key"
        )
    } else {
        format!(
            "Dual-role key: hold for {mod_name}, tap for another key\nClick to choose the tap key"
        )
    }
}

fn one_shot_modifier_tooltip(mod_name: &str, has_right_side: bool) -> String {
    if has_right_side {
        format!(
            "Applies {mod_name} to the next keypress only\nHold to use {mod_name} as a normal modifier\nLeft click assigns One-Shot Left {mod_name}\nRight click assigns One-Shot Right {mod_name}"
        )
    } else {
        format!(
            "Applies {mod_name} to the next keypress only\nHold to use {mod_name} as a normal modifier"
        )
    }
}

fn picker_ok_label(language: crate::i18n::Language) -> &'static str {
    match language {
        crate::i18n::Language::Russian => "Ок",
        crate::i18n::Language::English => "OK",
    }
}

#[derive(Clone, Debug, Default)]
pub struct TapDanceEntry {
    pub on_tap: u16,
    pub on_hold: u16,
    pub on_double_tap: u16,
    pub on_tap_hold: u16,
    pub tapping_term: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroAction {
    Text(String),
    Tap(u16),   // QMK/Vial keycode
    Down(u16),  // key press
    Up(u16),    // key release
    Delay(u16), // milliseconds
    Raw(Vec<u8>),
}

pub(crate) const MACRO_NAME_CHAR_LIMIT: usize = 7;
pub(crate) const MACRO_DESCRIPTION_CHAR_LIMIT: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MacroKeyPickKind {
    Tap,
    Down,
    Up,
}

pub struct KeycodePicker {
    pub open: bool,
    pub selected_tab: KeycodeTab,
    pub basic_layout: BasicPickerLayout,
    pub popup_view_mode: PickerViewMode,
    pub search_query: String,
    pub result: Option<u16>,
    pub custom_keycodes: Vec<(String, String, String, u16)>,
    pub supports_rgb: bool,
    pub supports_macro: bool,
    pub supports_tap_dance: bool,
    pub supports_mouse_keys: bool,
    pub supports_combo: bool,
    pub supports_auto_shift: bool,
    pub supports_caps_word: bool,
    pub supports_repeat_key: bool,
    pub supports_layer_lock: bool,
    pub supports_persistent_default_layer: bool,
    pub supports_macro_ext_keycodes: bool,
    pub macro_ext_keycodes_disabled_reason: Option<MacroExtKeycodesDisabledReason>,
    pub layer_names: Vec<String>,
    pub layer_count: usize,
    pub layer_has_content: Vec<bool>,
    pub listening: bool,
    // Vial Quantum tab pending state
    pub vial_quantum_pending_mod: Option<u16>,
    pub vial_quantum_pending_mt: Option<u16>,
    pub vial_layer_pending: Option<u16>,
    pub regular_key_pick: bool,
    pub regular_key_pick_allow_mod_key: bool,
    pub regular_mod_key_pick: Option<u16>,
    /// Open macro editor for this macro number (0..15), None = closed
    pub macro_count: usize,
    pub tap_dance_entries: Vec<TapDanceEntry>,
    pub tap_dance_names: Vec<String>,
    pub tap_dance_undo_stack: Vec<(usize, TapDanceEntry, String)>,
    pub tap_dance_editor_open: Option<u8>,
    pub tap_dance_dirty: bool,
    /// Which field is being edited: (td_idx, field: 0=tap,1=hold,2=dtap,3=taphold)
    pub td_key_pick: Option<(usize, u8)>,
    /// Pending tap dance Mod+Key selection: (td_idx, field, modifier base)
    pub td_mod_key_pick: Option<(usize, u8, u16)>,
    pub macro_inline_selected: Option<u8>,
    /// Macro editor bytecode buffers (one per macro)
    pub macro_texts: Vec<Vec<u8>>,
    /// User-visible names for macros (optional)
    pub macro_names: Vec<String>,
    /// User-visible descriptions for macros (optional, local metadata)
    pub macro_descriptions: Vec<String>,
    /// Macro actions for editor UI
    pub macro_actions: Vec<Vec<MacroAction>>,
    /// Flag: macro texts changed, need to write to device
    pub macros_dirty: bool,
    /// Undo stack for macro editor: (macro_idx, previous_actions)
    macro_undo_stack: Vec<(usize, Vec<MacroAction>)>,
    /// Macro key picker: (macro_idx, action_idx) being edited
    macro_key_pick: Option<(usize, usize)>,
    popup_state: PopupState,
    pub language: crate::i18n::Language,
    pub key_legend_layout: KeyLegendLayout,
    pub show_shifted_number_symbols: bool,
}

fn tr_picker(language: crate::i18n::Language, key: &'static str) -> &'static str {
    crate::i18n::tr_catalog(language, key)
}

const UNIVERSAL_MAIN_SYMBOL_ORDER: &[char] = &[
    '.', ',', ';', ':', '!', '?', '/', '`', '~', '\'', '"', '(', ')', '[', ']', '{', '}', '<', '>',
    '+', '*', '=', '#', '@', '$', '%', '^', '&', '|', '\\', '_',
];

const UNIVERSAL_EXTRA_SYMBOL_ORDER: &[char] = &[
    '₽', '€', '«', '»', '‘', '’', '„', '“', '”', '—', '–', '←', '↑', '→', '↓', '↔', '•', '×', '±',
    '≠', '≈', '✓', '§', '°', '‰', '′', '″', '™', '№',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_extra_symbols_include_common_arrows() {
        for symbol in ['←', '↑', '→', '↓', '↔'] {
            assert!(UNIVERSAL_EXTRA_SYMBOL_ORDER.contains(&symbol));
        }
    }

    #[test]
    fn rmk_macro_ext_guard_hides_layer_macro_choices_and_explains_why() {
        let mut picker = KeycodePicker::default();
        picker.supports_macro_ext_keycodes = false;
        picker.macro_ext_keycodes_disabled_reason =
            Some(MacroExtKeycodesDisabledReason::RmkVialMacroExtUnsupported);

        assert!(picker
            .macro_layer_key_choices(MacroKeyPickKind::Tap)
            .is_empty());
        assert!(picker
            .macro_ext_keycodes_notice(crate::i18n::Language::English)
            .expect("RMK notice should be present")
            .contains("RMK"));
    }
}

fn show_universal_symbol_section(
    ui: &mut egui::Ui,
    language: crate::i18n::Language,
    section_key: &'static str,
    symbols: &[char],
    show_setup_hint: bool,
) -> Option<u16> {
    let mut picked = None;

    ui.label(
        RichText::new(tr_picker(language, section_key))
            .size(11.0)
            .color(Color32::from_gray(150)),
    );
    if show_setup_hint {
        if let Some(hint) = crate::smart_input::universal_output_setup_hint() {
            ui.add_space(3.0);
            ui.label(
                RichText::new(crate::i18n::tr_text(language, hint))
                    .size(10.0)
                    .color(Color32::from_gray(120)),
            );
        }
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for wanted in symbols {
            let Some(smart) = crate::smart_input::SMART_SYMBOLS
                .iter()
                .copied()
                .find(|smart| smart.symbol == *wanted)
            else {
                continue;
            };
            let label = smart.symbol.to_string();
            let tip = format!(
                "Universal symbol: {} — types {} consistently regardless of the active keyboard language",
                smart.name, smart.symbol
            );
            let resp = ui
                .add_sized(KeycodePicker::picker_key_size(ui.ctx()), egui::Button::new(""))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            KeycodePicker::paint_compact_picker_label(ui, &resp, &label);
            if resp.clicked() {
                picked = Some(smart.trigger_keycode);
            }
            resp.on_hover_text(crate::i18n::tr_text(language, &tip));
        }
    });

    picked
}

fn picker_tab_label(language: crate::i18n::Language, tab: KeycodeTab) -> &'static str {
    tr_picker(language, tab.i18n_key())
}

fn picker_mod_key_label(base: u16) -> String {
    format!("{}/key", picker_modifier_label_from_bits(base >> 8))
}

fn picker_mod_tap_label(base: u16) -> String {
    format!(
        "Hold {}/key",
        picker_modifier_label_from_bits((base >> 8) & 0x1F)
    )
}

fn picker_modifier_label_from_bits(mods: u16) -> String {
    modifier_label_from_bits(mods)
        .replace("Ctl", "Ctrl")
        .replace("Sft", "Shift")
}

fn picker_action_label(label: &str) -> String {
    match label {
        "Brightness -" => "Bright\n-".to_string(),
        "Brightness +" => "Bright\n+".to_string(),
        "Saturation -" => "Sat\n-".to_string(),
        "Saturation +" => "Sat\n+".to_string(),
        "Hue -" => "Hue\n-".to_string(),
        "Hue +" => "Hue\n+".to_string(),
        "Speed -" => "Speed\n-".to_string(),
        "Speed +" => "Speed\n+".to_string(),
        "Effect -" => "Effect\n-".to_string(),
        "Effect +" => "Effect\n+".to_string(),
        "Prev Mode" => "Mode\nPrev".to_string(),
        "Next Mode" => "Mode\nNext".to_string(),
        other => other.to_string(),
    }
}

impl Default for KeycodePicker {
    fn default() -> Self {
        Self {
            open: false,
            selected_tab: KeycodeTab::Basic,
            basic_layout: BasicPickerLayout::Qwerty,
            popup_view_mode: PickerViewMode::default(),
            search_query: String::new(),
            result: None,
            custom_keycodes: vec![],
            supports_rgb: true,
            supports_macro: true,
            supports_tap_dance: true,
            supports_mouse_keys: true,
            supports_combo: true,
            supports_auto_shift: true,
            supports_caps_word: true,
            supports_repeat_key: true,
            supports_layer_lock: true,
            supports_persistent_default_layer: true,
            supports_macro_ext_keycodes: true,
            macro_ext_keycodes_disabled_reason: None,
            layer_names: (0..16).map(|i| i.to_string()).collect(),
            layer_count: 4,
            layer_has_content: vec![true; 16],
            listening: false,
            vial_quantum_pending_mod: None,
            vial_quantum_pending_mt: None,
            vial_layer_pending: None,
            regular_key_pick: false,
            regular_key_pick_allow_mod_key: false,
            regular_mod_key_pick: None,
            macro_inline_selected: None,
            macro_count: 16,
            tap_dance_entries: vec![],
            tap_dance_names: vec![],
            tap_dance_undo_stack: vec![],
            tap_dance_editor_open: None,
            tap_dance_dirty: false,
            td_key_pick: None,
            td_mod_key_pick: None,
            macro_texts: vec![Vec::new(); 16],
            macro_names: vec![String::new(); 16],
            macro_descriptions: vec![String::new(); 16],
            macro_actions: vec![vec![]; 16],
            macro_undo_stack: Vec::new(),
            macro_key_pick: None,
            macros_dirty: false,
            popup_state: PopupState::default(),
            language: crate::i18n::default_language(),
            key_legend_layout: KeyLegendLayout::default(),
            show_shifted_number_symbols: true,
        }
    }
}

impl KeycodePicker {
    fn picker_keycode_tooltip(
        &self,
        value: u16,
        custom_pairs: &[crate::keyboard::CustomKeycode],
    ) -> String {
        keycode_tooltip(value, custom_pairs, &self.layer_names)
    }

    fn assign_keycode_value(&mut self, value: u16) {
        self.result = Some(value);
        self.open = false;
    }

    fn macro_key_pick_kind(&self, macro_idx: usize, action_idx: usize) -> Option<MacroKeyPickKind> {
        self.macro_actions
            .get(macro_idx)
            .and_then(|actions| actions.get(action_idx))
            .and_then(|action| match action {
                MacroAction::Tap(_) => Some(MacroKeyPickKind::Tap),
                MacroAction::Down(_) => Some(MacroKeyPickKind::Down),
                MacroAction::Up(_) => Some(MacroKeyPickKind::Up),
                _ => None,
            })
    }

    fn set_macro_key_pick_value(&mut self, macro_idx: usize, action_idx: usize, value: u16) {
        let mut changed = false;
        if let Some(MacroAction::Tap(kc) | MacroAction::Down(kc) | MacroAction::Up(kc)) = self
            .macro_actions
            .get_mut(macro_idx)
            .and_then(|actions| actions.get_mut(action_idx))
        {
            changed = *kc != value;
            *kc = value;
        }
        if changed {
            self.encode_macro(macro_idx);
            self.macros_dirty = true;
        }
        self.macro_key_pick = None;
    }

    fn macro_layer_key_choices(&self, kind: MacroKeyPickKind) -> Vec<(u16, String, String)> {
        if !self.supports_macro_ext_keycodes {
            return Vec::new();
        }

        self.layer_action_key_choices(kind)
    }

    fn macro_ext_keycodes_notice(&self, language: crate::i18n::Language) -> Option<&'static str> {
        match self.macro_ext_keycodes_disabled_reason? {
            MacroExtKeycodesDisabledReason::RmkVialMacroExtUnsupported => match language {
                crate::i18n::Language::Russian => Some(
                    "RMK сейчас не выполняет Vial extended macro keycodes. Действия слоев в macro отключены, чтобы не сохранять macro, которые не сработают на клавиатуре.",
                ),
                crate::i18n::Language::English => Some(
                    "RMK currently does not execute Vial extended macro keycodes. Layer actions in macros are disabled to avoid saving macros that will not work on the keyboard.",
                ),
            },
        }
    }

    fn layer_action_key_choices(&self, kind: MacroKeyPickKind) -> Vec<(u16, String, String)> {
        let ops: Vec<(u16, &str)> = match kind {
            MacroKeyPickKind::Tap => {
                let mut ops = vec![
                    (0x5260, "TG"),
                    (0x5200, "TO"),
                    (0x5240, "DF"),
                    (0x5280, "OSL"),
                ];
                if self.supports_persistent_default_layer {
                    ops.push((0x52E0, "PDF"));
                }
                ops
            }
            MacroKeyPickKind::Down | MacroKeyPickKind::Up => vec![(0x5220, "MO")],
        };

        let count = self.layer_count.max(1);
        ops.into_iter()
            .flat_map(|(base, op)| {
                (0..count).map(move |layer| {
                    let value = base | layer as u16;
                    let label = self.layer_action_key_label(op, layer);
                    let tooltip = keycode_tooltip(value, &[], &self.layer_names);
                    (value, label, tooltip)
                })
            })
            .collect()
    }

    fn layer_action_key_label(&self, op: &str, layer: usize) -> String {
        match self.layer_names.get(layer) {
            Some(name) if !name.is_empty() && name != &layer.to_string() => {
                format!("{op}({layer})\n{name}")
            }
            _ => format!("{op}({layer})"),
        }
    }

    fn finish_quantum_pending_key(&mut self, base: u16, key_value: u16, is_mt: bool) {
        let _ = is_mt;
        self.result = Some(base | key_value);
        self.vial_quantum_pending_mod = None;
        self.vial_quantum_pending_mt = None;
        self.open = false;
    }

    fn finalize_vial_special_tab_close(&mut self) {
        if self.selected_tab == KeycodeTab::Macro {
            if let Some(raw_n) = self.macro_inline_selected {
                if (raw_n as usize) < self.macro_count {
                    self.encode_macro(raw_n as usize);
                    self.result = Some(0x7700 + raw_n as u16);
                    self.macros_dirty = true;
                }
            }
        }
        if self.selected_tab == KeycodeTab::TapDance {
            let td_n = self.tap_dance_editor_open.unwrap_or(0);
            if (td_n as usize) < self.tap_dance_entries.len() {
                self.result = Some(0x5700 + td_n as u16);
                self.tap_dance_dirty = true;
            }
        }
    }

    pub(crate) fn close_from_backdrop(&mut self) {
        let full_vial_picker = !self.regular_key_pick
            && self.regular_mod_key_pick.is_none()
            && self.vial_quantum_pending_mod.is_none()
            && self.vial_quantum_pending_mt.is_none()
            && self.vial_layer_pending.is_none()
            && self.macro_key_pick.is_none()
            && self.td_key_pick.is_none()
            && self.td_mod_key_pick.is_none();

        if full_vial_picker {
            self.finalize_vial_special_tab_close();
        }

        self.open = false;
        self.regular_key_pick = false;
        self.regular_key_pick_allow_mod_key = false;
        self.regular_mod_key_pick = None;
        self.vial_quantum_pending_mod = None;
        self.vial_quantum_pending_mt = None;
        self.vial_layer_pending = None;
        self.macro_key_pick = None;
        self.td_key_pick = None;
        self.td_mod_key_pick = None;
    }

    pub(crate) fn open_regular_key_picker(&mut self) {
        self.open_regular_key_picker_with_mod_key(false);
    }

    pub(crate) fn open_regular_key_picker_with_mod_key(&mut self, allow_mod_key: bool) {
        self.result = None;
        self.open = true;
        self.regular_key_pick = true;
        self.regular_key_pick_allow_mod_key = allow_mod_key;
        self.regular_mod_key_pick = None;
        self.search_query.clear();
        self.vial_quantum_pending_mod = None;
        self.vial_quantum_pending_mt = None;
        self.vial_layer_pending = None;
    }

    pub(crate) fn open_full_key_picker(&mut self, selected_tab: KeycodeTab) {
        self.result = None;
        self.open = true;
        self.regular_key_pick = false;
        self.regular_key_pick_allow_mod_key = false;
        self.regular_mod_key_pick = None;
        self.search_query.clear();
        self.vial_quantum_pending_mod = None;
        self.vial_quantum_pending_mt = None;
        self.vial_layer_pending = None;
        self.tap_dance_editor_open = None;
        self.td_key_pick = None;
        self.td_mod_key_pick = None;
        self.selected_tab = selected_tab;
    }

    pub(crate) fn select_tab_for_keycode(&mut self, value: u16) {
        let is_custom_keycode = self
            .custom_keycodes
            .iter()
            .any(|(_, _, _, custom_value)| *custom_value == value);
        let preferred_tab = KeycodeTab::preferred_for_vial_keycode(value, is_custom_keycode);
        self.selected_tab = if self.vial_tab_supported(preferred_tab) {
            preferred_tab
        } else {
            KeycodeTab::Basic
        };
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let macro_key_pick_open = self.macro_key_pick.is_some();
        let regular_key_pick_open = self.regular_key_pick || self.regular_mod_key_pick.is_some();
        let layer_pick_open = self.vial_layer_pending.is_some();
        let pending_key_pick_open =
            self.vial_quantum_pending_mod.is_some() || self.vial_quantum_pending_mt.is_some();
        let tap_dance_editor_open = self.tap_dance_editor_open.is_some();
        let td_key_pick_open = self.td_key_pick.is_some() || self.td_mod_key_pick.is_some();

        self.popup_state
            .begin_frame(PopupKey::PickerWindow, self.open);
        self.popup_state
            .begin_frame(PopupKey::MacroKeyPickWindow, macro_key_pick_open);
        self.popup_state
            .begin_frame(PopupKey::RegularKeyPickWindow, regular_key_pick_open);
        self.popup_state
            .begin_frame(PopupKey::PickLayerWindow, layer_pick_open);
        self.popup_state
            .begin_frame(PopupKey::PendingKeyPickWindow, pending_key_pick_open);
        self.popup_state
            .begin_frame(PopupKey::TapDanceEditorWindow, tap_dance_editor_open);
        self.popup_state
            .begin_frame(PopupKey::TdKeyPickWindow, td_key_pick_open);

        if !self.open {
            return;
        }

        // If pending mod/MT — show only the minimal second picker, not the full picker
        let has_pending = self.vial_quantum_pending_mod.is_some()
            || self.vial_quantum_pending_mt.is_some()
            || self.vial_layer_pending.is_some();
        if has_pending {
            self.show_pending_picker(ctx);
            return;
        }

        if self.regular_key_pick {
            self.show_regular_key_picker(ctx);
            return;
        }

        // Macro key picker (sub-window of macro editor)
        if let Some((macro_idx, action_idx)) = self.macro_key_pick {
            let mut pick_open = true;
            let popup_size = key_picker_popup_size(ctx);
            crate::ui_style::centered_modal_window(
                ctx,
                tr_picker(self.language, "key_picker.pick_key_title"),
                self.popup_state.id(PopupKey::MacroKeyPickWindow),
                &mut pick_open,
                popup_size,
            )
            .show(ctx, |ui| {
                apply_picker_button_visuals(ui);
                self.show_popup_view_mode_header(ui);
                crate::ui_style::modal_intro(
                    ui,
                    tr_picker(self.language, "key_picker.press_key_or_click"),
                );
                crate::ui_style::modal_hint(
                    ui,
                    tr_picker(self.language, "key_picker.best_for_normal"),
                );
                ui.add_space(crate::ui_style::modal_space_xs());
                // Physical key capture
                ctx.input(|i| {
                    for event in &i.events {
                        if let egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } = event
                        {
                            if *key == Key::Escape {
                                self.macro_key_pick = None;
                                return;
                            }
                            if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                                if qmk > 0 && (qmk < 0x0100 || self.supports_macro_ext_keycodes) {
                                    self.set_macro_key_pick_value(macro_idx, action_idx, qmk);
                                }
                            }
                        }
                    }
                });
                if picker_button(
                    ui,
                    tr_picker(self.language, "key_picker.none_clear"),
                    crate::ui_style::modal_action_button_size(),
                    true,
                    false,
                )
                .clicked()
                {
                    self.set_macro_key_pick_value(macro_idx, action_idx, 0);
                }
                ui.add_space(4.0);
                let layer_choices = self
                    .macro_key_pick_kind(macro_idx, action_idx)
                    .map(|kind| self.macro_layer_key_choices(kind))
                    .unwrap_or_default();
                let supports_macro_ext_keycodes = self.supports_macro_ext_keycodes;
                let key_choices: Vec<&'static crate::keycode::Keycode> = KEYCODES
                    .iter()
                    .filter(|kc| {
                        (supports_macro_ext_keycodes || is_8bit_tap_key_choice(kc))
                            && !kc.name.starts_with("RGB_")
                    })
                    .collect();
                egui::ScrollArea::vertical()
                    .max_height(key_picker_popup_scroll_height(popup_size))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.macro_key_pick.is_some() {
                            match self.popup_view_mode {
                                PickerViewMode::Layout => {
                                    if let Some(value) =
                                        self.show_popup_key_choice_view(ui, key_choices, true)
                                    {
                                        self.set_macro_key_pick_value(macro_idx, action_idx, value);
                                    }
                                }
                                PickerViewMode::List => {
                                    if let Some(value) = show_grouped_popup_key_buttons(
                                        ui,
                                        key_choices,
                                        &self.layer_names,
                                        true,
                                        self.language,
                                        self.key_legend_layout,
                                    ) {
                                        self.set_macro_key_pick_value(macro_idx, action_idx, value);
                                    }
                                    if self.macro_key_pick.is_some() {
                                        if let Some(value) = show_grouped_popup_choice_buttons(
                                            ui,
                                            vec![("Layers", layer_choices)],
                                            self.language,
                                        ) {
                                            self.set_macro_key_pick_value(
                                                macro_idx, action_idx, value,
                                            );
                                        }
                                    }
                                    if self.macro_key_pick.is_some() && supports_macro_ext_keycodes
                                    {
                                        if let Some(value) =
                                            self.show_custom_keycode_choice_section(ui)
                                        {
                                            self.set_macro_key_pick_value(
                                                macro_idx, action_idx, value,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    });
            });
            if !pick_open {
                self.macro_key_pick = None;
            }
            // Don't show macro editor while key picker is open
            return;
        }

        // Tap dance key picker
        if let Some((td_idx, field)) = self.td_key_pick {
            self.show_td_key_picker(ctx, td_idx, field);
            return;
        }
        if let Some((td_idx, field, _base)) = self.td_mod_key_pick {
            self.show_td_key_picker(ctx, td_idx, field);
            return;
        }

        self.show_vial(ctx);
    }

    // ─────────────────────────── VIAL PICKER ────────────────────────────────

    fn show_vial(&mut self, ctx: &egui::Context) {
        if self.selected_tab == KeycodeTab::Layers {
            self.selected_tab = KeycodeTab::Modifiers;
        }
        if self.selected_tab == KeycodeTab::Media {
            self.selected_tab = KeycodeTab::Special;
        }

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.vial_quantum_pending_mod.is_some() || self.vial_quantum_pending_mt.is_some() {
                self.vial_quantum_pending_mod = None;
                self.vial_quantum_pending_mt = None;
            } else {
                self.finalize_vial_special_tab_close();
                self.open = false;
            }
            return;
        }

        // Physical key capture is disabled on inline macro editing tab and while text inputs are focused
        if self.selected_tab != KeycodeTab::Macro && !ctx.wants_keyboard_input() {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                    {
                        // Physical key capture only when no pending mod (avoid accidental assignment)
                        if self.vial_quantum_pending_mod.is_none()
                            && self.vial_quantum_pending_mt.is_none()
                        {
                            if self.search_query.is_empty() || modifiers.any() {
                                if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                                    self.assign_keycode_value(qmk);
                                }
                            }
                        } else {
                            // Pending mod: only accept basic keys (no mods pressed)
                            if !modifiers.any() {
                                if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                                    if qmk > 0 && qmk < 0x0100 {
                                        let base = self
                                            .vial_quantum_pending_mod
                                            .or(self.vial_quantum_pending_mt)
                                            .unwrap_or(0);
                                        let is_mt = self.vial_quantum_pending_mod.is_none()
                                            && self.vial_quantum_pending_mt.is_some();
                                        self.finish_quantum_pending_key(base, qmk, is_mt);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        let mut still_open = true;
        let picker_size = key_picker_main_size(ctx);
        crate::ui_style::centered_modal_window(
            ctx,
            tr_picker(self.language, "key_picker.title"),
            self.popup_state.id(PopupKey::PickerWindow),
            &mut still_open,
            picker_size,
        )
        .show(ctx, |ui| {
            apply_picker_button_visuals(ui);
            ui.vertical_centered(|ui| {
                crate::ui_style::modal_intro(
                    ui,
                    tr_picker(self.language, "key_picker.press_key_or_pick"),
                );
            });
            ui.add_space(4.0);

            if !self.vial_tab_supported(self.selected_tab) {
                self.selected_tab = KeycodeTab::Basic;
            }

            // Tab bar
            let tabs = KeycodeTab::VIAL_TABS;
            let visible_tabs: Vec<KeycodeTab> = tabs
                .iter()
                .copied()
                .filter(|tab| self.vial_tab_supported(*tab))
                .collect();
            let tab_spacing = 6.0;
            let tab_bar_width: f32 = visible_tabs
                .iter()
                .map(|tab| picker_tab_width(picker_tab_label(self.language, *tab)))
                .sum::<f32>()
                + tab_spacing * visible_tabs.len().saturating_sub(1) as f32;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(tab_spacing, 6.0);
                let x_offset = ((ui.available_width() - tab_bar_width).max(0.0) * 0.5).floor();
                if x_offset > 0.0 {
                    ui.add_space(x_offset);
                }
                for tab in &visible_tabs {
                    let active = self.selected_tab == *tab;
                    let tab_label = picker_tab_label(self.language, *tab);
                    if picker_tab_button(ui, tab_label, active).clicked() {
                        self.selected_tab = *tab;
                        self.vial_quantum_pending_mod = None;
                        self.vial_quantum_pending_mt = None;
                        self.vial_layer_pending = None;
                    }
                }
            });
            ui.add_space(crate::ui_style::modal_space_sm());

            let content_height = key_picker_main_content_height(picker_size);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), content_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_height(content_height);
                    egui::ScrollArea::vertical()
                        .max_height(content_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.scope(|ui| {
                                apply_picker_button_visuals(ui);

                                if self.selected_tab == KeycodeTab::Basic {
                                    ui.add_space(28.0);
                                    self.show_vial_tab_content(ui);
                                } else {
                                    let centered_width = self.tab_content_width(ui);
                                    let x_offset =
                                        ((ui.available_width() - centered_width).max(0.0) * 0.5)
                                            .floor();
                                    ui.horizontal(|ui| {
                                        if x_offset > 0.0 {
                                            ui.add_space(x_offset);
                                        }
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(centered_width, 0.0),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| self.show_vial_tab_content(ui),
                                        );
                                    });
                                }
                            });
                        });
                },
            );
        });

        if !still_open {
            self.finalize_vial_special_tab_close();
            self.open = false;
        }
    }

    fn show_regular_key_picker(&mut self, ctx: &egui::Context) {
        let pending_mod_key = self.regular_mod_key_pick;
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if *key == egui::Key::Escape {
                        if self.regular_mod_key_pick.is_some() {
                            self.regular_mod_key_pick = None;
                        } else {
                            self.regular_key_pick = false;
                            self.regular_key_pick_allow_mod_key = false;
                            self.open = false;
                        }
                        return;
                    }
                    if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                        if let Some(base) = self.regular_mod_key_pick {
                            if !modifiers.any() && self.is_regular_key_pick_value(qmk) {
                                self.finish_regular_key_pick(base | qmk);
                            }
                        } else if qmk > 0 && qmk < 0x0100 {
                            if (self.regular_key_pick_allow_mod_key || !modifiers.any())
                                && self.is_regular_key_pick_value(qmk)
                            {
                                self.finish_regular_key_pick(qmk);
                            }
                        } else if self.regular_key_pick_allow_mod_key
                            && (0x0100..0x2000).contains(&qmk)
                            && self.is_regular_key_pick_value(qmk & 0x00FF)
                        {
                            self.finish_regular_key_pick(qmk);
                        }
                    }
                }
            }
        });

        let mut still_open = true;
        let popup_size = key_picker_popup_size(ctx);
        let window_title = if let Some(base) = pending_mod_key {
            format!(
                "{}: {}",
                tr_picker(self.language, "key_picker.pick_modifier_combo_title"),
                picker_mod_key_label(base)
            )
        } else {
            tr_picker(self.language, "key_picker.pick_key_title").to_string()
        };
        crate::ui_style::centered_modal_window(
            ctx,
            window_title.as_str(),
            self.popup_state.id(PopupKey::RegularKeyPickWindow),
            &mut still_open,
            popup_size,
        )
        .show(ctx, |ui| {
            apply_picker_button_visuals(ui);
            self.show_popup_view_mode_header(ui);
            crate::ui_style::modal_intro(
                ui,
                tr_picker(self.language, "key_picker.press_key_or_click_cancel"),
            );
            if pending_mod_key.is_some() {
                crate::ui_style::modal_hint(
                    ui,
                    tr_picker(self.language, "key_picker.pending_mod_hint"),
                );
            }
            ui.add_space(crate::ui_style::modal_space_sm());

            let key_choices = self.regular_key_pick_choices();
            egui::ScrollArea::vertical()
                .max_height(key_picker_popup_scroll_height(popup_size))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(base) = pending_mod_key {
                        if let Some(value) = self.show_popup_key_choice_view(ui, key_choices, false)
                        {
                            self.finish_regular_key_pick(base | value);
                        }
                    } else {
                        match self.popup_view_mode {
                            PickerViewMode::Layout => {
                                if let Some(value) =
                                    self.show_popup_key_choice_view(ui, key_choices, false)
                                {
                                    self.finish_regular_key_pick(value);
                                }
                            }
                            PickerViewMode::List => {
                                self.show_regular_plain_modifier_section(ui);

                                if self.regular_key_pick_allow_mod_key {
                                    self.show_regular_mod_key_section(ui);
                                }

                                if let Some(value) = show_grouped_popup_key_buttons(
                                    ui,
                                    key_choices,
                                    &self.layer_names,
                                    false,
                                    self.language,
                                    self.key_legend_layout,
                                ) {
                                    self.finish_regular_key_pick(value);
                                }
                            }
                        }
                    }
                });
        });
        if !still_open {
            self.regular_mod_key_pick = None;
            self.regular_key_pick = false;
            self.regular_key_pick_allow_mod_key = false;
            self.open = false;
        }
    }

    fn finish_regular_key_pick(&mut self, value: u16) {
        self.result = Some(value);
        self.regular_mod_key_pick = None;
        self.regular_key_pick = false;
        self.regular_key_pick_allow_mod_key = false;
        self.open = false;
    }

    fn regular_key_pick_choices(&self) -> Vec<&'static crate::keycode::Keycode> {
        KEYCODES
            .iter()
            .filter(|kc| {
                is_8bit_tap_key_choice(kc)
                    && !matches!(kc.category, KeycodeCategory::Modifier)
                    && !kc.name.starts_with("RGB_")
            })
            .collect()
    }

    fn is_regular_key_pick_value(&self, value: u16) -> bool {
        self.regular_key_pick_choices()
            .iter()
            .any(|kc| kc.value == value)
    }

    fn show_regular_plain_modifier_section(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(tr_picker(
                self.language,
                "key_picker.section_plain_modifiers",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let plain_modifiers = [
                ("Ctrl".to_owned(), 0x00E0u16, 0x00E4u16, "Ctrl".to_owned()),
                ("Shift".to_owned(), 0x00E1u16, 0x00E5u16, "Shift".to_owned()),
                ("Alt".to_owned(), 0x00E2u16, 0x00E6u16, "Alt".to_owned()),
                (
                    gui_label(false).to_string(),
                    0x00E3u16,
                    0x00E7u16,
                    gui_mod_name().to_string(),
                ),
            ];
            for (label, left_value, right_value, mod_name) in plain_modifiers {
                let resp =
                    picker_keycap_button(ui, &label, Self::picker_key_size(ui.ctx()), true, false)
                        .on_hover_text(crate::i18n::tr_text(
                            self.language,
                            &plain_modifier_tooltip(&mod_name),
                        ));
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.finish_regular_key_pick(left_value);
                }
                if resp.clicked_by(egui::PointerButton::Secondary) {
                    self.finish_regular_key_pick(right_value);
                }
            }
        });
        ui.add_space(crate::ui_style::modal_space_sm());
    }

    fn show_regular_mod_key_section(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_mod_key"))
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let shortcuts: Vec<(String, u16, u16, String)> = vec![
            (picker_mod_key_label(0x0100), 0x0100, 0x1100, "Ctrl".into()),
            (picker_mod_key_label(0x0200), 0x0200, 0x1200, "Shift".into()),
            (picker_mod_key_label(0x0400), 0x0400, 0x1400, "Alt".into()),
            (
                picker_mod_key_label(0x0800),
                0x0800,
                0x1800,
                gui_mod_name().to_string(),
            ),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, left_base, right_base, mod_name) in &shortcuts {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.regular_mod_key_pick = Some(*left_base);
                }
                if resp.clicked_by(egui::PointerButton::Secondary) {
                    self.regular_mod_key_pick = Some(*right_base);
                }
                resp.on_hover_text(crate::i18n::tr_text(
                    self.language,
                    &mod_combo_tooltip(mod_name, true),
                ));
            }
        });
        ui.add_space(crate::ui_style::modal_space_sm());
    }

    fn show_pending_picker(&mut self, ctx: &egui::Context) {
        // Layer picker
        if let Some(base) = self.vial_layer_pending {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key {
                        key, pressed: true, ..
                    } = event
                    {
                        if *key == egui::Key::Escape {
                            self.vial_layer_pending = None;
                            self.open = false;
                            return;
                        }
                    }
                }
            });
            let mut still_open = true;
            let _resp_win = crate::ui_style::centered_modal_window(
                ctx,
                tr_picker(self.language, "key_picker.pick_layer_title"),
                self.popup_state.id(PopupKey::PickLayerWindow),
                &mut still_open,
                Vec2::new(300.0, 120.0),
            )
            .show(ctx, |ui| {
                apply_picker_button_visuals(ui);
                crate::ui_style::modal_intro(
                    ui,
                    tr_picker(self.language, "key_picker.pick_layer_intro"),
                );
                ui.add_space(crate::ui_style::modal_space_sm());
                ui.horizontal_wrapped(|ui| {
                    for n in 0u16..self.layer_count.max(1) as u16 {
                        let raw = self
                            .layer_names
                            .get(n as usize)
                            .cloned()
                            .unwrap_or(n.to_string());
                        let label = if !raw.is_empty() && raw != n.to_string() {
                            format!("{}: {}", n, raw)
                        } else {
                            format!("Layer {}", n)
                        };
                        let resp = picker_button(
                            ui,
                            &label,
                            crate::ui_style::modal_small_button_size(84.0),
                            true,
                            false,
                        );
                        let resp = resp.on_hover_text(crate::i18n::tr_text(self.language, &label));
                        if resp.clicked() {
                            if base & 0xF000 == 0x4000 {
                                // LT: layer in bits 8..11, tap kc in bits 0..7.
                                let value = (base & 0xF0FF) | ((n & 0xF) << 8);
                                self.vial_layer_pending = None;
                                if value & 0xFF == 0 {
                                    self.vial_quantum_pending_mt = Some(value);
                                } else {
                                    self.result = Some(value);
                                    self.open = false;
                                }
                            } else {
                                self.result = Some(base + n);
                                self.vial_layer_pending = None;
                                self.open = false;
                            }
                        }
                    }
                });
            });
            if !still_open {
                self.vial_layer_pending = None;
                self.open = false;
            }
            return;
        }

        let pending = self
            .vial_quantum_pending_mod
            .or(self.vial_quantum_pending_mt);
        let is_mt =
            self.vial_quantum_pending_mod.is_none() && self.vial_quantum_pending_mt.is_some();
        // Physical key capture for pending
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if *key == egui::Key::Escape {
                        self.vial_quantum_pending_mod = None;
                        self.vial_quantum_pending_mt = None;
                        self.open = false;
                        return;
                    }
                    if !modifiers.any() {
                        if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                            if qmk > 0 && qmk < 0x0100 {
                                if let Some(base) = pending {
                                    self.finish_quantum_pending_key(base, qmk, is_mt);
                                }
                            }
                        }
                    }
                }
            }
        });

        if let Some(base) = pending {
            let title = if is_mt {
                tr_picker(self.language, "key_picker.pick_tap_key_title")
            } else {
                tr_picker(self.language, "key_picker.pick_modifier_combo_title")
            };
            let mut still_open = true;
            let popup_size = key_picker_popup_size(ctx);
            let _resp_win = crate::ui_style::centered_modal_window(
                ctx,
                title,
                self.popup_state.id(PopupKey::PendingKeyPickWindow),
                &mut still_open,
                popup_size,
            )
            .show(ctx, |ui| {
                apply_picker_button_visuals(ui);
                crate::ui_style::modal_intro(
                    ui,
                    tr_picker(self.language, "key_picker.press_key_or_click_cancel"),
                );
                ui.add_space(crate::ui_style::modal_space_sm());

                let key_choices: Vec<&'static crate::keycode::Keycode> = KEYCODES
                    .iter()
                    .filter(|kc| {
                        is_8bit_tap_key_choice(kc)
                            && !matches!(kc.category, KeycodeCategory::Modifier)
                    })
                    .collect();
                egui::ScrollArea::vertical()
                    .max_height(key_picker_popup_scroll_height(popup_size))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(tr_picker(
                                self.language,
                                "key_picker.section_plain_modifiers",
                            ))
                            .size(11.0)
                            .color(Color32::from_gray(150)),
                        );
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let plain_modifiers = [
                                ("Ctrl".to_owned(), 0x00E0u16, 0x00E4u16, "Ctrl".to_owned()),
                                ("Shift".to_owned(), 0x00E1u16, 0x00E5u16, "Shift".to_owned()),
                                ("Alt".to_owned(), 0x00E2u16, 0x00E6u16, "Alt".to_owned()),
                                (
                                    gui_label(false).to_string(),
                                    0x00E3u16,
                                    0x00E7u16,
                                    gui_mod_name().to_string(),
                                ),
                            ];
                            for (label, left_value, right_value, mod_name) in plain_modifiers {
                                let resp = picker_keycap_button(
                                    ui,
                                    &label,
                                    Self::picker_key_size(ui.ctx()),
                                    true,
                                    false,
                                )
                                .on_hover_text(
                                    crate::i18n::tr_text(
                                        self.language,
                                        &plain_modifier_tooltip(&mod_name),
                                    ),
                                );
                                if resp.clicked_by(egui::PointerButton::Primary) {
                                    self.finish_quantum_pending_key(base, left_value, is_mt);
                                }
                                if resp.clicked_by(egui::PointerButton::Secondary) {
                                    self.finish_quantum_pending_key(base, right_value, is_mt);
                                }
                            }
                        });
                        ui.add_space(crate::ui_style::modal_space_sm());

                        if let Some(value) = self.show_qwerty_popup_key_grid(ui, |value| {
                            key_choices.iter().any(|kc| kc.value == value)
                        }) {
                            self.finish_quantum_pending_key(base, value, is_mt);
                        }

                        if let Some(value) = show_grouped_popup_key_buttons(
                            ui,
                            key_choices,
                            &self.layer_names,
                            false,
                            self.language,
                            self.key_legend_layout,
                        ) {
                            self.finish_quantum_pending_key(base, value, is_mt);
                        }
                    });
            });
            if !still_open {
                self.vial_quantum_pending_mod = None;
                self.vial_quantum_pending_mt = None;
                self.open = false;
            }
        }
    }

    fn vial_tab_supported(&self, tab: KeycodeTab) -> bool {
        match tab {
            KeycodeTab::Rgb => self.supports_rgb,
            KeycodeTab::Macro => self.supports_macro,
            KeycodeTab::TapDance => self.supports_tap_dance,
            KeycodeTab::Custom => self.has_visible_custom_keycodes(),
            _ => true,
        }
    }

    fn vial_keycode_supported(&self, kc: &crate::keycode::Keycode) -> bool {
        match kc.name {
            "QK_CAPS_WORD_TOGGLE" => self.supports_caps_word,
            "QK_REPEAT_KEY" | "QK_ALT_REPEAT_KEY" => self.supports_repeat_key,
            "CMB_TOG" => self.supports_combo,
            "KC_ASTG" => self.supports_auto_shift,
            "QK_LAYER_LOCK" => self.supports_layer_lock,
            name if name.starts_with("RGB_") => self.supports_rgb,
            name if name.starts_with("BL_") => false,
            _ => true,
        }
    }

    fn show_vial_tab_content(&mut self, ui: &mut egui::Ui) {
        match self.selected_tab {
            KeycodeTab::Basic => self.show_vial_basic(ui),
            KeycodeTab::Symbols => self.show_vial_symbols(ui),
            KeycodeTab::Layers => self.show_vial_layers(ui),
            KeycodeTab::Modifiers => self.show_vial_modifiers(ui),
            KeycodeTab::Quantum => self.show_vial_quantum(ui),
            KeycodeTab::Rgb => self.show_vial_rgb(ui),
            KeycodeTab::Macro => self.show_vial_macros(ui),
            KeycodeTab::TapDance => self.show_vial_tap_dance(ui),
            KeycodeTab::Special => self.show_vial_special(ui),
            KeycodeTab::Custom => self.show_vial_custom(ui),
            _ => self.show_vial_generic(ui),
        }
    }
}
