use super::*;

fn module_setting_catalog_keys(title: &str) -> Option<(&'static str, &'static str)> {
    match title.to_ascii_lowercase().as_str() {
        "display brightness" => Some((
            "modules_settings.display_brightness",
            "modules_settings.display_brightness_tooltip",
        )),
        "display background" => Some((
            "modules_settings.display_background",
            "modules_settings.display_background_tooltip",
        )),
        "display text size" => Some((
            "modules_settings.display_text_size",
            "modules_settings.display_text_size_tooltip",
        )),
        "module" => Some((
            "modules_settings.phenom_module_type",
            "modules_settings.phenom_module_type_tooltip",
        )),
        "mode" => Some(("modules_settings.mode", "modules_settings.mode_tooltip")),
        "ball axis" => Some((
            "modules_settings.ball_axis",
            "modules_settings.ball_axis_tooltip",
        )),
        "touch axis" => Some((
            "modules_settings.touch_axis",
            "modules_settings.touch_axis_tooltip",
        )),
        "ball dpi" => Some((
            "modules_settings.ball_dpi",
            "modules_settings.ball_dpi_tooltip",
        )),
        "touch dpi" => Some((
            "modules_settings.touch_dpi",
            "modules_settings.touch_dpi_tooltip",
        )),
        "encoder interval" => Some((
            "modules_settings.encoder_interval",
            "modules_settings.encoder_interval_tooltip",
        )),
        "encoder steps" => Some((
            "modules_settings.encoder_steps",
            "modules_settings.encoder_steps_tooltip",
        )),
        "scroll sens" => Some((
            "modules_settings.scroll_sens",
            "modules_settings.scroll_sens_tooltip",
        )),
        "sniper sens" => Some((
            "modules_settings.sniper_sens",
            "modules_settings.sniper_sens_tooltip",
        )),
        "text sens" => Some((
            "modules_settings.text_sens",
            "modules_settings.text_sens_tooltip",
        )),
        "touch gestures" => Some((
            "modules_settings.touch_gestures",
            "modules_settings.touch_gestures_tooltip",
        )),
        "invert scroll" => Some((
            "modules_settings.invert_scroll",
            "modules_settings.invert_scroll_tooltip",
        )),
        "invert scroll vertical" => Some((
            "modules_settings.invert_scroll_vertical",
            "modules_settings.invert_scroll_vertical_tooltip",
        )),
        "invert scroll horizontal" => Some((
            "modules_settings.invert_scroll_horizontal",
            "modules_settings.invert_scroll_horizontal_tooltip",
        )),
        "invert text" => Some((
            "modules_settings.invert_text",
            "modules_settings.invert_text_tooltip",
        )),
        "invert text vertical" => Some((
            "modules_settings.invert_text_vertical",
            "modules_settings.invert_text_vertical_tooltip",
        )),
        "invert text horizontal" => Some((
            "modules_settings.invert_text_horizontal",
            "modules_settings.invert_text_horizontal_tooltip",
        )),
        "acceleration" => Some((
            "modules_settings.acceleration",
            "modules_settings.acceleration_tooltip",
        )),
        "sticky mode" => Some((
            "modules_settings.sticky_mode",
            "modules_settings.sticky_mode_tooltip",
        )),
        "led blinks" => Some((
            "modules_settings.led_blinks",
            "modules_settings.led_blinks_tooltip",
        )),
        "auto layer in normal" => Some((
            "modules_settings.auto_layer_in_normal",
            "modules_settings.auto_layer_normal_tooltip",
        )),
        "auto layer" => Some((
            "modules_settings.auto_layer",
            "modules_settings.auto_layer_tooltip",
        )),
        "auto layer in sniper" => Some((
            "modules_settings.auto_layer_in_sniper",
            "modules_settings.auto_layer_sniper_tooltip",
        )),
        "auto layer in scroll" => Some((
            "modules_settings.auto_layer_in_scroll",
            "modules_settings.auto_layer_scroll_tooltip",
        )),
        "auto layer in text" => Some((
            "modules_settings.auto_layer_in_text",
            "modules_settings.auto_layer_text_tooltip",
        )),
        "auto layer timeout" => Some((
            "modules_settings.auto_layer_timeout",
            "modules_settings.auto_layer_timeout_tooltip",
        )),
        "auto layer threshold" => Some((
            "modules_settings.auto_layer_threshold",
            "modules_settings.auto_layer_threshold_tooltip",
        )),
        "auto layer deactivate on key" => Some((
            "modules_settings.auto_layer_deactivate_on_key",
            "modules_settings.auto_layer_deactivate_on_key_tooltip",
        )),
        "trackball enabled" => Some((
            "modules_settings.trackball_enabled",
            "modules_settings.trackball_enabled_tooltip",
        )),
        _ => None,
    }
}

fn module_setting_variant_label(language: crate::i18n::Language, variant: &str) -> String {
    let key = match variant.to_ascii_lowercase().as_str() {
        "black" => Some("modules_settings.display_black"),
        "blue" => Some("modules_settings.display_blue"),
        "compact" => Some("modules_settings.display_compact"),
        "large" => Some("modules_settings.display_large"),
        "none" => Some("modules_settings.none"),
        "normal" => Some("modules_settings.normal"),
        "sniper" => Some("modules_settings.sniper"),
        "scroll" => Some("modules_settings.scroll"),
        "text" => Some("modules_settings.text"),
        "trackball" => Some("modules_settings.trackball"),
        "touchpad" => Some("modules_settings.touchpad"),
        "ball" => Some("modules_settings.ball"),
        "touch" => Some("modules_settings.touch"),
        "encoder" => Some("modules_settings.encoder"),
        _ => None,
    };
    key.map(|key| crate::i18n::tr_catalog(language, key).to_owned())
        .unwrap_or_else(|| crate::i18n::tr_text(language, variant))
}

#[derive(Clone, Copy)]
enum ModuleSettingsRow {
    SideSelector,
    Section(usize),
    Field { group_idx: usize, field_idx: usize },
}

// Vial's K:03 tabs are per encoder, but the general module parser coalesces them
// by side. Their field titles retain the side-local encoder number.
pub(super) fn fixed_encoder_side_size(layout: &KeyboardLayout) -> Option<usize> {
    let name = layout
        .name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match name.as_str() {
        "k03" | "ergohavenk03" => Some(3),
        "imperial44" | "ergohavenimperial44" => Some(1),
        _ => None,
    }
}

pub(super) fn fixed_encoder_field_number(title: &str, side_size: usize) -> Option<usize> {
    let words = title.split_whitespace().collect::<Vec<_>>();
    match words.as_slice() {
        [encoder, number, setting]
            if side_size == 3
                && encoder.eq_ignore_ascii_case("encoder")
                && matches!(setting.to_ascii_lowercase().as_str(), "interval" | "steps") =>
        {
            number
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=side_size).contains(n))
        }
        [encoder, setting]
            if side_size == 1
                && encoder.eq_ignore_ascii_case("encoder")
                && matches!(setting.to_ascii_lowercase().as_str(), "interval" | "steps") =>
        {
            Some(1)
        }
        _ => None,
    }
}

