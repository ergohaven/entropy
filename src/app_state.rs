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
        }
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
    tap_dance_names: &[String],
) -> String {
    if (0x7700..=0x77FF).contains(&value) {
        let idx = (value - 0x7700) as usize;
        let name = macro_display_name(macro_names, idx);
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

#[derive(Debug, Clone, Default)]
pub(crate) struct DeviceAboutInfo {
    pub(crate) manufacturer: String,
    pub(crate) product: String,
    pub(crate) vendor_id: u16,
    pub(crate) product_id: u16,
    pub(crate) path: String,
    pub(crate) firmware_version: Option<String>,
    pub(crate) via_protocol: u16,
    pub(crate) vial_protocol: u32,
    pub(crate) keyboard_id: u64,
    pub(crate) macro_entries: usize,
    pub(crate) macro_memory_bytes: Option<u16>,
    pub(crate) supports_macro_delays: bool,
    pub(crate) supports_macro_ext_keycodes: bool,
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

#[derive(Clone, Debug, Default)]
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
    /// Whether qsid 7 was readable (firmware support flag)
    pub(crate) supported: bool,
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
pub(crate) enum UndoAction {
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
    Macros,
    TextExpander,
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
    /// Built-in qmk-hid-host bridges for displays/presets that need host data
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) qmk_hid_hosts:
        std::collections::HashMap<String, crate::qmk_hid_host::QmkHidHostBridge>,
    /// Current firmware type (mirrors layout.firmware)
    pub(crate) firmware: FirmwareProtocol,
    /// Undo stack for key and encoder assignments
    pub(crate) undo_stack: Vec<UndoAction>,
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
    pub(crate) app_settings: AppSettings,
    pub(crate) text_expander_rules_signature: Vec<(String, Option<std::time::SystemTime>)>,
    pub(crate) text_expander_rules_last_check_at: f64,
    pub(crate) text_expander_settings_save_pending: bool,
    pub(crate) text_expander_settings_last_edit_at: f64,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) tray_icon: Option<tray_icon::TrayIcon>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_hwnd: Option<isize>,
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
    pub(crate) main_menu_tab: MainMenuTab,
    pub(crate) combo_entries: Vec<ComboEntry>,
    pub(crate) combo_names: Vec<String>,
    pub(crate) combo_colors: Vec<u32>,
    pub(crate) selected_combo: usize,
    pub(crate) combo_dirty: bool,
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
