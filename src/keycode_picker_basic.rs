use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QwertyPickerGridKey {
    row: usize,
    col: usize,
    span: usize,
    label: &'static str,
    value: u16,
}

const fn qwerty_picker_grid_key(
    row: usize,
    col: usize,
    span: usize,
    label: &'static str,
    value: u16,
) -> QwertyPickerGridKey {
    QwertyPickerGridKey {
        row,
        col,
        span,
        label,
        value,
    }
}

const QWERTY_PICKER_GRID_KEYS: &[QwertyPickerGridKey] = &[
    qwerty_picker_grid_key(0, 0, 1, "Esc", 0x0029),
    qwerty_picker_grid_key(0, 1, 1, "F1", 0x003A),
    qwerty_picker_grid_key(0, 2, 1, "F2", 0x003B),
    qwerty_picker_grid_key(0, 3, 1, "F3", 0x003C),
    qwerty_picker_grid_key(0, 4, 1, "F4", 0x003D),
    qwerty_picker_grid_key(0, 5, 1, "F5", 0x003E),
    qwerty_picker_grid_key(0, 6, 1, "F6", 0x003F),
    qwerty_picker_grid_key(0, 7, 1, "F7", 0x0040),
    qwerty_picker_grid_key(0, 8, 1, "F8", 0x0041),
    qwerty_picker_grid_key(0, 9, 1, "F9", 0x0042),
    qwerty_picker_grid_key(0, 10, 1, "F10", 0x0043),
    qwerty_picker_grid_key(0, 11, 1, "F11", 0x0044),
    qwerty_picker_grid_key(0, 12, 1, "F12", 0x0045),
    qwerty_picker_grid_key(1, 0, 1, "`", 0x0035),
    qwerty_picker_grid_key(1, 1, 1, "1", 0x001E),
    qwerty_picker_grid_key(1, 2, 1, "2", 0x001F),
    qwerty_picker_grid_key(1, 3, 1, "3", 0x0020),
    qwerty_picker_grid_key(1, 4, 1, "4", 0x0021),
    qwerty_picker_grid_key(1, 5, 1, "5", 0x0022),
    qwerty_picker_grid_key(1, 6, 1, "6", 0x0023),
    qwerty_picker_grid_key(1, 7, 1, "7", 0x0024),
    qwerty_picker_grid_key(1, 8, 1, "8", 0x0025),
    qwerty_picker_grid_key(1, 9, 1, "9", 0x0026),
    qwerty_picker_grid_key(1, 10, 1, "0", 0x0027),
    qwerty_picker_grid_key(1, 11, 1, "-", 0x002D),
    qwerty_picker_grid_key(1, 12, 1, "=", 0x002E),
    qwerty_picker_grid_key(1, 13, 1, "Backspace", 0x002A),
    qwerty_picker_grid_key(1, 14, 1, "Insert", 0x0049),
    qwerty_picker_grid_key(1, 15, 1, "Delete", 0x004C),
    qwerty_picker_grid_key(2, 0, 2, "Tab", 0x002B),
    qwerty_picker_grid_key(2, 2, 1, "Q", 0x0014),
    qwerty_picker_grid_key(2, 3, 1, "W", 0x001A),
    qwerty_picker_grid_key(2, 4, 1, "E", 0x0008),
    qwerty_picker_grid_key(2, 5, 1, "R", 0x0015),
    qwerty_picker_grid_key(2, 6, 1, "T", 0x0017),
    qwerty_picker_grid_key(2, 7, 1, "Y", 0x001C),
    qwerty_picker_grid_key(2, 8, 1, "U", 0x0018),
    qwerty_picker_grid_key(2, 9, 1, "I", 0x000C),
    qwerty_picker_grid_key(2, 10, 1, "O", 0x0012),
    qwerty_picker_grid_key(2, 11, 1, "P", 0x0013),
    qwerty_picker_grid_key(2, 12, 1, "[", 0x002F),
    qwerty_picker_grid_key(2, 13, 1, "]", 0x0030),
    qwerty_picker_grid_key(2, 14, 1, "\\", 0x0031),
    qwerty_picker_grid_key(3, 0, 2, "Caps\nLock", 0x0039),
    qwerty_picker_grid_key(3, 2, 1, "A", 0x0004),
    qwerty_picker_grid_key(3, 3, 1, "S", 0x0016),
    qwerty_picker_grid_key(3, 4, 1, "D", 0x0007),
    qwerty_picker_grid_key(3, 5, 1, "F", 0x0009),
    qwerty_picker_grid_key(3, 6, 1, "G", 0x000A),
    qwerty_picker_grid_key(3, 7, 1, "H", 0x000B),
    qwerty_picker_grid_key(3, 8, 1, "J", 0x000D),
    qwerty_picker_grid_key(3, 9, 1, "K", 0x000E),
    qwerty_picker_grid_key(3, 10, 1, "L", 0x000F),
    qwerty_picker_grid_key(3, 11, 1, ";", 0x0033),
    qwerty_picker_grid_key(3, 12, 1, "'", 0x0034),
    qwerty_picker_grid_key(3, 13, 2, "Enter", 0x0028),
    qwerty_picker_grid_key(4, 0, 3, "Shift", 0x00E1),
    qwerty_picker_grid_key(4, 3, 1, "Z", 0x001D),
    qwerty_picker_grid_key(4, 4, 1, "X", 0x001B),
    qwerty_picker_grid_key(4, 5, 1, "C", 0x0006),
    qwerty_picker_grid_key(4, 6, 1, "V", 0x0019),
    qwerty_picker_grid_key(4, 7, 1, "B", 0x0005),
    qwerty_picker_grid_key(4, 8, 1, "N", 0x0011),
    qwerty_picker_grid_key(4, 9, 1, "M", 0x0010),
    qwerty_picker_grid_key(4, 10, 1, ",", 0x0036),
    qwerty_picker_grid_key(4, 11, 1, ".", 0x0037),
    qwerty_picker_grid_key(4, 12, 1, "/", 0x0038),
    qwerty_picker_grid_key(4, 13, 2, "Shift", 0x00E5),
    qwerty_picker_grid_key(5, 0, 2, "Ctrl", 0x00E0),
    qwerty_picker_grid_key(5, 2, 1, "GUI", 0x00E3),
    qwerty_picker_grid_key(5, 3, 1, "Alt", 0x00E2),
    qwerty_picker_grid_key(5, 4, 4, "Space", 0x002C),
    qwerty_picker_grid_key(5, 8, 1, "Alt", 0x00E6),
    qwerty_picker_grid_key(5, 9, 1, "Menu", 0x0065),
    qwerty_picker_grid_key(5, 10, 1, "Ctrl", 0x00E4),
    qwerty_picker_grid_key(0, 13, 1, "Print\nScreen", 0x0046),
    qwerty_picker_grid_key(0, 14, 1, "Scroll\nLock", 0x0047),
    qwerty_picker_grid_key(0, 15, 1, "Pause", 0x0048),
    qwerty_picker_grid_key(2, 15, 1, "Home", 0x004A),
    qwerty_picker_grid_key(3, 15, 1, "End", 0x004D),
    qwerty_picker_grid_key(4, 15, 1, "Page\nUp", 0x004B),
    qwerty_picker_grid_key(5, 15, 1, "Page\nDown", 0x004E),
    qwerty_picker_grid_key(5, 11, 1, "←", 0x0050),
    qwerty_picker_grid_key(5, 12, 1, "↑", 0x0052),
    qwerty_picker_grid_key(5, 13, 1, "↓", 0x0051),
    qwerty_picker_grid_key(5, 14, 1, "→", 0x004F),
];

