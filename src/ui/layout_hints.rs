use super::*;

impl EntropyApp {
    pub(super) fn draw_layout_bottom_hints(
        &mut self,
        ui: &mut egui::Ui,
        center_x: f32,
        layer_name_hovered: bool,
    ) {
        // Hint text below layer name
        let hint_color = if self.dark_mode {
            Color32::from_gray(100)
        } else {
            Color32::from_gray(160)
        };
        let hint_font = FontId::proportional(12.0);
        let secondary_hint_font = hint_font.clone();
        let hint_y = ui.max_rect().bottom() - 36.0;
        let any_hovered = self.prev_hovered_key.is_some() || self.prev_hovered_encoder;
        let hint_language = self.app_settings.language;
        let tr_hint = |key: &'static str| crate::i18n::tr_catalog(hint_language, key);
        let hint_binding = || {
            self.prev_hovered_key
                .and_then(|ki| {
                    self.layout
                        .as_ref()
                        .map(|l| l.get_key_binding(self.selected_layer, ki))
                })
                .or_else(|| self.prev_hovered_encoder_keycode.map(Into::into))
                .or_else(|| {
                    self.selected_key.and_then(|(selected_layer, selected_ki)| {
                        (selected_layer == self.selected_layer)
                            .then(|| {
                                self.layout
                                    .as_ref()
                                    .map(|l| l.get_key_binding(self.selected_layer, selected_ki))
                            })
                            .flatten()
                    })
                })
        };
        let hint_kc = || hint_binding().map(crate::keyboard::KeyBinding::vial_keycode);
        if let Some(hl) = self.hover_layer {
            let hl_name = self
                .layer_names
                .get(hl)
                .cloned()
                .unwrap_or_else(|| hl.to_string());
            let hovered_is_lt = hint_kc().map(|kc| kc & 0xF000 == 0x4000).unwrap_or(false);
            let mut line = 0i32;
            let line_h = 14.0f32;
            let base_y = hint_y - 15.0;
            // Line 1: always
            ui.painter().text(
                egui::pos2(center_x, base_y + line as f32 * line_h),
                egui::Align2::CENTER_CENTER,
                tr_hint("key_hints.change_key"),
                hint_font.clone(),
                hint_color,
            );
            line += 1;
            // Line 2: go to layer (if not current)
            if hl != self.selected_layer {
                let layer_index = hl.to_string();
                let go_to_layer_hint = crate::i18n::tr_catalog_format(
                    hint_language,
                    "key_hints.go_to_layer",
                    &[("layer", layer_index.as_str()), ("name", hl_name.as_str())],
                );
                ui.painter().text(
                    egui::pos2(center_x, base_y + line as f32 * line_h),
                    egui::Align2::CENTER_CENTER,
                    go_to_layer_hint,
                    hint_font.clone(),
                    hint_color,
                );
                line += 1;
            }
            // Line 3: layer-specific secondary action
            ui.painter().text(
                egui::pos2(center_x, base_y + line as f32 * line_h),
                egui::Align2::CENTER_CENTER,
                if hovered_is_lt {
                    tr_hint("key_hints.change_tap_key")
                } else {
                    tr_hint("key_hints.change_layer_number")
                },
                hint_font.clone(),
                hint_color,
            );
            line += 1;
            // Line 4: go back (if in jump mode)
            if !self.jump_back_stack.is_empty() {
                ui.painter().text(
                    egui::pos2(center_x, base_y + line as f32 * line_h),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.esc_back"),
                    hint_font.clone(),
                    hint_color,
                );
            }
            let _ = hint_font;
        } else if !self.jump_back_stack.is_empty() {
            if any_hovered {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 9.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
            }
            ui.painter().text(
                egui::pos2(center_x, if any_hovered { hint_y + 5.0 } else { hint_y }),
                egui::Align2::CENTER_CENTER,
                tr_hint("key_hints.right_click_or_esc_back"),
                hint_font,
                hint_color,
            );
        } else if any_hovered {
            // Check if hovered key is a mod key
            let (
                hovered_is_mod,
                hovered_can_swap_side,
                hovered_can_retarget_mod_key,
                hovered_is_macro,
                hovered_is_tap_dance,
                hovered_is_mouse,
                hovered_is_alt_repeat,
                hovered_is_grave_escape,
                hovered_is_layer,
                hovered_is_lt,
                hovered_is_mod_tap,
            ) = {
                hint_binding()
                    .map(|binding| {
                        let kc = binding.vial_keycode();
                        let is_native_mod_tap = binding
                            .rmk_action()
                            .and_then(crate::rmk_native::rmk_mod_tap_parts)
                            .is_some();
                        let is_mod_tap = (0x2000..0x4000).contains(&kc) || is_native_mod_tap;
                        let is_plain_mod = (0x00E0..=0x00E7).contains(&kc)
                            || matches!(
                                kc,
                                0x52A1
                                    | 0x52A2
                                    | 0x52A4
                                    | 0x52A7
                                    | 0x52A8
                                    | 0x52AF
                                    | 0x52B1
                                    | 0x52B2
                                    | 0x52B4
                                    | 0x52B8
                            );
                        let is_mod = is_plain_mod
                            || is_mod_tap
                            || ((0x0100..0x2000).contains(&kc) && (kc & 0xFF) != 0);
                        let can_swap_side =
                            toggle_handed_modifier(kc).is_some() || is_native_mod_tap;
                        let is_macro = (0x7700..=0x77FF).contains(&kc);
                        let is_tap_dance = (0x5700..=0x57FF).contains(&kc);
                        let is_mouse = is_mouse_keycode(kc);
                        let is_alt_repeat = is_alt_repeat_keycode(kc);
                        let is_grave_escape = kc == 0x7C16;
                        let is_layer = vial_layer_target(kc).is_some();
                        let is_lt = kc & 0xF000 == 0x4000;
                        let can_retarget_mod_key = !is_layer
                            && (is_mod_tap || ((0x0100..0x2000).contains(&kc) && (kc & 0xFF) != 0));
                        (
                            is_mod,
                            can_swap_side,
                            can_retarget_mod_key,
                            is_macro,
                            is_tap_dance,
                            is_mouse,
                            is_alt_repeat,
                            is_grave_escape,
                            is_layer,
                            is_lt,
                            is_mod_tap,
                        )
                    })
                    .unwrap_or((
                        false, false, false, false, false, false, false, false, false, false, false,
                    ))
            };
            if hovered_is_mod {
                if hovered_can_swap_side {
                    let show_retarget = hovered_can_retarget_mod_key;
                    ui.painter().text(
                        egui::pos2(
                            center_x,
                            if show_retarget {
                                hint_y - 22.0
                            } else {
                                hint_y - 10.0
                            },
                        ),
                        egui::Align2::CENTER_CENTER,
                        tr_hint("key_hints.change_key"),
                        hint_font.clone(),
                        hint_color,
                    );
                    if show_retarget {
                        ui.painter().text(
                            egui::pos2(center_x, hint_y - 4.0),
                            egui::Align2::CENTER_CENTER,
                            if hovered_is_mod_tap {
                                tr_hint("key_hints.change_mod_tap_key")
                            } else {
                                tr_hint("key_hints.change_modifier_key")
                            },
                            secondary_hint_font.clone(),
                            hint_color,
                        );
                    }
                    ui.painter().text(
                        egui::pos2(
                            center_x,
                            if show_retarget {
                                hint_y + 12.0
                            } else {
                                hint_y + 8.0
                            },
                        ),
                        egui::Align2::CENTER_CENTER,
                        tr_hint("key_hints.switch_modifier_side"),
                        secondary_hint_font,
                        hint_color,
                    );
                } else {
                    ui.painter().text(
                        egui::pos2(center_x, hint_y - 14.0),
                        egui::Align2::CENTER_CENTER,
                        tr_hint("key_hints.change_key"),
                        hint_font.clone(),
                        hint_color,
                    );
                    ui.painter().text(
                        egui::pos2(center_x, hint_y + 4.0),
                        egui::Align2::CENTER_CENTER,
                        tr_hint("key_hints.change_modifier_key"),
                        secondary_hint_font,
                        hint_color,
                    );
                }
            } else if hovered_is_macro {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 14.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y + 4.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.edit_macro"),
                    secondary_hint_font.clone(),
                    hint_color,
                );
            } else if hovered_is_tap_dance {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 14.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y + 4.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.edit_tap_dance"),
                    secondary_hint_font,
                    hint_color,
                );
            } else if hovered_is_mouse {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 14.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y + 4.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.open_mouse_keys"),
                    secondary_hint_font.clone(),
                    hint_color,
                );
            } else if hovered_is_alt_repeat {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 14.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y + 4.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.open_alt_repeat"),
                    secondary_hint_font,
                    hint_color,
                );
            } else if hovered_is_grave_escape {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 14.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y + 4.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.open_grave_escape"),
                    secondary_hint_font,
                    hint_color,
                );
            } else if hovered_is_layer {
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 22.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y - 4.0),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.go_to_that_layer"),
                    secondary_hint_font.clone(),
                    hint_color,
                );
                ui.painter().text(
                    egui::pos2(center_x, hint_y + 12.0),
                    egui::Align2::CENTER_CENTER,
                    if hovered_is_lt {
                        tr_hint("key_hints.change_tap_key")
                    } else {
                        tr_hint("key_hints.change_layer_target")
                    },
                    secondary_hint_font,
                    hint_color,
                );
            } else {
                ui.painter().text(
                    egui::pos2(center_x, hint_y),
                    egui::Align2::CENTER_CENTER,
                    tr_hint("key_hints.change_key"),
                    hint_font,
                    hint_color,
                );
            }
        } else if layer_name_hovered {
            ui.painter().text(
                egui::pos2(center_x, hint_y),
                egui::Align2::CENTER_CENTER,
                tr_hint("key_hints.rename_layer"),
                hint_font,
                hint_color,
            );
        }
    }
}
