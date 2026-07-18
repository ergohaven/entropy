#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) static TRAY_QUIT_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) static TRAY_RESTORE_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(crate) const MATRIX_TESTER_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);
pub(crate) const MATRIX_TESTER_LOCK_CHECK_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(750);
pub(crate) const UI_SCALE_MIN: f32 = 0.5;
pub(crate) const UI_SCALE_MAX: f32 = 2.0;
pub(crate) const UI_SCALE_STEP: f32 = 0.1;
pub(crate) const TEXT_EXPANDER_SAVE_DEBOUNCE_SECS: f64 = 0.45;
pub(crate) const ONBOARDING_TOUR_VERSION: u16 = 1;
pub(crate) const COMBO_NO_COLOR: u32 = 0x000000;
pub(crate) const COMBO_COLOR_SEED_PALETTE: [u32; 16] = [
    0xC48490, 0x9280B8, 0x749AD4, 0xD29C5C, 0xC07458, 0x589E94, 0xB28A6A, 0x8A9A66, 0xB070A8,
    0x6F94B8, 0xB6A05F, 0x7DA986, 0xC18B74, 0x8F8BC0, 0xC2A078, 0x6FA4A0,
];

use super::*;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct AppSettings {
    #[serde(default)]
    pub(crate) minimize_to_tray_on_close: bool,
    #[serde(default)]
    pub(crate) close_to_tray_behavior: CloseToTrayBehavior,
    #[serde(default)]
    pub(crate) launch_at_startup: bool,
    #[serde(default = "default_show_shifted_number_symbols")]
    pub(crate) show_shifted_number_symbols: bool,
    #[serde(default = "default_layer_hover_preview")]
    pub(crate) layer_hover_preview: bool,
    #[serde(default)]
    pub(crate) sticky_layout_window: bool,
    #[serde(default = "default_sticky_layout_always_on_top")]
    pub(crate) sticky_layout_always_on_top: bool,
    #[serde(default = "default_sticky_layout_opacity")]
    pub(crate) sticky_layout_opacity: f32,
    #[serde(default)]
    pub(crate) sticky_layout_visibility_mode: StickyLayoutVisibilityMode,
    #[serde(default)]
    pub(crate) sticky_layout_dark_mode: bool,
    #[serde(default)]
    pub(crate) sticky_layout_window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) dark_mode: bool,
    #[serde(default = "crate::i18n::default_language")]
    pub(crate) language: crate::i18n::Language,
    #[serde(default = "default_encoder_hover_enlarge")]
    pub(crate) encoder_hover_enlarge: bool,
    #[serde(default = "default_show_made_by_signature")]
    pub(crate) show_made_by_signature: bool,
    #[serde(default)]
    pub(crate) key_legend_layout: KeyLegendLayout,
    #[serde(default)]
    pub(crate) layout_image_export: LayoutImageExportState,
    #[serde(default = "default_app_accent_color")]
    pub(crate) accent_color: AppAccentColor,
    #[serde(default = "default_ui_scale")]
    pub(crate) ui_scale: f32,
    #[serde(default)]
    pub(crate) diagnostics_enabled: bool,
    #[serde(default)]
    pub(crate) onboarding_tour_seen_version: u16,
    #[serde(default)]
    pub(crate) text_expander_enabled: bool,
    #[serde(default)]
    pub(crate) text_expander_app_blacklist: String,
    #[serde(default)]
    pub(crate) text_expander_rule_files: Vec<String>,
    #[serde(default)]
    pub(crate) text_expansion_rules: Vec<crate::text_expander::TextExpansionRule>,
    #[serde(default = "default_layout_sync_enabled")]
    pub(crate) layout_sync_enabled: bool,
    #[serde(default)]
    pub(crate) typing_trainer: TypingTrainerSettings,
    #[serde(default)]
    pub(crate) typing_trainer_history: Vec<TypingTrainerRunRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum CloseToTrayBehavior {
    #[default]
    Ask,
    Close,
    Tray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StickyLayoutVisibilityMode {
    LayoutAndPresses,
    PressedOnly,
}

impl Default for StickyLayoutVisibilityMode {
    fn default() -> Self {
        Self::LayoutAndPresses
    }
}

pub(crate) fn default_show_shifted_number_symbols() -> bool {
    true
}

pub(crate) fn default_layer_hover_preview() -> bool {
    true
}

pub(crate) fn default_encoder_hover_enlarge() -> bool {
    true
}

pub(crate) fn default_show_made_by_signature() -> bool {
    true
}

pub(crate) fn default_sticky_layout_always_on_top() -> bool {
    true
}

pub(crate) fn default_sticky_layout_opacity() -> f32 {
    1.0
}

pub(crate) fn default_app_accent_color() -> AppAccentColor {
    AppAccentColor::Rose
}

pub(crate) fn default_ui_scale() -> f32 {
    1.0
}

pub(crate) fn default_layout_sync_enabled() -> bool {
    true
}

pub(crate) fn clamp_ui_scale(scale: f32) -> f32 {
    let scale = if scale.is_finite() {
        scale
    } else {
        default_ui_scale()
    };
    (scale / UI_SCALE_STEP)
        .round()
        .mul_add(UI_SCALE_STEP, 0.0)
        .clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            minimize_to_tray_on_close: false,
            close_to_tray_behavior: CloseToTrayBehavior::Ask,
            launch_at_startup: false,
            show_shifted_number_symbols: default_show_shifted_number_symbols(),
            layer_hover_preview: default_layer_hover_preview(),
            sticky_layout_window: false,
            sticky_layout_always_on_top: default_sticky_layout_always_on_top(),
            sticky_layout_opacity: default_sticky_layout_opacity(),
            sticky_layout_visibility_mode: StickyLayoutVisibilityMode::default(),
            sticky_layout_dark_mode: false,
            sticky_layout_window_size: None,
            window_size: None,
            dark_mode: false,
            language: crate::i18n::default_language(),
            encoder_hover_enlarge: default_encoder_hover_enlarge(),
            show_made_by_signature: default_show_made_by_signature(),
            key_legend_layout: KeyLegendLayout::default(),
            layout_image_export: LayoutImageExportState::default(),
            accent_color: default_app_accent_color(),
            ui_scale: default_ui_scale(),
            diagnostics_enabled: false,
            onboarding_tour_seen_version: 0,
            text_expander_enabled: false,
            text_expander_app_blacklist: String::new(),
            text_expander_rule_files: Vec::new(),
            text_expansion_rules: Vec::new(),
            layout_sync_enabled: default_layout_sync_enabled(),
            typing_trainer: TypingTrainerSettings::default(),
            typing_trainer_history: Vec::new(),
        }
    }
}

#[cfg(test)]
mod app_settings_tests {
    use super::*;

    #[test]
    fn app_settings_default_dark_mode_is_light() {
        assert!(!AppSettings::default().dark_mode);
    }