fn qwerty_picker_grid_keys() -> &'static [QwertyPickerGridKey] {
    QWERTY_PICKER_GRID_KEYS
}

fn qwerty_picker_grid_keys_for_values(
    is_allowed: impl Fn(u16) -> bool,
) -> Vec<QwertyPickerGridKey> {
    QWERTY_PICKER_GRID_KEYS
        .iter()
        .copied()
        .filter(|key| is_allowed(key.value))
        .collect()
}

fn qwerty_picker_key_label(
    value: u16,
    fallback_label: &str,
    key_legend_layout: KeyLegendLayout,
    show_shifted_number_symbols: bool,
    layer_names: &[String],
) -> String {
    if show_shifted_number_symbols {
        if key_legend_layout == KeyLegendLayout::English {
            match value {
                0x0035 => return "~\n`".to_string(),
                0x001E => return "!\n1".to_string(),
                0x001F => return "@\n2".to_string(),
                0x0020 => return "#\n3".to_string(),
                0x0021 => return "$\n4".to_string(),
                0x0022 => return "%\n5".to_string(),
                0x0023 => return "^\n6".to_string(),
                0x0024 => return "&\n7".to_string(),
                0x0025 => return "*\n8".to_string(),
                0x0026 => return "(\n9".to_string(),
                0x0027 => return ")\n0".to_string(),
                0x002D => return "_\n-".to_string(),
                0x002E => return "+\n=".to_string(),
                _ => {}
            }
        } else if let Some(label) = picker_shifted_number_label(value, key_legend_layout) {
            return label;
        }
    }

    crate::keycode::find_keycode(value)
        .map(|_| keycode_label_with_names_and_layout(value, &[], layer_names, key_legend_layout))
        .unwrap_or_else(|| fallback_label.to_string())
}

