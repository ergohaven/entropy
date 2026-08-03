use super::*;

impl KeycodePicker {
    pub(super) fn show_vial_rgb(&mut self, ui: &mut egui::Ui) {
        // Backlight
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.backlight",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let bl_keys: &[(&str, u16, &str)] = &[
            ("Toggle", 0x7800, "Toggle backlight on/off"),
            ("Cycle", 0x7801, "Cycle through backlight brightness levels"),
            ("Breathing", 0x7802, "Toggle breathing effect on/off"),
            ("On", 0x7805, "Turn backlight on"),
            ("Off", 0x7806, "Turn backlight off"),
            ("Brightness -", 0x7804, "Decrease backlight brightness"),
            ("Brightness +", 0x7803, "Increase backlight brightness"),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, value, tip) in bl_keys {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let display_label = picker_action_label(label);
                Self::paint_compact_picker_label(ui, &resp, &display_label);
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp.on_hover_text(crate::i18n::tr_catalog(self.language, tip));
            }
        });

        ui.add_space(10.0);
        // RGB Underglow (QMK rgblight)
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.rgb_underglow",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let rgb_keys: &[(&str, u16, &str)] = &[
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
        ui.horizontal_wrapped(|ui| {
            for (label, value, tip) in rgb_keys {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let display_label = picker_action_label(label);
                Self::paint_compact_picker_label(ui, &resp, &display_label);
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp.on_hover_text(crate::i18n::tr_catalog(self.language, tip));
            }
        });

        ui.add_space(10.0);
        // RGB Matrix modes
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.rgb_matrix_modes",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let rgbm_keys: &[(&str, u16, &str)] = &[
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
        ui.horizontal_wrapped(|ui| {
            for (label, value, tip) in rgbm_keys {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let display_label = picker_action_label(label);
                Self::paint_compact_picker_label(ui, &resp, &display_label);
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp.on_hover_text(crate::i18n::tr_catalog(self.language, tip));
            }
        });

        ui.add_space(10.0);
        // RGB Matrix controls
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.rgb_matrix_controls",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let rgbm_ctrl: &[(&str, u16, &str)] = &[
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
        ui.horizontal_wrapped(|ui| {
            for (label, value, tip) in rgbm_ctrl {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let display_label = picker_action_label(label);
                Self::paint_compact_picker_label(ui, &resp, &display_label);
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp.on_hover_text(crate::i18n::tr_catalog(self.language, tip));
            }
        });
    }
}
