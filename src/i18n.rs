#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    English,
    Russian,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::Russian];

    pub fn native_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Russian => "Русский",
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        default_language()
    }
}

pub fn default_language() -> Language {
    system_language()
}

fn system_language() -> Language {
    system_locale_tag()
        .as_deref()
        .and_then(language_from_locale_tag)
        .unwrap_or(Language::English)
}

fn language_from_locale_tag(locale: &str) -> Option<Language> {
    let normalized = locale.trim().to_ascii_lowercase().replace('_', "-");
    let language = normalized.split(['-', '.', '@']).next().unwrap_or("");
    match language {
        "ru" => Some(Language::Russian),
        "en" => Some(Language::English),
        _ => None,
    }
}

fn system_locale_tag() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        windows_user_locale_tag().or_else(env_locale_tag)
    }
    #[cfg(not(target_os = "windows"))]
    {
        env_locale_tag()
    }
}

fn env_locale_tag() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
        .find(|value| {
            let value = value.trim();
            !value.is_empty() && value != "C" && value != "POSIX"
        })
}

#[cfg(target_os = "windows")]
fn windows_user_locale_tag() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if len <= 1 {
        return None;
    }
    String::from_utf16(&buffer[..len as usize - 1]).ok()
}

const EN_CATALOG: &str = include_str!("../i18n/en.toml");
const RU_CATALOG: &str = include_str!("../i18n/ru.toml");

pub fn tr_catalog(language: Language, key: &'static str) -> &'static str {
    let lookup_key = static_catalog_key(key).unwrap_or(key);
    let translated = match language {
        Language::English => catalog_lookup(EN_CATALOG, lookup_key),
        Language::Russian => catalog_lookup(RU_CATALOG, lookup_key),
    };

    translated
        .or_else(|| catalog_lookup(EN_CATALOG, lookup_key))
        .unwrap_or(key)
}

pub fn tr_catalog_format(language: Language, key: &'static str, vars: &[(&str, &str)]) -> String {
    let mut text = tr_catalog(language, key).to_owned();
    for (name, value) in vars {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

fn tr_catalog_string_format(
    language: Language,
    key: &'static str,
    vars: &[(&str, &str)],
) -> String {
    let mut text = tr_catalog_string(language, key);
    for (name, value) in vars {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

fn tr_catalog_string(language: Language, key: &'static str) -> String {
    let lookup_key = static_catalog_key(key).unwrap_or(key);
    let translated = match language {
        Language::English => catalog_lookup_owned(EN_CATALOG, lookup_key),
        Language::Russian => catalog_lookup_owned(RU_CATALOG, lookup_key),
    };

    translated
        .or_else(|| catalog_lookup_owned(EN_CATALOG, lookup_key))
        .unwrap_or_else(|| key.to_owned())
}

fn catalog_lookup(catalog: &'static str, key: &str) -> Option<&'static str> {
    let (wanted_section, wanted_name) = key.rsplit_once('.')?;
    let mut section = "";

    for raw_line in catalog.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim();
            continue;
        }

        if section != wanted_section {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != wanted_name {
            continue;
        }

        let value = value.trim();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            return Some(&value[1..value.len() - 1]);
        }
    }

    None
}

fn catalog_lookup_owned(catalog: &'static str, key: &str) -> Option<String> {
    let (wanted_section, wanted_name) = key.rsplit_once('.')?;
    let mut section = "";

    for raw_line in catalog.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim();
            continue;
        }

        if section != wanted_section {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != wanted_name {
            continue;
        }

        return parse_catalog_value(value.trim());
    }

    None
}

fn parse_catalog_value(value: &str) -> Option<String> {
    if value.len() < 2 {
        return None;
    }

    if value.starts_with('"') && value.ends_with('"') {
        let inner = &value[1..value.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }

            match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                other => out.push(other),
            }
        }
        return Some(out);
    }

    if value.starts_with('\'') && value.ends_with('\'') {
        return Some(value[1..value.len() - 1].to_owned());
    }

    None
}

fn ru_smart_symbol_name(name: &str) -> &str {
    match name {
        "Left brace" => "левая фигурная скобка",
        "Right brace" => "правая фигурная скобка",
        "Left bracket" => "левая квадратная скобка",
        "Right bracket" => "правая квадратная скобка",
        "Left parenthesis" => "левая круглая скобка",
        "Right parenthesis" => "правая круглая скобка",
        "Less-than" => "знак «меньше»",
        "Greater-than" => "знак «больше»",
        "Number sign" => "решётка",
        "At sign" => "собака",
        "Numero sign" => "знак номера",
        "Ruble sign" => "знак рубля",
        "Exclamation mark" => "восклицательный знак",
        "Quotation mark" => "двойная кавычка",
        "Dollar sign" => "знак доллара",
        "Percent sign" => "знак процента",
        "Ampersand" => "амперсанд",
        "Apostrophe" => "апостроф",
        "Asterisk" => "звёздочка",
        "Plus sign" => "плюс",
        "Equals sign" => "знак равенства",
        "Question mark" => "вопросительный знак",
        "Vertical bar" => "вертикальная черта",
        "Backslash" => "обратный слэш",
        "Left guillemet" => "левая ёлочка",
        "Right guillemet" => "правая ёлочка",
        "Euro sign" => "знак евро",
        "Em dash" => "длинное тире",
        "En dash" => "среднее тире",
        "Bullet" => "маркер списка",
        "Multiplication sign" => "знак умножения",
        "Plus-minus sign" => "плюс-минус",
        "Not equal sign" => "знак неравенства",
        "Almost equal sign" => "знак примерного равенства",
        "Check mark" => "галочка",
        "Section sign" => "знак параграфа",
        "Full stop" => "точка",
        "Comma" => "запятая",
        "Semicolon" => "точка с запятой",
        "Colon" => "двоеточие",
        "Slash" => "слэш",
        "Grave accent" => "гравис",
        "Caret" => "карет",
        "Cyrillic be" | "Cyrillic Be" => "кириллическая Б",
        "Cyrillic yu" | "Cyrillic Yu" => "кириллическая Ю",
        "Cyrillic zhe" | "Cyrillic Zhe" => "кириллическая Ж",
        "Cyrillic e" | "Cyrillic E" => "кириллическая Э",
        "Cyrillic ha" | "Cyrillic Ha" => "кириллическая Х",
        "Cyrillic hard sign" | "Cyrillic Hard Sign" => "кириллический твёрдый знак",
        "Cyrillic yo" | "Cyrillic Yo" => "кириллическая Ё",
        "Degree sign" => "знак градуса",
        "Per mille sign" => "промилле",
        "Prime" => "штрих",
        "Double prime" => "двойной штрих",
        "Left single quotation mark" => "левая одинарная кавычка",
        "Right single quotation mark" => "правая одинарная кавычка",
        "Double low quotation mark" => "нижняя двойная кавычка",
        "Left double quotation mark" => "левая двойная кавычка",
        "Right double quotation mark" => "правая двойная кавычка",
        "Trade mark sign" => "знак торговой марки",
        "Tilde" => "тильда",
        "Underscore" => "нижнее подчёркивание",
        _ => name,
    }
}

fn ru_modifier_name(name: &str) -> String {
    name.replace("Left ", "левый ").replace("Right ", "правый ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    MainTabLayout,
    MainTabAdvanced,
    MainTabConfig,
    NoDevicesFound,
    ComboTitle,
    AutoShiftTitle,
    KeyOverridesTitle,
    MouseKeysTitle,
    MatrixTesterTitle,
    MatrixTesterDescription,
    UniversalSymbolsSetupTitle,
    UniversalSymbolsTitle,
    RgbTitle,
    LayerLedsTitle,
    EncodersTitle,
    DisplayPresetsTitle,
    TouchpadTitle,
    LiveFeaturesTitle,
    MagicTitle,
    TapHoldOneShotTitle,
    AltRepeatTitle,
    AltRepeatDescription,
    AltRepeatUnavailable,
    GraveEscapeTitle,
    RgbUnavailableTooltip,
    AppSettingsTitle,
    AppSettingsDescription,
    LanguageLabel,
    LanguageTooltip,
    CloseToTrayLabel,
    CloseToTrayTooltip,
    RunInBackgroundLabel,
    RunInBackgroundTooltip,
    LaunchAtStartupLabel,
    LaunchAtStartupTooltip,
    ShiftedNumberSymbolsLabel,
    ShiftedNumberSymbolsTooltip,
    LayerHoverPreviewLabel,
    LayerHoverPreviewTooltip,
    EncoderHoverZoomLabel,
    EncoderHoverZoomTooltip,
    AccentColorLabel,
    AccentColorTooltip,
    MouseKeysDescription,
    MouseKeysUnavailable,
    MouseKeysEnableHint,
    RgbDescription,
    RgbConnect,
    KeyOverridesDescription,
    ComboDescription,
    EncodersDescription,
    EncodersUnavailable,
    DisplayPresetsDescription,
    DisplayPresetsUnavailable,
    DisplayPresetsConnect,
    AutoShiftDescription,
    AutoShiftUnavailable,
    AutoShiftEnableHint,
    KeyboardLocked,
    AutoShiftUnlockHint,
    LayerLedsDescription,
    LayerLedsUnavailable,
    LayerLedsEnableHint,
    LayerLedsConnect,
    GraveEscapeDescription,
    GraveEscapeUnavailable,
    GraveEscapeEnableHint,
    GraveEscapeConnect,
    MagicDescription,
    MagicUnavailable,
    MagicEnableHint,
    MagicConnect,
    TapHoldOneShotDescription,
    TapHoldOneShotUnavailable,
    QmkSettingsEnableHint,
    TapHoldOneShotConnect,
    LiveFeaturesDescription,
    LiveFeaturesInactive,
    LiveFeaturesSelectHint,
    LiveFeaturesReadyNote,
    TouchpadDescription,
    TouchpadUnavailable,
    TouchpadEnableHint,
    TouchpadConnect,
}
impl Key {
    fn i18n_key(self) -> &'static str {
        match self {
            Key::MainTabLayout => "ui.main_tab_layout",
            Key::MainTabAdvanced => "ui.main_tab_advanced",
            Key::MainTabConfig => "ui.main_tab_config",
            Key::NoDevicesFound => "ui.no_devices_found",
            Key::ComboTitle => "ui.combo_title",
            Key::AutoShiftTitle => "ui.auto_shift_title",
            Key::KeyOverridesTitle => "ui.key_overrides_title",
            Key::MouseKeysTitle => "ui.mouse_keys_title",
            Key::MatrixTesterTitle => "ui.matrix_tester_title",
            Key::MatrixTesterDescription => "ui.matrix_tester_description",
            Key::UniversalSymbolsSetupTitle => "ui.universal_symbols_setup_title",
            Key::UniversalSymbolsTitle => "ui.universal_symbols_title",
            Key::RgbTitle => "ui.rgb_title",
            Key::LayerLedsTitle => "ui.layer_leds_title",
            Key::EncodersTitle => "ui.encoders_title",
            Key::DisplayPresetsTitle => "ui.display_presets_title",
            Key::TouchpadTitle => "ui.touchpad_title",
            Key::LiveFeaturesTitle => "ui.live_features_title",
            Key::MagicTitle => "ui.magic_title",
            Key::TapHoldOneShotTitle => "ui.tap_hold_one_shot_title",
            Key::AltRepeatTitle => "ui.alt_repeat_title",
            Key::AltRepeatDescription => "ui.alt_repeat_description",
            Key::AltRepeatUnavailable => "ui.alt_repeat_unavailable",
            Key::GraveEscapeTitle => "ui.grave_escape_title",
            Key::RgbUnavailableTooltip => "ui.rgb_unavailable_tooltip",
            Key::AppSettingsTitle => "ui.app_settings_title",
            Key::AppSettingsDescription => "ui.app_settings_description",
            Key::LanguageLabel => "ui.language_label",
            Key::LanguageTooltip => "ui.language_tooltip",
            Key::CloseToTrayLabel => "ui.close_to_tray_label",
            Key::CloseToTrayTooltip => "ui.close_to_tray_tooltip",
            Key::RunInBackgroundLabel => "ui.run_in_background_label",
            Key::RunInBackgroundTooltip => "ui.run_in_background_tooltip",
            Key::LaunchAtStartupLabel => "ui.launch_at_startup_label",
            Key::LaunchAtStartupTooltip => "ui.launch_at_startup_tooltip",
            Key::ShiftedNumberSymbolsLabel => "ui.shifted_number_symbols_label",
            Key::ShiftedNumberSymbolsTooltip => "ui.shifted_number_symbols_tooltip",
            Key::LayerHoverPreviewLabel => "ui.layer_hover_preview_label",
            Key::LayerHoverPreviewTooltip => "ui.layer_hover_preview_tooltip",
            Key::EncoderHoverZoomLabel => "ui.encoder_hover_zoom_label",
            Key::EncoderHoverZoomTooltip => "ui.encoder_hover_zoom_tooltip",
            Key::AccentColorLabel => "ui.accent_color_label",
            Key::AccentColorTooltip => "ui.accent_color_tooltip",
            Key::MouseKeysDescription => "ui.mouse_keys_description",
            Key::MouseKeysUnavailable => "ui.mouse_keys_unavailable",
            Key::MouseKeysEnableHint => "ui.mouse_keys_enable_hint",
            Key::RgbDescription => "ui.rgb_description",
            Key::RgbConnect => "ui.rgb_connect",
            Key::KeyOverridesDescription => "ui.key_overrides_description",
            Key::ComboDescription => "ui.combo_description",
            Key::EncodersDescription => "ui.encoders_description",
            Key::EncodersUnavailable => "ui.encoders_unavailable",
            Key::DisplayPresetsDescription => "ui.display_presets_description",
            Key::DisplayPresetsUnavailable => "ui.display_presets_unavailable",
            Key::DisplayPresetsConnect => "ui.display_presets_connect",
            Key::AutoShiftDescription => "ui.auto_shift_description",
            Key::AutoShiftUnavailable => "ui.auto_shift_unavailable",
            Key::AutoShiftEnableHint => "ui.auto_shift_enable_hint",
            Key::KeyboardLocked => "ui.keyboard_locked",
            Key::AutoShiftUnlockHint => "ui.auto_shift_unlock_hint",
            Key::LayerLedsDescription => "ui.layer_leds_description",
            Key::LayerLedsUnavailable => "ui.layer_leds_unavailable",
            Key::LayerLedsEnableHint => "ui.layer_leds_enable_hint",
            Key::LayerLedsConnect => "ui.layer_leds_connect",
            Key::GraveEscapeDescription => "ui.grave_escape_description",
            Key::GraveEscapeUnavailable => "ui.grave_escape_unavailable",
            Key::GraveEscapeEnableHint => "ui.grave_escape_enable_hint",
            Key::GraveEscapeConnect => "ui.grave_escape_connect",
            Key::MagicDescription => "ui.magic_description",
            Key::MagicUnavailable => "ui.magic_unavailable",
            Key::MagicEnableHint => "ui.magic_enable_hint",
            Key::MagicConnect => "ui.magic_connect",
            Key::TapHoldOneShotDescription => "ui.tap_hold_one_shot_description",
            Key::TapHoldOneShotUnavailable => "ui.tap_hold_one_shot_unavailable",
            Key::QmkSettingsEnableHint => "ui.qmk_settings_enable_hint",
            Key::TapHoldOneShotConnect => "ui.tap_hold_one_shot_connect",
            Key::LiveFeaturesDescription => "ui.live_features_description",
            Key::LiveFeaturesInactive => "ui.live_features_inactive",
            Key::LiveFeaturesSelectHint => "ui.live_features_select_hint",
            Key::LiveFeaturesReadyNote => "ui.live_features_ready_note",
            Key::TouchpadDescription => "ui.touchpad_description",
            Key::TouchpadUnavailable => "ui.touchpad_unavailable",
            Key::TouchpadEnableHint => "ui.touchpad_enable_hint",
            Key::TouchpadConnect => "ui.touchpad_connect",
        }
    }
}

pub fn tr(language: Language, key: Key) -> &'static str {
    tr_catalog(language, key.i18n_key())
}

