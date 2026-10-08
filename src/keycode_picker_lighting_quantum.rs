use super::*;

// Lighting controls of the RGB tab: caption, keycode, tooltip.
const BACKLIGHT_KEYS: &[(&str, u16, &str)] = &[
    ("Toggle", 0x7800, "Toggle backlight on/off"),
    ("Cycle", 0x7801, "Cycle through backlight brightness levels"),
    ("Breathing", 0x7802, "Toggle breathing effect on/off"),
    ("On", 0x7805, "Turn backlight on"),
    ("Off", 0x7806, "Turn backlight off"),
    ("Brightness -", 0x7804, "Decrease backlight brightness"),
    ("Brightness +", 0x7803, "Increase backlight brightness"),
];

// QMK rgblight.
const RGB_UNDERGLOW_KEYS: &[(&str, u16, &str)] = &[
    ("Toggle", 0x7A00, "Toggle RGB lighting on/off"),
    ("Prev Mode", 0x7A02, "Switch to previous RGB animation mode"),
    ("Next Mode", 0x7A01, "Switch to next RGB animation mode"),
    ("Hue -", 0x7A04, "Decrease color hue"),
    ("Hue +", 0x7A03, "Increase color hue"),
    ("Saturation -", 0x7A06, "Decrease color saturation"),
    ("Saturation +", 0x7A05, "Increase color saturation"),
    ("Brightness -", 0x7A08, "Decrease brightness"),
    ("Brightness +", 0x7A07, "Increase brightness"),
    ("Speed -", 0x7A0A, "Decrease animation speed"),
    ("Speed +", 0x7A09, "Increase animation speed"),
    ("Effect -", 0x7A0C, "Previous RGB effect"),
    ("Effect +", 0x7A0B, "Next RGB effect"),
];

const RGB_MATRIX_MODE_KEYS: &[(&str, u16, &str)] = &[
    ("Plain", 0x7A0D, "RGB Matrix: solid color, no animation"),
    (
        "Breathe",
        0x7A0E,
        "RGB Matrix: breathing effect — smooth brightness fade",
    ),
    (
        "Rainbow",
        0x7A0F,
        "RGB Matrix: rainbow gradient across all keys",
    ),
    ("Swirl", 0x7A10, "RGB Matrix: swirling rainbow pattern"),
    (
        "Snake",
        0x7A11,
        "RGB Matrix: snake animation moving across keys",
    ),
    ("Knight", 0x7A12, "RGB Matrix: Knight Rider scanning effect"),
    (
        "Xmas",
        0x7A13,
        "RGB Matrix: alternating red and green like Christmas lights",
    ),
    ("Gradient", 0x7A14, "RGB Matrix: static gradient effect"),
    (
        "Test",
        0x7A15,
        "RGB Matrix: test mode — cycles through R, G, B",
    ),
];

const RGB_MATRIX_CONTROL_KEYS: &[(&str, u16, &str)] = &[
    ("On", 0x7A16, "Turn RGB Matrix on"),
    ("Off", 0x7A17, "Turn RGB Matrix off"),
    ("Toggle", 0x7A18, "Toggle RGB Matrix on/off"),
    ("Previous", 0x7A1A, "Previous RGB Matrix animation"),
    ("Next", 0x7A19, "Next RGB Matrix animation"),
    ("Hue -", 0x7A1C, "Decrease RGB Matrix hue"),
    ("Hue +", 0x7A1B, "Increase RGB Matrix hue"),
    ("Saturation -", 0x7A1E, "Decrease RGB Matrix saturation"),
    ("Saturation +", 0x7A1D, "Increase RGB Matrix saturation"),
    ("Brightness -", 0x7A20, "Decrease RGB Matrix brightness"),
    ("Brightness +", 0x7A1F, "Increase RGB Matrix brightness"),
    ("Speed -", 0x7A22, "Decrease RGB Matrix animation speed"),
    ("Speed +", 0x7A21, "Increase RGB Matrix animation speed"),
];

impl KeycodePicker {
    /// Lighting controls in the order the RGB tab shows them. The long
    /// caption stays searchable as an alias of the compact keycap label.
    pub(super) fn rgb_rows(&self) -> Vec<PickerRow> {
        let sections: [(&'static str, &[(&str, u16, &str)]); 4] = [
            ("key_picker_text.backlight", BACKLIGHT_KEYS),
            ("key_picker_text.rgb_underglow", RGB_UNDERGLOW_KEYS),
            ("key_picker_text.rgb_matrix_modes", RGB_MATRIX_MODE_KEYS),
            (
                "key_picker_text.rgb_matrix_controls",
                RGB_MATRIX_CONTROL_KEYS,
            ),
        ];
        let mut rows = Vec::new();
        for (heading_key, keys) in sections {
            let heading = crate::i18n::tr_catalog(self.language, heading_key);
            for (label, value, tip) in keys {
                rows.push(
                    self.vial_row(
                        KeycodeTab::Rgb,
                        heading,
                        *value,
                        picker_action_label(label),
                        crate::i18n::tr_catalog(self.language, tip),
                    )
                    .with_aliases(*label),
                );
            }
        }
        rows
    }
}
