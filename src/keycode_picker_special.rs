use super::*;

impl KeycodePicker {
    fn picker_value_supported(&self, value: u16) -> bool {
        let _ = value;
        true
    }

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
            KeycodeTab::Symbols | KeycodeTab::Special | KeycodeTab::Rgb | KeycodeTab::Custom => {
                Self::key_grid_width(ui, 13, spacing)
            }
            KeycodeTab::Modifiers => Self::key_grid_width(ui, 13, spacing),
            KeycodeTab::Macro | KeycodeTab::TapDance => Self::slot_grid_width(16, 4.0),
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

    pub(super) fn show_vial_special(&mut self, ui: &mut egui::Ui) {
        let special_keys: Vec<(String, u16, String)> = vec![
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
        ];

        let special_title =
            crate::i18n::tr_catalog(self.language, "key_picker_text.special_qmk_keys");
        ui.label(
            RichText::new(special_title)
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (label, value, tip) in &special_keys {
                if !self.picker_value_supported(*value) {
                    continue;
                }
                if let Some(kc) = crate::keycode::KEYCODES
                    .iter()
                    .find(|kc| kc.value == *value)
                {
                    if !self.vial_keycode_supported(kc) {
                        continue;
                    }
                }
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp.on_hover_text(crate::i18n::tr_text(self.language, tip));
            }
        });

        let mouse_values: Vec<u16> = crate::keycode::KEYCODES
            .iter()
            .filter(|kc| matches!(kc.category, crate::keycode::KeycodeCategory::Mouse))
            .map(|kc| kc.value)
            .filter(|value| self.picker_value_supported(*value))
            .collect();