fn static_catalog_key(text: &str) -> Option<&'static str> {
    match text {
        "Effect" => Some("shared_text.effect"),
        "Color" => Some("shared_text.color"),
        "Speed" => Some("shared_text.speed"),
        "Brightness" => Some("shared_text.brightness"),
        "ui.diagnostics_mode_label" => Some("ui.diagnostics_mode_label"),
        "ui.diagnostics_mode_tooltip" => Some("ui.diagnostics_mode_tooltip"),
        "ui.diagnostics_enabled_status" => Some("ui.diagnostics_enabled_status"),
        "ui.diagnostics_disabled_status" => Some("ui.diagnostics_disabled_status"),
        "Key Picker" => Some("key_picker_text.key_picker"),
        "Pick key" => Some("key_picker_text.pick_key"),
        "Press a key on your keyboard, or pick below" => Some("key_picker_text.press_a_key_on_your_keyboard_or_pick_below"),
        "Press a key on your keyboard, or click below" => Some("key_picker_text.press_a_key_on_your_keyboard_or_click_below"),
        "Press a key on your keyboard, or click below (Esc to cancel)" => Some("key_picker_text.press_a_key_on_your_keyboard_or_click_below_esc_to_cancel"),
        "Best for normal keys, navigation, media and special actions" => Some("key_picker_text.best_for_normal_keys_navigation_media_and_special_actions"),
        "None (clear)" => Some("key_picker_text.none_clear"),
        "Pick layer" => Some("key_picker_text.pick_layer"),
        "Choose which layer (Esc to cancel)" => Some("key_picker_text.choose_which_layer_esc_to_cancel"),
        "Pick tap key (hold = modifier)" => Some("key_picker_text.pick_tap_key_hold_modifier"),
        "Pick key for modifier combo" => Some("key_picker_text.pick_key_for_modifier_combo"),
        "Basic" => Some("key_picker_text.basic"),
        "Symbols" => Some("key_picker_text.symbols"),
        "Mods" => Some("key_picker_text.mods"),
        "Special" => Some("key_picker_text.special"),
        "Macros" => Some("key_picker_text.macros"),
        "Tap Dance" => Some("key_picker_text.tap_dance"),
        "Custom" => Some("key_picker_text.custom"),
        "Layers" => Some("key_picker_text.layers"),
        "Letters" => Some("key_picker_text.letters"),
        "Numbers" => Some("key_picker_text.numbers"),
        "Editing" => Some("key_picker_text.editing"),
        "Navigation" => Some("key_picker_text.navigation"),
        "Function keys" => Some("key_picker_text.function_keys"),
        "Modifiers" => Some("key_picker_text.modifiers"),
        "Other keys" => Some("key_picker_text.other_keys"),
        "Special QMK keys" => Some("key_picker_text.special_qmk_keys"),
        "Mouse" => Some("key_picker_text.mouse"),
        "Media, Apps, System" => Some("key_picker_text.media_apps_system"),
        "OS / edit shortcuts" => Some("key_picker_text.os_edit_shortcuts"),
        "Numpad" => Some("key_picker_text.numpad"),
        "Space Cadet" => Some("key_picker_text.space_cadet"),
        "International" => Some("key_picker_text.international"),
        "Backlight" => Some("key_picker_text.backlight"),
        "RGB Underglow" => Some("key_picker_text.rgb_underglow"),
        "RGB Matrix Modes" => Some("key_picker_text.rgb_matrix_modes"),
        "RGB Matrix Controls" => Some("key_picker_text.rgb_matrix_controls"),
        "Hold to activate, release to return" => Some("key_picker_text.hold_to_activate_release_to_return"),
        "Toggle layer on/off" => Some("key_picker_text.toggle_layer_on_off"),
        "Tap to toggle on/off" => Some("key_picker_text.tap_to_toggle_on_off"),
        "Active for next keypress only" => Some("key_picker_text.active_for_next_keypress_only"),
        "Hold = MO, tap = toggle" => Some("key_picker_text.hold_mo_tap_toggle"),
        "Tap multiple times to toggle layer" => Some("key_picker_text.tap_multiple_times_to_toggle_layer"),
        "Switch and stay on this layer" => Some("key_picker_text.switch_and_stay_on_this_layer"),
        "Set as permanent base layer" => Some("key_picker_text.set_as_permanent_base_layer"),
        "Hold = activate layer, tap = keycode; choose layer first, then tap key" => Some("key_picker_text.hold_activate_layer_tap_keycode_set_key_via_right_click_afterwards"),
        "Toggle backlight on/off" => Some("keycode_tooltips.toggle_backlight_on_off"),
        "Cycle through backlight brightness levels" => Some("keycode_tooltips.cycle_through_backlight_brightness_levels"),
        "Toggle breathing effect on/off" => Some("keycode_tooltips.toggle_breathing_effect_on_off"),
        "Turn backlight on" => Some("keycode_tooltips.turn_backlight_on"),
        "Turn backlight off" => Some("keycode_tooltips.turn_backlight_off"),
        "Decrease backlight brightness" => Some("keycode_tooltips.decrease_backlight_brightness"),
        "Increase backlight brightness" => Some("keycode_tooltips.increase_backlight_brightness"),
        "Toggle RGB lighting on/off" => Some("keycode_tooltips.toggle_rgb_lighting_on_off"),
        "Switch to previous RGB animation mode" => Some("keycode_tooltips.switch_to_previous_rgb_animation_mode"),
        "Switch to next RGB animation mode" => Some("keycode_tooltips.switch_to_next_rgb_animation_mode"),
        "Decrease color hue" => Some("keycode_tooltips.decrease_color_hue"),
        "Increase color hue" => Some("keycode_tooltips.increase_color_hue"),
        "Decrease color saturation" => Some("keycode_tooltips.decrease_color_saturation"),
        "Increase color saturation" => Some("keycode_tooltips.increase_color_saturation"),
        "Decrease brightness" => Some("keycode_tooltips.decrease_brightness"),
        "Increase brightness" => Some("keycode_tooltips.increase_brightness"),
        "Decrease animation speed" => Some("keycode_tooltips.decrease_animation_speed"),
        "Increase animation speed" => Some("keycode_tooltips.increase_animation_speed"),
        "Previous RGB effect" => Some("keycode_tooltips.previous_rgb_effect"),
        "Next RGB effect" => Some("keycode_tooltips.next_rgb_effect"),
        "Turn RGB Matrix on" => Some("keycode_tooltips.turn_rgb_matrix_on"),
        "Turn RGB Matrix off" => Some("keycode_tooltips.turn_rgb_matrix_off"),
        "Toggle RGB Matrix on/off" => Some("keycode_tooltips.toggle_rgb_matrix_on_off"),
        "Previous RGB Matrix animation" => Some("keycode_tooltips.previous_rgb_matrix_animation"),
        "Next RGB Matrix animation" => Some("keycode_tooltips.next_rgb_matrix_animation"),
        "Decrease RGB Matrix hue" => Some("keycode_tooltips.decrease_rgb_matrix_hue"),
        "Increase RGB Matrix hue" => Some("keycode_tooltips.increase_rgb_matrix_hue"),
        "Decrease RGB Matrix saturation" => Some("keycode_tooltips.decrease_rgb_matrix_saturation"),
        "Increase RGB Matrix saturation" => Some("keycode_tooltips.increase_rgb_matrix_saturation"),
        "Decrease RGB Matrix brightness" => Some("keycode_tooltips.decrease_rgb_matrix_brightness"),
        "Increase RGB Matrix brightness" => Some("keycode_tooltips.increase_rgb_matrix_brightness"),
        "Decrease RGB Matrix animation speed" => Some("keycode_tooltips.decrease_rgb_matrix_animation_speed"),
        "Increase RGB Matrix animation speed" => Some("keycode_tooltips.increase_rgb_matrix_animation_speed"),
        "Rose" => Some("keycode_tooltips.rose"),
        "Violet" => Some("keycode_tooltips.violet"),
        "Blue" => Some("keycode_tooltips.blue"),
        "Amber" => Some("keycode_tooltips.amber"),
        "Copper" => Some("keycode_tooltips.copper"),
        "Teal" => Some("keycode_tooltips.teal"),
        "White" => Some("keycode_tooltips.white"),
        "Red" => Some("keycode_tooltips.red"),
        "Orange" => Some("keycode_tooltips.orange"),
        "Goldenrod" => Some("keycode_tooltips.goldenrod"),
        "Gold" => Some("keycode_tooltips.gold"),
        "Yellow" => Some("keycode_tooltips.yellow"),
        "Chartreuse" => Some("keycode_tooltips.chartreuse"),
        "Lime" => Some("keycode_tooltips.lime"),
        "Green" => Some("keycode_tooltips.green"),
        "Spring Green" => Some("keycode_tooltips.spring_green"),
        "Turquoise" => Some("keycode_tooltips.turquoise"),
        "Cyan" => Some("keycode_tooltips.cyan"),
        "Azure" => Some("keycode_tooltips.azure"),
        "Sky" => Some("keycode_tooltips.sky"),
        "Indigo" => Some("keycode_tooltips.indigo"),
        "Purple" => Some("keycode_tooltips.purple"),
        "Magenta" => Some("keycode_tooltips.magenta"),
        "Pink" => Some("keycode_tooltips.pink"),
        "Coral" => Some("keycode_tooltips.coral"),
        "Salmon" => Some("keycode_tooltips.salmon"),
        "Warm White" => Some("keycode_tooltips.warm_white"),
        "Trigger" => Some("key_override_editor.trigger"),
        "Replacement" => Some("key_override_editor.replacement"),
        "Suppressed mods" => Some("key_override_editor.suppressed_mods"),
        "Trigger mods" => Some("key_override_editor.trigger_mods"),
        "Negative mods" => Some("key_override_editor.negative_mods"),
        "Enable on layers" => Some("key_override_editor.enable_on_layers"),
        "Pick trigger" => Some("key_override_editor.pick_trigger"),
        "Pick replacement" => Some("key_override_editor.pick_replacement"),
        "None" => Some("key_override_editor.none"),
        "All mods" => Some("key_override_editor.all_mods"),
        "No layers" => Some("key_override_editor.no_layers"),
        "All layers" => Some("key_override_editor.all_layers"),
        "Enable all" => Some("key_override_editor.enable_all"),
        "Disable all" => Some("key_override_editor.disable_all"),
        "Trigger press" => Some("key_override_editor.trigger_press"),
        "Required mod press" => Some("key_override_editor.required_mod_press"),
        "Blocked mod release" => Some("key_override_editor.blocked_mod_release"),
        "Any one mod" => Some("key_override_editor.any_one_mod"),
        "No re-send" => Some("key_override_editor.no_re_send"),
        "Stay active" => Some("key_override_editor.stay_active"),
        "Input keys" => Some("combo_editor.input_keys"),
        "Output key" => Some("combo_editor.output_key"),
        "Select Key Override slot" => Some("key_override_editor.select_key_override_slot"),
        "Local name for this Key Override slot" => Some("key_override_editor.local_name_for_this_key_override_slot"),
        "Original key that can be overridden" => Some("key_override_editor.original_key_that_can_be_overridden"),
        "Keycode sent while override conditions match" => Some("key_override_editor.keycode_sent_while_override_conditions_match"),
        "Modifiers hidden while the replacement is active" => Some("key_override_editor.modifiers_hidden_while_the_replacement_is_active"),
        "Modifiers required for this override" => Some("key_override_editor.modifiers_required_for_this_override"),
        "Modifiers that block this override" => Some("key_override_editor.modifiers_that_block_this_override"),
        "Layers where this override can activate" => Some("key_override_editor.layers_where_this_override_can_activate"),
        "Activate when the trigger key is pressed" => Some("key_override_editor.activate_when_the_trigger_key_is_pressed"),
        "Activate when a required modifier is pressed" => Some("key_override_editor.activate_when_a_required_modifier_is_pressed"),
        "Activate when a blocking modifier is released" => Some("key_override_editor.activate_when_a_blocking_modifier_is_released"),
        "Any one trigger modifier is enough" => Some("key_override_editor.any_one_trigger_modifier_is_enough"),
        "Do not resend the trigger after override ends" => Some("key_override_editor.do_not_resend_the_trigger_after_override_ends"),
        "Stay active when another key is pressed" => Some("key_override_editor.stay_active_when_another_key_is_pressed"),
        "Select Combo slot" => Some("combo_editor.select_combo_slot"),
        "Local name for this combo slot" => Some("combo_editor.local_name_for_this_combo_slot"),
        "Keys that must be pressed together" => Some("combo_editor.keys_that_must_be_pressed_together"),
        "Keycode sent when the combo activates" => Some("combo_editor.keycode_sent_when_the_combo_activates"),
        "Maximum time between combo key presses" => Some("combo_editor.maximum_time_between_combo_key_presses"),
        "Press 2-4 keys" => Some("combo_editor.press_2_4_keys"),
        "Record 2-4 keys" => Some("combo_editor.record_2_4_keys"),
        "Pick output" => Some("combo_editor.pick_output"),
        "Hold actions can use regular keys, shortcuts, mouse/media, custom keycodes, modifiers, and layers" => Some("key_picker_text.hold_actions_are_limited_to_left_right_modifiers_and_layers"),
        "Tap-then-hold actions can use regular keys, shortcuts, mouse/media, custom keycodes, modifiers, and layers" => Some("key_picker_text.tap_then_hold_actions_are_limited_to_left_right_modifiers_and_layers"),
        "Left Control" => Some("key_picker_text.left_control"),
        "Right Control" => Some("key_picker_text.right_control"),
        "Left Shift" => Some("key_picker_text.left_shift"),
        "Right Shift" => Some("key_picker_text.right_shift"),
        "Left Alt" => Some("key_picker_text.left_alt"),
        "Right Alt" => Some("key_picker_text.right_alt"),
        "Clear all" => Some("key_picker_text.clear_all"),
        "↩ Undo" => Some("key_picker_text.undo_undo"),
        "Undo last change" => Some("key_picker_text.undo_last_change"),
        "Remove all actions from this macro" => Some("key_picker_text.remove_all_actions_from_this_macro"),
        "Key sent on single tap" => Some("key_picker_text.key_sent_on_single_tap"),
        "Key sent when held" => Some("key_picker_text.key_sent_when_held"),
        "Key sent on double tap" => Some("key_picker_text.key_sent_on_double_tap"),
        "Key sent on tap then hold" => Some("key_picker_text.key_sent_on_tap_then_hold"),
        "Best for a second tap action, usually another normal key or command" => Some("key_picker_text.best_for_a_second_tap_action_usually_another_normal_key_or_command"),
        "Basic keys — standard keyboard layout" => Some("key_picker_text.basic_keys_standard_keyboard_layout"),
        "Universal symbols — same output in any language" => Some("key_picker_text.universal_symbols_same_output_in_any_language"),
        "Layout symbols — follow the active keyboard language" => Some("key_picker_text.layout_symbols_follow_the_active_keyboard_language"),
        "Extra universal symbols — typography and math" => Some("key_picker_text.extra_universal_symbols_typography_and_math"),
        "Custom keycodes — defined by this device" => Some("key_picker_text.custom_keycodes_defined_by_this_keyboard"),
        "Layers: choose a layer action, then pick the target layer" => Some("key_picker_text.layers_choose_a_layer_action_then_pick_the_target_layer"),
        "Plain modifiers" => Some("key_picker_text.plain_modifiers"),
        "Mod+Key — always sends modifier+key together" => Some("key_picker_text.mod_plus_key_always_sends_modifier_plus_key_together"),
        "Mod-Tap — hold for modifier, tap for regular key" => Some("key_picker_text.mod_tap_hold_for_modifier_tap_for_regular_key"),
        "One-Shot Mod — active for next keypress only" => Some("key_picker_text.one_shot_mod_active_for_next_keypress_only"),
        "Choose the key to pair with the modifier" => Some("key_picker_text.choose_the_key_to_pair_with_the_modifier"),
        "This key will always be sent together with the selected modifier" => Some("key_picker_text.this_key_will_always_be_sent_together_with_the_selected_modifier"),
        "Choose the tap key" => Some("key_picker_text.choose_the_tap_key"),
        "Hold will send the modifier; tap will send the key you pick" => Some("key_picker_text.hold_will_send_the_modifier_tap_will_send_the_key_you_pick"),
        "✕ Cancel" => Some("key_picker_text.cancel_cancel"),
        "Mod+Key — pick modifier, then key" => Some("key_picker_text.mod_plus_key_pick_modifier_then_key"),
        "Mod-Tap — pick modifier, then tap key" => Some("key_picker_text.mod_tap_pick_modifier_then_tap_key"),
        "Choose macro" => Some("macro_editor.choose_macro"),
        "Select a macro above to edit" => Some("macro_editor.select_a_macro_above_to_edit"),
        "Macro name" => Some("macro_editor.macro_name"),
        "Macro description" => Some("macro_editor.macro_description"),
        "Optional description for this macro" => Some("macro_editor.optional_description_for_this_macro"),
        "Move up" => Some("macro_editor.move_up"),
        "Move down" => Some("macro_editor.move_down"),
        "Text" => Some("macro_editor.text"),
        "Types text characters one by one" => Some("macro_editor.types_text_characters_one_by_one"),
        "Tap" => Some("macro_editor.tap"),
        "Press and release a key" => Some("macro_editor.press_and_release_a_key"),
        "Down" => Some("macro_editor.down"),
        "Press a key (hold until Up)" => Some("macro_editor.press_a_key_hold_until_up"),
        "Up" => Some("macro_editor.up"),
        "Release a previously pressed key" => Some("macro_editor.release_a_previously_pressed_key"),
        "Delay" => Some("macro_editor.delay"),
        "Wait before next action" => Some("macro_editor.wait_before_next_action"),
        "Type text here" => Some("macro_editor.type_text_here"),
        "Characters to type when this macro runs" => Some("macro_editor.characters_to_type_when_this_macro_runs"),
        "Click to change key — press and release this key" => Some("macro_editor.click_to_change_key_press_and_release_this_key"),
        "Click to change key — holds down until Up" => Some("macro_editor.click_to_change_key_holds_down_until_up"),
        "Click to change key — releases this key" => Some("macro_editor.click_to_change_key_releases_this_key"),
        "Delay is in milliseconds" => Some("macro_editor.delay_is_in_milliseconds"),
        "Remove this action" => Some("macro_editor.remove_this_action"),
        "+ Text" => Some("macro_editor.plus_text"),
        "Type characters" => Some("macro_editor.type_characters"),
        "+ Tap" => Some("macro_editor.plus_tap"),
        "+ Down" => Some("macro_editor.plus_down"),
        "Hold a key" => Some("macro_editor.hold_a_key"),
        "+ Up" => Some("macro_editor.plus_up"),
        "Release a key" => Some("macro_editor.release_a_key"),
        "+ Delay" => Some("macro_editor.plus_delay"),
        "Pause in milliseconds" => Some("macro_editor.pause_in_milliseconds"),
        "No Tap Dance slots available on this device" => Some("tap_dance_editor.no_tap_dance_slots_available_on_this_keyboard"),
        "Choose tap dance" => Some("tap_dance_editor.choose_tap_dance"),
        "TD name" => Some("tap_dance_editor.td_name"),
        "On Tap" => Some("tap_dance_editor.on_tap"),
        "On Hold" => Some("tap_dance_editor.on_hold"),
        "On Double Tap" => Some("tap_dance_editor.on_double_tap"),
        "On Tap + Hold" => Some("tap_dance_editor.on_tap_plus_hold"),
        "Click to assign a key" => Some("tap_dance_editor.click_to_assign_a_key"),
        "Tapping Term" => Some("tap_dance_editor.tapping_term"),
        "Time in milliseconds to distinguish tap from hold (default: 200)" => Some("tap_dance_editor.time_in_milliseconds_to_distinguish_tap_from_hold_default_200"),
        "Tapping term is in milliseconds" => Some("tap_dance_editor.tapping_term_is_in_milliseconds"),
        "Clear all actions for this tap dance" => Some("tap_dance_editor.clear_all_actions_for_this_tap_dance"),
        "Undo last tap dance change" => Some("tap_dance_editor.undo_last_tap_dance_change"),
        "Device is locked, unlock it to use Matrix Tester" => Some("matrix_tester.keyboard_is_locked_unlock_it_to_use_matrix_tester"),
        "Click to reset Matrix Tester" => Some("matrix_tester.click_to_reset_matrix_tester"),
        "Matrix Tester is currently available only for Vial devices" => Some("matrix_tester.matrix_tester_is_currently_available_only_for_vial_keyboards"),
        "Connect a Vial device to start live switch testing" => Some("matrix_tester.connect_a_vial_keyboard_to_start_live_switch_testing"),
        "Click Tested to reset progress" => Some("matrix_tester.click_tested_to_reset_progress"),
        "Tested" => Some("matrix_tester.tested"),
        "Toggle firmware layout/display option" => Some("auto_shift_settings.toggle_firmware_layout_display_option"),
        "Choose firmware preset" => Some("auto_shift_settings.choose_firmware_preset"),
        "Connect a Vial device to edit Auto Shift settings" => Some("auto_shift_settings.connect_a_vial_keyboard_to_edit_auto_shift_settings"),
        "Enable" => Some("auto_shift_settings.enable"),
        "Turn Auto Shift on or off" => Some("auto_shift_settings.turn_auto_shift_on_or_off"),
        "Enable for modifiers" => Some("auto_shift_settings.enable_for_modifiers"),
        "Allow Auto Shift behavior on modifier keys" => Some("auto_shift_settings.allow_auto_shift_behavior_on_modifier_keys"),
        "No special keys" => Some("auto_shift_settings.no_special_keys"),
        "Do not Auto Shift special keys such as Enter, Esc, Tab or Backspace" => Some("auto_shift_settings.do_not_auto_shift_special_keys_such_as_enter_esc_tab_or_backspace"),
        "No numeric keys" => Some("auto_shift_settings.no_numeric_keys"),
        "Do not Auto Shift number keys" => Some("auto_shift_settings.do_not_auto_shift_number_keys"),
        "No alpha keys" => Some("auto_shift_settings.no_alpha_keys"),
        "Do not Auto Shift letter keys" => Some("auto_shift_settings.do_not_auto_shift_letter_keys"),
        "Enable keyrepeat" => Some("auto_shift_settings.enable_keyrepeat"),
        "Allow held Auto Shift keys to repeat" => Some("auto_shift_settings.allow_held_auto_shift_keys_to_repeat"),
        "Stop repeat after timeout" => Some("auto_shift_settings.stop_repeat_after_timeout"),
        "Disable key repeat after the Auto Shift timeout is exceeded" => Some("auto_shift_settings.disable_key_repeat_after_the_auto_shift_timeout_is_exceeded"),
        "Timeout" => Some("auto_shift_settings.timeout"),
        "Hold time before Auto Shift sends the shifted key" => Some("auto_shift_settings.hold_time_before_auto_shift_sends_the_shifted_key"),
        "Timeout is in milliseconds" => Some("auto_shift_settings.timeout_is_in_milliseconds"),
        "Light" => Some("app_chrome.light"),
        "Dark" => Some("app_chrome.dark"),
        "☀ Light" => Some("app_chrome.light_light"),
        "🌙 Dark" => Some("app_chrome.dark_dark"),
        "Unlock device" => Some("app_chrome.unlock_unlock_keyboard"),
        "Unlock Device" => Some("app_chrome.unlock_keyboard"),
        "Macros saved" => Some("status_messages.macros_saved"),
        "Combos saved" => Some("status_messages.combos_saved"),
        "Combo timeout saved" => Some("status_messages.combo_timeout_saved"),
        "Entry" => Some("alt_repeat_editor.entry"),
        "Select Alt Repeat slot" => Some("alt_repeat_editor.select_alt_repeat_slot"),
        "Name" => Some("alt_repeat_editor.name"),
        "Local name for this slot" => Some("alt_repeat_editor.local_name_for_this_slot"),
        "Stored locally in Entropy" => Some("alt_repeat_editor.stored_locally_in_entropy"),
        "Last key" => Some("alt_repeat_editor.last_key"),
        "Key that triggers alternate repeat behavior" => Some("alt_repeat_editor.key_that_triggers_alternate_repeat_behavior"),
        "Alt key" => Some("alt_repeat_editor.alt_key"),
        "Key repeated when alternate repeat activates" => Some("alt_repeat_editor.key_repeated_when_alternate_repeat_activates"),
        "Ctrl mods" => Some("alt_repeat_editor.ctrl_mods"),
        "Shift mods" => Some("alt_repeat_editor.shift_mods"),
        "Alt mods" => Some("alt_repeat_editor.alt_mods"),
        "Allowed Ctrl modifiers" => Some("alt_repeat_editor.allowed_ctrl_modifiers"),
        "Allowed Shift modifiers" => Some("alt_repeat_editor.allowed_shift_modifiers"),
        "Allowed Alt modifiers" => Some("alt_repeat_editor.allowed_alt_modifiers"),
        "Allowed OS modifiers" => Some("alt_repeat_editor.allowed_os_modifiers"),
        "Right-side modifier" => Some("alt_repeat_editor.right_side_modifier"),
        "Left-side modifier" => Some("alt_repeat_editor.left_side_modifier"),
        "Default alt key" => Some("alt_repeat_editor.default_alt_key"),
        "Use this alt key by default" => Some("alt_repeat_editor.use_this_alt_key_by_default"),
        "Bidirectional" => Some("alt_repeat_editor.bidirectional"),
        "Allow both keys to alternate each other" => Some("alt_repeat_editor.allow_both_keys_to_alternate_each_other"),
        "Ignore handedness" => Some("alt_repeat_editor.ignore_handedness"),
        "Treat left and right modifiers as equivalent" => Some("alt_repeat_editor.treat_left_and_right_modifiers_as_equivalent"),
        "Clear" => Some("alt_repeat_editor.clear"),
        "Undo" => Some("alt_repeat_editor.undo"),
        "↶ Undo" => Some("alt_repeat_editor.undo_curved"),
        "LED brightness" => Some("advanced_settings.led_brightness"),
        "Global LED brightness for layer color lighting" => Some("advanced_settings.global_led_brightness_for_layer_color_lighting"),
        "LED timeout" => Some("advanced_settings.led_timeout"),
        "Minutes before LEDs turn off automatically, 0 disables timeout" => Some("advanced_settings.minutes_before_leds_turn_off_automatically_0_disables_timeout"),
        "Off" => Some("advanced_settings.off"),
        "Alt forces Esc" => Some("advanced_settings.alt_forces_esc"),
        "When Alt is held, Grave Escape sends Esc instead of ` or ~" => Some("advanced_settings.when_alt_is_held_grave_escape_sends_esc_instead_of_grave_or_tilde"),
        "Control forces Esc" => Some("advanced_settings.control_forces_esc"),
        "When Control is held, Grave Escape sends Esc instead of ` or ~" => Some("advanced_settings.when_control_is_held_grave_escape_sends_esc_instead_of_grave_or_tilde"),
        "Shift forces Esc" => Some("advanced_settings.shift_forces_esc"),
        "When Shift is held, Grave Escape sends Esc instead of ` or ~" => Some("advanced_settings.when_shift_is_held_grave_escape_sends_esc_instead_of_grave_or_tilde"),
        "Swap Caps Lock and Left Control" => Some("advanced_settings.swap_caps_lock_and_left_control"),
        "Caps Lock sends Left Control and Left Control sends Caps Lock" => Some("advanced_settings.caps_lock_sends_left_control_and_left_control_sends_caps_lock"),
        "Treat Caps Lock as Control" => Some("advanced_settings.treat_caps_lock_as_control"),
        "Caps Lock sends Control without swapping Left Control" => Some("advanced_settings.caps_lock_sends_control_without_swapping_left_control"),
        "Disable OS keys" => Some("advanced_settings.disable_os_keys"),
        "Ignore both OS keys while this option is enabled" => Some("advanced_settings.ignore_both_os_keys_while_this_option_is_enabled"),
        "Swap ` and Escape" => Some("advanced_settings.swap_grave_and_escape"),
        "Grave sends Escape and Escape sends Grave" => Some("advanced_settings.grave_sends_escape_and_escape_sends_grave"),
        "Swap \\ and Backspace" => Some("advanced_settings.swap_backslash_and_backspace"),
        "Backslash sends Backspace and Backspace sends Backslash" => Some("advanced_settings.backslash_sends_backspace_and_backspace_sends_backslash"),
        "Enable N-key rollover" => Some("advanced_settings.enable_n_key_rollover"),
        "Allow more simultaneous key presses when the device supports it" => Some("advanced_settings.allow_more_simultaneous_key_presses_when_the_keyboard_supports_it"),
        "Tapping term" => Some("tap_hold_settings.tapping_term_label"),
        "Global tap-vs-hold decision window for dual-role keys" => Some("tap_hold_settings.global_tap_vs_hold_decision_window_for_dual_role_keys"),
        "Permissive hold" => Some("tap_hold_settings.permissive_hold"),
        "Nested taps choose hold for Mod-Tap and Layer-Tap keys" => Some("tap_hold_settings.nested_taps_choose_hold_for_mod_tap_and_layer_tap_keys"),
        "Hold on other key" => Some("tap_hold_settings.hold_on_other_key"),
        "Pressing another key immediately chooses hold for dual-role keys" => Some("tap_hold_settings.pressing_another_key_immediately_chooses_hold_for_dual_role_keys"),
        "Retro tapping" => Some("tap_hold_settings.retro_tapping"),
        "A held-and-released-alone dual-role key still sends its tap action" => Some("tap_hold_settings.a_held_and_released_alone_dual_role_key_still_sends_its_tap_action"),
        "Chordal hold" => Some("tap_hold_settings.chordal_hold"),
        "Same-hand chords prefer tap to reduce home-row mod accidents" => Some("tap_hold_settings.same_hand_chords_prefer_tap_to_reduce_home_row_mod_accidents"),
        "Quick tap term" => Some("tap_hold_settings.quick_tap_term"),
        "Tap-then-hold repeat window for dual-role key tap actions" => Some("tap_hold_settings.tap_then_hold_repeat_window_for_dual_role_key_tap_actions"),
        "Tap code delay" => Some("tap_hold_settings.tap_code_delay"),
        "Delay between register and unregister in tap_code" => Some("tap_hold_settings.delay_between_register_and_unregister_in_tap_code"),
        "Tap hold caps delay" => Some("tap_hold_settings.tap_hold_caps_delay"),
        "Extra delay for LT/MT keys whose tap action is Caps Lock" => Some("tap_hold_settings.extra_delay_for_lt_mt_keys_whose_tap_action_is_caps_lock"),
        "Tapping toggle" => Some("tap_hold_settings.tapping_toggle"),
        "Number of taps needed for TT layer toggle" => Some("tap_hold_settings.number_of_taps_needed_for_tt_layer_toggle"),
        "Flow tap" => Some("tap_hold_settings.flow_tap"),
        "Fast typing timeout that forces MT/LT keys to tap" => Some("tap_hold_settings.fast_typing_timeout_that_forces_mt_lt_keys_to_tap"),
        "One Shot Keys" => Some("tap_hold_settings.one_shot_keys"),
        "One-shot tap toggle" => Some("tap_hold_settings.one_shot_tap_toggle"),
        "Tap this many times to keep a one-shot key held until tapped again" => Some("tap_hold_settings.tap_this_many_times_to_keep_a_one_shot_key_held_until_tapped_again"),
        "One-shot timeout" => Some("tap_hold_settings.one_shot_timeout"),
        "How long one-shot state waits before it is released" => Some("tap_hold_settings.how_long_one_shot_state_waits_before_it_is_released"),
        "Value is in milliseconds" => Some("tap_hold_settings.value_is_in_milliseconds"),
        "DPI" => Some("touchpad_settings.dpi"),
        "Touchpad pointer resolution in dots per inch" => Some("touchpad_settings.touchpad_pointer_resolution_in_dots_per_inch"),
        "Sniper sens" => Some("touchpad_settings.sniper_sens"),
        "Sniper divisor: lower is faster, higher is more precise" => Some("touchpad_settings.sniper_divisor_lower_is_faster_higher_is_more_precise"),
        "Scroll sens" => Some("touchpad_settings.scroll_sens"),
        "Scroll divisor: lower is faster, higher is smoother" => Some("touchpad_settings.scroll_divisor_lower_is_faster_higher_is_smoother"),
        "Text sens" => Some("touchpad_settings.text_sens"),
        "Text mode divisor: lower is faster, higher is slower" => Some("touchpad_settings.text_mode_divisor_lower_is_faster_higher_is_slower"),
        "Invert scroll" => Some("touchpad_settings.invert_scroll"),
        "Reverse the touchpad scroll direction" => Some("touchpad_settings.reverse_the_touchpad_scroll_direction"),
        "Acceleration" => Some("touchpad_settings.acceleration"),
        "Use firmware pointer acceleration for touchpad movement" => Some("touchpad_settings.use_firmware_pointer_acceleration_for_touchpad_movement"),
        "Sticky mode" => Some("touchpad_settings.sticky_mode"),
        "Keep the selected touchpad mode active until another mode is selected" => Some("touchpad_settings.keep_the_selected_touchpad_mode_active_until_another_mode_is_selected"),
        "Auto layer enable" => Some("touchpad_settings.auto_layer_enable"),
        "Automatically switch to the selected layer while the touchpad is active" => Some("touchpad_settings.automatically_switch_to_the_selected_layer_while_the_touchpad_is_activ"),
        "Auto layer" => Some("touchpad_settings.auto_layer"),
        "Layer selected automatically while the touchpad is active" => Some("touchpad_settings.layer_selected_automatically_while_the_touchpad_is_active"),
        "Module" => Some("modules_settings.module"),
        "Left mode" => Some("modules_settings.left_mode"),
        "Left ball axis" => Some("modules_settings.left_ball_axis"),
        "Left touch axis" => Some("modules_settings.left_touch_axis"),
        "Left ball DPI" => Some("modules_settings.left_ball_dpi"),
        "Left touch DPI" => Some("modules_settings.left_touch_dpi"),
        "Left scroll sens" => Some("modules_settings.left_scroll_sens"),
        "Left sniper sens" => Some("modules_settings.left_sniper_sens"),
        "Left text sens" => Some("modules_settings.left_text_sens"),
        "Left invert scroll" => Some("modules_settings.left_invert_scroll"),
        "Left acceleration" => Some("modules_settings.left_acceleration"),
        "Right mode" => Some("modules_settings.right_mode"),
        "Right ball axis" => Some("modules_settings.right_ball_axis"),
        "Right touch axis" => Some("modules_settings.right_touch_axis"),
        "Right ball DPI" => Some("modules_settings.right_ball_dpi"),
        "Right touch DPI" => Some("modules_settings.right_touch_dpi"),
        "Right scroll sens" => Some("modules_settings.right_scroll_sens"),
        "Right sniper sens" => Some("modules_settings.right_sniper_sens"),
        "Right text sens" => Some("modules_settings.right_text_sens"),
        "Right invert scroll" => Some("modules_settings.right_invert_scroll"),
        "Right acceleration" => Some("modules_settings.right_acceleration"),
        "LED blinks" => Some("modules_settings.led_blinks"),
        "Touch gestures" => Some("modules_settings.touch_gestures"),
        "Auto layer in Normal" => Some("modules_settings.auto_layer_in_normal"),
        "Auto layer in Sniper" => Some("modules_settings.auto_layer_in_sniper"),
        "Auto layer in Scroll" => Some("modules_settings.auto_layer_in_scroll"),
        "Auto layer in Text" => Some("modules_settings.auto_layer_in_text"),
        "Auto layer timeout" => Some("modules_settings.auto_layer_timeout"),
        "Normal" => Some("modules_settings.normal"),
        "Sniper" => Some("modules_settings.sniper"),
        "Scroll" => Some("modules_settings.scroll"),
        "Trackball" => Some("modules_settings.trackball"),
        "Touchpad" => Some("modules_settings.touchpad"),
        "Ball" => Some("modules_settings.ball"),
        "Touch" => Some("modules_settings.touch"),
        "Encoder" => Some("modules_settings.encoder"),
        "Entropy background" => Some("live_features.entropy_background"),
        "Keep Entropy running in the background for live firmware data" => Some("live_features.keep_entropy_running_in_the_background_for_live_firmware_data"),
        "Time sync" => Some("live_features.time_sync"),
        "Uses the local system clock" => Some("live_features.uses_the_local_system_clock"),
        "Volume sync" => Some("live_features.volume_sync"),
        "native Windows audio" => Some("live_features.native_windows_audio"),
        "Uses the Windows default output device" => Some("live_features.uses_the_windows_default_output_device"),
        "Uses PipeWire default sink volume" => Some("live_features.uses_pipewire_default_sink_volume"),
        "Uses PulseAudio/PipeWire Pulse default sink volume" => Some("live_features.uses_pulseaudio_pipewire_pulse_default_sink_volume"),
        "missing wpctl/pactl" => Some("live_features.missing_wpctl_pactl"),
        "Install wireplumber or pulseaudio-utils/pavucontrol package for volume sync" => Some("live_features.install_wireplumber_or_pulseaudio_utils_pavucontrol_package_for_volume"),
        "Uses macOS system output volume" => Some("live_features.uses_macos_system_output_volume"),
        "unsupported OS" => Some("live_features.unsupported_os"),
        "Volume sync is implemented for Windows, Linux and macOS" => Some("live_features.volume_sync_is_implemented_for_windows_linux_and_macos"),
        "native Windows media session" => Some("live_features.native_windows_media_session"),
        "Uses Windows global media session metadata" => Some("live_features.uses_windows_global_media_session_metadata"),
        "Uses MPRIS metadata from the active player" => Some("live_features.uses_mpris_metadata_from_the_active_player"),
        "missing playerctl" => Some("live_features.missing_playerctl"),
        "Install playerctl and use an MPRIS-compatible player for media info" => Some("live_features.install_playerctl_and_use_an_mpris_compatible_player_for_media_info"),
        "Spotify / Music via AppleScript" => Some("live_features.spotify_music_via_applescript"),
        "macOS may ask for Automation permission for Entropy, System Events, Spotify or Music" => Some("live_features.macos_may_ask_for_automation_permission_for_entropy_system_events_spot"),
        "Media sync is implemented for Windows, Linux and macOS" => Some("live_features.media_sync_is_implemented_for_windows_linux_and_macos"),
        "Media info" => Some("live_features.media_info"),
        "Layout sync" => Some("live_features.layout_sync"),
        "Sync the active OS keyboard layout for ru/en firmware keys" => Some("live_features.layout_sync_tooltip"),
        "native Windows input layout" => Some("live_features.native_windows_input_layout"),
        "Uses the foreground window keyboard layout" => Some("live_features.uses_the_foreground_window_keyboard_layout"),
        "X11 / XKB" => Some("live_features.x11_xkb"),
        "Uses the active XKB keyboard group" => Some("live_features.uses_the_active_xkb_keyboard_group"),
        "missing X11 / XKB" => Some("live_features.missing_x11_xkb"),
        "Layout sync currently needs an X11 session" => Some("live_features.layout_sync_currently_needs_an_x11_session"),
        "macOS input source" => Some("live_features.macos_input_source"),
        "Uses the current macOS keyboard input source" => Some("live_features.uses_the_current_macos_keyboard_input_source"),
        "Layout sync is implemented for Windows, Linux X11 and macOS" => Some("live_features.layout_sync_is_implemented_for_windows_linux_x11_and_macos"),
        "ready" => Some("live_features.ready"),
        "needs setup" => Some("live_features.needs_setup"),
        "active" => Some("live_features.active"),
        "starting" => Some("live_features.starting"),
        "inactive" => Some("live_features.inactive"),
        _ => None,
    }
}