    #[test]
    fn app_settings_deserializes_saved_dark_mode() {
        let settings: AppSettings = serde_json::from_str(r#"{"dark_mode":true}"#).unwrap();

        assert!(settings.dark_mode);
    }

    #[test]
    fn app_settings_deserializes_legacy_settings_without_dark_mode() {
        let settings: AppSettings = serde_json::from_str(r#"{"language":"english"}"#).unwrap();

        assert!(!settings.dark_mode);
    }
}

pub(crate) fn keycode_label_with_macro_names(
    value: u16,
    custom: &[crate::keyboard::CustomKeycode],
    layer_names: &[String],
    macro_names: &[String],
    tap_dance_names: &[String],
    key_legend_layout: KeyLegendLayout,
) -> String {
    if (0x7700..=0x77FF).contains(&value) {
        let idx = (value - 0x7700) as usize;
        if let Some(name) = macro_custom_name(macro_names, idx) {
            return format!("M{}\n{}", idx, name);
        }
        return format!("M{}", idx);
    }
    if (0x5700..=0x57FF).contains(&value) {
        let idx = (value - 0x5700) as usize;
        if let Some(name) = tap_dance_custom_name(tap_dance_names, idx) {
            return format!("TD{}\n{}", idx, name);
        }
        return format!("TD{}", idx);
    }
    keycode_label_with_names_and_layout(value, custom, layer_names, key_legend_layout)
}

pub(crate) fn keycode_tooltip_with_macro_names(
    value: u16,
    custom: &[crate::keyboard::CustomKeycode],
    layer_names: &[String],
    macro_names: &[String],
    macro_descriptions: &[String],
    tap_dance_names: &[String],
) -> String {
    if (0x7700..=0x77FF).contains(&value) {
        let idx = (value - 0x7700) as usize;
        let name = macro_display_name(macro_names, idx);
        if let Some(description) = macro_description(macro_descriptions, idx) {
            return format!("{} — macro {}\n{}", name, idx, description);
        }
        return format!("{} — macro {}", name, idx);
    }
    if (0x5700..=0x57FF).contains(&value) {
        let idx = (value - 0x5700) as usize;
        let name = tap_dance_display_name(tap_dance_names, idx);
        return format!("{} — tap dance {}", name, idx);
    }
    keycode_tooltip(value, custom, layer_names)
}

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;

#[derive(Debug, Clone, Default)]
pub(crate) struct VialFeatureSupport {
    pub(crate) caps_word: bool,
    pub(crate) layer_lock: bool,
    pub(crate) persistent_default_layer: bool,
    pub(crate) repeat_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacroExtKeycodesDisabledReason {
    RmkVialMacroExtUnsupported,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DeviceAboutInfo {
    pub(crate) manufacturer: String,
    pub(crate) product: String,
    pub(crate) vendor_id: u16,
    pub(crate) product_id: u16,
    pub(crate) path: String,
    pub(crate) firmware_version: Option<String>,
    pub(crate) battery_halves: Option<crate::hid::BatteryHalves>,
    pub(crate) via_protocol: u16,
    pub(crate) vial_protocol: u32,
    pub(crate) keyboard_id: u64,
    pub(crate) macro_entries: usize,
    pub(crate) macro_memory_bytes: Option<u16>,
    pub(crate) supports_macro_delays: bool,
    pub(crate) supports_macro_ext_keycodes: bool,
    pub(crate) macro_ext_keycodes_disabled_reason: Option<MacroExtKeycodesDisabledReason>,
    pub(crate) tap_dance_entries: usize,
    pub(crate) combo_entries: usize,
    pub(crate) key_override_entries: usize,
    pub(crate) alt_repeat_entries: usize,
    pub(crate) caps_word: bool,
    pub(crate) layer_lock: bool,
    pub(crate) qmk_settings: bool,
}

/// Result sent back from the background connect thread.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct ConnectResult {
    pub(crate) device_name: String,
    /// Stable Vial keyboard definition id used for per-keyboard local settings.
    pub(crate) keyboard_id: u64,
    /// Open HID connection used during loading; kept for live writes just like vial-gui.
    pub(crate) hid_device: Option<crate::hid::HidDevice>,
    pub(crate) layout: KeyboardLayout,
    pub(crate) layer_count: usize,
    /// Device/protocol summary shown by the About Device page.
    pub(crate) about_info: DeviceAboutInfo,
    /// Macro bytecode entries read from device
    pub(crate) macro_texts: Vec<Vec<u8>>,
    /// Vial protocol >= 5 supports 2-byte keycodes in macros.
    pub(crate) supports_macro_ext_keycodes: bool,
    pub(crate) macro_ext_keycodes_disabled_reason: Option<MacroExtKeycodesDisabledReason>,
    /// Tap dance entries
    pub(crate) tap_dance_entries: Vec<crate::keycode_picker::TapDanceEntry>,
    /// Combo entries
    pub(crate) combo_entries: Vec<ComboEntry>,
    /// Global combo timeout/term from QMK settings, if supported
    pub(crate) combo_term: Option<u16>,
    /// Auto Shift flags from QMK settings, if supported
    pub(crate) auto_shift_options: AutoShiftOptionsState,
    /// Auto Shift timeout from QMK settings, if supported
    pub(crate) auto_shift_timeout: Option<u16>,
    /// Mouse Keys settings from QMK settings, if supported (qsid 9..=17)
    pub(crate) mouse_keys_settings: MouseKeysSettingsState,
    /// Ergohaven K:03 Pro touchpad settings from QMK settings, if supported
    pub(crate) touchpad_settings: TouchpadSettingsState,
    /// Bluetooth settings from RMK/Vial QMK settings, if supported
    pub(crate) bluetooth_settings: BluetoothSettingsState,
    /// Keyboard-specific module settings from QMK Settings, if supported
    pub(crate) module_settings: ModuleSettingsState,
    /// Tap-Hold settings from QMK settings, if supported
    pub(crate) tap_hold_settings: TapHoldSettingsState,
    /// Magic settings from QMK settings, if supported
    pub(crate) magic_settings: MagicSettingsState,
    /// One Shot Keys settings from QMK settings, if supported
    pub(crate) one_shot_settings: OneShotSettingsState,
    /// Grave Escape settings from QMK settings, if supported (qsid 1 bits 0..=3)
    pub(crate) grave_escape_settings: GraveEscapeSettingsState,
    /// Ergohaven LED settings from QMK settings, if supported
    pub(crate) layer_led_settings: LayerLedSettingsState,
    /// Runtime RGB settings, if supported by the current Vial/QMK lighting backend
    pub(crate) rgb_settings: RgbSettingsState,
    /// Vial layout/display option bitfield, if exposed by `layouts.labels`
    pub(crate) layout_options_value: Option<u32>,
    /// Key Override entries
    pub(crate) key_override_entries: Vec<KeyOverrideEntry>,
    /// Alt Repeat entries
    pub(crate) alt_repeat_entries: Vec<AltRepeatKeyEntry>,
    /// Feature bits reported by Vial dynamic entries.
    pub(crate) vial_features: VialFeatureSupport,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum ConnectTaskMessage {
    Progress(String),
    Done(Box<Result<ConnectResult, String>>),
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum ConnectState {
    Idle,
    Loading {
        rx: mpsc::Receiver<ConnectTaskMessage>,
        started_at: std::time::Instant,
        last_progress_at: std::time::Instant,
    },
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum DeviceScanState {
    Idle,
    Scanning(mpsc::Receiver<Vec<Device>>),
}

pub(crate) fn toggle_handed_modifier(value: u16) -> Option<u16> {
    match value {
        0x00E0 => Some(0x00E4),
        0x00E4 => Some(0x00E0),
        0x00E1 => Some(0x00E5),
        0x00E5 => Some(0x00E1),
        0x00E2 => Some(0x00E6),
        0x00E6 => Some(0x00E2),
        0x00E3 => Some(0x00E7),
        0x00E7 => Some(0x00E3),
        0x52A1 => Some(0x52B1),
        0x52B1 => Some(0x52A1),
        0x52A2 => Some(0x52B2),
        0x52B2 => Some(0x52A2),
        0x52A4 => Some(0x52B4),
        0x52B4 => Some(0x52A4),
        0x52A8 => Some(0x52B8),
        0x52B8 => Some(0x52A8),
        _ => {
            let base = value & 0xFF00;
            let low = value & 0x00FF;
            match base {
                0x2100 => Some(0x3100 | low),
                0x3100 => Some(0x2100 | low),
                0x2200 => Some(0x3200 | low),
                0x3200 => Some(0x2200 | low),
                0x2400 => Some(0x3400 | low),
                0x3400 => Some(0x2400 | low),
                0x2800 => Some(0x3800 | low),
                0x3800 => Some(0x2800 | low),
                _ => None,
            }
        }
    }
}

pub(crate) fn vial_layer_target(kc: u16) -> Option<usize> {
    if (0x5200..0x5300).contains(&kc) {
        let op = (kc >> 5) & 0x7;
        // QK_ONE_SHOT_MOD also lives in the 0x52xx range (op=5), but it is a
        // modifier keycode, not a layer key. Do not preview/jump layers for OSM.
        (op != 5).then_some((kc & 0x1F) as usize)
    } else if kc & 0xF000 == 0x4000 {
        Some(((kc >> 8) & 0xF) as usize)
    } else {
        None
    }
}

pub(crate) fn vial_layer_op_target(kc: u16) -> Option<(u16, usize)> {
    if (0x5200..0x5300).contains(&kc) {
        let op = (kc >> 5) & 0x7;
        (op != 5).then_some((op, (kc & 0x1F) as usize))
    } else {
        None
    }
}

pub(crate) fn vial_layer_retarget_base(kc: u16) -> Option<u16> {
    if (0x5200..0x5300).contains(&kc) {
        let op = (kc >> 5) & 0x7;
        (op != 5).then_some(kc & 0xFFE0)
    } else if kc & 0xF000 == 0x4000 {
        Some(kc & 0xF0FF)
    } else {
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComboEntry {
    pub(crate) keys: [u16; 4],
    pub(crate) output: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct ComboUndoSnapshot {
    pub(crate) entries: Vec<ComboEntry>,
    pub(crate) names: Vec<String>,
    pub(crate) colors: Vec<u32>,
    pub(crate) term: Option<u16>,
    pub(crate) selected: usize,
    pub(crate) visible_count: usize,
}

pub(crate) fn combo_color_palette(len: usize) -> Vec<u32> {
    (0..len).map(combo_default_color).collect()
}

pub(crate) fn combo_default_color(idx: usize) -> u32 {
    COMBO_COLOR_SEED_PALETTE
        .get(idx)
        .copied()
        .unwrap_or_else(|| combo_generated_color(idx))
}

pub(crate) fn combo_color32(rgb: u32) -> Color32 {
    Color32::from_rgb(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    )
}

pub(crate) fn normalize_combo_colors(colors: &mut Vec<u32>, len: usize) {
    let start = colors.len();
    colors.truncate(len);
    for _ in start..len {
        colors.push(COMBO_NO_COLOR);
    }

    let palette = combo_color_palette(len);
    let mut used = Vec::new();
    for color in colors.iter_mut() {
        if *color == COMBO_NO_COLOR {
            continue;
        }
        if !used.contains(color) {
            used.push(*color);
            continue;
        }
        if let Some(replacement) = palette
            .iter()
            .copied()
            .find(|candidate| *candidate != COMBO_NO_COLOR && !used.contains(candidate))
        {
            *color = replacement;
            used.push(replacement);
        } else {
            *color = COMBO_NO_COLOR;
        }
    }
}

pub(crate) fn migrate_legacy_combo_default_colors(colors: &mut [u32]) {
    if !colors.is_empty()
        && colors
            .iter()
            .enumerate()
            .all(|(idx, color)| *color == combo_default_color(idx))
    {
        colors.fill(COMBO_NO_COLOR);
    }
}

fn combo_generated_color(idx: usize) -> u32 {
    let hue = ((idx as f32 * 0.618_034) % 1.0 + 0.96) % 1.0;
    hsl_to_rgb_u32(hue, 0.34, 0.60)
}

fn hsl_to_rgb_u32(h: f32, s: f32, l: f32) -> u32 {
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    ((float_to_u8(r) as u32) << 16) | ((float_to_u8(g) as u32) << 8) | float_to_u8(b) as u32
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

fn float_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyOverrideOptionsState {
    pub(crate) activation_trigger_down: bool,
    pub(crate) activation_required_mod_down: bool,
    pub(crate) activation_negative_mod_up: bool,
    pub(crate) one_mod: bool,
    pub(crate) no_reregister_trigger: bool,
    pub(crate) no_unregister_on_other_key_down: bool,
    pub(crate) enabled: bool,
}

impl KeyOverrideOptionsState {
    pub(crate) fn from_bits(bits: u8) -> Self {
        Self {
            activation_trigger_down: bits & (1 << 0) != 0,
            activation_required_mod_down: bits & (1 << 1) != 0,
            activation_negative_mod_up: bits & (1 << 2) != 0,
            one_mod: bits & (1 << 3) != 0,
            no_reregister_trigger: bits & (1 << 4) != 0,
            no_unregister_on_other_key_down: bits & (1 << 5) != 0,
            enabled: bits & (1 << 7) != 0,
        }
    }

    pub(crate) fn bits(&self) -> u8 {
        (self.activation_trigger_down as u8)
            | (self.activation_required_mod_down as u8) << 1
            | (self.activation_negative_mod_up as u8) << 2
            | (self.one_mod as u8) << 3
            | (self.no_reregister_trigger as u8) << 4
            | (self.no_unregister_on_other_key_down as u8) << 5
            | (self.enabled as u8) << 7
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyOverrideEntry {
    pub(crate) trigger: u16,
    pub(crate) replacement: u16,
    pub(crate) layers: u16,
    pub(crate) trigger_mods: u8,
    pub(crate) negative_mod_mask: u8,
    pub(crate) suppressed_mods: u8,
    pub(crate) options: KeyOverrideOptionsState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AltRepeatKeyOptionsState {
    pub(crate) default_to_this_alt_key: bool,
    pub(crate) bidirectional: bool,
    pub(crate) ignore_mod_handedness: bool,
    pub(crate) enabled: bool,
}

impl AltRepeatKeyOptionsState {
    pub(crate) fn from_bits(bits: u8) -> Self {
        Self {
            default_to_this_alt_key: bits & (1 << 0) != 0,
            bidirectional: bits & (1 << 1) != 0,
            ignore_mod_handedness: bits & (1 << 2) != 0,
            enabled: bits & (1 << 3) != 0,
        }
    }

    pub(crate) fn bits(self) -> u8 {
        (self.default_to_this_alt_key as u8)
            | ((self.bidirectional as u8) << 1)
            | ((self.ignore_mod_handedness as u8) << 2)
            | ((self.enabled as u8) << 3)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AltRepeatKeyEntry {
    pub(crate) keycode: u16,
    pub(crate) alt_keycode: u16,
    pub(crate) allowed_mods: u8,
    pub(crate) options: AltRepeatKeyOptionsState,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AutoShiftOptionsState {
    pub(crate) enabled: bool,
    pub(crate) enable_for_modifiers: bool,
    pub(crate) no_special: bool,
    pub(crate) no_numeric: bool,
    pub(crate) no_alpha: bool,
    pub(crate) enable_keyrepeat: bool,
    pub(crate) disable_keyrepeat_timeout: bool,
}

impl AutoShiftOptionsState {
    pub(crate) fn from_bits(bits: u8) -> Self {
        Self {
            enabled: bits & (1 << 0) != 0,
            enable_for_modifiers: bits & (1 << 1) != 0,
            no_special: bits & (1 << 2) != 0,
            no_numeric: bits & (1 << 3) != 0,
            no_alpha: bits & (1 << 4) != 0,
            enable_keyrepeat: bits & (1 << 5) != 0,
            disable_keyrepeat_timeout: bits & (1 << 6) != 0,
        }
    }

    pub(crate) fn bits(self) -> u8 {
        (self.enabled as u8)
            | ((self.enable_for_modifiers as u8) << 1)
            | ((self.no_special as u8) << 2)
            | ((self.no_numeric as u8) << 3)
            | ((self.no_alpha as u8) << 4)
            | ((self.enable_keyrepeat as u8) << 5)
            | ((self.disable_keyrepeat_timeout as u8) << 6)
    }
}

/// Mirrors Vial GUI Mouse keys settings (qsid 9..=17). All values are u16.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MouseKeysSettingsState {
    /// qsid 9: Delay between pressing a movement key and cursor movement
    pub(crate) delay: u16,
    /// qsid 10: Time between cursor movements in milliseconds
    pub(crate) interval: u16,
    /// qsid 11: Step size
    pub(crate) max_speed: u16,
    /// qsid 12: Maximum cursor speed at which acceleration stops
    pub(crate) time_to_max: u16,
    /// qsid 13: Time until maximum cursor speed is reached
    pub(crate) move_delta: u16,
    /// qsid 14: Delay between pressing a wheel key and wheel movement
    pub(crate) wheel_delay: u16,
    /// qsid 15: Time between wheel movements
    pub(crate) wheel_interval: u16,
    /// qsid 16: Maximum number of scroll steps per scroll action
    pub(crate) wheel_max_speed: u16,
    /// qsid 17: Time until maximum scroll speed is reached
    pub(crate) wheel_time_to_max: u16,
    /// Whether any of the qsids were readable (firmware support flag)
    pub(crate) supported: bool,
}

/// Ergohaven K:03 Pro touchpad settings exposed by firmware QMK Settings.
#[derive(Clone, Debug, Default)]
pub(crate) struct TouchpadSettingsState {
    /// qsid 120: touchpad DPI/CPI, either direct value or select index depending on definition
    pub(crate) dpi: u16,
    /// qsid 120 variants when the firmware exposes DPI as a select setting
    pub(crate) dpi_variants: Vec<String>,
    /// qsid 121: sensitivity in sniper mode
    pub(crate) sniper_sens: u8,
    /// qsid 122: sensitivity in scroll mode
    pub(crate) scroll_sens: u8,
    /// qsid 123: sensitivity in text mode
    pub(crate) text_sens: u8,
    /// qsid 124 bits 0..=2: invert scroll, acceleration, sticky mode
    pub(crate) bits: u8,
    /// qsid 142: auto layer enable, if exposed by this firmware
    pub(crate) auto_layer_enable: bool,
    /// Whether qsid 142 is exposed by this firmware
    pub(crate) auto_layer_enable_supported: bool,
    /// qsid 143: auto layer select, if exposed by this firmware
    pub(crate) auto_layer: u8,
    /// qsid 143 variants when exposed by this firmware
    pub(crate) auto_layer_variants: Vec<String>,
    /// Whether qsid 120..124 were readable and advertised by firmware definition/query
    pub(crate) supported: bool,
}

impl TouchpadSettingsState {
    pub(crate) fn bit(&self, bit: u8) -> bool {
        self.bits & (1 << bit) != 0
    }

    pub(crate) fn set_bit(&mut self, bit: u8, enabled: bool) {
        if enabled {
            self.bits |= 1 << bit;
        } else {
            self.bits &= !(1 << bit);
        }
    }

    pub(crate) fn auto_layer_supported(&self) -> bool {
        self.auto_layer_enable_supported && !self.auto_layer_variants.is_empty()
    }

    pub(crate) fn row_count(&self) -> usize {
        7 + self.auto_layer_enable_supported as usize + self.auto_layer_supported() as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BluetoothSelectSetting {
    pub(crate) qsid: u16,
    pub(crate) width: u8,
    pub(crate) value: u16,
    pub(crate) variants: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BluetoothProfileColorSetting {
    pub(crate) profile: usize,
    pub(crate) setting: BluetoothSelectSetting,
}

/// RMK wireless settings exposed by the Bluetooth settings tab in Vial.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BluetoothSettingsState {
    /// Sleep timeout before the keyboard enters Bluetooth sleep mode
    pub(crate) sleep_timeout: Option<BluetoothSelectSetting>,
    /// Palette color index for each firmware-supported Bluetooth profile
    pub(crate) profile_colors: Vec<BluetoothProfileColorSetting>,
    /// Whether any Bluetooth setting was readable and advertised by firmware
    pub(crate) supported: bool,
}

impl BluetoothSettingsState {
    pub(crate) fn row_count(&self) -> usize {
        self.sleep_timeout.is_some() as usize + self.profile_colors.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModuleSettingKind {
    Boolean,
    Integer,
    Select,
}

#[derive(Clone, Debug)]
pub(crate) struct ModuleSettingField {
    pub(crate) title: String,
    pub(crate) qsid: u16,
    pub(crate) kind: ModuleSettingKind,
    pub(crate) bit: u8,
    pub(crate) width: u8,
    pub(crate) min: u16,
    pub(crate) max: u16,
    pub(crate) variants: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModuleSettingsGroupKind {
    Left,
    Right,
    AutoLayer,
    Other,
}

#[derive(Clone, Debug)]
pub(crate) struct ModuleSettingsGroup {
    pub(crate) title: String,
    pub(crate) kind: ModuleSettingsGroupKind,
    pub(crate) fields: Vec<ModuleSettingField>,
}

/// Keyboard-specific module settings exposed by firmware QMK Settings.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModuleSettingsState {
    pub(crate) fields: Vec<ModuleSettingField>,
    pub(crate) groups: Vec<ModuleSettingsGroup>,
    pub(crate) selected_module_group: usize,
    pub(crate) values: std::collections::BTreeMap<u16, u16>,
    pub(crate) supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModuleSettingWritebackError {
    SetFailed(String),
    ReadbackFailed(String),
    ReadbackMismatch { expected: u16, actual: u16 },
}

pub(crate) const MODULE_SETTING_READBACK_ATTEMPTS: usize = 3;

impl std::fmt::Display for ModuleSettingWritebackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetFailed(error) => write!(f, "set failed: {error}"),
            Self::ReadbackFailed(error) => write!(f, "read-back failed: {error}"),
            Self::ReadbackMismatch { expected, actual } => {
                write!(f, "read back {actual}, expected {expected}")
            }
        }
    }
}

impl ModuleSettingsState {
    pub(crate) fn selected_module_group(&self) -> Option<usize> {
        let selected = self.groups.get(self.selected_module_group)?;
        if matches!(
            selected.kind,
            ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
        ) {
            Some(self.selected_module_group)
        } else {
            self.groups.iter().position(|group| {
                matches!(
                    group.kind,
                    ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
                )
            })
        }
    }

    pub(crate) fn set_selected_module_group(&mut self, group_idx: usize) {
        let Some(group) = self.groups.get(group_idx) else {
            return;
        };
        if matches!(
            group.kind,
            ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
        ) {
            self.selected_module_group = group_idx;
        }
    }

    pub(crate) fn value(&self, qsid: u16) -> u16 {
        self.values.get(&qsid).copied().unwrap_or(0)
    }

    pub(crate) fn set_value(&mut self, qsid: u16, value: u16) {
        self.values.insert(qsid, value);
    }

    pub(crate) fn write_verified_value(
        &mut self,
        qsid: u16,
        expected: u16,
        write: impl FnOnce() -> Result<(), String>,
        mut read_back: impl FnMut() -> Result<u16, String>,
    ) -> Result<u16, ModuleSettingWritebackError> {
        write().map_err(ModuleSettingWritebackError::SetFailed)?;

        let mut last_error = None;
        for _ in 0..MODULE_SETTING_READBACK_ATTEMPTS {
            match read_back() {
                Ok(actual) if actual == expected => {
                    self.set_value(qsid, actual);
                    return Ok(actual);
                }
                Ok(actual) => {
                    last_error =
                        Some(ModuleSettingWritebackError::ReadbackMismatch { expected, actual });
                }
                Err(error) => {
                    return Err(ModuleSettingWritebackError::ReadbackFailed(error));
                }
            }
        }

        Err(last_error.expect("module setting readback attempts must be non-zero"))
    }
}

/// Mirrors Vial GUI Tap-Hold settings. Values are QMK settings qsids.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TapHoldSettingsState {
    /// qsid 7: Global tap-vs-hold decision window in milliseconds
    pub(crate) tapping_term: u16,
    /// qsid 22: Prefer hold for nested taps
    pub(crate) permissive_hold: bool,
    /// qsid 23: Prefer hold as soon as another key is pressed
    pub(crate) hold_on_other_key_press: bool,
    /// qsid 24: Send tap when a dual-role key is held and released alone
    pub(crate) retro_tapping: bool,
    /// qsid 25: Tap-then-hold repeat window in milliseconds
    pub(crate) quick_tap_term: u16,
    /// qsid 18: Delay between register_code and unregister_code in tap_code
    pub(crate) tap_code_delay: u16,
    /// qsid 19: Delay for LT/MT keys when tap key is KC_CAPS_LOCK
    pub(crate) tap_hold_caps_delay: u16,
    /// qsid 20: Number of taps needed for TT(layer) toggle
    pub(crate) tapping_toggle: u16,
    /// qsid 26: Same-hand chords prefer tap for tap-hold keys
    pub(crate) chordal_hold: bool,
    /// qsid 27: Fast-typing timeout that forces MT/LT tap behavior
    pub(crate) flow_tap: u16,
    /// Bitset of tap-hold qsids advertised by this firmware.
    pub(crate) supported_qsids: u64,
    /// Whether qsid 7 was readable (firmware support flag)
    pub(crate) supported: bool,
}

impl TapHoldSettingsState {
    pub(crate) fn set_qsid_supported(&mut self, qsid: u16) {
        if qsid < u64::BITS as u16 {
            self.supported_qsids |= 1u64 << qsid;
        }
    }

    pub(crate) fn supports_qsid(&self, qsid: u16) -> bool {
        qsid < u64::BITS as u16 && self.supported_qsids & (1u64 << qsid) != 0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MagicSettingsState {
    /// qsid 21 bits 0..=9: QMK Magic runtime swaps/options
    pub(crate) bits: u16,
    /// Whether qsid 21 was readable (firmware support flag)
    pub(crate) supported: bool,
}

impl MagicSettingsState {
    pub(crate) fn bit(self, bit: u8) -> bool {
        self.bits & (1u16 << bit) != 0
    }

    pub(crate) fn set_bit(&mut self, bit: u8, enabled: bool) {
        if enabled {
            self.bits |= 1u16 << bit;
        } else {
            self.bits &= !(1u16 << bit);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OneShotSettingsState {
    /// qsid 5: Tap count that makes a one-shot key stay held until tapped again
    pub(crate) tap_toggle: u8,
    /// qsid 6: Timeout in milliseconds before one-shot state is released
    pub(crate) timeout: u16,
    /// Whether qsid 5 was readable (firmware support flag)
    pub(crate) supported: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GraveEscapeSettingsState {
    /// qsid 1 bits 0..=3: force Esc when Alt/Ctrl/GUI/Shift is held for KC_GESC.
    pub(crate) bits: u8,
    /// Whether qsid 1 was readable (firmware support flag)
    pub(crate) supported: bool,
}

impl GraveEscapeSettingsState {
    pub(crate) fn bit(self, bit: u8) -> bool {
        self.bits & (1 << bit) != 0
    }

    pub(crate) fn set_bit(&mut self, bit: u8, enabled: bool) {
        if enabled {
            self.bits |= 1 << bit;
        } else {
            self.bits &= !(1 << bit);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayerLedColorSetting {
    pub(crate) qsid: u16,
    pub(crate) linked_qsids: Vec<u16>,
    pub(crate) value: u8,
}

impl LayerLedColorSetting {
    pub(crate) fn new(qsid: u16, value: u8) -> Self {
        Self {
            qsid,
            linked_qsids: Vec::new(),
            value,
        }
    }

    pub(crate) fn with_linked_qsids(qsid: u16, linked_qsids: Vec<u16>, value: u8) -> Self {
        Self {
            qsid,
            linked_qsids,
            value,
        }
    }

    pub(crate) fn all_qsids(&self) -> impl Iterator<Item = u16> + '_ {
        std::iter::once(self.qsid).chain(self.linked_qsids.iter().copied())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerLedTimeoutUnit {
    Minutes,
    Seconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayerLedNumericSetting {
    pub(crate) qsid: u16,
    pub(crate) width: u8,
    pub(crate) value: u16,
    pub(crate) max: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayerLedSettingsState {
    /// Palette color index for each firmware-supported Bluetooth profile
    pub(crate) bt_profile_colors: Vec<LayerLedColorSetting>,
    /// Palette color index for each firmware-supported logical layer
    pub(crate) layer_colors: Vec<LayerLedColorSetting>,
    /// Global LED brightness, clamped by firmware to the advertised max
    pub(crate) brightness: Option<LayerLedNumericSetting>,
    /// LED timeout, 0 disables timeout
    pub(crate) timeout: Option<LayerLedNumericSetting>,
    pub(crate) timeout_unit: LayerLedTimeoutUnit,
    /// Whether any Ergohaven LED QMK setting was readable (firmware support flag)
    pub(crate) supported: bool,
}

impl Default for LayerLedSettingsState {
    fn default() -> Self {
        Self {
            bt_profile_colors: Vec::new(),
            layer_colors: Vec::new(),
            brightness: None,
            timeout: None,
            timeout_unit: LayerLedTimeoutUnit::Minutes,
            supported: false,
        }
    }
}

pub(crate) const LAYER_LED_PALETTE: [&str; 25] = [
    "Off",
    "White",
    "Red",
    "Orange",
    "Goldenrod",
    "Gold",
    "Yellow",
    "Chartreuse",
    "Lime",
    "Green",
    "Spring Green",
    "Turquoise",
    "Teal",
    "Cyan",
    "Azure",
    "Sky",
    "Blue",
    "Indigo",
    "Purple",
    "Magenta",
    "Pink",
    "Coral",
    "Salmon",
    "Warm White",
    "Amber",
];

pub(crate) fn layer_led_palette_name(index: u8) -> &'static str {
    LAYER_LED_PALETTE
        .get(index as usize)
        .copied()
        .unwrap_or("Unknown")
}

pub(crate) const LAYER_LED_PALETTE_HSV: [(u8, u8, u8); 25] = [
    (0, 0, 0),
    (0, 0, 255),
    (0, 255, 255),
    (16, 255, 255),
    (27, 255, 255),
    (38, 255, 255),
    (53, 255, 255),
    (74, 255, 255),
    (90, 255, 255),
    (106, 255, 255),
    (117, 255, 255),
    (128, 255, 255),
    (138, 255, 170),
    (149, 255, 255),
    (160, 255, 255),
    (165, 255, 255),
    (170, 255, 255),
    (186, 255, 255),
    (202, 255, 255),
    (213, 255, 255),
    (234, 180, 255),
    (8, 176, 255),
    (14, 128, 255),
    (32, 64, 255),
    (22, 255, 255),
];

pub(crate) fn layer_led_palette_color(index: u8) -> Color32 {
    let (h, s, v) = LAYER_LED_PALETTE_HSV
        .get(index as usize)
        .copied()
        .unwrap_or((0, 0, 0));
    if v == 0 {
        Color32::from_rgb(18, 18, 20)
    } else {
        let pastel_s = (s as f32 / 255.0 * 0.68).clamp(0.0, 1.0);
        let pastel_v = (v as f32 / 255.0 * 0.82 + 0.12).clamp(0.0, 0.96);
        Color32::from(egui::ecolor::Hsva::new(
            h as f32 / 255.0,
            pastel_s,
            pastel_v,
            1.0,
        ))
    }
}

pub(crate) fn layer_led_outline_color(index: u8) -> Color32 {
    let (h, s, v) = LAYER_LED_PALETTE_HSV
        .get(index as usize)
        .copied()
        .unwrap_or((0, 0, 0));
    if v == 0 {
        Color32::from_rgb(18, 18, 20)
    } else {
        let pastel_s = (s as f32 / 255.0 * 0.26).clamp(0.0, 1.0);
        let pastel_v = (v as f32 / 255.0 * 0.48 + 0.22).clamp(0.0, 0.72);
        Color32::from(egui::ecolor::Hsva::new(
            h as f32 / 255.0,
            pastel_s,
            pastel_v,
            1.0,
        ))
    }
}

pub(crate) fn blend_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

pub(crate) fn layer_led_hover_fill(index: u8, dark: bool) -> Color32 {
    let (h, s, v) = LAYER_LED_PALETTE_HSV
        .get(index as usize)
        .copied()
        .unwrap_or((0, 0, 0));
    let base = crate::ui_style::hover_fill(dark);
    if v == 0 {
        base
    } else {
        let tint_s = (s as f32 / 255.0 * 0.22).clamp(0.0, 1.0);
        let tint_v = if dark { 0.36 } else { 0.92 };
        let tint = Color32::from(egui::ecolor::Hsva::new(
            h as f32 / 255.0,
            tint_s,
            tint_v,
            1.0,
        ));
        blend_color(base, tint, if dark { 0.62 } else { 0.52 })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RgbSupportKind {
    #[default]
    None,
    QmkRgblight,
    VialRgb,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RgbSettingsState {
    pub(crate) supported: bool,
    pub(crate) kind: RgbSupportKind,
    pub(crate) effect: u16,
    pub(crate) brightness: u8,
    pub(crate) speed: u8,
    pub(crate) hue: u8,
    pub(crate) saturation: u8,
    pub(crate) max_brightness: u8,
    pub(crate) supported_effects: Vec<u16>,
    pub(crate) last_enabled_effect: u16,
}

impl RgbSettingsState {
    pub(crate) fn is_enabled(&self) -> bool {
        self.supported && self.effect != 0
    }

    pub(crate) fn fallback_effect(&self) -> u16 {
        match self.kind {
            RgbSupportKind::QmkRgblight => 1,
            RgbSupportKind::VialRgb => 2,
            RgbSupportKind::None => 0,
        }
    }

    pub(crate) fn effect_or_default(&self) -> u16 {
        let candidate = if self.last_enabled_effect != 0 {
            self.last_enabled_effect
        } else {
            self.fallback_effect()
        };
        match self.kind {
            RgbSupportKind::VialRgb => {
                if self.supported_effects.is_empty() || self.supported_effects.contains(&candidate)
                {
                    candidate
                } else {
                    self.supported_effects.first().copied().unwrap_or(candidate)
                }
            }
            _ => candidate,
        }
    }
}

pub(crate) const QMK_RGBLIGHT_EFFECTS: &[(u16, &str)] = &[
    (0, "All Off"),
    (1, "Solid Color"),
    (2, "Breathing 1"),
    (3, "Breathing 2"),
    (4, "Breathing 3"),
    (5, "Breathing 4"),
    (6, "Rainbow Mood 1"),
    (7, "Rainbow Mood 2"),
    (8, "Rainbow Mood 3"),
    (9, "Rainbow Swirl 1"),
    (10, "Rainbow Swirl 2"),
    (11, "Rainbow Swirl 3"),
    (12, "Rainbow Swirl 4"),
    (13, "Rainbow Swirl 5"),
    (14, "Rainbow Swirl 6"),
    (15, "Snake 1"),
    (16, "Snake 2"),
    (17, "Snake 3"),
    (18, "Snake 4"),
    (19, "Snake 5"),
    (20, "Snake 6"),
    (21, "Knight 1"),
    (22, "Knight 2"),
    (23, "Knight 3"),
    (24, "Christmas"),
    (25, "Gradient 1"),
    (26, "Gradient 2"),
    (27, "Gradient 3"),
    (28, "Gradient 4"),
    (29, "Gradient 5"),
    (30, "Gradient 6"),
    (31, "Gradient 7"),
    (32, "Gradient 8"),
    (33, "Gradient 9"),
    (34, "Gradient 10"),
    (35, "RGB Test"),
    (36, "Alternating"),
];

pub(crate) const VIALRGB_EFFECTS: &[(u16, &str)] = &[
    (0, "Disable"),
    (1, "Direct Control"),
    (2, "Solid Color"),
    (3, "Alphas Mods"),
    (4, "Gradient Up Down"),
    (5, "Gradient Left Right"),
    (6, "Breathing"),
    (7, "Band Sat"),
    (8, "Band Val"),
    (9, "Band Pinwheel Sat"),
    (10, "Band Pinwheel Val"),
    (11, "Band Spiral Sat"),
    (12, "Band Spiral Val"),
    (13, "Cycle All"),
    (14, "Cycle Left Right"),
    (15, "Cycle Up Down"),
    (16, "Rainbow Moving Chevron"),
    (17, "Cycle Out In"),
    (18, "Cycle Out In Dual"),
    (19, "Cycle Pinwheel"),
    (20, "Cycle Spiral"),
    (21, "Dual Beacon"),
    (22, "Rainbow Beacon"),
    (23, "Rainbow Pinwheels"),
    (24, "Raindrops"),
    (25, "Jellybean Raindrops"),
    (26, "Hue Breathing"),
    (27, "Hue Pendulum"),
    (28, "Hue Wave"),
    (29, "Typing Heatmap"),
    (30, "Digital Rain"),
    (31, "Solid Reactive Simple"),
    (32, "Solid Reactive"),
    (33, "Solid Reactive Wide"),
    (34, "Solid Reactive Multiwide"),
    (35, "Solid Reactive Cross"),
    (36, "Solid Reactive Multicross"),
    (37, "Solid Reactive Nexus"),
    (38, "Solid Reactive Multinexus"),
    (39, "Splash"),
    (40, "Multisplash"),
    (41, "Solid Splash"),
    (42, "Solid Multisplash"),
    (43, "Pixel Rain"),
    (44, "Pixel Fractal"),
];

pub(crate) fn load_rgb_settings(
    dev_conn: &crate::hid::HidDevice,
    layout: &KeyboardLayout,
) -> RgbSettingsState {
    let mut candidates = Vec::new();
    match layout.lighting_mode.as_deref() {
        Some("vialrgb") => {
            candidates.extend([RgbSupportKind::VialRgb, RgbSupportKind::QmkRgblight])
        }
        Some("qmk_rgblight") | Some("qmk_backlight_rgblight") => {
            candidates.extend([RgbSupportKind::QmkRgblight, RgbSupportKind::VialRgb]);
        }
        _ => return RgbSettingsState::default(),
    }

    for kind in candidates {
        match kind {
            RgbSupportKind::VialRgb => {
                let Ok((version, max_brightness)) = dev_conn.get_vialrgb_info() else {
                    continue;
                };
                if version != 1 {
                    continue;
                }
                let Ok((effect, speed, hue, saturation, brightness)) = dev_conn.get_vialrgb_mode()
                else {
                    continue;
                };
                let mut supported_effects =
                    dev_conn.get_vialrgb_supported_effects().unwrap_or_default();
                if !supported_effects.contains(&0) {
                    supported_effects.insert(0, 0);
                }
                let mut state = RgbSettingsState {
                    supported: true,
                    kind,
                    effect,
                    brightness,
                    speed,
                    hue,
                    saturation,
                    max_brightness,
                    supported_effects,
                    last_enabled_effect: effect,
                };
                if state.last_enabled_effect == 0 {
                    state.last_enabled_effect = state.fallback_effect();
                }
                return state;
            }
            RgbSupportKind::QmkRgblight => {
                let Ok(brightness) = dev_conn.get_qmk_rgblight_brightness() else {
                    continue;
                };
                let Ok(effect) = dev_conn.get_qmk_rgblight_effect() else {
                    continue;
                };
                let speed = dev_conn.get_qmk_rgblight_effect_speed().unwrap_or(0);
                let (hue, saturation) = dev_conn.get_qmk_rgblight_color().unwrap_or((0, 0));
                let mut state = RgbSettingsState {
                    supported: true,
                    kind,
                    effect: effect as u16,
                    brightness,
                    speed,
                    hue,
                    saturation,
                    max_brightness: u8::MAX,
                    supported_effects: vec![],
                    last_enabled_effect: effect as u16,
                };
                if state.last_enabled_effect == 0 {
                    state.last_enabled_effect = state.fallback_effect();
                }
                return state;
            }
            RgbSupportKind::None => {}
        }
    }

    RgbSettingsState::default()
}

/// Returns true if the given Vial keycode is a QMK mouse key (0x00CD..=0x00DF).
pub(crate) fn is_mouse_keycode(kc: u16) -> bool {
    (0x00CD..=0x00DF).contains(&kc)
}

pub(crate) fn is_alt_repeat_keycode(kc: u16) -> bool {
    kc == 0x7C7A
}

#[derive(Clone, Debug)]
pub(super) enum UndoAction {
    Key {
        layer: usize,
        key_idx: usize,
        old_kc: u16,
    },
    Encoder {
        layer: usize,
        encoder_visual_idx: usize,
        old_kc: u16,
    },
    Layer {
        layer: usize,
        old: LayerSnapshot,
        requires_firmware: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyOverridePickField {
    Trigger,
    Replacement,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AltRepeatPickField {
    LastKey,
    AltKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainMenuTab {
    Keyboard,
    Advanced,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComboPickField {
    Trigger(usize),
    Output,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    AppSettings,
    MatrixTester,
    UniversalSymbolsSetup,
    TextExpander,
    TypingTrainer,
    AutoShift,
    Rgb,
    LayerLeds,
    Encoders,
    Magic,
    TapHold,
    GraveEscape,
    LayoutOptions,
    Modules,
    Touchpad,
    Bluetooth,
    LiveFeatures,
    AboutDevice,
    AboutEntropy,
    Combo,
    KeyOverrides,
    AltRepeat,
    MouseKeys,
    LayoutImageExport,
}

pub(crate) const TYPING_TRAINER_DURATIONS: [u32; 4] = [15, 30, 60, 120];
pub(crate) const TYPING_TRAINER_WORD_COUNTS: [usize; 4] = [10, 25, 50, 100];
pub(crate) const TYPING_TRAINER_HISTORY_LIMIT: usize = 20;
const TYPING_TRAINER_DEFAULT_TEXT_WORDS: usize = 72;
pub(crate) use super::typing_trainer_words::{TypingTrainerLanguage, TYPING_TRAINER_LANGUAGES};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TypingTrainerMode {
    Time,
    Words,
}

impl Default for TypingTrainerMode {
    fn default() -> Self {
        Self::Time
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypingTrainerSettings {
    #[serde(default)]
    pub(crate) language: TypingTrainerLanguage,
    #[serde(default)]
    pub(crate) mode: TypingTrainerMode,
    #[serde(default)]
    pub(crate) punctuation_enabled: bool,
    #[serde(default)]
    pub(crate) numbers_enabled: bool,
    #[serde(default = "default_typing_trainer_duration_secs")]
    pub(crate) duration_secs: u32,
    #[serde(default = "default_typing_trainer_word_count")]
    pub(crate) word_count: usize,
}

impl Default for TypingTrainerSettings {
    fn default() -> Self {
        Self {
            language: TypingTrainerLanguage::English,
            mode: TypingTrainerMode::Time,
            punctuation_enabled: false,
            numbers_enabled: false,
            duration_secs: default_typing_trainer_duration_secs(),
            word_count: default_typing_trainer_word_count(),
        }
    }
}

impl TypingTrainerSettings {
    pub(crate) fn normalized(self) -> Self {
        Self {
            duration_secs: if TYPING_TRAINER_DURATIONS.contains(&self.duration_secs) {
                self.duration_secs
            } else {
                default_typing_trainer_duration_secs()
            },
            word_count: if TYPING_TRAINER_WORD_COUNTS.contains(&self.word_count) {
                self.word_count
            } else {
                default_typing_trainer_word_count()
            },
            ..self
        }
    }
}

pub(crate) fn default_typing_trainer_duration_secs() -> u32 {
    30
}

pub(crate) fn default_typing_trainer_word_count() -> usize {
    25
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypingTrainerRunRecord {
    pub(crate) finished_at_unix_secs: i64,
    pub(crate) language: TypingTrainerLanguage,
    pub(crate) mode: TypingTrainerMode,
    pub(crate) duration_secs: u32,
    pub(crate) word_count: usize,
    pub(crate) punctuation_enabled: bool,
    pub(crate) numbers_enabled: bool,
    pub(crate) wpm: u32,
    pub(crate) accuracy_percent: u32,
    pub(crate) errors: usize,
    pub(crate) typed_chars: usize,
    pub(crate) elapsed_secs: u32,
}

impl TypingTrainerRunRecord {
    pub(crate) fn from_state(
        state: &TypingTrainerState,
        now: std::time::Instant,
        finished_at_unix_secs: i64,
    ) -> Option<Self> {
        if !state.is_finished() {
            return None;
        }

        let stats = state.stats_at(now);
        if stats.typed_chars == 0 {
            return None;
        }

        Some(Self {
            finished_at_unix_secs,
            language: state.language,
            mode: state.mode,
            duration_secs: state.duration_secs,
            word_count: state.word_count,
            punctuation_enabled: state.punctuation_enabled,
            numbers_enabled: state.numbers_enabled,
            wpm: stats.wpm,
            accuracy_percent: stats.accuracy.round().clamp(0.0, 100.0) as u32,
            errors: stats.errors,
            typed_chars: stats.typed_chars,
            elapsed_secs: state.elapsed_secs_at(now).ceil().max(0.0) as u32,
        })
    }

    fn matches_settings(&self, settings: TypingTrainerSettings) -> bool {
        self.language == settings.language
            && self.mode == settings.mode
            && self.punctuation_enabled == settings.punctuation_enabled
            && self.numbers_enabled == settings.numbers_enabled
            && match self.mode {
                TypingTrainerMode::Time => self.duration_secs == settings.duration_secs,
                TypingTrainerMode::Words => self.word_count == settings.word_count,
            }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TypingTrainerHistorySummary {
    pub(crate) run_count: usize,
    pub(crate) best_wpm: Option<u32>,
    pub(crate) average_wpm: Option<u32>,
    pub(crate) average_accuracy_percent: Option<u32>,
}

pub(crate) fn typing_trainer_history_summary_for_settings(
    history: &[TypingTrainerRunRecord],
    settings: TypingTrainerSettings,
) -> TypingTrainerHistorySummary {
    let settings = settings.normalized();
    let mut run_count = 0usize;
    let mut best_wpm = 0u32;
    let mut wpm_sum = 0u64;
    let mut accuracy_sum = 0u64;

    for record in history
        .iter()
        .filter(|record| record.matches_settings(settings))
    {
        run_count += 1;
        best_wpm = best_wpm.max(record.wpm);
        wpm_sum += u64::from(record.wpm);
        accuracy_sum += u64::from(record.accuracy_percent);
    }

    if run_count == 0 {
        return TypingTrainerHistorySummary::default();
    }

    let run_count_u64 = run_count as u64;
    TypingTrainerHistorySummary {
        run_count,
        best_wpm: Some(best_wpm),
        average_wpm: Some(((wpm_sum + run_count_u64 / 2) / run_count_u64) as u32),
        average_accuracy_percent: Some(((accuracy_sum + run_count_u64 / 2) / run_count_u64) as u32),
    }
}

pub(crate) fn push_typing_trainer_history(
    history: &mut Vec<TypingTrainerRunRecord>,
    record: TypingTrainerRunRecord,
) {
    history.insert(0, record);
    normalize_typing_trainer_history(history);
}

pub(crate) fn normalize_typing_trainer_history(history: &mut Vec<TypingTrainerRunRecord>) {
    history.truncate(TYPING_TRAINER_HISTORY_LIMIT);
}

#[derive(Clone)]
pub(crate) struct TypingTrainerState {
    pub(crate) target_text: String,
    pub(crate) typed_chars: Vec<char>,
    pub(crate) language: TypingTrainerLanguage,
    pub(crate) mode: TypingTrainerMode,
    pub(crate) punctuation_enabled: bool,
    pub(crate) numbers_enabled: bool,
    pub(crate) duration_secs: u32,
    pub(crate) word_count: usize,
    pub(crate) started_at: Option<std::time::Instant>,
    pub(crate) paused_at: Option<std::time::Instant>,
    pub(crate) finished_at: Option<std::time::Instant>,
    pub(crate) completed_correct_chars: usize,
    pub(crate) completed_errors: usize,
    pub(crate) completed_typed_chars: usize,
    pub(crate) run_seed: usize,
    pub(crate) text_seed: usize,
    pub(crate) history_recorded: bool,
    pub(crate) ui_hidden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TypingTrainerStats {
    pub(crate) wpm: u32,
    pub(crate) accuracy: f32,
    pub(crate) errors: usize,
    pub(crate) correct_chars: usize,
    pub(crate) typed_chars: usize,
}

impl Default for TypingTrainerState {
    fn default() -> Self {
        Self::from_settings(TypingTrainerSettings::default())
    }
}

impl TypingTrainerState {
    pub(crate) fn from_settings(settings: TypingTrainerSettings) -> Self {
        let settings = settings.normalized();
        let text_seed = 0;
        let target_text = match settings.mode {
            TypingTrainerMode::Time => typing_trainer_text_for_language(
                text_seed,
                settings.language,
                settings.punctuation_enabled,
                settings.numbers_enabled,
            ),
            TypingTrainerMode::Words => typing_trainer_text_for_word_count(
                text_seed,
                settings.word_count,
                settings.language,
                settings.punctuation_enabled,
                settings.numbers_enabled,
            ),
        };
        Self {
            target_text,
            typed_chars: Vec::new(),
            language: settings.language,
            mode: settings.mode,
            punctuation_enabled: settings.punctuation_enabled,
            numbers_enabled: settings.numbers_enabled,
            duration_secs: settings.duration_secs,
            word_count: settings.word_count,
            started_at: None,
            paused_at: None,
            finished_at: None,
            completed_correct_chars: 0,
            completed_errors: 0,
            completed_typed_chars: 0,
            run_seed: text_seed,
            text_seed,
            history_recorded: false,
            ui_hidden: false,
        }
    }

    pub(crate) fn settings(&self) -> TypingTrainerSettings {
        TypingTrainerSettings {
            language: self.language,
            mode: self.mode,
            punctuation_enabled: self.punctuation_enabled,
            numbers_enabled: self.numbers_enabled,
            duration_secs: self.duration_secs,
            word_count: self.word_count,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.start_run(self.text_seed.wrapping_add(17));
    }

    fn start_run(&mut self, text_seed: usize) {
        self.run_seed = text_seed;
        self.text_seed = text_seed;
        self.target_text = self.new_target_text();
        self.clear_progress();
    }

    pub(crate) fn retry(&mut self) {
        self.start_run(self.run_seed);
    }

    fn clear_progress(&mut self) {
        self.typed_chars.clear();
        self.started_at = None;
        self.paused_at = None;
        self.finished_at = None;
        self.completed_correct_chars = 0;
        self.completed_errors = 0;
        self.completed_typed_chars = 0;
        self.history_recorded = false;
        self.ui_hidden = false;
    }

    pub(crate) fn set_mode(&mut self, mode: TypingTrainerMode) {
        if self.mode != mode {
            self.mode = mode;
            self.reset();
        }
    }

    pub(crate) fn set_language(&mut self, language: TypingTrainerLanguage) {
        if self.language != language {
            self.language = language;
            self.reset();
        }
    }

    pub(crate) fn set_punctuation_enabled(&mut self, enabled: bool) {
        if self.punctuation_enabled != enabled {
            self.punctuation_enabled = enabled;
            self.reset();
        }
    }

    pub(crate) fn set_numbers_enabled(&mut self, enabled: bool) {
        if self.numbers_enabled != enabled {
            self.numbers_enabled = enabled;
            self.reset();
        }
    }

    pub(crate) fn set_duration(&mut self, duration_secs: u32) {
        if self.duration_secs != duration_secs {
            self.duration_secs = duration_secs;
            self.reset();
        }
    }

    pub(crate) fn set_word_count(&mut self, word_count: usize) {
        let word_count = word_count.max(1);
        if self.word_count != word_count {
            self.word_count = word_count;
            self.reset();
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished_at.is_some()
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    pub(crate) fn pause_if_running(&mut self, now: std::time::Instant) {
        if self.started_at.is_none() || self.finished_at.is_some() || self.paused_at.is_some() {
            return;
        }
        if self.mode == TypingTrainerMode::Time
            && self.elapsed_secs_at(now) >= self.duration_secs as f32
        {
            self.finished_at = Some(now);
        } else {
            self.paused_at = Some(now);
        }
    }

    pub(crate) fn resume_if_paused(&mut self, now: std::time::Instant) {
        let Some(paused_at) = self.paused_at.take() else {
            return;
        };
        if let Some(started_at) = self.started_at {
            self.started_at = Some(started_at + now.saturating_duration_since(paused_at));
        }
    }

    pub(crate) fn type_char(&mut self, ch: char, now: std::time::Instant) {
        if self.is_finished() || !typing_trainer_accepts_char(ch) {
            return;
        }
        self.resume_if_paused(now);
        if self.started_at.is_none() {
            self.started_at = Some(now);
        }
        if self.typed_chars.len() < self.target_text.chars().count() {
            self.typed_chars.push(ch);
        }
        if self.typed_chars.len() >= self.target_text.chars().count() {
            match self.mode {
                TypingTrainerMode::Time => self.advance_to_next_text(),
                TypingTrainerMode::Words => self.finish(now),
            }
        }
    }

    pub(crate) fn finish(&mut self, now: std::time::Instant) {
        if self.finished_at.is_some() {
            return;
        }
        self.resume_if_paused(now);
        self.started_at.get_or_insert(now);
        self.finished_at = Some(now);
        self.ui_hidden = false;
    }

    pub(crate) fn history_record_pending(&self) -> bool {
        self.finished_at.is_some() && !self.history_recorded
    }

    pub(crate) fn mark_history_recorded(&mut self) {
        self.history_recorded = true;
    }

    pub(crate) fn extend_target_text(&mut self) {
        if self.mode == TypingTrainerMode::Words {
            return;
        }
        self.text_seed = self.text_seed.wrapping_add(17);
        if !self.target_text.is_empty() {
            self.target_text.push(' ');
        }
        self.target_text.push_str(&typing_trainer_text_for_language(
            self.text_seed,
            self.language,
            self.punctuation_enabled,
            self.numbers_enabled,
        ));
    }

    pub(crate) fn backspace(&mut self) {
        if self.is_finished() {
            return;
        }
        self.typed_chars.pop();
    }

    pub(crate) fn elapsed_secs_at(&self, now: std::time::Instant) -> f32 {
        let Some(started_at) = self.started_at else {
            return 0.0;
        };
        let end = self.finished_at.or(self.paused_at).unwrap_or(now);
        end.saturating_duration_since(started_at).as_secs_f32()
    }

    pub(crate) fn remaining_secs_at(&mut self, now: std::time::Instant) -> u32 {
        if self.mode == TypingTrainerMode::Words {
            return self.duration_secs;
        }
        let elapsed = self.elapsed_secs_at(now);
        if self.started_at.is_some() && elapsed >= self.duration_secs as f32 {
            self.finished_at.get_or_insert(now);
            return 0;
        }
        (self.duration_secs as f32 - elapsed).ceil().max(0.0) as u32
    }

    pub(crate) fn stats_at(&self, now: std::time::Instant) -> TypingTrainerStats {
        let current_stats = typing_trainer_stats(
            &self.target_text,
            &self.typed_chars,
            self.elapsed_secs_at(now),
        );
        let typed_chars = self.completed_typed_chars + current_stats.typed_chars;
        let correct_chars = self.completed_correct_chars + current_stats.correct_chars;
        let errors = self.completed_errors + current_stats.errors;
        let accuracy = if typed_chars == 0 {
            100.0
        } else {
            correct_chars as f32 / typed_chars as f32 * 100.0
        };
        let minutes = (self.elapsed_secs_at(now) / 60.0).max(1.0 / 60.0);
        let wpm = ((correct_chars as f32 / 5.0) / minutes).round() as u32;

        TypingTrainerStats {
            wpm,
            accuracy,
            errors,
            correct_chars,
            typed_chars,
        }
    }

    pub(crate) fn word_progress(&self) -> (usize, usize) {
        (
            typing_trainer_completed_words(&self.target_text, self.typed_chars.len())
                .min(self.word_count),
            self.word_count,
        )
    }

    fn new_target_text(&self) -> String {
        match self.mode {
            TypingTrainerMode::Time => typing_trainer_text_for_language(
                self.text_seed,
                self.language,
                self.punctuation_enabled,
                self.numbers_enabled,
            ),
            TypingTrainerMode::Words => typing_trainer_text_for_word_count(
                self.text_seed,
                self.word_count,
                self.language,
                self.punctuation_enabled,
                self.numbers_enabled,
            ),
        }
    }

    fn advance_to_next_text(&mut self) {
        let stats = typing_trainer_stats(&self.target_text, &self.typed_chars, 0.0);
        self.completed_correct_chars += stats.correct_chars;
        self.completed_errors += stats.errors;
        self.completed_typed_chars += stats.typed_chars;
        self.text_seed = self.text_seed.wrapping_add(17);
        self.target_text = self.new_target_text();
        self.typed_chars.clear();
    }
}

fn typing_trainer_text_for_language(
    seed: usize,
    language: TypingTrainerLanguage,
    punctuation_enabled: bool,
    numbers_enabled: bool,
) -> String {
    typing_trainer_text_for_word_count(
        seed,
        TYPING_TRAINER_DEFAULT_TEXT_WORDS,
        language,
        punctuation_enabled,
        numbers_enabled,
    )
}

fn typing_trainer_text_for_word_count(
    seed: usize,
    word_count: usize,
    language: TypingTrainerLanguage,
    punctuation_enabled: bool,
    numbers_enabled: bool,
) -> String {
    let mut words = Vec::with_capacity(word_count);
    let len = super::typing_trainer_words::word_count(language);
    for i in 0..word_count {
        let mut word = if numbers_enabled && typing_trainer_should_insert_number(seed, i) {
            typing_trainer_number_token(seed, i)
        } else {
            let idx = seed.wrapping_add(i * 29).wrapping_add((i / 7) * 11) % len;
            super::typing_trainer_words::word_at(language, idx).to_owned()
        };
        if punctuation_enabled && typing_trainer_should_append_punctuation(seed, i, word_count) {
            word.push(typing_trainer_punctuation_mark(seed, i));
        }
        words.push(word);
    }
    words.join(" ")
}

fn typing_trainer_should_insert_number(seed: usize, idx: usize) -> bool {
    idx > 0 && idx % 8 == seed % 8
}

fn typing_trainer_number_token(seed: usize, idx: usize) -> String {
    let value = seed
        .wrapping_mul(37)
        .wrapping_add(idx.wrapping_mul(97))
        .wrapping_add((idx / 5).wrapping_mul(19));
    match seed.wrapping_add(idx) % 4 {
        0 => (value % 10).to_string(),
        1 => (10 + value % 90).to_string(),
        2 => (100 + value % 900).to_string(),
        _ => (1000 + value % 9000).to_string(),
    }
}

fn typing_trainer_should_append_punctuation(seed: usize, idx: usize, word_count: usize) -> bool {
    idx + 1 < word_count && seed.wrapping_add(idx * 17).wrapping_add(idx / 4) % 5 == 0
}

fn typing_trainer_punctuation_mark(seed: usize, idx: usize) -> char {
    const MARKS: [char; 8] = [',', '.', '.', '?', '!', ';', ':', ','];
    MARKS[seed.wrapping_add(idx * 7).wrapping_add(idx / 2) % MARKS.len()]
}

fn typing_trainer_completed_words(target_text: &str, typed_len: usize) -> usize {
    if typed_len == 0 {
        return 0;
    }
    let mut completed_words = 0;
    let mut in_word = false;
    let mut word_end = 0;
    for (idx, ch) in target_text.chars().enumerate() {
        if ch.is_whitespace() {
            if in_word && typed_len >= word_end {
                completed_words += 1;
            }
            in_word = false;
        } else {
            in_word = true;
            word_end = idx + 1;
        }
    }
    if in_word && typed_len >= word_end {
        completed_words += 1;
    }
    completed_words
}

pub(crate) fn typing_trainer_accepts_char(ch: char) -> bool {
    ch == ' '
        || ch.is_alphanumeric()
        || ch.is_ascii_punctuation()
        || matches!(ch, '«' | '»' | '—' | '–' | '…')
}

pub(crate) fn typing_trainer_stats(
    target_text: &str,
    typed_chars: &[char],
    elapsed_secs: f32,
) -> TypingTrainerStats {
    let mut correct_chars = 0;
    let mut errors = 0;
    for (typed, target) in typed_chars.iter().zip(target_text.chars()) {
        if *typed == target {
            correct_chars += 1;
        } else {
            errors += 1;
        }
    }
    errors += typed_chars
        .len()
        .saturating_sub(target_text.chars().count());

    let typed_count = typed_chars.len();
    let accuracy = if typed_count == 0 {
        100.0
    } else {
        correct_chars as f32 / typed_count as f32 * 100.0
    };
    let minutes = (elapsed_secs / 60.0).max(1.0 / 60.0);
    let wpm = ((correct_chars as f32 / 5.0) / minutes).round() as u32;

    TypingTrainerStats {
        wpm,
        accuracy,
        errors,
        correct_chars,
        typed_chars: typed_count,
    }
}

#[cfg(test)]
mod typing_trainer_tests {
    use super::*;

    #[test]
    fn typing_trainer_stats_count_wpm_accuracy_and_errors() {
        let typed: Vec<char> = "hello worx".chars().collect();
        let stats = typing_trainer_stats("hello world", &typed, 30.0);

        assert_eq!(stats.correct_chars, 9);
        assert_eq!(stats.typed_chars, 10);
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.wpm, 4);
        assert!((stats.accuracy - 90.0).abs() < f32::EPSILON);
    }

    #[test]
    fn typing_trainer_reset_generates_new_text_and_clears_progress() {
        let mut state = TypingTrainerState::default();
        let first_text = state.target_text.clone();
        state.typed_chars = "about".chars().collect();
        state.started_at = Some(std::time::Instant::now());
        state.paused_at = Some(std::time::Instant::now());
        state.completed_correct_chars = 20;
        state.completed_errors = 1;
        state.completed_typed_chars = 21;
        state.history_recorded = true;
        state.ui_hidden = true;

        state.reset();

        assert_ne!(state.target_text, first_text);
        assert!(state.typed_chars.is_empty());
        assert!(state.started_at.is_none());
        assert!(state.paused_at.is_none());
        assert!(state.finished_at.is_none());
        assert_eq!(state.completed_correct_chars, 0);
        assert_eq!(state.completed_errors, 0);
        assert_eq!(state.completed_typed_chars, 0);
        assert!(!state.history_recorded);
        assert!(!state.punctuation_enabled);
        assert!(!state.numbers_enabled);
        assert_eq!(state.language, TypingTrainerLanguage::English);
        assert_eq!(state.mode, TypingTrainerMode::Time);
        assert_eq!(state.word_count, 25);
        assert!(!state.ui_hidden);
    }

    #[test]
    fn typing_trainer_from_settings_uses_persisted_choices() {
        let state = TypingTrainerState::from_settings(TypingTrainerSettings {
            language: TypingTrainerLanguage::Russian,
            mode: TypingTrainerMode::Words,
            punctuation_enabled: true,
            numbers_enabled: true,
            duration_secs: 60,
            word_count: 50,
        });

        assert_eq!(state.language, TypingTrainerLanguage::Russian);
        assert_eq!(state.mode, TypingTrainerMode::Words);
        assert!(state.punctuation_enabled);
        assert!(state.numbers_enabled);
        assert_eq!(state.duration_secs, 60);
        assert_eq!(state.word_count, 50);
        assert_eq!(state.target_text.split_whitespace().count(), 50);
    }

    #[test]
    fn typing_trainer_retry_keeps_same_text_and_clears_progress() {
        let mut state = TypingTrainerState::default();
        let text = state.target_text.clone();
        let start = std::time::Instant::now();
        state.type_char('a', start);
        state.finish(start + std::time::Duration::from_secs(5));

        state.retry();

        assert_eq!(state.target_text, text);
        assert!(state.typed_chars.is_empty());
        assert!(state.started_at.is_none());
        assert!(state.finished_at.is_none());
    }

    #[test]
    fn typing_trainer_retry_rewinds_time_mode_sequence() {
        let mut state = TypingTrainerState::default();
        let first_text = state.target_text.clone();
        let now = std::time::Instant::now();

        for ch in first_text.chars() {
            state.type_char(ch, now);
        }
        assert_ne!(state.target_text, first_text);

        state.finish(now + std::time::Duration::from_secs(10));
        state.retry();

        assert_eq!(state.target_text, first_text);
        assert!(state.typed_chars.is_empty());
    }

    #[test]
    fn typing_trainer_finished_stats_are_stable() {
        let mut state = TypingTrainerState::default();
        let start = std::time::Instant::now();
        state.type_char('a', start);
        state.finish(start + std::time::Duration::from_secs(10));

        let finished_stats = state.stats_at(start + std::time::Duration::from_secs(10));
        let later_stats = state.stats_at(start + std::time::Duration::from_secs(60));

        assert_eq!(later_stats, finished_stats);
    }

    #[test]
    fn typing_trainer_finished_record_contains_run_result() {
        let mut state = TypingTrainerState::default();
        let start = std::time::Instant::now();
        state.type_char('a', start);
        state.finish(start + std::time::Duration::from_secs(10));

        let record = TypingTrainerRunRecord::from_state(
            &state,
            start + std::time::Duration::from_secs(20),
            1_700_000_000,
        )
        .expect("finished run should produce a history record");

        assert_eq!(record.finished_at_unix_secs, 1_700_000_000);
        assert_eq!(record.language, TypingTrainerLanguage::English);
        assert_eq!(record.mode, TypingTrainerMode::Time);
        assert_eq!(record.wpm, state.stats_at(start).wpm);
        assert_eq!(record.errors, state.stats_at(start).errors);
        assert_eq!(record.typed_chars, 1);
        assert_eq!(record.elapsed_secs, 10);
    }

    #[test]
    fn typing_trainer_empty_finished_run_has_no_history_record() {
        let mut state = TypingTrainerState::default();
        let now = std::time::Instant::now();

        state.finish(now);

        assert!(TypingTrainerRunRecord::from_state(&state, now, 1_700_000_000).is_none());
    }

    #[test]
    fn typing_trainer_history_keeps_newest_runs_under_limit() {
        let mut history = Vec::new();
        for idx in 0..(TYPING_TRAINER_HISTORY_LIMIT + 3) {
            push_typing_trainer_history(
                &mut history,
                TypingTrainerRunRecord {
                    finished_at_unix_secs: idx as i64,
                    language: TypingTrainerLanguage::English,
                    mode: TypingTrainerMode::Time,
                    duration_secs: 30,
                    word_count: 25,
                    punctuation_enabled: false,
                    numbers_enabled: false,
                    wpm: idx as u32,
                    accuracy_percent: 100,
                    errors: 0,
                    typed_chars: 10,
                    elapsed_secs: 30,
                },
            );
        }

        assert_eq!(history.len(), TYPING_TRAINER_HISTORY_LIMIT);
        assert_eq!(
            history.first().map(|record| record.finished_at_unix_secs),
            Some((TYPING_TRAINER_HISTORY_LIMIT + 2) as i64)
        );
        assert_eq!(
            history.last().map(|record| record.finished_at_unix_secs),
            Some(3)
        );
    }

    #[test]
    fn typing_trainer_history_summary_filters_current_settings() {
        let settings = TypingTrainerSettings {
            language: TypingTrainerLanguage::English,
            mode: TypingTrainerMode::Time,
            punctuation_enabled: true,
            numbers_enabled: false,
            duration_secs: 30,
            word_count: 25,
        };
        let history = vec![
            TypingTrainerRunRecord {
                finished_at_unix_secs: 1,
                language: TypingTrainerLanguage::English,
                mode: TypingTrainerMode::Time,
                duration_secs: 30,
                word_count: 25,
                punctuation_enabled: true,
                numbers_enabled: false,
                wpm: 42,
                accuracy_percent: 96,
                errors: 2,
                typed_chars: 100,
                elapsed_secs: 30,
            },
            TypingTrainerRunRecord {
                finished_at_unix_secs: 2,
                language: TypingTrainerLanguage::English,
                mode: TypingTrainerMode::Time,
                duration_secs: 30,
                word_count: 25,
                punctuation_enabled: true,
                numbers_enabled: false,
                wpm: 50,
                accuracy_percent: 98,
                errors: 1,
                typed_chars: 120,
                elapsed_secs: 30,
            },
            TypingTrainerRunRecord {
                finished_at_unix_secs: 3,
                language: TypingTrainerLanguage::English,
                mode: TypingTrainerMode::Time,
                duration_secs: 60,
                word_count: 25,
                punctuation_enabled: true,
                numbers_enabled: false,
                wpm: 80,
                accuracy_percent: 100,
                errors: 0,
                typed_chars: 250,
                elapsed_secs: 60,
            },
        ];

        let summary = typing_trainer_history_summary_for_settings(&history, settings);

        assert_eq!(
            summary,
            TypingTrainerHistorySummary {
                run_count: 2,
                best_wpm: Some(50),
                average_wpm: Some(46),
                average_accuracy_percent: Some(97),
            }
        );
    }

    #[test]
    fn typing_trainer_generates_next_text_when_current_text_is_complete() {
        let mut state = TypingTrainerState::default();
        let first_text = state.target_text.clone();
        let now = std::time::Instant::now();

        for ch in first_text.chars() {
            state.type_char(ch, now);
        }

        assert_ne!(state.target_text, first_text);
        assert!(state.typed_chars.is_empty());
        assert!(state.finished_at.is_none());
        assert_eq!(state.completed_correct_chars, first_text.chars().count());
        assert_eq!(state.completed_errors, 0);
        assert_eq!(state.completed_typed_chars, first_text.chars().count());
    }

    #[test]
    fn typing_trainer_extends_target_text_without_clearing_progress() {
        let mut state = TypingTrainerState::default();
        let first_text = state.target_text.clone();
        state.typed_chars = "about".chars().collect();

        state.extend_target_text();

        assert!(state.target_text.starts_with(&format!("{first_text} ")));
        assert!(state.target_text.chars().count() > first_text.chars().count());
        assert_eq!(state.typed_chars, "about".chars().collect::<Vec<_>>());
        assert!(state.started_at.is_none());
        assert!(state.finished_at.is_none());
    }

    #[test]
    fn typing_trainer_pause_freezes_elapsed_until_typing_resumes() {
        let mut state = TypingTrainerState::default();
        let start = std::time::Instant::now();
        let pause_at = start + std::time::Duration::from_secs(5);
        let resume_at = start + std::time::Duration::from_secs(20);

        state.type_char('a', start);
        state.pause_if_running(pause_at);

        assert!(state.is_paused());
        assert_eq!(state.elapsed_secs_at(resume_at), 5.0);
        assert_eq!(state.remaining_secs_at(resume_at), state.duration_secs - 5);

        state.type_char('b', resume_at);

        assert!(!state.is_paused());
        assert_eq!(state.elapsed_secs_at(resume_at), 5.0);
        assert_eq!(
            state.elapsed_secs_at(resume_at + std::time::Duration::from_secs(1)),
            6.0
        );
    }

    #[test]
    fn typing_trainer_pause_can_finish_if_time_already_expired() {
        let mut state = TypingTrainerState::default();
        let start = std::time::Instant::now();

        state.type_char('a', start);
        state.pause_if_running(start + std::time::Duration::from_secs(31));

        assert!(state.is_finished());
        assert!(!state.is_paused());
    }

    #[test]
    fn typing_trainer_word_mode_finishes_after_selected_words() {
        let mut state = TypingTrainerState::default();
        state.set_mode(TypingTrainerMode::Words);
        state.set_word_count(10);
        let target_text = state.target_text.clone();
        let now = std::time::Instant::now();

        assert_eq!(target_text.split_whitespace().count(), 10);

        for ch in target_text.chars() {
            state.type_char(ch, now);
        }

        assert!(state.is_finished());
        assert_eq!(state.target_text, target_text);
        assert_eq!(state.typed_chars.len(), target_text.chars().count());
        assert_eq!(state.stats_at(now).typed_chars, target_text.chars().count());
        assert_eq!(state.word_progress(), (10, 10));
    }

    #[test]
    fn typing_trainer_language_changes_target_text() {
        let mut state = TypingTrainerState::default();

        state.set_language(TypingTrainerLanguage::Russian);

        assert_eq!(state.language, TypingTrainerLanguage::Russian);
        assert!(state
            .target_text
            .chars()
            .any(|ch| ('а'..='я').contains(&ch)));
        assert!(!state.target_text.chars().any(|ch| ch.is_ascii_alphabetic()));
    }

    #[test]
    fn typing_trainer_accepts_russian_letters() {
        assert!(typing_trainer_accepts_char('ф'));
        assert!(typing_trainer_accepts_char('Я'));
        assert!(typing_trainer_accepts_char(' '));
        assert!(typing_trainer_accepts_char('.'));
    }

    #[test]
    fn typing_trainer_punctuation_option_adds_punctuation() {
        let text =
            typing_trainer_text_for_word_count(0, 30, TypingTrainerLanguage::English, true, false);

        assert!(text.chars().any(|ch| ch.is_ascii_punctuation()));
        assert!(!text.chars().any(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn typing_trainer_numbers_option_adds_numbers() {
        let text =
            typing_trainer_text_for_word_count(0, 30, TypingTrainerLanguage::English, false, true);

        assert!(text.chars().any(|ch| ch.is_ascii_digit()));
        assert!(!text.chars().any(|ch| ch.is_ascii_punctuation()));
    }

    #[test]
    fn typing_trainer_modifier_toggles_regenerate_text() {
        let mut state = TypingTrainerState::default();

        state.set_punctuation_enabled(true);
        assert!(state.punctuation_enabled);
        assert!(state
            .target_text
            .chars()
            .any(|ch| ch.is_ascii_punctuation()));

        state.set_numbers_enabled(true);
        assert!(state.numbers_enabled);
        assert!(state.target_text.chars().any(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn typing_trainer_word_mode_does_not_finish_from_timer() {
        let mut state = TypingTrainerState::default();
        state.set_mode(TypingTrainerMode::Words);
        let start = std::time::Instant::now();

        state.type_char('a', start);
        assert_eq!(
            state.remaining_secs_at(start + std::time::Duration::from_secs(120)),
            state.duration_secs
        );

        assert!(!state.is_finished());
    }

    #[test]
    fn typing_trainer_finish_keeps_partial_result() {
        let mut state = TypingTrainerState::default();
        let start = std::time::Instant::now();

        state.type_char('a', start);
        state.ui_hidden = true;
        state.finish(start + std::time::Duration::from_secs(5));

        assert!(state.is_finished());
        assert!(!state.ui_hidden);
        assert_eq!(
            state.elapsed_secs_at(start + std::time::Duration::from_secs(30)),
            5.0
        );
        assert_eq!(
            state
                .stats_at(start + std::time::Duration::from_secs(30))
                .typed_chars,
            1
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum LayoutImageExportTheme {
    #[default]
    Current,
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum LayoutImageExportFormat {
    #[default]
    Png,
    Svg,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct LayoutImageExportState {
    #[serde(default)]
    pub(crate) format: LayoutImageExportFormat,
    #[serde(default)]
    pub(crate) theme: LayoutImageExportTheme,
    #[serde(default)]
    pub(crate) key_legend_layout: KeyLegendLayout,
    #[serde(default = "default_layout_image_export_show_layer_names")]
    pub(crate) show_layer_names: bool,
    #[serde(default)]
    pub(crate) selected_layers: Vec<bool>,
}

fn default_layout_image_export_show_layer_names() -> bool {
    true
}

impl Default for LayoutImageExportState {
    fn default() -> Self {
        Self {
            format: LayoutImageExportFormat::Png,
            theme: LayoutImageExportTheme::Current,
            key_legend_layout: KeyLegendLayout::default(),
            show_layer_names: true,
            selected_layers: Vec::new(),
        }
    }
}

pub(crate) const LAYOUT_BASE_UNIT: f32 = 54.0_f32 * 1.15;
pub(crate) const LAYOUT_KEY_PADDING: f32 = 2.5_f32;
pub(crate) const LAYOUT_FIT_MARGIN: f32 = 40.0_f32;
pub(crate) const LAYOUT_ENCODER_RADIUS_FACTOR: f32 = 0.47_f32;
pub(crate) const LAYOUT_ENCODER_FILL_EXTRA: f32 = 1.0_f32;
pub(crate) const LAYOUT_TOP_RESERVED_H: f32 = 32.0_f32 + 4.0_f32 + 68.0_f32;
pub(crate) const LAYOUT_BOTTOM_RESERVED_H: f32 = 76.0_f32;

pub struct EntropyApp {
    pub(crate) device_manager: DeviceManager,
    pub(crate) selected_device: Option<usize>,
    pub(crate) selected_layer: usize,
    pub(crate) selected_key: Option<(usize, usize)>,
    pub(crate) selected_encoder: Option<(usize, usize)>,
    pub(crate) layout: Option<KeyboardLayout>,
    pub(crate) layer_count: usize,
    pub(crate) keycode_picker: KeycodePicker,
    pub(crate) status_msg: String,
    pub(crate) import_report_open: bool,
    pub(crate) import_report_title: String,
    pub(crate) import_report_body: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) pending_entlayout_import_path: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) pending_entsettings_import_path: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) import_progress_started_at: Option<f64>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) import_progress_title: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) import_progress_body: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) connect_state: ConnectState,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) device_scan_state: DeviceScanState,
    /// Persistent open HID device for real-time writes (Vial)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) hid_device: Option<crate::hid::HidDevice>,
    /// Background whole-layer HID write. Owns the device handle while active.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) layer_write_task: Option<LayerWriteTask>,
    /// Background combo HID write. Owns the device handle while active.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) combo_write_task: Option<ComboWriteTask>,
    /// Built-in qmk-hid-host bridges for displays/presets that need host data
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) qmk_hid_hosts:
        std::collections::HashMap<String, crate::qmk_hid_host::QmkHidHostBridge>,
    /// Current firmware type (mirrors layout.firmware)
    pub(crate) firmware: FirmwareProtocol,
    /// Undo stack for key, encoder, and whole-layer assignments
    pub(super) undo_stack: Vec<UndoAction>,
    /// In-memory whole-layer clipboard. Kept across device reconnects so keyboards
    /// with compatible geometry can exchange layers during one Entropy session.
    pub(super) layer_clipboard: Option<LayerClipboard>,
    /// Frame counter for periodic device scan
    pub(crate) scan_frame: u32,
    /// Last device scan timestamp in egui seconds
    pub(crate) last_device_scan_at: f64,
    /// Layer to preview on hover (None = show selected_layer)
    pub(crate) hover_layer: Option<usize>,
    /// Last main keyboard layout geometry: offset_x, offset_y, unit, padding
    pub(crate) last_layout_geometry: Option<(f32, f32, f32, f32)>,
    /// Key index hovered in previous frame (for hint display)
    pub(crate) prev_hovered_key: Option<usize>,
    pub(crate) prev_hovered_encoder: bool,
    pub(crate) prev_hovered_encoder_keycode: Option<u16>,
    /// Set when secondary click was handled by a key (prevents global jump-back)
    pub(crate) secondary_click_handled: bool,
    /// Deferred left/right modifier swap, applied after Ctrl is released
    pub(crate) pending_handed_swap: Option<(usize, usize, u16)>,
    /// Animation progress for hover layer preview (0.0 = hidden, 1.0 = fully shown)
    pub(crate) hover_layer_progress: f32,
    /// Stack of layers to return to on right-click (last = most recent)
    pub(crate) jump_back_stack: Vec<usize>,
    pub(crate) dark_mode: bool,
    pub(crate) last_applied_theme: Option<(bool, AppAccentColor)>,
    pub(crate) app_settings: AppSettings,
    pub(crate) text_expander_rules_signature: Vec<(String, Option<std::time::SystemTime>)>,
    pub(crate) text_expander_rules_last_check_at: f64,
    pub(crate) text_expander_settings_save_pending: bool,
    pub(crate) text_expander_settings_last_edit_at: f64,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) tray_icon: Option<tray_icon::TrayIcon>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_hwnd: Option<isize>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_window_hidden_to_tray: bool,
    #[cfg(target_os = "macos")]
    pub(crate) macos_ns_window: Option<usize>,
    #[cfg(target_os = "macos")]
    pub(crate) macos_window_hidden_to_menu_bar: bool,
    #[cfg(target_os = "macos")]
    pub(crate) macos_app_was_hidden: bool,
    #[cfg(target_os = "macos")]
    pub(crate) macos_hidden_to_menu_bar_at: Option<std::time::Instant>,
    pub(crate) close_to_tray_prompt_open: bool,
    pub(crate) close_to_tray_prompt_remember: bool,
    pub(crate) force_close_requested: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) exit_after_hid_write: bool,
    pub(crate) main_menu_tab: MainMenuTab,
    pub(crate) combo_entries: Vec<ComboEntry>,
    pub(crate) combo_synced_entries: Vec<ComboEntry>,
    pub(crate) combo_names: Vec<String>,
    pub(crate) combo_colors: Vec<u32>,
    pub(crate) selected_combo: usize,
    pub(crate) combo_dirty: bool,
    pub(crate) combo_edit_revision: u64,
    pub(crate) combo_attempted_revision: Option<u64>,
    pub(crate) combo_names_dirty: bool,
    pub(crate) combo_colors_dirty: bool,
    pub(crate) combo_term: Option<u16>,
    pub(crate) auto_shift_options: AutoShiftOptionsState,
    pub(crate) auto_shift_timeout: Option<u16>,
    pub(crate) auto_shift_timeout_text: String,
    pub(crate) mouse_keys_settings: MouseKeysSettingsState,
    pub(crate) touchpad_settings: TouchpadSettingsState,
    pub(crate) bluetooth_settings: BluetoothSettingsState,
    pub(crate) module_settings: ModuleSettingsState,
    pub(crate) tap_hold_settings: TapHoldSettingsState,
    pub(crate) pending_tap_hold_numeric_writes: std::collections::BTreeMap<u16, u16>,
    pub(crate) tap_hold_numeric_write_due: Option<std::time::Instant>,
    pub(crate) magic_settings: MagicSettingsState,
    pub(crate) one_shot_settings: OneShotSettingsState,
    pub(crate) grave_escape_settings: GraveEscapeSettingsState,
    pub(crate) layer_led_settings: LayerLedSettingsState,
    pub(crate) alt_repeat_entries: Vec<AltRepeatKeyEntry>,
    pub(crate) alt_repeat_names: Vec<String>,
    pub(crate) alt_repeat_undo_stack: Vec<(Vec<AltRepeatKeyEntry>, Vec<String>, usize)>,
    pub(crate) selected_alt_repeat: usize,
    pub(crate) alt_repeat_visible_count: usize,
    pub(crate) alt_repeat_pick_target: Option<AltRepeatPickField>,
    pub(crate) last_single_instance_signal: String,
    pub(crate) rgb_settings: RgbSettingsState,
    pub(crate) layout_options_value: Option<u32>,
    pub(crate) encoder_visibility: Vec<bool>,
    pub(crate) combo_term_dirty: bool,
    pub(crate) combo_visible_count: usize,
    pub(crate) combo_undo_stack: Vec<ComboUndoSnapshot>,
    pub(crate) combo_pick_target: Option<(usize, ComboPickField)>,
    pub(crate) key_override_entries: Vec<KeyOverrideEntry>,
    pub(crate) key_override_names: Vec<String>,
    pub(crate) key_override_visible_count: usize,
    pub(crate) key_override_undo_stack: Vec<(Vec<KeyOverrideEntry>, Vec<String>, usize, usize)>,
    pub(crate) text_expander_deleted_rules: Vec<(usize, crate::text_expander::TextExpansionRule)>,
    pub(crate) typing_trainer: TypingTrainerState,
    pub(crate) typing_trainer_history_open: bool,
    pub(crate) selected_key_override: usize,
    pub(crate) key_override_pick_target: Option<KeyOverridePickField>,
    pub(crate) matrix_tester_pressed: Vec<bool>,
    pub(crate) matrix_tester_ever_pressed: Vec<bool>,
    pub(crate) matrix_tester_rmk_byte_order: bool,
    pub(crate) sticky_layout_prev_pressed: Vec<bool>,
    pub(crate) sticky_layout_pressed_key_layers: Vec<Option<usize>>,
    pub(crate) sticky_layout_toggled_layers: Vec<bool>,
    pub(crate) sticky_layout_base_layer: usize,
    pub(crate) sticky_layout_last_size: Option<Vec2>,
    pub(crate) sticky_layout_resize_opacity_hold_frames: u8,
    pub(crate) pending_layout_indicator_open_after_unlock: bool,
    pub(crate) matrix_tester_last_poll: std::time::Instant,
    pub(crate) matrix_tester_last_lock_check: std::time::Instant,
    pub(crate) matrix_tester_unlock_prompted: bool,
    pub(crate) matrix_tester_lock_checked: bool,
    pub(crate) macro_auto_unlock_cancelled: bool,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) layer_names: Vec<String>,
    pub(crate) editing_layer: Option<usize>, // layer being renamed
    pub(crate) editing_layer_text: String,
    pub(crate) editing_layer_focus_requested: bool,
    /// Current connected device name (for per-device layer names)
    pub(crate) current_device_name: String,
    /// Stable Vial keyboard id for the current firmware definition, when available.
    pub(crate) current_keyboard_id: Option<u64>,
    /// Stable local settings key for encoder visibility. Uses Vial keyboard id when available
    /// so keyboards with the same display name do not share hidden/shown encoder settings.
    pub(crate) current_encoder_visibility_id: String,
    /// Friendly names learned from firmware/device info, keyed by device path.
    pub(crate) device_display_names: std::collections::HashMap<String, String>,
    pub(crate) device_about_info: Option<DeviceAboutInfo>,
    pub(crate) update_check: UpdateCheckState,
    pub(crate) tour_state: TourState,
    pub(crate) tour_target_rects: Vec<(TourTarget, egui::Rect)>,
    /// Vial unlock dialog open
    pub(crate) unlock_open: bool,
    pub(crate) vial_unlock_keys: Vec<(u8, u8)>,
    pub(crate) vial_unlock_polling: bool,
    pub(crate) vial_unlock_counter: u8,
    pub(crate) vial_unlock_best: u8,
    pub(crate) vial_unlock_total: u8,
    pub(crate) vial_unlock_last_poll: Option<std::time::Instant>,
    pub(crate) vial_unlock_animation_nonce: u64,
}

#[cfg(test)]
mod module_settings_state_tests {
    use super::*;

    #[test]
    fn verified_module_setting_write_commits_read_back_value() {
        let mut settings = ModuleSettingsState::default();
        settings.set_value(42, 7);

        let result = settings.write_verified_value(42, 9, || Ok(()), || Ok(9));

        assert_eq!(result, Ok(9));
        assert_eq!(settings.value(42), 9);
    }

    #[test]
    fn verified_module_setting_write_retries_stale_readback() {
        let mut settings = ModuleSettingsState::default();
        settings.set_value(42, 7);
        let mut attempts = 0;

        let result = settings.write_verified_value(
            42,
            9,
            || Ok(()),
            || {
                attempts += 1;
                Ok(if attempts == 1 { 7 } else { 9 })
            },
        );

        assert_eq!(result, Ok(9));
        assert_eq!(attempts, 2);
        assert_eq!(settings.value(42), 9);
    }

    #[test]
    fn verified_module_setting_write_keeps_old_value_when_set_fails() {
        let mut settings = ModuleSettingsState::default();
        settings.set_value(42, 7);

        let result =
            settings.write_verified_value(42, 9, || Err("device offline".to_owned()), || Ok(9));

        assert_eq!(
            result,
            Err(ModuleSettingWritebackError::SetFailed(
                "device offline".to_owned()
            ))
        );
        assert_eq!(settings.value(42), 7);
    }

    #[test]
    fn verified_module_setting_write_keeps_old_value_when_readback_fails() {
        let mut settings = ModuleSettingsState::default();
        settings.set_value(42, 7);
        let mut attempts = 0;

        let result = settings.write_verified_value(
            42,
            9,
            || Ok(()),
            || {
                attempts += 1;
                Err("read failed".to_owned())
            },
        );

        assert_eq!(
            result,
            Err(ModuleSettingWritebackError::ReadbackFailed(
                "read failed".to_owned()
            ))
        );
        assert_eq!(attempts, 1);
        assert_eq!(settings.value(42), 7);
    }

    #[test]
    fn verified_module_setting_write_keeps_old_value_when_readback_mismatches() {
        let mut settings = ModuleSettingsState::default();
        settings.set_value(42, 7);
        let mut attempts = 0;

        let result = settings.write_verified_value(
            42,
            9,
            || Ok(()),
            || {
                attempts += 1;
                Ok(8)
            },
        );

        assert_eq!(
            result,
            Err(ModuleSettingWritebackError::ReadbackMismatch {
                expected: 9,
                actual: 8
            })
        );
        assert_eq!(attempts, MODULE_SETTING_READBACK_ATTEMPTS);
        assert_eq!(settings.value(42), 7);
    }
}