impl KeycodePicker {
    fn basic_key_button_at(
        &mut self,
        ui: &mut egui::Ui,
        origin: egui::Pos2,
        cell_w: f32,
        cell_h: f32,
        gap: f32,
        row: usize,
        col: usize,
        span: usize,
        label: &str,
        value: u16,
    ) {
        let x = origin.x + col as f32 * (cell_w + gap);
        let right_nav_extra_gap = if col >= 16 && matches!(row, 1 | 2) {
            14.0
        } else {
            0.0
        };
        let y = origin.y + row as f32 * (cell_h + gap) + right_nav_extra_gap;
        let width = span as f32 * cell_w + span.saturating_sub(1) as f32 * gap;
        let rect = egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(width, cell_h));
        let resp = picker_keycap_button_in_rect(ui, rect, label, true, false);
        if resp.clicked() {
            self.assign_keycode_value(value);
        }
        if resp.hovered() {
            resp.on_hover_text(crate::i18n::tr_text(
                self.language,
                &keycode_tooltip(value, &[], &self.layer_names),
            ));
        }
    }

    pub(super) fn show_vial_basic(&mut self, ui: &mut egui::Ui) {
        const COLS: usize = 16;
        const ROWS: usize = 6;

        let scale = responsive_picker_element_scale(ui.ctx());
        let cell_w = 54.0 * scale;
        let cell_h = 54.0 * scale;
        let gap = 3.0 * scale;
        let width = COLS as f32 * cell_w + (COLS.saturating_sub(1)) as f32 * gap;
        let height = ROWS as f32 * cell_h + (ROWS.saturating_sub(1)) as f32 * gap;
        let available_width = ui.available_width();
        let x_offset = ((available_width - width).max(0.0) * 0.5).floor();

        ui.horizontal(|ui| {
            if x_offset > 0.0 {
                ui.add_space(x_offset);
            }
            ui.allocate_ui_with_layout(
                Vec2::new(width, 32.0 * scale),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(tr_picker(self.language, "key_picker.section_basic"))
                            .size(11.0 * scale)
                            .color(Color32::from_gray(150)),
                    );
                    let dropdown_width = 126.0 * scale;
                    let spacer = (ui.available_width() - dropdown_width).max(0.0);
                    if spacer > 0.0 {
                        ui.add_space(spacer);
                    }
                    let dropdown_id = ui.make_persistent_id("basic_layout_dropdown");
                    let dropdown_resp = crate::ui_style::modern_dropdown_button(
                        ui,
                        dropdown_id,
                        self.basic_layout.label(),
                        ui.visuals().text_color(),
                        dropdown_width,
                    );
                    egui::popup_below_widget(
                        ui,
                        dropdown_id,
                        &dropdown_resp,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(dropdown_width);
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                            for layout in BasicPickerLayout::ALL {
                                let selected = self.basic_layout == layout;
                                let (option_rect, option_resp) = ui.allocate_exact_size(
                                    Vec2::new(dropdown_width, 28.0 * scale),
                                    egui::Sense::click(),
                                );
                                if option_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                let fill = if selected {
                                    if ui.visuals().dark_mode {
                                        Color32::from_rgb(58, 58, 61)
                                    } else {
                                        Color32::from_rgb(236, 236, 238)
                                    }
                                } else if option_resp.hovered() {
                                    crate::ui_style::hover_fill(ui.visuals().dark_mode)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(option_rect, 7.0, fill);
                                ui.painter().text(
                                    option_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    layout.label(),
                                    egui::FontId::proportional(12.0 * scale),
                                    if selected {
                                        ui.visuals().text_color()
                                    } else {
                                        Color32::from_gray(150)
                                    },
                                );
                                if option_resp.clicked() {
                                    self.basic_layout = layout;
                                    ui.memory_mut(|m| m.close_popup());
                                }
                            }
                        },
                    );
                },
            );
        });
        ui.add_space(4.0);

        let content_width = available_width.max(width);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(content_width, height), egui::Sense::hover());
        let origin = egui::pos2(rect.min.x + x_offset, rect.min.y);
        for key in qwerty_picker_grid_keys() {
            let assigned_value = self.basic_layout.map_value(key.value);
            let display_label = qwerty_picker_key_label(
                assigned_value,
                key.label,
                self.key_legend_layout,
                self.show_shifted_number_symbols,
                &self.layer_names,
            );
            self.basic_key_button_at(
                ui,
                origin,
                cell_w,
                cell_h,
                gap,
                key.row,
                key.col,
                key.span,
                &display_label,
                assigned_value,
            );
        }
    }

    pub(super) fn show_qwerty_popup_key_grid(
        &self,
        ui: &mut egui::Ui,
        is_allowed: impl Fn(u16) -> bool,
    ) -> Option<u16> {
        if qwerty_picker_grid_keys_for_values(|value| is_allowed(value)).is_empty() {
            return None;
        }

        const COLS: usize = 16;
        const ROWS: usize = 6;

        let scale = responsive_picker_element_scale(ui.ctx());
        let gap = 4.0 * scale;
        let available_width = ui.available_width();
        let key_size = popup_key_button_size(ui, "");
        let cell_w = key_size.x.floor();
        let cell_h = key_size.y.floor();
        let width = COLS as f32 * cell_w + (COLS.saturating_sub(1)) as f32 * gap;
        let height = ROWS as f32 * cell_h + (ROWS.saturating_sub(1)) as f32 * gap;
        let mut selected = None;

        ui.add_space(2.0);
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_basic"))
                .size(11.0)
                .color(Color32::from_gray(150))
                .strong(),
        );
        ui.add_space(4.0);

        let content_width = available_width.max(width);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(content_width, height), egui::Sense::hover());
        let origin = rect.min;

        for key in qwerty_picker_grid_keys() {
            let value = key.value;
            let enabled = is_allowed(value);
            let x = origin.x + key.col as f32 * (cell_w + gap);
            let right_nav_extra_gap = if key.col >= 16 && matches!(key.row, 1 | 2) {
                10.0
            } else {
                0.0
            };
            let y = origin.y + key.row as f32 * (cell_h + gap) + right_nav_extra_gap;
            let width = key.span as f32 * cell_w + key.span.saturating_sub(1) as f32 * gap;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(width, cell_h));
            let label = qwerty_picker_key_label(
                value,
                key.label,
                self.key_legend_layout,
                self.show_shifted_number_symbols,
                &self.layer_names,
            );
            let resp = picker_keycap_button_in_rect(ui, rect, &label, enabled, false);
            if enabled && resp.clicked() {
                selected = Some(value);
            }
            if enabled && resp.hovered() {
                resp.on_hover_text(crate::i18n::tr_text(
                    self.language,
                    &keycode_tooltip(value, &[], &self.layer_names),
                ));
            }
        }
        ui.add_space(crate::ui_style::modal_space_sm());

        selected
    }

    pub(super) fn show_popup_view_mode_toggle(&mut self, ui: &mut egui::Ui) {
        let scale = responsive_picker_element_scale(ui.ctx());
        let labels: Vec<String> = PickerViewMode::ALL
            .iter()
            .map(|mode| tr_picker(self.language, mode.i18n_key()).to_string())
            .collect();
        if let Some(index) = crate::ui_style::settings_segmented_control(
            ui,
            "key_picker_popup_view_mode",
            &labels,
            self.popup_view_mode.index(),
            Vec2::new(184.0 * scale, 32.0 * scale),
        ) {
            self.popup_view_mode = PickerViewMode::from_index(index);
        }
    }

    pub(super) fn show_popup_view_mode_header(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            self.show_popup_view_mode_toggle(ui);
        });
        ui.add_space(crate::ui_style::modal_space_sm());
    }

    pub(super) fn show_popup_key_choice_view(
        &mut self,
        ui: &mut egui::Ui,
        key_choices: Vec<&'static crate::keycode::Keycode>,
        friendly_mods: bool,
    ) -> Option<u16> {
        match self.popup_view_mode {
            PickerViewMode::Layout => self.show_qwerty_popup_key_grid(ui, |value| {
                key_choices.iter().any(|kc| kc.value == value)
            }),
            PickerViewMode::List => show_grouped_popup_key_buttons(
                ui,
                key_choices,
                &self.layer_names,
                friendly_mods,
                self.language,
                self.key_legend_layout,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwerty_picker_grid_exposes_standard_letter_positions() {
        let keys = qwerty_picker_grid_keys();

        assert!(keys
            .iter()
            .any(|key| key.row == 2 && key.col == 2 && key.label == "Q" && key.value == 0x0014));
        assert!(keys
            .iter()
            .any(|key| key.row == 3 && key.col == 2 && key.label == "A" && key.value == 0x0004));
        assert!(keys
            .iter()
            .any(|key| key.row == 4 && key.col == 9 && key.label == "M" && key.value == 0x0010));
    }

    #[test]
    fn qwerty_popup_grid_omits_keys_disallowed_by_picker_context() {
        let allowed = [0x0014, 0x0004, 0x002C];
        let keys = qwerty_picker_grid_keys_for_values(|value| allowed.contains(&value));

        assert_eq!(
            keys.iter().map(|key| key.value).collect::<Vec<_>>(),
            allowed
        );
    }
}