fn exact_text_catalog_key(text: &str) -> Option<&'static str> {
    match text {
        "No extra setup is required on Windows" => Some("universal_symbols_setup.no_extra_setup_is_required_on_windows"),
        "Keep Entropy running while using Universal Symbols" => Some("universal_symbols_setup.keep_entropy_running_while_using_universal_symbols"),
        "Assign keys from Symbols → Universal symbols in the key picker" => Some("universal_symbols_setup.assign_keys_from_symbols_to_universal_symbols_in_the_key_picker"),
        "Open Privacy & Security" => Some("universal_symbols_setup.open_privacy_and_security"),
        "Allow Entropy in Accessibility" => Some("universal_symbols_setup.allow_entropy_in_accessibility"),
        "If prompted, allow Entropy in Input Monitoring too" => Some("universal_symbols_setup.if_prompted_allow_entropy_in_input_monitoring_too"),
        "Restart Entropy after changing permissions" => Some("universal_symbols_setup.restart_entropy_after_changing_permissions"),
        "X11: install xdotool and keep Entropy running" => Some("universal_symbols_setup.x11_install_xdotool_and_keep_entropy_running"),
        "Wayland + IBus: install Entropy Universal Symbols and select it as an input source" => Some("universal_symbols_setup.wayland_plus_ibus_install_entropy_universal_symbols_and_select_it_as_a"),
        "Universal Symbols are not supported on this OS yet" => Some("universal_symbols_setup.universal_symbols_are_not_supported_on_this_os_yet"),
        "Open Config → Universal Symbols to finish permissions setup" => Some("universal_symbols_setup.open_config_to_universal_symbols_to_finish_permissions_setup"),
        "Open Config → Universal Symbols to finish Linux setup" => Some("universal_symbols_setup.open_config_to_universal_symbols_to_finish_linux_setup"),
        "Disabled" => Some("status_text.disabled"),
        "OLED master" => Some("status_text.oled_master"),
        "OLED slave" => Some("status_text.oled_slave"),
        "Clock" => Some("status_text.clock"),
        "Volume" => Some("status_text.volume"),
        "Media" => Some("status_text.media"),
        "default" => Some("status_text.default"),
        "Unknown" => Some("status_text.unknown"),
        "Universal output backend: Windows native" => Some("universal_symbols_setup.universal_output_backend_windows_native"),
        "Universal output backend: macOS native — requires Accessibility/Input Monitoring permission" => Some("universal_symbols_setup.universal_output_backend_macos_native_requires_accessibility_input_mon"),
        "Universal output backend: unsupported on this OS" => Some("universal_symbols_setup.universal_output_backend_unsupported_on_this_os"),
        "Esc" => Some("key_names.esc"),
        "Escape" => Some("key_names.escape"),
        "Backspace" => Some("key_names.backspace"),
        "Insert" => Some("key_names.insert"),
        "Delete" => Some("key_names.delete"),
        "Caps Lock" => Some("key_names.caps_lock"),
        "Print Screen" => Some("key_names.print_screen"),
        "Scroll Lock" => Some("key_names.scroll_lock"),
        "Page Up" => Some("key_names.page_up"),
        "Page Down" => Some("key_names.page_down"),
        "Space" => Some("key_names.space"),
        "Menu" => Some("key_names.menu"),
        "Pause" => Some("key_names.pause"),
        "Home" => Some("key_names.home"),
        "End" => Some("key_names.end"),
        r#"Left
Ctrl"# => Some("key_names.left_ctrl"),
        r#"Right
Ctrl"# => Some("key_names.right_ctrl"),
        r#"Left
Shift"# => Some("key_names.left_shift"),
        r#"Right
Shift"# => Some("key_names.right_shift"),
        r#"Left
Alt"# => Some("key_names.left_alt"),
        r#"Right
Alt"# => Some("key_names.right_alt"),
        "No key — this key does nothing" => Some("keycode_tooltips.no_key_this_key_does_nothing"),
        "Transparent — uses the key assigned on the layer below" => Some("keycode_tooltips.transparent_uses_the_key_assigned_on_the_layer_below"),
        "Enter — confirm / new line" => Some("keycode_tooltips.enter_confirm_new_line"),
        "Escape — cancel / close" => Some("keycode_tooltips.escape_cancel_close"),
        "Backspace — delete character before cursor" => Some("keycode_tooltips.backspace_delete_character_before_cursor"),
        "Tab — indent / move focus forward" => Some("keycode_tooltips.tab_indent_move_focus_forward"),
        "Caps Lock — toggle uppercase input" => Some("keycode_tooltips.caps_lock_toggle_uppercase_input"),
        "Menu key — open right-click context menu" => Some("keycode_tooltips.menu_key_open_right_click_context_menu"),
        "Minus — type -, Shift gives underscore (_)" => Some("keycode_tooltips.minus_type_shift_gives_underscore"),
        "Equals — type =, Shift gives plus (+)" => Some("keycode_tooltips.equals_type_shift_gives_plus_plus"),
        "Left bracket — type [, Shift gives left brace ({)" => Some("keycode_tooltips.left_bracket_type_shift_gives_left_brace"),
        "Right bracket — type ], Shift gives right brace (})" => Some("keycode_tooltips.right_bracket_type_shift_gives_right_brace"),
        r#"Backslash — type \, Shift gives pipe (|)"# => Some("keycode_tooltips.backslash_type_backslash_shift_gives_pipe"),
        "Non-US hash key — type #, Shift gives tilde (~)" => Some("keycode_tooltips.non_us_hash_key_type_shift_gives_tilde_tilde"),
        "Semicolon key — tap for semicolon (;), Shift gives colon (:)" => Some("keycode_tooltips.semicolon_key_tap_for_semicolon_shift_gives_colon"),
        r#"Quote — type apostrophe ('), Shift gives double quote (")"# => Some("keycode_tooltips.quote_type_apostrophe_shift_gives_double_quote"),
        "Grave accent — type `, Shift gives tilde (~)" => Some("keycode_tooltips.grave_accent_type_grave_shift_gives_tilde_tilde"),
        "Comma — type comma (,), Shift gives less-than (<)" => Some("keycode_tooltips.comma_type_comma_shift_gives_less_than"),
        "Period — type dot (.), Shift gives greater-than (>)" => Some("keycode_tooltips.period_type_dot_shift_gives_greater_than"),
        "Slash — type /, Shift gives question mark (?)" => Some("keycode_tooltips.slash_type_shift_gives_question_mark"),
        r#"Non-US backslash key — type \, Shift gives pipe (|)"# => Some("keycode_tooltips.non_us_backslash_key_type_backslash_shift_gives_pipe"),
        "Home — jump to beginning of line" => Some("keycode_tooltips.home_jump_to_beginning_of_line"),
        "End — jump to end of line" => Some("keycode_tooltips.end_jump_to_end_of_line"),
        "Page Up — scroll up one page" => Some("keycode_tooltips.page_up_scroll_up_one_page"),
        "Page Down — scroll down one page" => Some("keycode_tooltips.page_down_scroll_down_one_page"),
        "Insert — toggle insert/overwrite mode" => Some("keycode_tooltips.insert_toggle_insert_overwrite_mode"),
        "Delete — delete character after cursor" => Some("keycode_tooltips.delete_delete_character_after_cursor"),
        "Print Screen — take a screenshot" => Some("keycode_tooltips.print_screen_take_a_screenshot"),
        "Pause / Break" => Some("keycode_tooltips.pause_break"),
        "Execute — run the currently selected action or file" => Some("keycode_tooltips.execute_run_the_currently_selected_action_or_file"),
        "Help — open help for the current app or context" => Some("keycode_tooltips.help_open_help_for_the_current_app_or_context"),
        "Select — select the current item" => Some("keycode_tooltips.select_select_the_current_item"),
        "Stop — cancel the current action or loading" => Some("keycode_tooltips.stop_cancel_the_current_action_or_loading"),
        "Again — repeat the previous action" => Some("keycode_tooltips.again_repeat_the_previous_action"),
        "Undo — revert the last action" => Some("keycode_tooltips.undo_revert_the_last_action"),
        "Cut — remove selection and copy it to clipboard" => Some("keycode_tooltips.cut_remove_selection_and_copy_it_to_clipboard"),
        "Copy — copy selection to clipboard" => Some("keycode_tooltips.copy_copy_selection_to_clipboard"),
        "Paste — insert clipboard contents" => Some("keycode_tooltips.paste_insert_clipboard_contents"),
        "Find — search in the current document or view" => Some("keycode_tooltips.find_search_in_the_current_document_or_view"),
        "Mouse acceleration 0 — slowest cursor speed profile" => Some("keycode_tooltips.mouse_acceleration_0_slowest_cursor_speed_profile"),
        "Mouse acceleration 1 — medium cursor speed profile" => Some("keycode_tooltips.mouse_acceleration_1_medium_cursor_speed_profile"),
        "Mouse acceleration 2 — fastest cursor speed profile" => Some("keycode_tooltips.mouse_acceleration_2_fastest_cursor_speed_profile"),
        r#"JIS \ and _"# => Some("keycode_tooltips.jis_backslash_and"),
        "JIS Katakana/Hiragana" => Some("keycode_tooltips.jis_katakana_hiragana"),
        "JIS ¥ and |" => Some("keycode_tooltips.jis_and"),
        "JIS Henkan" => Some("keycode_tooltips.jis_henkan"),
        "JIS Muhenkan" => Some("keycode_tooltips.jis_muhenkan"),
        "JIS Numpad ," => Some("keycode_tooltips.jis_numpad"),
        "Hangul/English" => Some("keycode_tooltips.hangul_english"),
        "Hanja" => Some("keycode_tooltips.hanja"),
        "JIS Katakana" => Some("keycode_tooltips.jis_katakana"),
        "JIS Hiragana" => Some("keycode_tooltips.jis_hiragana"),
        "JIS Zenkaku/Hankaku" => Some("keycode_tooltips.jis_zenkaku_hankaku"),
        "Compile firmware" => Some("keycode_tooltips.compile_firmware"),
        "RGB lighting — toggle on/off" => Some("keycode_tooltips.rgb_lighting_toggle_on_off"),
        "RGB lighting — next animation mode" => Some("keycode_tooltips.rgb_lighting_next_animation_mode"),
        "RGB lighting — previous animation mode" => Some("keycode_tooltips.rgb_lighting_previous_animation_mode"),
        "RGB lighting — hue +" => Some("keycode_tooltips.rgb_lighting_hue_plus"),
        "RGB lighting — hue −" => Some("keycode_tooltips.rgb_lighting_hue"),
        "RGB lighting — saturation +" => Some("keycode_tooltips.rgb_lighting_saturation_plus"),
        "RGB lighting — saturation −" => Some("keycode_tooltips.rgb_lighting_saturation"),
        "RGB lighting — brightness +" => Some("keycode_tooltips.rgb_lighting_brightness_plus"),
        "RGB lighting — brightness −" => Some("keycode_tooltips.rgb_lighting_brightness"),
        "RGB lighting — animation speed +" => Some("keycode_tooltips.rgb_lighting_animation_speed_plus"),
        "RGB lighting — animation speed −" => Some("keycode_tooltips.rgb_lighting_animation_speed"),
        r#"✕
None"# => Some("keycode_tooltips.cancel_none"),
        r#"▽
Inherit"# => Some("keycode_tooltips.inherit_inherit"),
        r#"🔒
Lock"# => Some("keycode_tooltips.lock_lock"),
        r#"Combo
Toggle"# => Some("keycode_tooltips.combo_toggle"),
        "KC_NO — disables this key completely, it sends nothing when pressed" => Some("keycode_tooltips.kc_no_disables_this_key_completely_it_sends_nothing_when_pressed"),
        "KC_TRNS — inherits the key from the layer below" => Some("keycode_tooltips.kc_trns_inherits_the_key_from_the_layer_below"),
        "Bootloader — put device into flash mode" => Some("keycode_tooltips.bootloader_put_keyboard_into_flash_mode"),
        "Debug toggle — enable/disable debug output" => Some("keycode_tooltips.debug_toggle_enable_disable_debug_output"),
        "Lock — lock a key in pressed state until pressed again" => Some("keycode_tooltips.lock_lock_a_key_in_pressed_state_until_pressed_again"),
        "Toggles the state of the Auto Shift feature" => Some("keycode_tooltips.toggles_the_state_of_the_auto_shift_feature"),
        "Toggles Combo feature on and off" => Some("keycode_tooltips.toggles_combo_feature_on_and_off"),
        "Capitalizes until end of current word" => Some("keycode_tooltips.capitalizes_until_end_of_current_word"),
        "Repeats the last pressed key" => Some("keycode_tooltips.repeats_the_last_pressed_key"),
        "Alt repeats the last pressed key" => Some("keycode_tooltips.alt_repeats_the_last_pressed_key"),
        r#"Mouse
Up"# => Some("keycode_tooltips.mouse_up"),
        r#"Mouse
Down"# => Some("keycode_tooltips.mouse_down"),
        r#"Mouse
Left"# => Some("keycode_tooltips.mouse_left"),
        r#"Mouse
Right"# => Some("keycode_tooltips.mouse_right"),
        r#"Scroll
Up"# => Some("keycode_tooltips.scroll_up"),
        r#"Scroll
Down"# => Some("keycode_tooltips.scroll_down"),
        r#"Scroll
Left"# => Some("keycode_tooltips.scroll_left"),
        r#"Scroll
Right"# => Some("keycode_tooltips.scroll_right"),
        r#"Accel
0"# => Some("keycode_tooltips.accel_0"),
        r#"Accel
1"# => Some("keycode_tooltips.accel_1"),
        r#"Accel
2"# => Some("keycode_tooltips.accel_2"),
        r#"⏻
Power"# => Some("keycode_tooltips.power_power"),
        r#"🌙
Sleep"# => Some("keycode_tooltips.sleep_sleep"),
        r#"☀
Wake"# => Some("keycode_tooltips.sun_wake"),
        r#"🔇
Mute"# => Some("keycode_tooltips.mute_mute"),
        r#"🔉
Vol-"# => Some("keycode_tooltips.vol_down_vol"),
        r#"🔊
Vol+"# => Some("keycode_tooltips.vol_up_vol_plus"),
        r#"⏮
Prev"# => Some("keycode_tooltips.previous_prev"),
        r#"⏭
Next"# => Some("keycode_tooltips.next_next"),
        r#"⏹
Stop"# => Some("keycode_tooltips.stop_stop"),
        r#"🎵
Media"# => Some("keycode_tooltips.media_media"),
        r#"✉
Mail"# => Some("keycode_tooltips.mail_mail"),
        r#"🖩
Calc"# => Some("keycode_tooltips.calc_calc"),
        r#"💻
Files"# => Some("keycode_tooltips.files_files"),
        r#"🔍
Search"# => Some("keycode_tooltips.search_search"),
        r#"⬅
Back"# => Some("keycode_tooltips.back_back"),
        r#"➡
Forward"# => Some("keycode_tooltips.forward_forward"),
        r#"↻
Refresh"# => Some("keycode_tooltips.refresh_refresh"),
        r#"★
Favs"# => Some("keycode_tooltips.favs_favs"),
        r#"⏩
Fwd"# => Some("keycode_tooltips.fast_forward_fwd"),
        r#"⏪
Rew"# => Some("keycode_tooltips.rewind_rew"),
        r#"☀+
Bright"# => Some("keycode_tooltips.sun_plus_bright"),
        r#"☀-
Bright"# => Some("keycode_tooltips.sun_bright"),
        r#"Ctrl
View"# => Some("keycode_tooltips.ctrl_view"),
        r#"Launch
Pad"# => Some("keycode_tooltips.launch_pad"),
        "Mute / Unmute audio" => Some("keycode_tooltips.mute_unmute_audio"),
        "Volume Up" => Some("keycode_tooltips.volume_up"),
        "Volume Down" => Some("keycode_tooltips.volume_down"),
        "Next Track" => Some("keycode_tooltips.next_track"),
        "Previous Track" => Some("keycode_tooltips.previous_track"),
        "Stop playback" => Some("keycode_tooltips.stop_playback"),
        "Play / Pause" => Some("keycode_tooltips.play_pause"),
        "Open media player" => Some("keycode_tooltips.open_media_player"),
        "Open email client" => Some("keycode_tooltips.open_email_client"),
        "Open calculator" => Some("keycode_tooltips.open_calculator"),
        "Open My Computer / file manager" => Some("keycode_tooltips.open_my_computer_file_manager"),
        "Browser search" => Some("keycode_tooltips.browser_search"),
        "Browser home page" => Some("keycode_tooltips.browser_home_page"),
        "Browser back" => Some("keycode_tooltips.browser_back"),
        "Browser forward" => Some("keycode_tooltips.browser_forward"),
        "Browser stop loading" => Some("keycode_tooltips.browser_stop_loading"),
        "Browser refresh" => Some("keycode_tooltips.browser_refresh"),
        "Browser favourites" => Some("keycode_tooltips.browser_favourites"),
        "Sleep — put computer to sleep" => Some("keycode_tooltips.sleep_put_computer_to_sleep"),
        "Wake — wake computer from sleep" => Some("keycode_tooltips.wake_wake_computer_from_sleep"),
        "Brightness Up" => Some("keycode_tooltips.brightness_up"),
        "Brightness Down" => Some("keycode_tooltips.brightness_down"),
        "Power — system power button" => Some("keycode_tooltips.power_system_power_button"),
        "Eject — eject removable media" => Some("keycode_tooltips.eject_eject_removable_media"),
        "Fast Forward — jump forward in media" => Some("keycode_tooltips.fast_forward_jump_forward_in_media"),
        "Rewind — jump backward in media" => Some("keycode_tooltips.rewind_jump_backward_in_media"),
        "Mission Control / Task View — show open windows and spaces" => Some("keycode_tooltips.mission_control_task_view_show_open_windows_and_spaces"),
        "Launchpad / app launcher" => Some("keycode_tooltips.launchpad_app_launcher"),
        "Undo" => Some("key_names.undo"),
        "On" => Some("key_names.on"),
        "Off" => Some("key_names.off"),
        r#"⏯
Play"# => Some("key_names.play"),
        r#"⏏
Eject"# => Some("key_names.eject"),
        r#"🏠
Home"# => Some("key_names.home_icon"),
        "Redo" => Some("key_names.redo"),
        "Cut" => Some("key_names.cut"),
        "Copy" => Some("key_names.copy"),
        "Paste" => Some("key_names.paste"),
        "Find" => Some("key_names.find"),
        r#"Prev
Word"# => Some("key_names.prev_word"),
        r#"Next
Word"# => Some("key_names.next_word"),
        r#"Prev
App"# => Some("key_names.prev_app"),
        r#"Next
App"# => Some("key_names.next_app"),
        "Lock" => Some("key_names.lock"),
        "Swap" => Some("key_names.swap"),
        "Restore" => Some("key_names.restore"),
        "Toggle" => Some("key_names.toggle"),
        "as Caps" => Some("key_names.as_caps"),
        "as Ctrl" => Some("key_names.as_ctrl"),
        "Left" => Some("key_names.left"),
        "Right" => Some("key_names.right"),
        "Num Lock — toggle numpad number input" => Some("keycode_tooltips.num_lock_toggle_numpad_number_input"),
        "Numpad ÷ (divide)" => Some("keycode_tooltips.numpad_divide"),
        "Numpad × (multiply)" => Some("keycode_tooltips.numpad_multiply"),
        "Numpad − (minus)" => Some("keycode_tooltips.numpad_minus"),
        "Numpad + (plus)" => Some("keycode_tooltips.numpad_plus_plus"),
        "Numpad Enter" => Some("keycode_tooltips.numpad_enter"),
        "Numpad . (decimal point)" => Some("keycode_tooltips.numpad_decimal_point"),
        "Numpad = (equals)" => Some("keycode_tooltips.numpad_equals"),
        "Numpad , (comma)" => Some("keycode_tooltips.numpad_comma"),
        "Left Control when held, ( when tapped" => Some("keycode_tooltips.left_control_when_held_when_tapped"),
        "Right Control when held, ) when tapped" => Some("keycode_tooltips.right_control_when_held_when_tapped"),
        "Left Shift when held, ( when tapped" => Some("keycode_tooltips.left_shift_when_held_when_tapped"),
        "Right Shift when held, ) when tapped" => Some("keycode_tooltips.right_shift_when_held_when_tapped"),
        "Left Alt when held, ( when tapped" => Some("keycode_tooltips.left_alt_when_held_when_tapped"),
        "Right Alt when held, ) when tapped" => Some("keycode_tooltips.right_alt_when_held_when_tapped"),
        "Right Shift when held, Enter when tapped" => Some("keycode_tooltips.right_shift_when_held_enter_when_tapped"),
        "Left Ctrl — modifier key (hold to activate shortcuts)" => Some("keycode_tooltips.left_ctrl_modifier_key_hold_to_activate_shortcuts"),
        "Right Ctrl — modifier key (hold to activate shortcuts)" => Some("keycode_tooltips.right_ctrl_modifier_key_hold_to_activate_shortcuts"),
        "Left Shift — hold to type uppercase / shifted symbols" => Some("keycode_tooltips.left_shift_hold_to_type_uppercase_shifted_symbols"),
        "Right Shift — hold to type uppercase / shifted symbols" => Some("keycode_tooltips.right_shift_hold_to_type_uppercase_shifted_symbols"),
        "Left Alt — modifier key (hold to activate shortcuts)" => Some("keycode_tooltips.left_alt_modifier_key_hold_to_activate_shortcuts"),
        "Right Alt / AltGr — access special characters" => Some("keycode_tooltips.right_alt_altgr_access_special_characters"),
        "Plain modifier — hold for left/right side, tap nothing" => Some("keycode_tooltips.plain_modifier_hold_for_left_right_side_tap_nothing"),
        "Arrow Up" => Some("keycode_tooltips.arrow_up"),
        "Arrow Down" => Some("keycode_tooltips.arrow_down"),
        "Arrow Left" => Some("keycode_tooltips.arrow_left"),
        "Arrow Right" => Some("keycode_tooltips.arrow_right"),
        "Left Control" => Some("keycode_tooltips.left_control"),
        "Right Control" => Some("keycode_tooltips.right_control"),
        "Left Shift" => Some("keycode_tooltips.left_shift_key"),
        "Right Shift" => Some("keycode_tooltips.right_shift_key"),
        "Left Alt" => Some("keycode_tooltips.left_alt_key"),
        "Right Alt" => Some("keycode_tooltips.right_alt_key"),
        "Swap Caps Lock and Left Control" => Some("keycode_tooltips.swap_caps_lock_and_left_control"),
        "Unswap Caps Lock and Left Control" => Some("keycode_tooltips.unswap_caps_lock_and_left_control"),
        "Toggle Caps Lock and Left Control swap" => Some("keycode_tooltips.toggle_caps_lock_and_left_control_swap"),
        "Stop treating Caps Lock as Control" => Some("keycode_tooltips.stop_treating_caps_lock_as_control"),
        "Treat Caps Lock as Control" => Some("keycode_tooltips.treat_caps_lock_as_control"),
        "Swap ` and Escape" => Some("keycode_tooltips.swap_grave_and_escape"),
        "Unswap ` and Escape" => Some("keycode_tooltips.unswap_grave_and_escape"),
        r#"Swap \ and Backspace"# => Some("keycode_tooltips.swap_backslash_and_backspace"),
        r#"Unswap \ and Backspace"# => Some("keycode_tooltips.unswap_backslash_and_backspace"),
        r#"Toggle \ and Backspace swap state"# => Some("keycode_tooltips.toggle_backslash_and_backspace_swap_state"),
        "Enable N-key rollover" => Some("keycode_tooltips.enable_n_key_rollover"),
        "Disable N-key rollover" => Some("keycode_tooltips.disable_n_key_rollover"),
        "Toggle N-key rollover" => Some("keycode_tooltips.toggle_n_key_rollover"),
        "Set the master half of a split keyboard as the left hand (for EE_HANDS)" => Some("keycode_tooltips.set_the_master_half_of_a_split_keyboard_as_the_left_hand_for_ee_hands"),
        "Set the master half of a split keyboard as the right hand (for EE_HANDS)" => Some("keycode_tooltips.set_the_master_half_of_a_split_keyboard_as_the_right_hand_for_ee_hands"),
        "Swap Caps Lock and Escape" => Some("keycode_tooltips.swap_caps_lock_and_escape"),
        "Unswap Caps Lock and Escape" => Some("keycode_tooltips.unswap_caps_lock_and_escape"),
        "Toggle Caps Lock and Escape swap" => Some("keycode_tooltips.toggle_caps_lock_and_escape_swap"),
        _ => None,
    }
}