        if self.supports_mouse_keys && !mouse_values.is_empty() {
            ui.add_space(10.0);
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "key_picker_text.mouse",
                ))
                .size(11.0)
                .color(Color32::from_gray(150)),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for value in &mouse_values {
                    let resp = ui
                        .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let label = keycode_label_with_names_and_layout(
                        *value,
                        &[],
                        &self.layer_names,
                        self.key_legend_layout,
                    );
                    Self::paint_compact_picker_label(ui, &resp, &label);
                    if resp.clicked() {
                        self.assign_keycode_value(*value);
                    }
                    if resp.hovered() {
                        resp.on_hover_text(crate::i18n::tr_text(
                            self.language,
                            &self.picker_keycode_tooltip(*value, &[]),
                        ));
                    }
                }
            });
        }

        let media_keys: &[(&str, &str, u16)] = &[
            ("⏻", "Power", 0x00A5),
            ("🌙", "Sleep", 0x00A6),
            ("☀", "Wake", 0x00A7),
            ("🔇", "Mute", 0x00A8),
            ("🔉", "Vol-", 0x00AA),
            ("🔊", "Vol+", 0x00A9),
            ("⏮", "Prev", 0x00AC),
            ("⏭", "Next", 0x00AB),
            ("⏹", "Stop", 0x00AD),
            ("⏯", "Play", 0x00AE),
            ("🎵", "Media", 0x00AF),
            ("⏏", "Eject", 0x00B0),
            ("✉", "Mail", 0x00B1),
            ("∑", "Calc", 0x00B2),
            ("📁", "Files", 0x00B3),
            ("🔍", "Search", 0x00B4),
            ("🌐", "Home", 0x00B5),
            ("←", "Back", 0x00B6),
            ("→", "Fwd", 0x00B7),
            ("⏹", "Web", 0x00B8),
            ("↻", "Reload", 0x00B9),
            ("★", "Favs", 0x00BA),
            ("⏪", "Rewind", 0x00BC),
            ("⏩", "Fast+", 0x00BB),
            ("🔅", "Bright-", 0x00BE),
            ("🔆", "Bright+", 0x00BD),
            ("🪟", "Mission", 0x00BF),
            ("🚀", "Launch", 0x00C0),
        ];
        ui.add_space(10.0);
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.media_apps_system",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (_, _, value) in media_keys {
                if !self.picker_value_supported(*value) {
                    continue;
                }
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let label = keycode_label_with_names_and_layout(
                    *value,
                    &[],
                    &self.layer_names,
                    self.key_legend_layout,
                );
                Self::paint_compact_picker_label(ui, &resp, &label);
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                if resp.hovered() {
                    resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &self.picker_keycode_tooltip(*value, &[]),
                    ));
                }
            }
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.os_edit_shortcuts",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let mut os_shortcuts: Vec<(&str, &str, u16, &str)> = Vec::new();
        #[cfg(target_os = "macos")]
        {
            os_shortcuts.extend_from_slice(&[
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
            ]);
        }
        #[cfg(target_os = "windows")]
        {
            os_shortcuts.extend_from_slice(&[
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
            ]);
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            os_shortcuts.extend_from_slice(&[
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
            ]);
        }
        ui.horizontal_wrapped(|ui| {
            for (_os, text, value, tip) in os_shortcuts {
                if !self.picker_value_supported(value) {
                    continue;
                }
                let resp = ui.add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""));
                Self::paint_compact_picker_label(ui, &resp, text);
                if resp.clicked() {
                    self.assign_keycode_value(value);
                }
                resp.on_hover_text(crate::i18n::tr_text(self.language, tip));
            }
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.numpad",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let num_text_color = if ui.visuals().dark_mode {
                Color32::from_gray(105)
            } else {
                Color32::from_gray(145)
            };
            for kc in crate::keycode::KEYCODES
                .iter()
                .filter(|kc| matches!(kc.category, crate::keycode::KeycodeCategory::Numpad))
            {
                if !self.picker_value_supported(kc.value) {
                    continue;
                }
                let display = match kc.name {
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
                };
                let font_size = if display.chars().count() > 2 {
                    10.5
                } else {
                    13.0
                };
                let mut resp = ui.add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""));
                let rect = resp.rect;
                let painter = ui.painter();
                let main_color = if resp.hovered() {
                    ui.visuals().widgets.hovered.fg_stroke.color
                } else {
                    ui.visuals().widgets.inactive.fg_stroke.color
                };
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y - 6.0),
                    egui::Align2::CENTER_CENTER,
                    "Num",
                    egui::FontId::proportional(9.5),
                    num_text_color,
                );
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y + 6.0),
                    egui::Align2::CENTER_CENTER,
                    display,
                    egui::FontId::proportional(font_size),
                    main_color,
                );
                if resp.clicked() {
                    self.assign_keycode_value(kc.value);
                }
                if resp.hovered() {
                    resp = resp.on_hover_text(crate::i18n::tr_text(
                        self.language,
                        &self.picker_keycode_tooltip(kc.value, &[]),
                    ));
                    let _ = resp;
                }
            }
        });

        if !self.supports_mouse_keys {
            return;
        }

        let magic_keys: &[u16] = &[
            0x7000, 0x7001, 0x7002, 0x7004, 0x7003, 0x7020, 0x7021, 0x7022, 0x7017, 0x7018, 0x7019,
            0x701A, 0x701B, 0x701C, 0x701D, 0x7005, 0x7006, 0x7007, 0x7008, 0x7014, 0x7015, 0x7016,
            0x700A, 0x7009, 0x700B, 0x700C, 0x700D, 0x700E, 0x700F, 0x7010, 0x7011, 0x7012, 0x7013,
            0x701E, 0x701F,
        ];
        let visible_magic_keys: Vec<u16> = magic_keys
            .iter()
            .copied()
            .filter(|value| self.picker_value_supported(*value))
            .collect();
        if !visible_magic_keys.is_empty() {
            ui.add_space(10.0);
            ui.label(
                RichText::new(crate::i18n::tr_catalog(self.language, "ui.magic_title"))
                    .size(11.0)
                    .color(Color32::from_gray(150)),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let magic_top_color = if ui.visuals().dark_mode {
                    Color32::from_gray(105)
                } else {
                    Color32::from_gray(145)
                };
                for value in &visible_magic_keys {
                    let label = crate::keycode::keycode_label(*value);
                    let mut parts = label.splitn(2, '\n');
                    let top = parts.next().unwrap_or("");
                    let bottom = parts.next().unwrap_or("");
                    let mut resp =
                        ui.add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""));
                    let rect = resp.rect;
                    let painter = ui.painter();
                    let main_color = if resp.hovered() {
                        ui.visuals().widgets.hovered.fg_stroke.color
                    } else {
                        ui.visuals().widgets.inactive.fg_stroke.color
                    };
                    let top_font = if top.chars().count() > 10 { 8.6 } else { 9.2 };
                    let bottom_font = if bottom.chars().count() > 8 {
                        9.4
                    } else {
                        10.2
                    };
                    if !top.is_empty() {
                        painter.text(
                            egui::pos2(rect.center().x, rect.center().y - 6.5),
                            egui::Align2::CENTER_CENTER,
                            top,
                            egui::FontId::proportional(top_font),
                            magic_top_color,
                        );
                    }
                    painter.text(
                        egui::pos2(rect.center().x, rect.center().y + 6.5),
                        egui::Align2::CENTER_CENTER,
                        if bottom.is_empty() { top } else { bottom },
                        egui::FontId::proportional(bottom_font),
                        main_color,
                    );
                    if resp.clicked() {
                        self.assign_keycode_value(*value);
                    }
                    if resp.hovered() {
                        resp = resp.on_hover_text(crate::i18n::tr_text(
                            self.language,
                            &self.picker_keycode_tooltip(*value, &[]),
                        ));
                        let _ = resp;
                    }
                }
            });
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.space_cadet",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let space_cadet_keys: &[(&str, &str, u16, &str)] = &[
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
        ui.horizontal_wrapped(|ui| {
            let cadet_top_color = if ui.visuals().dark_mode {
                Color32::from_gray(105)
            } else {
                Color32::from_gray(145)
            };
            for (top, bottom, value, tip) in space_cadet_keys {
                if !self.picker_value_supported(*value) {
                    continue;
                }
                let mut resp = ui.add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""));
                let rect = resp.rect;
                let painter = ui.painter();
                let main_color = if resp.hovered() {
                    ui.visuals().widgets.hovered.fg_stroke.color
                } else {
                    ui.visuals().widgets.inactive.fg_stroke.color
                };
                let top_font = if top.chars().count() > 6 { 8.7 } else { 9.3 };
                let bottom_font = if bottom.chars().count() > 5 {
                    9.4
                } else {
                    10.6
                };
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y - 6.5),
                    egui::Align2::CENTER_CENTER,
                    *top,
                    egui::FontId::proportional(top_font),
                    cadet_top_color,
                );
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y + 6.5),
                    egui::Align2::CENTER_CENTER,
                    *bottom,
                    egui::FontId::proportional(bottom_font),
                    main_color,
                );
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp = resp.on_hover_text(crate::i18n::tr_text(self.language, tip));
                let _ = resp;
            }
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.international",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        let international_keys: &[(&str, &str, u16, &str)] = &[
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
        ui.horizontal_wrapped(|ui| {
            let intl_top_color = if ui.visuals().dark_mode {
                Color32::from_gray(105)
            } else {
                Color32::from_gray(145)
            };
            for (top, bottom, value, tip) in international_keys {
                let mut resp = ui.add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""));
                let rect = resp.rect;
                let painter = ui.painter();
                let main_color = if resp.hovered() {
                    ui.visuals().widgets.hovered.fg_stroke.color
                } else {
                    ui.visuals().widgets.inactive.fg_stroke.color
                };
                let top_font = if top.chars().count() > 6 { 8.5 } else { 9.2 };
                let bottom_font = if bottom.chars().count() > 6 {
                    9.0
                } else {
                    10.2
                };
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y - 6.5),
                    egui::Align2::CENTER_CENTER,
                    *top,
                    egui::FontId::proportional(top_font),
                    intl_top_color,
                );
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y + 6.5),
                    egui::Align2::CENTER_CENTER,
                    *bottom,
                    egui::FontId::proportional(bottom_font),
                    main_color,
                );
                if resp.clicked() {
                    self.assign_keycode_value(*value);
                }
                resp = resp.on_hover_text(crate::i18n::tr_text(self.language, tip));
                let _ = resp;
            }
        });
    }
}