impl EntropyApp {
    fn phenom_vial_model(&self) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let name: String = layout
            .name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        if layout.firmware != FirmwareProtocol::Vial
            || !matches!(
                name.as_str(),
                "phenom"
                    | "ergohavenphenom"
                    | "phenommini"
                    | "ergohavenphenommini"
                    | "phenommicro"
                    | "ergohavenphenommicro"
            )
        {
            return false;
        }
        true
    }

    fn module_presentation_model(&self) -> bool {
        if self.phenom_vial_model() {
            return true;
        }
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let name: String = layout
            .name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        layout.firmware == FirmwareProtocol::Rynk
            && matches!(
                name.as_str(),
                "k04"
                    | "ergohavenk04"
                    | "k04mini"
                    | "ergohavenk04mini"
                    | "k04micro"
                    | "ergohavenk04micro"
                    | "k04qube"
                    | "ergohavenk04qube"
                    | "k04miniqube"
                    | "ergohavenk04miniqube"
                    | "k04microqube"
                    | "ergohavenk04microqube"
                    | "k04qubemini"
                    | "ergohavenk04qubemini"
                    | "k04qubemicro"
                    | "ergohavenk04qubemicro"
            )
    }

    // Presentation parity does not opt RMK into Phenom wire/value projections.
    fn module_presentation_pointing_group(&self, kind: ModuleSettingsGroupKind) -> bool {
        if !self.module_presentation_model() {
            return false;
        }
        let Some(idx) = self
            .module_settings
            .groups
            .iter()
            .position(|g| g.kind == kind)
        else {
            return false;
        };
        matches!(
            kind,
            ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
        ) && (self.module_settings.groups[idx]
            .module_selector_field()
            .is_none()
            || matches!(
                self.selected_module_kind(idx),
                Some(ModuleDeviceKind::Trackball | ModuleDeviceKind::Touchpad)
            ))
    }

    fn phenom_pointing_module_group(&self, kind: ModuleSettingsGroupKind) -> bool {
        self.phenom_vial_model() && self.module_presentation_pointing_group(kind)
    }

    fn module_presentation_trackball_group(&self, kind: ModuleSettingsGroupKind) -> bool {
        self.module_presentation_pointing_group(kind)
            && self
                .module_settings
                .groups
                .iter()
                .position(|g| g.kind == kind)
                .is_some_and(|idx| {
                    self.module_settings.groups[idx]
                        .module_selector_field()
                        .is_none()
                        || self.selected_module_kind(idx) == Some(ModuleDeviceKind::Trackball)
                })
    }

    fn phenom_trackball_catalog_keys(
        &self,
        kind: ModuleSettingsGroupKind,
        title: &str,
    ) -> Option<(&'static str, &'static str)> {
        if !self.module_presentation_pointing_group(kind) {
            return None;
        }
        if self.module_presentation_trackball_group(kind) {
            let keys = match kind.field_base_title(title).to_ascii_lowercase().as_str() {
                "module" => Some((
                    "modules_settings.phenom_module_type",
                    "modules_settings.phenom_module_type_tooltip",
                )),
                "mode" => Some((
                    "modules_settings.phenom_module_mode",
                    "modules_settings.mode_tooltip",
                )),
                "ball dpi" => Some((
                    "modules_settings.phenom_ball_dpi",
                    "modules_settings.phenom_ball_dpi_tooltip",
                )),
                "acceleration" => Some((
                    "modules_settings.acceleration",
                    "modules_settings.phenom_acceleration_tooltip",
                )),
                _ => None,
            };
            if keys.is_some() {
                return keys;
            }
        }
        if !self.module_presentation_trackball_group(kind) {
            let keys = match kind.field_base_title(title).to_ascii_lowercase().as_str() {
                "mode" => Some((
                    "modules_settings.phenom_module_mode",
                    "modules_settings.mode_tooltip",
                )),
                "touch dpi" => Some((
                    "modules_settings.phenom_touch_dpi",
                    "modules_settings.phenom_touch_dpi_tooltip",
                )),
                "acceleration" => Some((
                    "modules_settings.acceleration",
                    "modules_settings.phenom_touch_acceleration_tooltip",
                )),
                "touch axis" => Some((
                    "modules_settings.phenom_touch_rotation",
                    "modules_settings.phenom_touch_rotation_tooltip",
                )),
                "touch gestures" => Some((
                    "modules_settings.touch_gestures",
                    "modules_settings.phenom_touch_gestures_tooltip",
                )),
                _ => None,
            };
            if keys.is_some() {
                return keys;
            }
        }
        match kind.field_base_title(title).to_ascii_lowercase().as_str() {
            "ball axis" => Some((
                "modules_settings.trackball_rotation",
                "modules_settings.trackball_rotation_tooltip",
            )),
            "sniper sens" => Some((
                "modules_settings.precise_speed",
                "modules_settings.precise_speed_tooltip",
            )),
            "scroll sens" => Some((
                "modules_settings.trackball_scroll_speed",
                "modules_settings.trackball_scroll_speed_tooltip",
            )),
            "text sens" => Some((
                "modules_settings.caret_speed",
                "modules_settings.caret_speed_tooltip",
            )),
            _ => None,
        }
    }

    fn phenom_ball_rotation_field(
        &self,
        kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> bool {
        // QMK stores an orientation enum, not degrees. Restrict this display
        // convention to the exact Phenom per-side Ball axis contract.
        self.phenom_pointing_module_group(kind)
            && matches!(
                (kind, field.qsid),
                (ModuleSettingsGroupKind::Left, 130) | (ModuleSettingsGroupKind::Right, 131)
            )
            && kind
                .field_base_title(&field.title)
                .eq_ignore_ascii_case("Ball axis")
            && field.kind == ModuleSettingKind::Select
            && field.width == 1
            && field.min == 0
            && field.max == 3
            && field.variants == ["0°", "90°", "180°", "270°"]
    }

    fn phenom_left_touch_rotation_field(
        &self,
        kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> bool {
        // Touch axis uses enum indices 0..3, not degrees. Only the left
        // touchpad has a 180-degree UI origin; the right remains identity.
        self.phenom_pointing_module_group(kind)
            && self
                .module_settings
                .groups
                .iter()
                .position(|g| g.kind == kind)
                .is_some_and(|idx| {
                    self.selected_module_kind(idx) == Some(ModuleDeviceKind::Touchpad)
                })
            && kind == ModuleSettingsGroupKind::Left
            && field.qsid == 132
            && kind
                .field_base_title(&field.title)
                .eq_ignore_ascii_case("Touch axis")
            && field.kind == ModuleSettingKind::Select
            && field.width == 1
            && field.min == 0
            && field.max == 3
            && field.variants == ["0°", "90°", "180°", "270°"]
    }

    fn phenom_mode_speed_field(
        &self,
        kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> bool {
        let expected = match (
            kind,
            kind.field_base_title(&field.title)
                .to_ascii_lowercase()
                .as_str(),
        ) {
            (ModuleSettingsGroupKind::Left, "sniper sens") => 124,
            (ModuleSettingsGroupKind::Left, "scroll sens") => 125,
            (ModuleSettingsGroupKind::Left, "text sens") => 126,
            (ModuleSettingsGroupKind::Right, "sniper sens") => 127,
            (ModuleSettingsGroupKind::Right, "scroll sens") => 128,
            (ModuleSettingsGroupKind::Right, "text sens") => 129,
            _ => return false,
        };
        // K04 RMK uses the same per-side divisor contract, but not Phenom's
        // independent rotation origins. Keep those predicates separate.
        self.module_presentation_pointing_group(kind)
            && field.qsid == expected
            && field.kind == ModuleSettingKind::Integer
            && field.width == 1
            && field.min >= 1
            && field.min < field.max
            && field.max <= 255
    }

    pub(super) fn global_encoder_settings_group(&self) -> Option<usize> {
        if !self.module_settings.supported {
            return None;
        }
        self.module_settings
            .groups
            .iter()
            .position(ModuleSettingsGroup::is_global_encoder_settings)
    }

    // Never move a real module selector (K:04) or a mixed module page.
    pub(super) fn encoder_only_module_settings(&self, layout: &KeyboardLayout) -> bool {
        if self.global_encoder_settings_group().is_some() {
            return self.module_settings.groups.len() == 1 && layout.encoder_count() > 0;
        }
        let Some(side_size) = fixed_encoder_side_size(layout) else {
            return false;
        };
        if !self.module_settings.supported || layout.encoder_count() != side_size * 2 {
            return false;
        }
        let mut qsids = std::collections::BTreeSet::new();
        for group in &self.module_settings.groups {
            let base = match group.kind {
                ModuleSettingsGroupKind::Left => 0,
                ModuleSettingsGroupKind::Right => side_size,
                _ => return false,
            };
            for field in &group.fields {
                let Some(number) = fixed_encoder_field_number(
                    group.kind.field_base_title(&field.title),
                    side_size,
                ) else {
                    return false;
                };
                let offset = if field.title.to_ascii_lowercase().ends_with(" interval") {
                    0
                } else {
                    1
                };
                if field.qsid != 340 + (2 * (base + number - 1) + offset) as u16
                    || !qsids.insert(field.qsid)
                {
                    return false;
                }
            }
        }
        qsids.len() == side_size * 4
    }

    // Before the first shared write, older firmware may hold distinct values.
    // Do not show the first encoder's value as if it applied to every encoder.
    fn shared_encoder_values_mixed(&self, qsid: u16) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        if self.global_encoder_settings_group().is_some() || !self.encoder_only_module_settings(layout) {
            return false;
        }
        let count = fixed_encoder_side_size(layout).unwrap_or(0) * 2;
        let parity = qsid.saturating_sub(340) % 2;
        let mut values = (0..count).map(|idx| {
            let id = 340 + (idx * 2) as u16 + parity;
            self.pending_settings_write_value(id)
                .unwrap_or_else(|| self.module_settings.value(id))
        });
        let Some(first) = values.next() else {
            return false;
        };
        values.any(|value| value != first)
    }

    pub(super) fn module_settings_title_key(&self) -> &'static str {
        if self.module_settings.is_trackball_page() {
            "modules_settings.trackball_title"
        } else if self.module_settings.is_touchpad_page() {
            "modules_settings.touchpad_title"
        } else {
            "modules_settings.title"
        }
    }

    fn module_settings_description_key(&self) -> &'static str {
        if self.module_settings.is_trackball_page() {
            "modules_settings.trackball_description"
        } else if self.module_settings.is_touchpad_page() {
            "modules_settings.touchpad_description"
        } else {
            "modules_settings.description"
        }
    }

    fn phenom_auto_layer_catalog_keys(
        &self,
        kind: ModuleSettingsGroupKind,
        title: &str,
    ) -> Option<(&'static str, &'static str)> {
        if !self.module_presentation_model() {
            return None;
        }
        let base = kind.field_base_title(title).to_ascii_lowercase();
        if kind == ModuleSettingsGroupKind::AutoLayer {
            match base.as_str() {
                "auto layer in normal" => Some((
                    "modules_settings.phenom_auto_enabled",
                    "modules_settings.phenom_auto_enabled_tooltip",
                )),
                "auto layer in sniper" => Some((
                    "modules_settings.phenom_auto_sniper",
                    "modules_settings.phenom_auto_sniper_tooltip",
                )),
                "auto layer in scroll" => Some((
                    "modules_settings.phenom_auto_scroll",
                    "modules_settings.phenom_auto_scroll_tooltip",
                )),
                "auto layer in text" => Some((
                    "modules_settings.phenom_auto_text",
                    "modules_settings.phenom_auto_text_tooltip",
                )),
                "auto layer" => Some((
                    "modules_settings.phenom_auto_number",
                    "modules_settings.phenom_auto_number_tooltip",
                )),
                "auto layer timeout" => Some((
                    "modules_settings.auto_layer_timeout",
                    "modules_settings.phenom_auto_timeout_tooltip",
                )),
                "auto layer deactivate on key" => Some((
                    "modules_settings.auto_layer_deactivate_on_key",
                    "modules_settings.phenom_auto_key_tooltip",
                )),
                _ => None,
            }
        } else if self.module_presentation_pointing_group(kind) && base == "sticky mode" {
            Some((
                "modules_settings.phenom_sticky_mode",
                "modules_settings.phenom_sticky_mode_tooltip",
            ))
        } else {
            None
        }
    }

    fn k04_threshold_field(
        &self,
        kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> bool {
        self.module_presentation_model()
            && !self.phenom_vial_model()
            && kind == ModuleSettingsGroupKind::AutoLayer
            && field.qsid == 334
            && field.title.eq_ignore_ascii_case("Auto layer threshold")
            && field.kind == ModuleSettingKind::Integer
            && field.width == 1
            && field.min == 0
            && field.max == 100
    }

    fn phenom_auto_timeout_field(
        &self,
        kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> bool {
        self.module_presentation_model()
            && kind == ModuleSettingsGroupKind::AutoLayer
            && field.qsid == 324
            && field.title.eq_ignore_ascii_case("Auto layer timeout")
            && field.kind == ModuleSettingKind::Select
            && field.width == 1
            && field.min == 0
            && usize::from(field.max) + 1 == field.variants.len()
            && field.variants.len() >= 2
            && field
                .variants
                .iter()
                .map(|v| v.strip_suffix(" ms").and_then(|s| s.parse::<u16>().ok()))
                .collect::<Option<Vec<_>>>()
                .is_some_and(|values| values[0] > 0 && values.windows(2).all(|p| p[0] < p[1]))
    }

    fn module_setting_label(&self, group_kind: ModuleSettingsGroupKind, title: &str) -> String {
        let lang = self.app_settings.language;
        if self
            .module_settings
            .groups
            .iter()
            .filter(|g| g.kind == group_kind)
            .flat_map(|g| &g.fields)
            .any(|f| f.title == title && self.k04_threshold_field(group_kind, f))
        {
            return crate::i18n::tr_catalog(lang, "modules_settings.k04_threshold").to_owned();
        }
        let display_title = group_kind.field_base_title(title);
        if self
            .layout
            .as_ref()
            .is_some_and(|layout| self.encoder_only_module_settings(layout))
        {
            let key = if display_title.to_ascii_lowercase().ends_with(" interval") {
                Some("modules_settings.encoder_interval")
            } else if display_title.to_ascii_lowercase().ends_with(" steps") {
                Some("modules_settings.encoder_steps")
            } else {
                None
            };
            if let Some(key) = key {
                return crate::i18n::tr_catalog(lang, key).to_owned();
            }
        }
        self.phenom_auto_layer_catalog_keys(group_kind, display_title)
            .or_else(|| self.phenom_trackball_catalog_keys(group_kind, display_title))
            .or_else(|| module_setting_catalog_keys(display_title))
            .map(|(label_key, _)| crate::i18n::tr_catalog(lang, label_key).to_owned())
            .unwrap_or_else(|| crate::i18n::tr_text(lang, display_title))
    }

    fn module_setting_tooltip(
        &self,
        group_kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> String {
        let lang = self.app_settings.language;
        if self.k04_threshold_field(group_kind, field) {
            return crate::i18n::tr_catalog(lang, "modules_settings.k04_threshold_tooltip")
                .to_owned();
        }
        let display_title = group_kind.field_base_title(&field.title);
        let base_title = if self
            .layout
            .as_ref()
            .is_some_and(|layout| self.encoder_only_module_settings(layout))
        {
            if display_title.to_ascii_lowercase().ends_with(" interval") {
                "Encoder interval"
            } else if display_title.to_ascii_lowercase().ends_with(" steps") {
                "Encoder steps"
            } else {
                display_title
            }
        } else {
            display_title
        };
        let key = self
            .phenom_auto_layer_catalog_keys(group_kind, base_title)
            .or_else(|| self.phenom_trackball_catalog_keys(group_kind, base_title))
            .or_else(|| module_setting_catalog_keys(base_title))
            .map(|(_, tooltip_key)| tooltip_key)
            .unwrap_or("modules_settings.generic_tooltip");
        let field_label = self.module_setting_label(group_kind, &field.title);
        crate::i18n::tr_catalog_format(lang, key, &[("field", field_label.as_str())])
    }

    fn module_setting_transport_value(field: &ModuleSettingField, value: u16) -> u16 {
        if field.width > 1 {
            value
        } else {
            value.min(u8::MAX as u16)
        }
    }

    fn write_module_setting_value(
        &mut self,
        group_idx: usize,
        field: &ModuleSettingField,
        value: u16,
    ) {
        // K:03 and Imperial44 expose per-encoder QSIDs in firmware. One UI
        // control must update every encoder, including hidden ones on both halves.
        if let Some(layout) = self.layout.as_ref() {
            if self.global_encoder_settings_group().is_none()
                && self.encoder_only_module_settings(layout)
            {
                let side_size = fixed_encoder_side_size(layout).expect("fixed encoder model");
                let selected_title = self
                    .module_settings
                    .groups
                    .get(group_idx)
                    .map(|group| group.kind.field_base_title(&field.title));
                let offset = match selected_title.and_then(|title| title.split_whitespace().last())
                {
                    Some(setting) if setting.eq_ignore_ascii_case("interval") => Some(0),
                    Some(setting) if setting.eq_ignore_ascii_case("steps") => Some(1),
                    _ => None,
                };
                if let Some(offset) = offset {
                    let targets = self
                        .module_settings
                        .groups
                        .iter()
                        .flat_map(|group| {
                            group.fields.iter().filter_map(move |target| {
                                let qsid = target.qsid;
                                let slot = (qsid.checked_sub(340)? as usize) / 2;
                                (slot < side_size * 2 && qsid == 340 + (slot * 2 + offset) as u16)
                                    .then(|| (group.title.clone(), target.clone()))
                            })
                        })
                        .collect::<Vec<_>>();
                    for (group_title, target) in targets {
                        let requested = Self::module_setting_transport_value(&target, value);
                        let old_value = self.module_settings.value(target.qsid);
                        let display_label = self.module_setting_label(
                            if target.qsid < 340 + side_size as u16 * 2 {
                                ModuleSettingsGroupKind::Left
                            } else {
                                ModuleSettingsGroupKind::Right
                            },
                            &target.title,
                        );
                        self.queue_module_setting_write(
                            group_title,
                            target.title,
                            display_label,
                            target.qsid,
                            target.width,
                            old_value,
                            requested,
                        );
                    }
                    self.sync_firmware_managed_layout_options();
                    return;
                }
            }
        }
        let group = self.module_settings.groups.get(group_idx);
        let group_title = group
            .map(|group| group.title.clone())
            .unwrap_or_else(|| "Modules".to_owned());
        let group_kind = group
            .map(|group| group.kind)
            .unwrap_or(ModuleSettingsGroupKind::Other);
        let field_title = field.title.clone();
        let display_label = self.module_setting_label(group_kind, &field_title);
        let old_value = self.module_settings.value(field.qsid);
        let requested = Self::module_setting_transport_value(field, value);
        if self.phenom_mode_speed_field(group_kind, field)
            || self.phenom_auto_timeout_field(group_kind, field)
            || self.k04_threshold_field(group_kind, field)
        {
            self.coalesce_pending_module_setting(field.qsid);
        }

        self.queue_module_setting_write(
            group_title,
            field_title,
            display_label,
            field.qsid,
            field.width,
            old_value,
            requested,
        );
        self.sync_firmware_managed_layout_options();
    }

    // Anchor every module slider to the existing Text navigation track. Reserve
    // the widest localized endpoint, right-aligned, without moving that track.
    fn module_slider_columns(
        ui: &egui::Ui,
        lang: crate::i18n::Language,
        metrics: crate::ui_style::ResponsiveMetrics,
        content_width: f32,
    ) -> (f32, f32, f32, f32) {
        let text_width = |key| {
            egui::WidgetText::from(crate::i18n::tr_catalog(lang, key))
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::FontSelection::Default,
                )
                .size()
                .x
        };
        let reference = text_width("modules_settings.speed_slower");
        let left = reference
            .max(text_width("modules_settings.speed_more_precise"))
            .max(text_width("modules_settings.speed_smoother"));
        let extra = left - reference;
        // The adaptive list may be wider than a very narrow viewport. Keep
        // these controls inside the visible row rather than painting past it.
        let right_edge = ui.clip_rect().right().min(ui.ctx().content_rect().right());
        let overflow = (ui.max_rect().left() + content_width - right_edge).max(0.0);
        let control = (metrics.value(250.0) + extra + overflow).min(content_width);
        let gap = ui.spacing().item_spacing.x;
        let right = text_width("modules_settings.speed_faster");
        let track = metrics
            .value(110.0)
            .min((control - overflow - left - right - 2.0 * gap).max(1.0));
        (content_width, control, left, track)
    }

    pub(super) fn draw_module_settings_field_row(
        &mut self,
        ui: &mut egui::Ui,
        group_idx: usize,
        field_idx: usize,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return;
        };
        let group_kind = group.kind;
        let Some(field) = group.fields.get(field_idx).cloned() else {
            return;
        };
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let dark = ui.visuals().dark_mode;
        let label = self.module_setting_label(group_kind, &field.title);
        let tooltip = if suppress_tooltips {
            None
        } else {
            Some(self.module_setting_tooltip(group_kind, &field))
        };
        let raw_value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
        match field.kind {
            ModuleSettingKind::Boolean => {
                let switch_width = metrics.value(46.0);
                let switch_size = metrics.size(46.0, 24.0);
                let mask = 1u16 << field.bit;
                let mut checked = raw_value & mask != 0;
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    switch_width,
                    |ui| {
                        let resp = crate::ui_style::settings_switch_sized_stable(
                            ui,
                            ("module_settings", group_idx, field.qsid, field.bit),
                            &mut checked,
                            switch_size,
                        );
                        if resp.changed() {
                            let new_value = if checked {
                                raw_value | mask
                            } else {
                                raw_value & !mask
                            };
                            self.write_module_setting_value(group_idx, &field, new_value);
                        }
                    },
                );
            }
            ModuleSettingKind::Integer if self.k04_threshold_field(group_kind, &field) => {
                let lang = self.app_settings.language;
                let mut value = raw_value;
                let (content_width, control_width, left_width, track_width) =
                    Self::module_slider_columns(
                        ui,
                        self.app_settings.language,
                        metrics,
                        content_width,
                    );
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    control_width,
                    |ui| {
                        ui.horizontal(|ui| {
                            let height = ui
                                .text_style_height(&egui::TextStyle::Body)
                                .max(ui.spacing().interact_size.y);
                            ui.allocate_ui_with_layout(
                                egui::vec2(left_width, height),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(crate::i18n::tr_catalog(
                                        lang,
                                        "modules_settings.k04_threshold_less",
                                    ));
                                },
                            );
                            ui.spacing_mut().slider_width = track_width;
                            let response = ui.add(
                                egui::Slider::new(&mut value, 0..=100)
                                    .clamping(egui::SliderClamping::Never)
                                    .step_by(1.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::slider(ui.is_enabled(), f64::from(value), &label)
                            });
                            // A numeric suffix overflows this fixed control slot and
                            // expands the centered parent, shifting following rows.
                            ui.label(crate::i18n::tr_catalog(
                                lang,
                                "modules_settings.k04_threshold_more",
                            ));
                            if response.changed() && value <= 100 && value != raw_value {
                                self.write_module_setting_value(group_idx, &field, value);
                            }
                        });
                    },
                );
            }
            ModuleSettingKind::Integer if self.phenom_mode_speed_field(group_kind, &field) => {
                let lang = self.app_settings.language;
                let (content_width, control_width, left_width, track_width) =
                    Self::module_slider_columns(ui, lang, metrics, content_width);
                // Integer reflection is lossless for every advertised divisor, not
                // a quantized percentage or a new firmware/storage representation.
                let sum = u32::from(field.min) + u32::from(field.max);
                let mut speed = (sum - u32::from(raw_value.clamp(field.min, field.max))) as u16;
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    control_width,
                    |ui| {
                        ui.horizontal(|ui| {
                            let height = ui
                                .text_style_height(&egui::TextStyle::Body)
                                .max(ui.spacing().interact_size.y);
                            ui.allocate_ui_with_layout(
                                egui::vec2(left_width, height),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(crate::i18n::tr_catalog(
                                        lang,
                                        match field.qsid {
                                            124 | 127 => "modules_settings.speed_more_precise",
                                            125 | 128 => "modules_settings.speed_smoother",
                                            _ => "modules_settings.speed_slower",
                                        },
                                    ));
                                },
                            );
                            ui.spacing_mut().slider_width = track_width;
                            let response = ui
                                .push_id(("phenom_speed", group_idx, field.qsid), |ui| {
                                    ui.add(
                                        egui::Slider::new(&mut speed, field.min..=field.max)
                                            .show_value(false)
                                            .trailing_fill(true),
                                    )
                                })
                                .inner;
                            response.widget_info(|| {
                                egui::WidgetInfo::slider(ui.is_enabled(), f64::from(speed), &label)
                            });
                            ui.label(crate::i18n::tr_catalog(
                                lang,
                                "modules_settings.speed_faster",
                            ));
                            if response.changed() {
                                let value = (sum - u32::from(speed)) as u16;
                                if value != raw_value {
                                    self.write_module_setting_value(group_idx, &field, value);
                                }
                            }
                        });
                    },
                );
            }
            ModuleSettingKind::Integer => {
                let field_width = metrics.value(86.0);
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    field_width,
                    |ui| {
                        let edit_id =
                            egui::Id::unique(("module_setting_edit", group_idx, field.qsid));
                        let current = raw_value.clamp(field.min, field.max);
                        let mut text = ui.ctx().data_mut(|d| {
                            d.get_temp::<String>(edit_id)
                                .unwrap_or_else(|| current.to_string())
                        });
                        if text.parse::<u16>().ok() != Some(current)
                            && !ui.memory(|m| m.has_focus(edit_id))
                        {
                            text = current.to_string();
                        }
                        let resp = crate::ui_style::modern_text_field_sized(
                            ui,
                            edit_id,
                            &mut text,
                            field_width,
                            metrics.settings_control_height(),
                            "",
                            5,
                            egui::Align::Center,
                        );
                        let commit = resp.lost_focus()
                            || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if commit {
                            match text.trim().parse::<u16>() {
                                Ok(value) => {
                                    let value = value.clamp(field.min, field.max);
                                    if value != raw_value {
                                        self.write_module_setting_value(group_idx, &field, value);
                                    }
                                    text = value.to_string();
                                }
                                Err(_) => text = current.to_string(),
                            }
                        }
                        ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                    },
                );
            }
            ModuleSettingKind::Select if self.phenom_auto_timeout_field(group_kind, &field) => {
                // Positions are advertised wire indices, not rounded milliseconds.
                // Keep an unknown loaded index untouched until an explicit edit.
                let mut index = raw_value;
                let (content_width, control_width, left_width, track_width) =
                    Self::module_slider_columns(
                        ui,
                        self.app_settings.language,
                        metrics,
                        content_width,
                    );
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    control_width,
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.allocate_exact_size(
                                egui::vec2(left_width, 0.0),
                                egui::Sense::hover(),
                            );
                            ui.spacing_mut().slider_width = track_width;
                            let response = ui
                                .push_id(("phenom_auto_timeout", group_idx, field.qsid), |ui| {
                                    ui.add(
                                        egui::Slider::new(&mut index, field.min..=field.max)
                                            .clamping(egui::SliderClamping::Never)
                                            .show_value(false)
                                            .trailing_fill(true),
                                    )
                                })
                                .inner;
                            response.widget_info(|| {
                                egui::WidgetInfo::slider(ui.is_enabled(), f64::from(index), &label)
                            });
                            let shown = field
                                .variants
                                .get(usize::from(raw_value))
                                .cloned()
                                .unwrap_or_else(|| format!("? ({raw_value})"));
                            ui.label(shown);
                            if response.changed() && index <= field.max && index != raw_value {
                                self.write_module_setting_value(group_idx, &field, index);
                            }
                        });
                    },
                );
            }
            ModuleSettingKind::Select => {
                let dropdown_width = metrics.value(120.0);
                let mixed = self.shared_encoder_values_mixed(field.qsid);
                let rotation_shift = if self.phenom_ball_rotation_field(group_kind, &field) {
                    3 // UI = (firmware - 90°) mod 360°, both trackball sides.
                } else if self.phenom_left_touch_rotation_field(group_kind, &field) {
                    2 // UI = (firmware - 180°) mod 360°, left touchpad only.
                } else {
                    0
                };
                let mut choices = field
                    .variants
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| {
                        *idx >= usize::from(field.min) && *idx <= usize::from(field.max)
                    })
                    .map(|(idx, variant)| {
                        // Keep the original wire index paired with its projected
                        // label: a UI choice writes the inverse mapping, never degrees.
                        (
                            idx,
                            if rotation_shift != 0 {
                                &field.variants[(idx + rotation_shift) % 4]
                            } else {
                                variant
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                if rotation_shift != 0 {
                    choices.sort_by_key(|(idx, _)| (idx + rotation_shift) % 4);
                }
                let mut selected_idx = choices
                    .iter()
                    .position(|(value, _)| *value == usize::from(raw_value))
                    .unwrap_or(0);
                let mut variants = choices
                    .iter()
                    .map(|(_, variant)| {
                        module_setting_variant_label(self.app_settings.language, variant)
                    })
                    .collect::<Vec<_>>();
                if mixed {
                    selected_idx = variants.len();
                    variants.push(
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "encoder_settings.mixed_values",
                        )
                        .to_owned(),
                    );
                }
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    dropdown_width,
                    |ui| {
                        let dropdown_id = ui.make_persistent_id((
                            "module_setting_dropdown",
                            group_idx,
                            field.qsid,
                        ));
                        let (_, picked) = Self::draw_touchpad_select_control(
                            ui,
                            dark,
                            dropdown_id,
                            selected_idx,
                            &variants,
                            dropdown_width,
                        );
                        if let Some((value, _)) = picked.and_then(|idx| choices.get(idx)) {
                            self.write_module_setting_value(group_idx, &field, *value as u16);
                        }
                    },
                );
            }
        }
    }

    fn module_settings_rows(&self) -> Vec<ModuleSettingsRow> {
        let mut rows = Vec::new();
        let side_groups = self.module_settings_side_group_indices();
        let selected_side_group = self.module_settings.selected_module_group();
        let selected_module =
            selected_side_group.and_then(|group_idx| self.selected_module_kind(group_idx));
        let selected_mode =
            selected_side_group.and_then(|group_idx| self.selected_pointer_mode(group_idx));
        if side_groups.len() > 1 {
            rows.push(ModuleSettingsRow::SideSelector);
        }
        // Placement only: keep the original side group, field and QSID so the
        // saved value and existing read/write path remain independent of auto-layer flags.
        let auto_group = self
            .module_settings
            .groups
            .iter()
            .position(|group| group.kind == ModuleSettingsGroupKind::AutoLayer);
        let sticky_field = selected_side_group.and_then(|group_idx| {
            let group = &self.module_settings.groups[group_idx];
            (auto_group.is_some() && self.module_presentation_pointing_group(group.kind))
                .then(|| {
                    group.fields.iter().position(|field| {
                        matches!(
                            (group.kind, field.qsid),
                            (ModuleSettingsGroupKind::Left, 140)
                                | (ModuleSettingsGroupKind::Right, 141)
                        ) && group
                            .kind
                            .field_base_title(&field.title)
                            .eq_ignore_ascii_case("Sticky mode")
                    })
                })
                .flatten()
                .map(|field_idx| (group_idx, field_idx))
        });
        if let Some(group_idx) = selected_side_group {
            rows.extend(
                self.module_settings_field_rows(group_idx, selected_module, selected_mode)
                    .into_iter()
                    .filter(|row| {
                        !matches!(row,
                    ModuleSettingsRow::Field { group_idx, field_idx }
                        if Some((*group_idx, *field_idx)) == sticky_field)
                    }),
            );
        }
        for (group_idx, group) in self.module_settings.groups.iter().enumerate() {
            if Some(group_idx) == self.global_encoder_settings_group() {
                continue;
            }
            if matches!(
                group.kind,
                ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
            ) {
                continue;
            }
            if group.kind == ModuleSettingsGroupKind::AutoLayer
                && matches!(
                    selected_module,
                    Some(ModuleDeviceKind::None | ModuleDeviceKind::Encoder)
                )
            {
                continue;
            }
            rows.push(ModuleSettingsRow::Section(group_idx));
            let mut fields =
                self.module_settings_field_rows(group_idx, selected_module, selected_mode);
            if Some(group_idx) == auto_group && self.module_presentation_model() {
                // Preserve selected-mode visibility and every original field owner.
                // Hide only the known threshold row; retain it for reads/presets.
                fields.retain(|row| !matches!(row, ModuleSettingsRow::Field { field_idx, .. }
                    if self.phenom_vial_model() && group.fields[*field_idx].qsid == 374
                        && group.fields[*field_idx].title.eq_ignore_ascii_case("Auto layer threshold")));
                fields.sort_by_key(|row| match row {
                    ModuleSettingsRow::Field { field_idx, .. } => {
                        match group.fields[*field_idx].qsid {
                            142 | 144 | 145 | 146 => 0,
                            143 => 1,
                            324 => 2,
                            375 => 3,
                            _ => 4,
                        }
                    }
                    _ => 4,
                });
                if let Some((group_idx, field_idx)) = sticky_field {
                    fields.push(ModuleSettingsRow::Field {
                        group_idx,
                        field_idx,
                    });
                }
            }
            rows.extend(fields);
        }
        rows
    }

    fn selected_module_kind(&self, group_idx: usize) -> Option<ModuleDeviceKind> {
        let group = self.module_settings.groups.get(group_idx)?;
        let field = group.module_selector_field()?;
        let value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
        group.selected_module_kind(value)
    }

    fn selected_pointer_mode(&self, group_idx: usize) -> Option<PointerModeKind> {
        let group = self.module_settings.groups.get(group_idx)?;
        let field = group.mode_selector_field()?;
        let value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
        group.selected_pointer_mode(value)
    }

    fn module_settings_field_rows(
        &self,
        group_idx: usize,
        selected_module: Option<ModuleDeviceKind>,
        selected_mode: Option<PointerModeKind>,
    ) -> Vec<ModuleSettingsRow> {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return Vec::new();
        };
        let mut fields = group
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                group.field_visible_for_selection(field, selected_module, selected_mode)
                    && !(self.module_presentation_model()
                        && !self.phenom_vial_model()
                        && matches!(field.qsid, 325 | 326)
                        && group
                            .kind
                            .field_base_title(&field.title)
                            .eq_ignore_ascii_case("Encoder interval"))
            })
            .map(|(field_idx, _)| ModuleSettingsRow::Field {
                group_idx,
                field_idx,
            })
            .collect::<Vec<_>>();
        if self.module_presentation_pointing_group(group.kind) {
            // Reorder rendered rows only; descriptor indices, saved values and
            // QSID ownership remain untouched. Mode-local rows retain their order.
            let touchpad = !self.module_presentation_trackball_group(group.kind);
            fields.sort_by_key(|row| match row {
                ModuleSettingsRow::Field { field_idx, .. } => match group
                    .kind
                    .field_base_title(&group.fields[*field_idx].title)
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "module" => 0,
                    "ball dpi" | "touch dpi" => 1,
                    "acceleration" => 2,
                    "ball axis" => 3,
                    "touch gestures" => {
                        if touchpad {
                            3
                        } else {
                            4
                        }
                    }
                    "touch axis" => {
                        if touchpad {
                            4
                        } else {
                            3
                        }
                    }
                    "mode" => 5,
                    _ => 6,
                },
                _ => 5,
            });
        }
        fields
    }

    fn module_settings_side_group_indices(&self) -> Vec<usize> {
        self.module_settings
            .groups
            .iter()
            .enumerate()
            .filter_map(|(idx, group)| {
                matches!(
                    group.kind,
                    ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
                )
                .then_some(idx)
            })
            .collect()
    }

    pub(super) fn module_settings_include_encoder_visibility(
        &self,
        layout: &KeyboardLayout,
    ) -> bool {
        self.module_settings.supported
            && self
                .module_settings_side_group_indices()
                .into_iter()
                .any(|group_idx| {
                    let Some(group) = self.module_settings.groups.get(group_idx) else {
                        return false;
                    };
                    group.supports_module_kind(ModuleDeviceKind::Encoder)
                        && Self::encoder_visibility_entry_for_module_group(layout, group.kind)
                            .is_some()
                })
    }

    pub(super) fn module_encoder_selectors_loaded(&self, layout: &KeyboardLayout) -> bool {
        self.module_settings
            .groups
            .iter()
            .filter(|group| {
                group.supports_module_kind(ModuleDeviceKind::Encoder)
                    && Self::encoder_visibility_entry_for_module_group(layout, group.kind).is_some()
            })
            .filter_map(ModuleSettingsGroup::module_selector_field)
            .all(|field| self.module_settings.values.contains_key(&field.qsid))
    }

    pub(super) fn hide_modular_encoders_by_default(&self, layout: &KeyboardLayout) -> bool {
        self.module_settings_include_encoder_visibility(layout)
            && !self.module_encoder_selectors_loaded(layout)
    }

    fn module_settings_group_label(&self, group: &ModuleSettingsGroup) -> String {
        let lang = self.app_settings.language;
        match group.kind {
            ModuleSettingsGroupKind::Left => {
                crate::i18n::tr_catalog(lang, "modules_settings.left_half").to_owned()
            }
            ModuleSettingsGroupKind::Right => {
                crate::i18n::tr_catalog(lang, "modules_settings.right_half").to_owned()
            }
            ModuleSettingsGroupKind::AutoLayer => crate::i18n::tr_catalog(
                lang,
                if self.module_presentation_model() {
                    "modules_settings.phenom_auto_section"
                } else {
                    "modules_settings.auto_layer_section"
                },
            )
            .to_owned(),
            ModuleSettingsGroupKind::Other if group.title.eq_ignore_ascii_case("Qube display") => {
                crate::i18n::tr_catalog(lang, "modules_settings.qube_display").to_owned()
            }
            ModuleSettingsGroupKind::Other => crate::i18n::tr_text(lang, &group.title),
        }
    }

    pub(super) fn draw_module_settings_section(
        &self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        group_idx: usize,
    ) {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return;
        };
        let dark = ui.visuals().dark_mode;
        let title = self.module_settings_group_label(group);
        let (row_rect, _) =
            ui.allocate_exact_size(egui::vec2(content_width, row_height), egui::Sense::hover());
        let separator =
            crate::ui_style::border_color(dark).gamma_multiply(if dark { 0.72 } else { 0.9 });
        ui.painter().line_segment(
            [row_rect.left_bottom(), row_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, separator),
        );
        ui.painter().text(
            row_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.5),
            app_muted_text(dark),
        );
    }

    fn draw_module_settings_side_selector(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let side_groups = self.module_settings_side_group_indices();
        if side_groups.len() <= 1 {
            return;
        }
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let dark = ui.visuals().dark_mode;
        let lang = self.app_settings.language;
        let selected_group = self
            .module_settings
            .selected_module_group()
            .unwrap_or(side_groups[0]);
        let selected_idx = side_groups
            .iter()
            .position(|group_idx| *group_idx == selected_group)
            .unwrap_or(0);
        let labels = side_groups
            .iter()
            .filter_map(|group_idx| self.module_settings.groups.get(*group_idx))
            .map(|group| {
                crate::i18n::tr_catalog(
                    lang,
                    if group.kind == ModuleSettingsGroupKind::Left {
                        "modules_settings.phenom_left"
                    } else {
                        "modules_settings.phenom_right"
                    },
                )
                .to_owned()
            })
            .collect::<Vec<_>>();
        let dropdown_width = metrics.value(120.0);
        let tooltip = (!suppress_tooltips).then(|| {
            crate::i18n::tr_catalog(lang, "modules_settings.module_side_tooltip").to_owned()
        });
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "modules_settings.module_side"),
            true,
            tooltip.as_deref(),
            dropdown_width,
            |ui| {
                let dropdown_id = ui.make_persistent_id("module_settings_side_dropdown");
                let (_, picked) = Self::draw_touchpad_select_control(
                    ui,
                    dark,
                    dropdown_id,
                    selected_idx,
                    &labels,
                    dropdown_width,
                );
                if let Some(picked) = picked.and_then(|idx| side_groups.get(idx).copied()) {
                    self.module_settings.set_selected_module_group(picked);
                }
            },
        );
    }

    fn draw_module_settings_row_entry(
        &mut self,
        ui: &mut egui::Ui,
        row: ModuleSettingsRow,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        match row {
            ModuleSettingsRow::SideSelector => self.draw_module_settings_side_selector(
                ui,
                content_width,
                row_height,
                suppress_tooltips,
            ),
            ModuleSettingsRow::Section(group_idx) => {
                self.draw_module_settings_section(ui, content_width, row_height, group_idx)
            }
            ModuleSettingsRow::Field {
                group_idx,
                field_idx,
            } => self.draw_module_settings_field_row(
                ui,
                group_idx,
                field_idx,
                content_width,
                row_height,
                suppress_tooltips,
            ),
        }
    }

    pub(super) fn draw_module_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        self.module_settings_title_key(),
                    ))
                    .size(18.0)
                    .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        self.module_settings_description_key(),
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.module_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "modules_settings.unavailable"),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::QmkSettingsEnableHint,
                        )),
                    );
                    return;
                }

                let rows = self.module_settings_rows();
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "module_settings",
                    metrics,
                    rows.len(),
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row_idx in list.first_visible_row..list.last_visible_row {
                        if let Some(row) = rows.get(row_idx).copied() {
                            self.draw_module_settings_row_entry(
                                ui,
                                row,
                                list.row_content_width,
                                list.row_height,
                                list.suppress_tooltips,
                            );
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
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("module_ui_series_tests.inc");
    include!("k04_sliders_tests.inc");
    include!("module_scope_regression.inc");
    include!("phenom_trackball_speed_tests.inc");
    include!("phenom_auto_layer_tests.inc");
    include!("phenom_timeout20_tests.inc");
    include!("phenom_slider_alignment_tests.inc");
    include!("phenom_sticky_section_tests.inc");
    include!("phenom_rotation_offset_tests.inc");
    include!("phenom_trackball_page_tests.inc");
    include!("module_side_selector_tests.inc");

    #[test]
    fn hpd_touchpad_groups_keep_dynamic_fields_and_own_the_touchpad_route() {
        let mut definition = serde_json::json!({
            "matrix": {"rows": 12, "cols": 7},
            "layouts": {"keymap": [["0,0", "0,0\n\n\n\n\n\n\n\n\ne", "0,1\n\n\n\n\n\n\n\n\ne"]]},
            "settings": [
                {"name": "Encoders", "fields": [
                    {"type": "select", "title": "Encoder interval", "qsid": 325, "variants": ["0 ms", "5 ms"]},
                    {"type": "select", "title": "Encoder steps", "qsid": 332, "variants": ["1", "2"]}
                ]},
                {"name": "Touchpad settings", "fields": [
                    {"type": "select", "title": "DPI", "qsid": 120, "variants": ["100", "200"]},
                    {"type": "integer", "title": "Sniper sens", "qsid": 121, "min": 1, "max": 255},
                    {"type": "integer", "title": "Scroll sens", "qsid": 122, "min": 1, "max": 255},
                    {"type": "integer", "title": "Text sens", "qsid": 123, "min": 1, "max": 255},
                    {"type": "boolean", "title": "Touch gestures", "qsid": 327, "bit": 0},
                    {"type": "boolean", "title": "Invert scroll vertical", "qsid": 124, "bit": 0},
                    {"type": "boolean", "title": "Invert scroll horizontal", "qsid": 124, "bit": 4},
                    {"type": "boolean", "title": "Invert text vertical", "qsid": 124, "bit": 3},
                    {"type": "boolean", "title": "Invert text horizontal", "qsid": 124, "bit": 5},
                    {"type": "boolean", "title": "Acceleration", "qsid": 124, "bit": 1},
                    {"type": "boolean", "title": "Sticky mode", "qsid": 124, "bit": 2},
                    {"type": "boolean", "title": "Auto layer enable", "qsid": 142, "bit": 0},
                    {"type": "select", "title": "Auto layer", "qsid": 143, "variants": ["0", "1"]},
                    {"type": "select", "title": "Auto layer timeout", "qsid": 324, "variants": ["250 ms", "500 ms"]}
                ]}
            ]
        });
        let supported = [120, 121, 122, 123, 124, 142, 143, 324, 325, 327, 332];
        for name in ["HPD v2 enc/touch", "HPD v2 touch/enc"] {
            definition["name"] = name.into();
            let layout = KeyboardLayout::from_vial_json(&definition).unwrap();
            let mut app = test_app();
            app.module_settings =
                EntropyApp::module_settings_from_definition(&definition, &supported);
            assert!(!EntropyApp::layout_json_has_touchpad_settings(&definition));
            assert!(app.module_settings.is_touchpad_page());
            assert!(!app.encoder_only_module_settings(&layout));
            assert!(app.global_encoder_settings_group().is_some());
            assert_eq!(
                app.module_settings_title_key(),
                "modules_settings.touchpad_title"
            );
            assert_eq!(
                crate::i18n::tr_catalog(
                    crate::i18n::Language::English,
                    app.module_settings_title_key()
                ),
                "Touchpad Settings"
            );
            let fields = app
                .module_settings_rows()
                .into_iter()
                .filter_map(|row| {
                    if let ModuleSettingsRow::Field {
                        group_idx,
                        field_idx,
                    } = row
                    {
                        Some(&app.module_settings.groups[group_idx].fields[field_idx])
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                definition["settings"][1]["fields"]
                    .as_array()
                    .unwrap()
                    .len()
            );
            assert_eq!(fields.iter().filter(|field| field.qsid == 124).count(), 6);
            assert!(fields.iter().any(|field| field.qsid == 327));
            assert!(fields.iter().any(|field| field.qsid == 324));
            assert!(!fields.iter().any(|field| matches!(field.qsid, 325 | 332)));
        }
        let partial = EntropyApp::module_settings_from_definition(&definition, &[120, 325, 332]);
        assert!(partial.is_touchpad_page());
        assert!(!partial.fields.iter().any(|field| field.qsid == 327));
        let encoders_only = EntropyApp::module_settings_from_definition(&definition, &[325, 332]);
        assert!(!encoders_only.is_touchpad_page());
    }

    #[test]
    fn mixed_module_groups_do_not_claim_a_touchpad_only_title() {
        let mut app = test_app();
        app.module_settings = EntropyApp::module_settings_from_definition(
            &serde_json::json!({"settings": [
                {"name": "Touchpad settings", "fields": [
                    {"type": "select", "title": "DPI", "qsid": 120, "variants": ["100"]}
                ]},
                {"name": "Left module", "fields": [
                    {"type": "select", "title": "Module", "qsid": 140, "variants": ["None", "Encoder", "Trackball", "Touchpad"]}
                ]}
            ]}),
            &[120, 140],
        );
        assert!(!app.module_settings.is_touchpad_page());
        assert_eq!(app.module_settings_title_key(), "modules_settings.title");
    }

    fn qube_display_definition() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/qube-display-settings.json"
        ))
        .unwrap()
    }

    #[test]
    fn qube_display_controls_require_firmware_fields_and_capabilities() {
        let json = qube_display_definition();
        let groups = EntropyApp::module_settings_groups(&json, &[336, 337, 338]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Qube display");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Other);
        assert_eq!(
            groups[0]
                .fields
                .iter()
                .map(|f| (f.qsid, f.width, f.max))
                .collect::<Vec<_>>(),
            vec![(336, 1, 100), (337, 1, 1), (338, 1, 1)]
        );
        assert!(EntropyApp::module_settings_groups(&json, &[316, 318, 330]).is_empty());
        assert!(
            EntropyApp::module_settings_groups(&serde_json::json!({}), &[336, 337, 338]).is_empty()
        );
        let partial = EntropyApp::module_settings_groups(&json, &[336]);
        assert_eq!(partial[0].fields.len(), 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn qube_display_writes_use_declared_byte_settings_and_stay_visible_without_modules() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        app.module_settings
            .groups
            .extend(EntropyApp::module_settings_groups(
                &qube_display_definition(),
                &[336, 337, 338],
            ));
        let group_idx = app.module_settings.groups.len() - 1;
        for module in 0..=3 {
            app.module_settings.set_value(149, module);
            let qsids = visible_module_qsids(&app);
            for qsid in [336, 337, 338] {
                assert!(qsids.contains(&qsid));
            }
        }
        let (device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(device.into());
        for (field_idx, value) in [35, 1, 0].into_iter().enumerate() {
            let field = app.module_settings.groups[group_idx].fields[field_idx].clone();
            app.write_module_setting_value(group_idx, &field, value);
            drain_hid_writes(&mut app, &ctx);
            assert_eq!(app.module_settings.value(field.qsid), value);
        }
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        for (request, (qsid, value)) in requests.iter().zip([(336u16, 35), (337, 1), (338, 0)]) {
            assert_eq!(&request[2..4], &qsid.to_le_bytes());
            assert_eq!(request[4], value);
        }
    }

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

    fn test_module_field() -> ModuleSettingField {
        ModuleSettingField {
            title: "Mode".to_owned(),
            qsid: 134,
            kind: ModuleSettingKind::Select,
            bit: 0,
            layout_option: None,
            width: 1,
            min: 0,
            max: 3,
            variants: vec![
                "Normal".to_owned(),
                "Sniper".to_owned(),
                "Scroll".to_owned(),
                "Text".to_owned(),
            ],
        }
    }

    fn add_test_module_group(app: &mut EntropyApp, field: &ModuleSettingField) {
        app.module_settings.groups.push(ModuleSettingsGroup {
            title: "Left Modules".to_owned(),
            kind: ModuleSettingsGroupKind::Left,
            fields: vec![field.clone()],
        });
    }

    fn module_filter_field(title: &str, qsid: u16) -> ModuleSettingField {
        ModuleSettingField {
            title: title.to_owned(),
            qsid,
            kind: ModuleSettingKind::Select,
            bit: 0,
            layout_option: None,
            width: 1,
            min: 0,
            max: 3,
            variants: vec!["Off".to_owned(), "On".to_owned()],
        }
    }

    fn add_filterable_module_groups(app: &mut EntropyApp) {
        let mut selector = module_filter_field("Module", 149);
        selector.variants = vec![
            "None".to_owned(),
            "Encoder".to_owned(),
            "Trackball".to_owned(),
            "Touchpad".to_owned(),
        ];
        let mut mode = module_filter_field("Mode", 134);
        mode.variants = vec![
            "Normal".to_owned(),
            "Sniper".to_owned(),
            "Scroll".to_owned(),
            "Text".to_owned(),
            "Experimental".to_owned(),
        ];
        mode.max = 4;
        app.module_settings.groups = vec![
            ModuleSettingsGroup {
                title: "Left Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Left,
                fields: vec![
                    selector,
                    module_filter_field("Encoder interval", 325),
                    module_filter_field("Encoder steps", 332),
                    mode,
                    module_filter_field("Ball axis", 130),
                    module_filter_field("Touch axis", 132),
                    module_filter_field("Ball DPI", 120),
                    module_filter_field("Touch DPI", 122),
                    module_filter_field("Scroll sens", 125),
                    module_filter_field("Sniper sens", 124),
                    module_filter_field("Text sens", 126),
                    module_filter_field("Touch gestures", 151),
                    module_filter_field("Invert scroll vertical", 136),
                    module_filter_field("Invert scroll horizontal", 327),
                    module_filter_field("Invert text vertical", 147),
                    module_filter_field("Invert text horizontal", 329),
                    module_filter_field("Acceleration", 137),
                    module_filter_field("Sticky mode", 140),
                ],
            },
            ModuleSettingsGroup {
                title: "Auto Layer".to_owned(),
                kind: ModuleSettingsGroupKind::AutoLayer,
                fields: vec![
                    module_filter_field("Auto layer", 143),
                    module_filter_field("Auto layer in Normal", 142),
                    module_filter_field("Auto layer in Sniper", 144),
                    module_filter_field("Auto layer in Scroll", 145),
                    module_filter_field("Auto layer in Text", 146),
                    module_filter_field("Auto layer timeout", 324),
                ],
            },
        ];
        app.module_settings.values.insert(149, 0);
        app.module_settings.values.insert(134, 0);
    }

    fn encoder_visibility_layout(left_label: &str, right_label: &str) -> KeyboardLayout {
        KeyboardLayout {
            name: "Modular keyboard".to_owned(),
            rows: 1,
            cols: 1,
            keys: Vec::new(),
            encoders: [0, 1]
                .into_iter()
                .map(|encoder_idx| PhysicalEncoder {
                    x: encoder_idx as f32,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    label: String::new(),
                    encoder_idx,
                    direction: 0,
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            layers: Vec::new(),
            encoder_layers: Vec::new(),
            layer_names: Vec::new(),
            custom_keycodes: Vec::new(),
            layout_options: vec![
                LayoutOption {
                    label: left_label.to_owned(),
                    choices: Vec::new(),
                },
                LayoutOption {
                    label: right_label.to_owned(),
                    choices: Vec::new(),
                },
            ],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn add_encoder_module_groups(app: &mut EntropyApp) {
        let mut left_selector = module_filter_field("Module", 149);
        left_selector.variants = vec!["Encoder".to_owned(), "Trackball".to_owned()];
        let mut right_selector = module_filter_field("Module", 150);
        right_selector.variants = left_selector.variants.clone();
        app.module_settings.groups = vec![
            ModuleSettingsGroup {
                title: "Left Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Left,
                fields: vec![left_selector],
            },
            ModuleSettingsGroup {
                title: "Right Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Right,
                fields: vec![right_selector],
            },
        ];
        app.module_settings.values.insert(149, 0);
        app.module_settings.values.insert(150, 0);
        app.module_settings.supported = true;
    }

    fn encoder_module_settings_json() -> serde_json::Value {
        serde_json::json!({
            "settings": [
                {
                    "name": "Left modules",
                    "fields": [{
                        "type": "select",
                        "title": "Module",
                        "qsid": 149,
                        "variants": ["None", "Encoder", "Trackball", "Touchpad"]
                    }]
                },
                {
                    "name": "Right modules",
                    "fields": [{
                        "type": "select",
                        "title": "Module",
                        "qsid": 150,
                        "variants": ["None", "Encoder", "Trackball", "Touchpad"]
                    }]
                }
            ]
        })
    }

    // Mirrors keyboards/k03/vial.json and keyboards/imperial44/vial.json:
    // per-encoder tabs with side metadata, side-local field numbers and QSIDs.
    fn fixed_encoder_settings_json(side_size: usize) -> serde_json::Value {
        let tabs = (0..side_size * 2)
            .map(|idx| {
                let side = if idx < side_size { "left" } else { "right" };
                let number = idx % side_size + 1;
                let title = |suffix| {
                    if side_size == 1 {
                        format!("Encoder {suffix}")
                    } else {
                        format!("Encoder {number} {suffix}")
                    }
                };
                serde_json::json!({
                    "name": format!("{side} encoder {number} (ID {idx})"),
                    "side": side,
                    "fields": [
                        {"type": "select", "title": title("interval"), "qsid": 340 + idx * 2, "variants": ["0 ms", "5 ms"]},
                        {"type": "select", "title": title("steps"), "qsid": 341 + idx * 2, "variants": ["1", "2"]}
                    ]
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({"settings": tabs})
    }

    #[test]
    fn fixed_encoders_share_one_pair_even_when_hidden() {
        use super::super::encoder_visibility_settings_ui::EncoderSettingsRow;
        for (side_size, name) in [
            (3, "K:03"),
            (3, "Ergohaven K:03"),
            (1, "Imperial44"),
            (1, "Ergohaven Imperial44"),
        ] {
            let mut app = test_app();
            app.module_settings = EntropyApp::module_settings_from_definition(
                &fixed_encoder_settings_json(side_size),
                &(340..340 + side_size as u16 * 4).collect::<Vec<_>>(),
            );
            let mut layout = encoder_visibility_layout("Hide left encoder", "Hide right encoder");
            layout.name = name.to_owned();
            layout.encoders = (0..side_size * 2)
                .map(|idx| PhysicalEncoder {
                    encoder_idx: idx as u8,
                    ..layout.encoders[0].clone()
                })
                .collect();
            assert!(app.encoder_only_module_settings(&layout));
            let rows = |app: &EntropyApp| {
                app.encoder_settings_rows(&layout)
                    .iter()
                    .map(|row| match row {
                        EncoderSettingsRow::Visibility(idx, _) => format!("toggle:{idx}"),
                        EncoderSettingsRow::Field(group, field) => format!(
                            "qsid:{}",
                            app.module_settings.groups[*group].fields[*field].qsid
                        ),
                    })
                    .collect::<Vec<_>>()
            };
            let expected = vec!["qsid:340".to_owned(), "qsid:341".to_owned()];
            assert_eq!(rows(&app), expected);
            app.encoder_visibility = vec![true; side_size * 2];
            app.encoder_visibility[side_size] = false;
            assert_eq!(rows(&app), expected);
            layout.name = "K:04".to_owned();
            assert!(!app.encoder_only_module_settings(&layout));
            assert_eq!(app.encoder_settings_rows(&layout).len(), side_size * 2);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn shared_encoder_controls_write_both_halves_including_hidden() {
        use super::super::encoder_visibility_settings_ui::EncoderSettingsRow;
        for (side_size, name) in [
            (3, "K:03"),
            (3, "Ergohaven K:03"),
            (1, "Imperial44"),
            (1, "Ergohaven Imperial44"),
        ] {
            let mut app = test_app();
            app.module_settings = EntropyApp::module_settings_from_definition(
                &fixed_encoder_settings_json(side_size),
                &(340..340 + side_size as u16 * 4).collect::<Vec<_>>(),
            );
            let mut layout = encoder_visibility_layout("Hide left encoder", "Hide right encoder");
            layout.name = name.to_owned();
            layout.encoders = (0..side_size * 2)
                .map(|idx| PhysicalEncoder {
                    encoder_idx: idx as u8,
                    ..layout.encoders[0].clone()
                })
                .collect();
            app.encoder_visibility = vec![true; side_size * 2];
            app.encoder_visibility[side_size] = false;
            let fields = app
                .encoder_settings_rows(&layout)
                .into_iter()
                .filter_map(|row| match row {
                    EncoderSettingsRow::Field(group, field) => Some((
                        group,
                        app.module_settings.groups[group].fields[field].clone(),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(fields.len(), 2);
            app.layout = Some(layout);
            let right_qsid = 340 + side_size as u16 * 2;
            app.module_settings.values.insert(right_qsid, 1);
            app.module_settings.values.insert(right_qsid + 1, 1);
            assert!(app.shared_encoder_values_mixed(340));
            assert!(app.shared_encoder_values_mixed(341));
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            for (group, field) in fields {
                app.write_module_setting_value(group, &field, 1);
            }
            drain_hid_writes(&mut app, &egui::Context::default());
            let requests = recorder.requests();
            assert!(!app.shared_encoder_values_mixed(340));
            assert!(!app.shared_encoder_values_mixed(341));
            for qsid in 340..340 + side_size as u16 * 4 {
                assert_eq!(app.module_settings.value(qsid), 1, "{name}: {qsid}");
                assert!(
                    requests
                        .iter()
                        .any(|r| r[2] == qsid as u8 && r[3] == (qsid >> 8) as u8),
                    "{name}: {qsid}"
                );
            }
        }
    }

    #[test]
    fn mixed_or_selectable_module_pages_remain_modules() {
        let mut app = test_app();
        let mut layout = encoder_visibility_layout("Hide left encoder", "Hide right encoder");
        layout.name = "K:03".to_owned();
        app.module_settings = EntropyApp::module_settings_from_definition(
            &fixed_encoder_settings_json(3),
            &(340..352).collect::<Vec<_>>(),
        );
        layout.encoders = (0..6)
            .map(|idx| PhysicalEncoder {
                encoder_idx: idx,
                ..layout.encoders[0].clone()
            })
            .collect();
        app.module_settings.groups[0]
            .fields
            .push(module_filter_field("Ball DPI", 120));
        assert!(!app.encoder_only_module_settings(&layout));
        app.module_settings.groups[0].fields.pop();
        let mut selector = module_filter_field("Module", 149);
        selector.variants = vec!["Encoder".to_owned(), "Trackball".to_owned()];
        app.module_settings.groups[0].fields.push(selector);
        assert!(!app.encoder_only_module_settings(&layout));
    }

    fn visible_module_qsids(app: &EntropyApp) -> Vec<u16> {
        app.module_settings_rows()
            .into_iter()
            .filter_map(|row| match row {
                ModuleSettingsRow::Field {
                    group_idx,
                    field_idx,
                } => app
                    .module_settings
                    .groups
                    .get(group_idx)
                    .and_then(|group| group.fields.get(field_idx))
                    .map(|field| field.qsid),
                ModuleSettingsRow::SideSelector | ModuleSettingsRow::Section(_) => None,
            })
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drain_hid_writes(app: &mut EntropyApp, ctx: &egui::Context) {
        for _ in 0..200 {
            app.poll_combo_write(ctx);
            app.poll_settings_write(ctx);
            if !app.hid_write_task_active() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("HID writes did not drain");
    }

    #[test]
    fn modular_encoder_visibility_is_automatic() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder module",
            "Hide right encoder module",
        ));

        assert!(
            app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
        assert_eq!(visible_module_qsids(&app), vec![149]);

        app.module_settings.set_selected_module_group(1);
        assert_eq!(visible_module_qsids(&app), vec![150]);
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn deferred_module_definition_owns_encoder_visibility_before_values_load() {
        let json = encoder_module_settings_json();
        let mut app = test_app();
        app.module_settings = EntropyApp::module_settings_from_definition(&json, &[149, 150]);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder module",
            "Hide right encoder module",
        ));

        assert!(app.module_settings.supported);
        assert!(app.module_settings.values.is_empty());
        assert!(
            app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
        assert!(
            app.hide_modular_encoders_by_default(app.layout.as_ref().expect("layout")),
            "unknown module selectors must not guess that an encoder is installed"
        );
    }

    #[test]
    fn loaded_module_selectors_show_installed_encoders_by_default() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        let layout =
            encoder_visibility_layout("Hide left encoder module", "Hide right encoder module");

        assert!(app.module_encoder_selectors_loaded(&layout));
        assert!(!app.hide_modular_encoders_by_default(&layout));
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(
                &layout,
                Some(0),
                None,
                app.hide_modular_encoders_by_default(&layout),
                app.encoder_only_module_settings(&layout),
            ),
            vec![true, true]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn staged_module_selector_reader_preloads_layout_critical_values() {
        let json = encoder_module_settings_json();
        let (hid, _) = crate::hid::HidDevice::test_device();
        hid.set_qmk_setting_u8(149, 1).unwrap();
        hid.set_qmk_setting_u8(150, 2).unwrap();
        let mut settings = EntropyApp::module_settings_from_definition(&json, &[149, 150]);

        EntropyApp::read_initial_module_values(&mut settings, &hid);

        assert_eq!(settings.values.get(&149), Some(&1));
        assert_eq!(settings.values.get(&150), Some(&2));
    }

    #[test]
    fn phenom_encoder_labels_are_owned_by_module_settings() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder",
            "Hide right encoder",
        ));

        assert!(
            app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn encoder_visibility_control_is_absent_for_selected_pointing_module() {
        let mut app = test_app();
        add_encoder_module_groups(&mut app);
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder",
            "Hide right encoder",
        ));
        app.module_settings.values.insert(149, 1);

        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn keyboards_without_module_settings_do_not_get_encoder_visibility_page() {
        let mut app = test_app();
        app.layout = Some(encoder_visibility_layout(
            "Hide left encoder",
            "Hide right encoder",
        ));

        assert!(
            !app.module_settings_include_encoder_visibility(app.layout.as_ref().expect("layout"))
        );
        assert!(
            !app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
        );
    }

    #[test]
    fn k03_and_imperial44_keep_their_encoder_page() {
        let mut app = test_app();
        for name in ["K:03", "Imperial44"] {
            app.layout = Some(encoder_visibility_layout(
                "Hide left encoder",
                "Hide right encoder",
            ));
            app.layout.as_mut().expect("layout").name = name.to_owned();

            assert!(
                app.show_separate_encoder_visibility_settings(app.layout.as_ref().expect("layout"))
            );
        }
    }

    #[test]
    fn all_firmware_module_fields_have_catalog_entries_case_insensitively() {
        for title in [
            "Module",
            "Encoder interval",
            "Encoder steps",
            "Sticky mode",
            "Invert scroll vertical",
            "invert scroll horizontal",
            "Invert text vertical",
            "invert text horizontal",
            "Auto layer in Normal",
            "auto layer in sniper",
            "Auto layer in Scroll",
            "auto layer in text",
            "Auto layer timeout",
            "Auto layer deactivate on key",
        ] {
            assert!(
                module_setting_catalog_keys(title).is_some(),
                "missing module translation for {title}"
            );
        }
    }

    #[test]
    fn lowercase_module_variants_are_localized_and_capitalized() {
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "trackball"),
            "Трекбол"
        );
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "normal"),
            "Обычный"
        );
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "none"),
            "Нет"
        );
    }

    #[test]
    fn module_selector_keeps_none_and_transport_values_aligned() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        let group = &app.module_settings.groups[0];
        let field = &group.fields[0];

        assert_eq!(
            field.variants,
            vec!["None", "Encoder", "Trackball", "Touchpad"]
        );
    }

    #[test]
    fn stored_none_selects_none() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        let group = &app.module_settings.groups[0];

        assert_eq!(group.selected_module_kind(0), Some(ModuleDeviceKind::None));
    }

    #[test]
    fn selected_module_filters_rows_to_relevant_settings() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);

        assert_eq!(visible_module_qsids(&app), vec![149]);

        app.module_settings.set_value(149, 1);
        assert_eq!(visible_module_qsids(&app), vec![149, 325, 332]);

        app.module_settings.set_value(149, 2);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 137, 140, 143, 142, 324]
        );

        app.module_settings.set_value(149, 3);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 132, 122, 151, 137, 140, 143, 142, 324]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn editing_inactive_mode_does_not_change_active_mode() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 2);
        let (device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(device.into());
        let group = 0;
        let field = app.module_settings.groups[group]
            .fields
            .iter()
            .find(|field| field.qsid == 125)
            .unwrap()
            .clone();
        app.write_module_setting_value(group, &field, 2);
        drain_hid_writes(&mut app, &ctx);
        assert_eq!(app.module_settings.value(125), 2);
        assert_eq!(app.module_settings.value(134), 0);
        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(&requests[0][2..4], &125u16.to_le_bytes());
    }

    #[test]
    fn module_page_shows_selected_mode_in_one_settings_list() {
        fn collect_visible_text(
            shape: &egui::Shape,
            clip: egui::Rect,
            found: &mut Vec<(String, egui::Pos2)>,
        ) {
            match shape {
                egui::Shape::Text(text) if clip.contains(text.pos) => {
                    found.push((text.galley.job.text.clone(), text.pos));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_visible_text(shape, clip, found);
                    }
                }
                _ => {}
            }
        }
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            for width in [1400.0, 600.0] {
                for mode in 0..=3 {
                    let ctx = egui::Context::default();
                    let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
                    let mut app = EntropyApp::new(&creation_context);
                    app.app_settings.language = language;
                    add_filterable_module_groups(&mut app);
                    app.module_settings.supported = true;
                    app.module_settings.set_value(149, 2);
                    app.module_settings.set_value(134, mode);
                    let label_for = |qsid| {
                        let group = &app.module_settings.groups[0];
                        let field = group
                            .fields
                            .iter()
                            .find(|field| field.qsid == qsid)
                            .unwrap();
                        app.module_setting_label(group.kind, &field.title)
                    };
                    let mode_labels = [label_for(124), label_for(125), label_for(126)];
                    let mut common_labels = vec![label_for(149), label_for(134), label_for(120)];
                    let mut found = Vec::new();
                    for _ in 0..6 {
                        let output = crate::app::discard_texture_deltas(ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 1200.0),
                                )),
                                ..Default::default()
                            },
                            |ui| app.draw_module_settings_page(ui, ui.max_rect()),
                        ));
                        found.clear();
                        for shape in &output.shapes {
                            collect_visible_text(&shape.shape, shape.clip_rect, &mut found);
                        }
                    }
                    for (index, label) in mode_labels.iter().enumerate() {
                        let visible = found.iter().any(|(text, _)| text == label);
                        assert_eq!(
                            visible,
                            mode as usize == index + 1,
                            "mode {mode}, width {width}, language {language:?}: {label}"
                        );
                    }
                    if mode > 0 {
                        common_labels.push(mode_labels[mode as usize - 1].clone());
                    }
                    let positions = common_labels
                        .iter()
                        .map(|label| found.iter().find(|(text, _)| text == label).unwrap().1)
                        .collect::<Vec<_>>();
                    assert!(positions
                        .iter()
                        .all(|pos| (pos.x - positions[0].x).abs() < 1.0));
                    if width > 1000.0 {
                        assert!(positions[0].x > width / 4.0, "settings list stays centered");
                    }
                    assert_eq!(app.module_settings.value(134), mode);
                    assert!(app.settings_write_task.is_none());
                }
            }
        }
    }

    #[test]
    fn selected_pointer_mode_filters_rows_to_relevant_settings() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 2);

        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 137, 140, 143, 142, 324]
        );

        app.module_settings.set_value(134, 1);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 124, 137, 140, 143, 144, 324]
        );

        app.module_settings.set_value(134, 2);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 125, 136, 327, 137, 140, 143, 145, 324]
        );

        app.module_settings.set_value(134, 3);
        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 126, 147, 329, 137, 140, 143, 146, 324]
        );
    }

    #[test]
    fn unknown_pointer_mode_keeps_all_mode_fields_visible() {
        let mut app = test_app();
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 2);
        app.module_settings.set_value(134, 4);

        assert_eq!(
            visible_module_qsids(&app),
            vec![
                149, 134, 130, 120, 125, 124, 126, 136, 327, 147, 329, 137, 140, 143, 142, 144,
                145, 146, 324
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_module_selection_updates_visible_rows_immediately() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device.into());
        add_filterable_module_groups(&mut app);
        let selector = app.module_settings.groups[0].fields[0].clone();

        app.write_module_setting_value(0, &selector, 2);

        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 130, 120, 137, 140, 143, 142, 324]
        );

        drain_hid_writes(&mut app, &ctx);
        assert_eq!(app.module_settings.value(149), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_pointer_mode_updates_visible_rows_immediately() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device.into());
        add_filterable_module_groups(&mut app);
        app.module_settings.set_value(149, 3);
        let mode = app.module_settings.groups[0].fields[3].clone();

        app.write_module_setting_value(0, &mode, 2);

        assert_eq!(
            visible_module_qsids(&app),
            vec![149, 134, 132, 122, 125, 151, 136, 327, 137, 140, 143, 145, 324]
        );

        drain_hid_writes(&mut app, &ctx);
        assert_eq!(app.module_settings.value(134), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_setting_write_matches_vial_set_without_readback_or_debounce() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device.into());
        let field = test_module_field();
        add_test_module_group(&mut app, &field);
        app.status_msg = "unchanged".to_owned();

        app.write_module_setting_value(0, &field, 2);

        assert!(app.settings_write_task.is_some());
        assert_eq!(app.pending_settings_write_value(field.qsid), Some(2));
        assert_eq!(app.pending_qmk_settings_write_value(field.qsid), None);
        assert_eq!(app.status_msg, "unchanged");

        drain_hid_writes(&mut app, &ctx);

        assert_eq!(app.module_settings.value(field.qsid), 2);
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(app.status_msg, "unchanged");
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|request| {
                    request[2] == field.qsid as u8 && request[3] == (field.qsid >> 8) as u8
                })
                .count(),
            1
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_setting_write_waits_for_busy_hid_owner() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device.into());
        app.combo_entries = vec![ComboEntry {
            keys: [0x0004, 0x0005, 0, 0],
            output: 0x0006.into(),
            layer: None,
        }];
        app.combo_synced_entries = vec![ComboEntry::default()];
        app.mark_combo_dirty();
        app.maybe_start_combo_write(&ctx);
        assert!(app.combo_write_task.is_some());
        assert!(app.hid_device.is_none());

        let field = test_module_field();
        add_test_module_group(&mut app, &field);
        app.write_module_setting_value(0, &field, 3);

        assert!(app.settings_write_task.is_none());
        assert_eq!(app.pending_settings_write_value(field.qsid), Some(3));
        assert_eq!(app.pending_qmk_settings_write_value(field.qsid), None);

        drain_hid_writes(&mut app, &ctx);

        assert_eq!(app.module_settings.value(field.qsid), 3);
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|request| {
                    request[2] == field.qsid as u8 && request[3] == (field.qsid >> 8) as u8
                })
                .count(),
            1
        );
    }

    #[test]
    fn dedicated_trackball_groups_use_trackball_page_title() {
        let mut app = test_app();
        app.module_settings.groups = vec![
            ModuleSettingsGroup {
                title: "Trackball".to_owned(),
                kind: ModuleSettingsGroupKind::Other,
                fields: vec![test_module_field()],
            },
            ModuleSettingsGroup {
                title: "Auto layer".to_owned(),
                kind: ModuleSettingsGroupKind::AutoLayer,
                fields: vec![module_filter_field("Auto layer timeout", 324)],
            },
        ];

        assert!(app.module_settings.is_trackball_page());
        assert_eq!(
            app.module_settings_title_key(),
            "modules_settings.trackball_title"
        );
    }

    #[test]
    fn firmware_exposed_deactivate_on_key_becomes_a_localized_auto_layer_toggle() {
        let json = serde_json::json!({
            "settings": [{
                "name": "Auto layer",
                "fields": [
                    {"qsid": 143, "title": "Auto layer", "type": "select"},
                    {
                        "qsid": 335,
                        "title": "Auto layer deactivate on key",
                        "type": "boolean",
                        "bit": 0
                    }
                ]
            }]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[143, 335]);
        let group = groups
            .iter()
            .find(|group| group.kind == ModuleSettingsGroupKind::AutoLayer)
            .expect("auto layer group");
        let field = group
            .fields
            .iter()
            .find(|field| field.qsid == 335)
            .expect("deactivate on key field");

        assert!(matches!(field.kind, ModuleSettingKind::Boolean));
        for mode in [
            PointerModeKind::Normal,
            PointerModeKind::Sniper,
            PointerModeKind::Scroll,
            PointerModeKind::Text,
        ] {
            assert!(group.field_visible_for_pointer_mode(field, mode));
        }

        let mut app = test_app();
        assert_eq!(
            app.module_setting_label(ModuleSettingsGroupKind::AutoLayer, &field.title),
            "Auto layer deactivate on key"
        );
        app.app_settings.language = crate::i18n::Language::Russian;
        assert_eq!(
            app.module_setting_label(ModuleSettingsGroupKind::AutoLayer, &field.title),
            "Выключение автослоя по клавише"
        );
    }

    #[test]
    fn russian_inversion_labels_use_direction_words_without_arrows() {
        let mut app = test_app();
        app.app_settings.language = crate::i18n::Language::Russian;

        for (title, direction) in [
            ("Invert scroll vertical", "вертикали"),
            ("Invert scroll horizontal", "горизонтали"),
            ("Invert text vertical", "вертикали"),
            ("Invert text horizontal", "горизонтали"),
        ] {
            let label = app.module_setting_label(ModuleSettingsGroupKind::Left, title);
            assert!(label.contains(direction), "{label}");
            assert!(
                !label
                    .chars()
                    .any(|character| matches!(character, '↑' | '↓' | '←' | '→')),
                "{label}"
            );
        }
    }
}