pub fn tr_text(language: Language, text: &str) -> String {
    if let Some(key) = static_catalog_key(text).or_else(|| exact_text_catalog_key(text)) {
        return tr_catalog_string(language, key);
    }

    if !matches!(language, Language::Russian) {
        return text.to_owned();
    }

    match text {
        "Device not found" => tr_catalog_string(language, "dynamic_status.device_not_found"),
        "Connect thread died" => tr_catalog_string(language, "dynamic_status.connect_thread_died"),
        other
            if other
                == format!(
                    "Open failed: {}",
                    crate::hid::MACOS_HID_INPUT_MONITORING_REQUIRED
                ) =>
        {
            tr_catalog_string(
                language,
                "dynamic_status.macos_hid_input_monitoring_required",
            )
        }
        "✓ Saved" => tr_catalog_string(language, "dynamic_status.saved_check"),
        "Device locked" => tr_catalog_string(language, "dynamic_status.device_locked"),
        "Entropy refreshed from a repeated launch" => {
            tr_catalog_string(language, "dynamic_status.entropy_refreshed")
        }
        other if other.starts_with("Connecting to ") => {
            let device = other
                .strip_prefix("Connecting to ")
                .unwrap_or(other)
                .trim_end_matches('…');
            tr_catalog_string_format(
                language,
                "dynamic_status.connecting_to",
                &[("device", device)],
            )
        }
        other if other.starts_with("Connected: ") => tr_catalog_string_format(
            language,
            "dynamic_status.connected",
            &[("device", other.strip_prefix("Connected: ").unwrap_or(other))],
        ),
        other if other.starts_with("Failed to save ") && other.contains(": ") => {
            let rest = other.strip_prefix("Failed to save ").unwrap_or(other);
            let (item, error) = rest.split_once(": ").unwrap_or((rest, ""));
            tr_catalog_string_format(
                language,
                "dynamic_status.failed_to_save",
                &[("item", item), ("error", error)],
            )
        }
        other if other.starts_with("Failed to update ") && other.contains(": ") => {
            let rest = other.strip_prefix("Failed to update ").unwrap_or(other);
            let (item, error) = rest.split_once(": ").unwrap_or((rest, ""));
            tr_catalog_string_format(
                language,
                "dynamic_status.failed_to_update",
                &[("item", item), ("error", error)],
            )
        }
        other if other.starts_with("Set encoder failed: ") => tr_catalog_string_format(
            language,
            "dynamic_status.set_encoder_failed",
            &[(
                "error",
                other.strip_prefix("Set encoder failed: ").unwrap_or(other),
            )],
        ),
        other if other.starts_with("Write error: ") => tr_catalog_string_format(
            language,
            "dynamic_status.write_error",
            &[(
                "error",
                other.strip_prefix("Write error: ").unwrap_or(other),
            )],
        ),
        other if other.starts_with("Lock failed: ") => tr_catalog_string_format(
            language,
            "dynamic_status.lock_failed",
            &[(
                "error",
                other.strip_prefix("Lock failed: ").unwrap_or(other),
            )],
        ),
        other
            if other.starts_with("Could not find ")
                && other.ends_with("; run it from the Entropy folder") =>
        {
            let script = other
                .strip_prefix("Could not find ")
                .and_then(|s| s.strip_suffix("; run it from the Entropy folder"))
                .unwrap_or(other);
            tr_catalog_string_format(
                language,
                "dynamic_status.could_not_find_script",
                &[("script", script)],
            )
        }
        other if other.contains(" install failed: ") => {
            let (name, error) = other.split_once(" install failed: ").unwrap_or((other, ""));
            tr_catalog_string_format(
                language,
                "dynamic_status.install_failed",
                &[("name", name), ("error", error)],
            )
        }
        other if other.starts_with("Could not run ") && other.contains(": ") => {
            let rest = other.strip_prefix("Could not run ").unwrap_or(other);
            let (command, error) = rest.split_once(": ").unwrap_or((rest, ""));
            tr_catalog_string_format(
                language,
                "dynamic_status.could_not_run",
                &[("command", command), ("error", error)],
            )
        }
        other if other.starts_with("Grave/Escape — sends Esc normally") => other
            .replace(
                "Grave/Escape — sends Esc normally, ` when Shift or",
                tr_catalog(language, "dynamic_tooltips.grave_escape_prefix"),
            )
            .replace("is held", ""),
        other if other.starts_with("Mouse cursor — move ") => other
            .replace(
                "Mouse cursor — move up",
                tr_catalog(language, "dynamic_tooltips.mouse_cursor_move_up"),
            )
            .replace(
                "Mouse cursor — move down",
                tr_catalog(language, "dynamic_tooltips.mouse_cursor_move_down"),
            )
            .replace(
                "Mouse cursor — move left",
                tr_catalog(language, "dynamic_tooltips.mouse_cursor_move_left"),
            )
            .replace(
                "Mouse cursor — move right",
                tr_catalog(language, "dynamic_tooltips.mouse_cursor_move_right"),
            ),
        other if other.starts_with("Mouse button ") => other
            .replace(
                "Mouse button",
                tr_catalog(language, "dynamic_tooltips.mouse_button"),
            )
            .replace(
                "left click",
                tr_catalog(language, "dynamic_tooltips.left_click"),
            )
            .replace(
                "right click",
                tr_catalog(language, "dynamic_tooltips.right_click"),
            )
            .replace(
                "middle click",
                tr_catalog(language, "dynamic_tooltips.middle_click"),
            )
            .replace("back", tr_catalog(language, "dynamic_tooltips.back"))
            .replace("forward", tr_catalog(language, "dynamic_tooltips.forward")),
        other if other.starts_with("Mouse wheel — scroll ") => other
            .replace(
                "Mouse wheel — scroll up",
                tr_catalog(language, "dynamic_tooltips.mouse_wheel_scroll_up"),
            )
            .replace(
                "Mouse wheel — scroll down",
                tr_catalog(language, "dynamic_tooltips.mouse_wheel_scroll_down"),
            )
            .replace(
                "Mouse wheel — scroll left",
                tr_catalog(language, "dynamic_tooltips.mouse_wheel_scroll_left"),
            )
            .replace(
                "Mouse wheel — scroll right",
                tr_catalog(language, "dynamic_tooltips.mouse_wheel_scroll_right"),
            ),
        other if other.starts_with("Browser ") => other
            .replace(
                "Browser search",
                tr_catalog(language, "dynamic_tooltips.browser_search"),
            )
            .replace(
                "Browser home page",
                tr_catalog(language, "dynamic_tooltips.browser_home_page"),
            )
            .replace(
                "Browser back",
                tr_catalog(language, "dynamic_tooltips.browser_back"),
            )
            .replace(
                "Browser forward",
                tr_catalog(language, "dynamic_tooltips.browser_forward"),
            )
            .replace(
                "Browser stop loading",
                tr_catalog(language, "dynamic_tooltips.browser_stop_loading"),
            )
            .replace(
                "Browser refresh",
                tr_catalog(language, "dynamic_tooltips.browser_refresh"),
            )
            .replace(
                "Browser favourites",
                tr_catalog(language, "dynamic_tooltips.browser_favourites"),
            ),
        other if other.starts_with("Left Cmd, ") => other.replace(
            "Left Cmd, macOS modifier key and app shortcuts",
            tr_catalog(language, "dynamic_tooltips.left_cmd_desc"),
        ),
        other if other.starts_with("Right Cmd, ") => other.replace(
            "Right Cmd, macOS modifier key and app shortcuts",
            tr_catalog(language, "dynamic_tooltips.right_cmd_desc"),
        ),
        other if other.starts_with("Left Win, ") => other.replace(
            "Left Win, Windows modifier key and OS shortcuts",
            tr_catalog(language, "dynamic_tooltips.left_win_desc"),
        ),
        other if other.starts_with("Right Win, ") => other.replace(
            "Right Win, Windows modifier key and OS shortcuts",
            tr_catalog(language, "dynamic_tooltips.right_win_desc"),
        ),
        other if other.starts_with("Left Super, ") => other.replace(
            "Left Super, desktop modifier key and OS shortcuts",
            tr_catalog(language, "dynamic_tooltips.left_super_desc"),
        ),
        other if other.starts_with("Right Super, ") => other.replace(
            "Right Super, desktop modifier key and OS shortcuts",
            tr_catalog(language, "dynamic_tooltips.right_super_desc"),
        ),
        other if other.starts_with("Use ") && other.contains(" by itself as a held modifier") => {
            let modifier = other
                .strip_prefix("Use ")
                .and_then(|s| s.split(" by itself as a held modifier").next())
                .unwrap_or("");
            let modifier_ru = ru_modifier_name(modifier);
            if other.contains("Left click assigns Left ") {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.plain_modifier_with_clicks",
                    &[
                        ("modifier_ru", modifier_ru.as_str()),
                        ("modifier", modifier),
                    ],
                )
            } else {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.plain_modifier_simple",
                    &[("modifier_ru", modifier_ru.as_str())],
                )
            }
        }
        other if other.starts_with("Hold ") && other.contains(" together with another key") => {
            let modifier = other
                .strip_prefix("Hold ")
                .and_then(|s| s.split(" together with another key").next())
                .unwrap_or("");
            let modifier_ru = ru_modifier_name(modifier);
            if other.contains("Left click starts a Left ") {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.mod_plus_key_with_clicks",
                    &[
                        ("modifier_ru", modifier_ru.as_str()),
                        ("modifier", modifier),
                    ],
                )
            } else {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.mod_plus_key_simple",
                    &[("modifier_ru", modifier_ru.as_str())],
                )
            }
        }
        other if other.starts_with("Hold ") && other.contains(" with the key you choose next") => {
            other
                .replace("Hold ", tr_catalog(language, "dynamic_tooltips.hold"))
                .replace("Left ", tr_catalog(language, "dynamic_tooltips.left"))
                .replace("Right ", tr_catalog(language, "dynamic_tooltips.right"))
                .replace(
                    " together with the key you choose next",
                    tr_catalog(language, "dynamic_tooltips.hold_with_next_suffix"),
                )
        }
        other if other.starts_with("Dual-role key: hold for ") => {
            let modifier = other
                .strip_prefix("Dual-role key: hold for ")
                .and_then(|s| s.split(',').next())
                .unwrap_or("");
            let tap_text = if other.contains("tap for the key you choose next") {
                tr_catalog(language, "dynamic_tooltips.tap_text_next")
            } else {
                tr_catalog(language, "dynamic_tooltips.tap_text_other")
            };
            let modifier_ru = ru_modifier_name(modifier);
            if other.contains("Left click uses Left ") {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.dual_role_with_clicks",
                    &[
                        ("modifier_ru", modifier_ru.as_str()),
                        ("tap_text", tap_text),
                        ("modifier", modifier),
                    ],
                )
            } else {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.dual_role_simple",
                    &[
                        ("modifier_ru", modifier_ru.as_str()),
                        ("tap_text", tap_text),
                    ],
                )
            }
        }
        other if other.starts_with("Applies ") && other.contains(" to the next keypress only") => {
            let modifier = other
                .strip_prefix("Applies ")
                .and_then(|s| s.split(" to the next keypress only").next())
                .unwrap_or("");
            let modifier_ru = ru_modifier_name(modifier);
            if other.contains("Left click assigns One-Shot Left ") {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.one_shot_modifier_with_clicks",
                    &[
                        ("modifier_ru", modifier_ru.as_str()),
                        ("modifier", modifier),
                    ],
                )
            } else {
                tr_catalog_string_format(
                    language,
                    "dynamic_tooltips.one_shot_modifier_simple",
                    &[("modifier_ru", modifier_ru.as_str())],
                )
            }
        }
        other if other.starts_with("Universal symbol: ") && other.contains(" — types ") => {
            let rest = other.strip_prefix("Universal symbol: ").unwrap_or(other);
            let (name, rest) = rest.split_once(" — types ").unwrap_or((rest, ""));
            let symbol = rest
                .split(" consistently regardless of the active keyboard language")
                .next()
                .unwrap_or(rest);
            tr_catalog_string_format(
                language,
                "dynamic_tooltips.universal_symbol",
                &[("name", ru_smart_symbol_name(name)), ("symbol", symbol)],
            )
        }
        other if other.starts_with("One-Shot ") => other
            .replace(
                " — active for the next keypress only",
                tr_catalog(language, "dynamic_tooltips.one_shot_active_next"),
            )
            .replace(
                " — applies ",
                tr_catalog(language, "dynamic_tooltips.applies"),
            )
            .replace(
                " to the next keypress only",
                tr_catalog(language, "dynamic_tooltips.to_the_next_keypress_only"),
            )
            .replace(
                " — activates ",
                tr_catalog(language, "dynamic_tooltips.activates"),
            )
            .replace(
                " for the very next keypress only",
                tr_catalog(language, "dynamic_tooltips.one_shot_active_next"),
            ),
        other if other.starts_with("RGB Matrix: solid color") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_solid_color")
        }
        other if other.starts_with("RGB Matrix: breathing effect") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_breathing_effect")
        }
        other if other.starts_with("RGB Matrix: rainbow gradient") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_rainbow_gradient")
        }
        other if other.starts_with("RGB Matrix: swirling rainbow") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_swirl")
        }
        other if other.starts_with("RGB Matrix: snake animation") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_snake")
        }
        other if other.starts_with("RGB Matrix: Knight Rider") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_knight_rider")
        }
        other if other.starts_with("RGB Matrix: alternating red and green") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_christmas")
        }
        other if other.starts_with("RGB Matrix: static gradient") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_static_gradient")
        }
        other if other.starts_with("RGB Matrix: test mode") => {
            tr_catalog_string(language, "dynamic_tooltips.rgb_matrix_test_mode")
        }
        other if other.starts_with("Swap Left Alt and ") => other.replace(
            "Swap Left Alt and ",
            tr_catalog(language, "dynamic_tooltips.swap_left_alt_and"),
        ),
        other if other.starts_with("Unswap Left Alt and ") => other.replace(
            "Unswap Left Alt and ",
            tr_catalog(language, "dynamic_tooltips.unswap_left_alt_and"),
        ),
        other if other.starts_with("Swap Right Alt and ") => other.replace(
            "Swap Right Alt and ",
            tr_catalog(language, "dynamic_tooltips.swap_right_alt_and"),
        ),
        other if other.starts_with("Unswap Right Alt and ") => other.replace(
            "Unswap Right Alt and ",
            tr_catalog(language, "dynamic_tooltips.unswap_right_alt_and"),
        ),
        other if other.starts_with("Enable the ") && other.ends_with(" keys") => other
            .replace(
                "Enable the ",
                tr_catalog(language, "dynamic_tooltips.enable_the"),
            )
            .replace(" keys", ""),
        other if other.starts_with("Disable the ") && other.ends_with(" keys") => other
            .replace(
                "Disable the ",
                tr_catalog(language, "dynamic_tooltips.disable_the"),
            )
            .replace(" keys", ""),
        other if other.starts_with("Toggles the status of the ") => other
            .replace(
                "Toggles the status of the ",
                tr_catalog(language, "dynamic_tooltips.toggle_keys_prefix"),
            )
            .replace(" keys", ""),
        other if other.starts_with("Swap Alt and ") && other.ends_with(" on both sides") => other
            .replace(
                "Swap Alt and ",
                tr_catalog(language, "dynamic_tooltips.swap_alt_and"),
            )
            .replace(
                " on both sides",
                tr_catalog(language, "dynamic_tooltips.on_both_sides"),
            ),
        other if other.starts_with("Unswap Alt and ") && other.ends_with(" on both sides") => other
            .replace(
                "Unswap Alt and ",
                tr_catalog(language, "dynamic_tooltips.unswap_alt_and"),
            )
            .replace(
                " on both sides",
                tr_catalog(language, "dynamic_tooltips.on_both_sides"),
            ),
        other if other.starts_with("Toggle Alt and ") && other.ends_with(" swap on both sides") => {
            other
                .replace(
                    "Toggle Alt and ",
                    tr_catalog(language, "dynamic_tooltips.toggle_alt_and"),
                )
                .replace(
                    " swap on both sides",
                    tr_catalog(language, "dynamic_tooltips.swap_on_both_sides"),
                )
        }
        other if other.starts_with("Swap Left Control and ") => other.replace(
            "Swap Left Control and ",
            tr_catalog(language, "dynamic_tooltips.swap_left_control_and"),
        ),
        other if other.starts_with("Unswap Left Control and ") => other.replace(
            "Unswap Left Control and ",
            tr_catalog(language, "dynamic_tooltips.unswap_left_control_and"),
        ),
        other if other.starts_with("Swap Right Control and ") => other.replace(
            "Swap Right Control and ",
            tr_catalog(language, "dynamic_tooltips.swap_right_control_and"),
        ),
        other if other.starts_with("Unswap Right Control and ") => other.replace(
            "Unswap Right Control and ",
            tr_catalog(language, "dynamic_tooltips.unswap_right_control_and"),
        ),
        other if other.starts_with("Swap Control and ") && other.ends_with(" on both sides") => {
            other
                .replace(
                    "Swap Control and ",
                    tr_catalog(language, "dynamic_tooltips.swap_control_and"),
                )
                .replace(
                    " on both sides",
                    tr_catalog(language, "dynamic_tooltips.on_both_sides"),
                )
        }
        other if other.starts_with("Unswap Control and ") && other.ends_with(" on both sides") => {
            other
                .replace(
                    "Unswap Control and ",
                    tr_catalog(language, "dynamic_tooltips.unswap_control_and"),
                )
                .replace(
                    " on both sides",
                    tr_catalog(language, "dynamic_tooltips.on_both_sides"),
                )
        }
        other
            if other.starts_with("Toggle Control and ")
                && other.ends_with(" swap on both sides") =>
        {
            other
                .replace(
                    "Toggle Control and ",
                    tr_catalog(language, "dynamic_tooltips.toggle_control_and"),
                )
                .replace(
                    " swap on both sides",
                    tr_catalog(language, "dynamic_tooltips.swap_on_both_sides"),
                )
        }
        other
            if other.starts_with("Layer ") && other[6..].chars().all(|ch| ch.is_ascii_digit()) =>
        {
            other.replace("Layer ", tr_catalog(language, "dynamic_tooltips.layer"))
        }
        other if other.starts_with("Pick key for ") => other.replace(
            "Pick key for ",
            tr_catalog(language, "dynamic_tooltips.pick_key_for"),
        ),
        other if other.starts_with("Momentarily activate layer ") => other
            .replace(
                "Momentarily activate layer ",
                tr_catalog(language, "dynamic_tooltips.momentary_layer_prefix"),
            )
            .replace(
                " while held",
                tr_catalog(language, "dynamic_tooltips.while_held"),
            ),
        other if other.starts_with("Key: ") => other
            .replace("Key: ", tr_catalog(language, "dynamic_tooltips.key"))
            .replace(
                "Left Ctrl",
                tr_catalog(language, "dynamic_tooltips.left_ctrl"),
            )
            .replace(
                "Right Ctrl",
                tr_catalog(language, "dynamic_tooltips.right_ctrl"),
            )
            .replace(
                "Left Shift",
                tr_catalog(language, "dynamic_tooltips.left_shift"),
            )
            .replace(
                "Right Shift",
                tr_catalog(language, "dynamic_tooltips.right_shift"),
            )
            .replace(
                "Left Alt",
                tr_catalog(language, "dynamic_tooltips.left_alt"),
            )
            .replace(
                "Right Alt",
                tr_catalog(language, "dynamic_tooltips.right_alt"),
            )
            .replace(
                "Left Cmd",
                tr_catalog(language, "dynamic_tooltips.left_cmd"),
            )
            .replace(
                "Right Cmd",
                tr_catalog(language, "dynamic_tooltips.right_cmd"),
            )
            .replace(
                "Left Win",
                tr_catalog(language, "dynamic_tooltips.left_win"),
            )
            .replace(
                "Right Win",
                tr_catalog(language, "dynamic_tooltips.right_win"),
            )
            .replace(
                "Left Super",
                tr_catalog(language, "dynamic_tooltips.left_super"),
            )
            .replace(
                "Right Super",
                tr_catalog(language, "dynamic_tooltips.right_super"),
            ),
        other if other.ends_with(" function key") => other.replace(
            " function key",
            tr_catalog(language, "dynamic_tooltips.function_key"),
        ),
        other if other.starts_with("Numpad ") => {
            other.replace("Numpad ", tr_catalog(language, "dynamic_tooltips.numpad"))
        }
        other if other.starts_with("Shortcut: ") => other
            .replace(
                "Shortcut: ",
                tr_catalog(language, "dynamic_tooltips.shortcut"),
            )
            .replace("Right ", tr_catalog(language, "dynamic_tooltips.right")),
        other
            if other.starts_with("Macro ")
                && other.contains(" — sends a sequence of keystrokes") =>
        {
            other
                .replace("Macro ", tr_catalog(language, "dynamic_tooltips.macro"))
                .replace(
                    " — sends a sequence of keystrokes",
                    tr_catalog(language, "dynamic_tooltips.macro_sends_suffix"),
                )
        }
        other
            if other.starts_with("Tap Dance ")
                && other.contains(" — different actions on tap, hold, double tap") =>
        {
            other.replace(
                " — different actions on tap, hold, double tap",
                tr_catalog(language, "dynamic_tooltips.tap_dance_desc_suffix"),
            )
        }
        other if other.contains(" — macro ") => other.replace(
            " — macro ",
            tr_catalog(language, "dynamic_tooltips.macro_separator"),
        ),
        other if other.contains(" — tap dance ") => {
            other.replace(" — tap dance ", " — Tap Dance ")
        }
        other if other.starts_with("MO(") => other
            .replace(
                " — activate layer ",
                tr_catalog(language, "dynamic_tooltips.activate_layer"),
            )
            .replace(
                " — activate ",
                tr_catalog(language, "dynamic_tooltips.activate"),
            )
            .replace(
                " while held, return when released",
                tr_catalog(language, "dynamic_tooltips.layer_held_return_suffix"),
            ),
        other if other.starts_with("TO(") => other
            .replace(
                " — switch to layer ",
                tr_catalog(language, "dynamic_tooltips.switch_to_layer"),
            )
            .replace(
                " — switch to ",
                tr_catalog(language, "dynamic_tooltips.switch_to"),
            )
            .replace(
                " and stay there",
                tr_catalog(language, "dynamic_tooltips.and_stay_there"),
            ),
        other if other.starts_with("TG(") => other
            .replace(
                " — toggle layer ",
                tr_catalog(language, "dynamic_tooltips.toggle_layer"),
            )
            .replace(
                " — toggle ",
                tr_catalog(language, "dynamic_tooltips.toggle"),
            )
            .replace(" on/off", tr_catalog(language, "dynamic_tooltips.on_off")),
        other if other.starts_with("DF(") || other.starts_with("PDF(") => other
            .replace(" — set ", tr_catalog(language, "dynamic_tooltips.set"))
            .replace(
                " — permanently set ",
                tr_catalog(language, "dynamic_tooltips.permanently_set"),
            )
            .replace(
                " as the default base layer",
                tr_catalog(language, "dynamic_tooltips.as_the_default_base_layer"),
            )
            .replace(
                " as the default layer",
                tr_catalog(language, "dynamic_tooltips.as_the_default_layer"),
            ),
        other if other.starts_with("OSL(") => other
            .replace(
                " — activate layer ",
                tr_catalog(language, "dynamic_tooltips.activate_layer"),
            )
            .replace(
                " — activate ",
                tr_catalog(language, "dynamic_tooltips.activate"),
            )
            .replace(
                " for next keypress only",
                tr_catalog(language, "dynamic_tooltips.for_next_keypress_only"),
            ),
        other if other.starts_with("TT(") => other
            .replace(
                " — tap to toggle layer ",
                tr_catalog(language, "dynamic_tooltips.tap_to_toggle_layer"),
            )
            .replace(
                " — tap to toggle ",
                tr_catalog(language, "dynamic_tooltips.tap_to_toggle"),
            )
            .replace(
                ", hold to activate while held",
                tr_catalog(language, "dynamic_tooltips.tt_hold_suffix"),
            ),
        other if other.starts_with("Layer Tap — tap for ") => other
            .replace(
                "Layer Tap — tap for ",
                tr_catalog(language, "dynamic_tooltips.layer_tap_tap_for"),
            )
            .replace(
                ", hold to activate layer ",
                tr_catalog(language, "dynamic_tooltips.hold_to_activate_layer"),
            )
            .replace(
                ", hold to activate ",
                tr_catalog(language, "dynamic_tooltips.hold_to_activate"),
            ),
        other if other.starts_with("LM(") => other
            .replace(
                " — activate layer ",
                tr_catalog(language, "dynamic_tooltips.activate_layer"),
            )
            .replace(" with ", tr_catalog(language, "dynamic_tooltips.with"))
            .replace(
                " held while key is pressed",
                tr_catalog(language, "dynamic_tooltips.lm_held_suffix"),
            ),
        other if other.starts_with("Unknown layer op ") => other.replace(
            "Unknown layer op",
            tr_catalog(language, "dynamic_tooltips.unknown_layer_op"),
        ),
        other if other.starts_with("Custom key ") => other.replace(
            "Custom key",
            tr_catalog(language, "dynamic_tooltips.custom_key"),
        ),
        other if other.starts_with("Unknown keycode ") => other.replace(
            "Unknown keycode",
            tr_catalog(language, "dynamic_tooltips.unknown_keycode"),
        ),
        other if other.starts_with("Mod Tap — tap for ") => other
            .replace(
                "Mod Tap — tap for ",
                tr_catalog(language, "dynamic_tooltips.mod_tap_tap_for"),
            )
            .replace(
                ", hold for Left ",
                tr_catalog(language, "dynamic_tooltips.hold_for_left"),
            )
            .replace(
                ", hold for Right ",
                tr_catalog(language, "dynamic_tooltips.hold_for_right"),
            )
            .replace(
                ", hold for ",
                tr_catalog(language, "dynamic_tooltips.hold_for"),
            ),
        other if other.starts_with("Universal Cyrillic ") => other
            .replace("types", tr_catalog(language, "dynamic_tooltips.types"))
            .replace(
                "consistently regardless of the active keyboard language",
                tr_catalog(language, "dynamic_tooltips.universal_cyrillic_consistent"),
            )
            .replace(
                "hold Shift for",
                tr_catalog(language, "dynamic_tooltips.hold_shift_for"),
            ),
        other if other.starts_with("Universal output backend: Wayland via IBus input method") => {
            other.replacen(
                "Universal output backend: Wayland via IBus input method",
                tr_catalog(language, "dynamic_tooltips.backend_wayland_ibus"),
                1,
            )
        }
        other if other.starts_with("Universal output backend: Linux X11 native") => other.replacen(
            "Universal output backend: Linux X11 native; Wayland uses IBus",
            tr_catalog(language, "dynamic_tooltips.backend_linux_x11_native"),
            1,
        ),
        other if other.starts_with("Universal output backend: Linux; use IBus for Wayland") => {
            other.replacen(
                "Universal output backend: Linux; use IBus for Wayland",
                tr_catalog(language, "dynamic_tooltips.backend_linux_wayland_hint"),
                1,
            )
        }
        other => other.to_owned(),
    }
}
