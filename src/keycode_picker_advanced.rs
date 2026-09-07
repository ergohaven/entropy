use super::*;

impl KeycodePicker {
    pub(super) fn show_special_action_kind_button(
        &mut self,
        ui: &mut egui::Ui,
        kind: AdvancedSlotKind,
        label: &str,
        tooltip: &str,
        data_state: DeferredPickerDataState,
    ) {
        let ready = data_state == DeferredPickerDataState::Ready;
        let compact_label = label.replace(' ', "\n");
        let response = picker_button(
            ui,
            &compact_label,
            Self::picker_key_size(ui.ctx()),
            ready,
            false,
        )
        .on_hover_text(tooltip);
        if response.clicked() && ready {
            self.advanced_slot_picker = Some(kind);
        }

        if response.hovered() {
            match data_state {
                DeferredPickerDataState::Loading => {
                    response
                        .on_hover_text(tr_picker(self.language, "connection.loading_device_data"));
                }
                DeferredPickerDataState::Failed => {
                    response.on_hover_text(tr_picker(
                        self.language,
                        "connection.device_data_load_failed",
                    ));
                    if ui.input(|input| input.pointer.primary_clicked()) {
                        self.deferred_retry_tab = Some(match kind {
                            AdvancedSlotKind::Macro => KeycodeTab::Macro,
                            AdvancedSlotKind::TapDance => KeycodeTab::TapDance,
                        });
                    }
                }
                DeferredPickerDataState::Ready => {}
            }
        }
    }

    pub(super) fn close_advanced_slot_picker(&mut self) {
        self.advanced_slot_picker = None;
        self.popup_state.on_close(PopupKey::AdvancedSlotWindow);
    }

    pub(super) fn close_advanced_slot_picker_on_escape(&mut self, escape_pressed: bool) -> bool {
        if escape_pressed && self.advanced_slot_picker.is_some() {
            self.close_advanced_slot_picker();
            return true;
        }
        false
    }

    pub(super) fn show_advanced_slot_picker(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.advanced_slot_picker else {
            return;
        };
        let mut open = true;
        let title = match kind {
            AdvancedSlotKind::Macro => tr_picker(self.language, "macro_editor.choose_macro"),
            AdvancedSlotKind::TapDance => {
                tr_picker(self.language, "tap_dance_editor.choose_tap_dance")
            }
        };
        let slot_count = match kind {
            AdvancedSlotKind::Macro => self.macro_count,
            AdvancedSlotKind::TapDance => self.tap_dance_entries.len(),
        };
        let selected_slot = match kind {
            AdvancedSlotKind::Macro => self.macro_inline_selected.unwrap_or(0),
            AdvancedSlotKind::TapDance => self.tap_dance_editor_open.unwrap_or(0),
        };
        let choices = (0..slot_count.min(u8::MAX as usize + 1))
            .map(|slot| {
                let slot = slot as u8;
                let id = match kind {
                    AdvancedSlotKind::Macro => format!("M{slot}"),
                    AdvancedSlotKind::TapDance => format!("TD{slot}"),
                };
                let display_name = match kind {
                    AdvancedSlotKind::Macro => self.macro_display_name(slot as usize),
                    AdvancedSlotKind::TapDance => self.tap_dance_display_name(slot as usize),
                };
                let label = advanced_slot_label(&id, &display_name);
                let tooltip = match kind {
                    AdvancedSlotKind::Macro => self
                        .macro_description(slot as usize)
                        .map(|description| format!("{label}\n{description}"))
                        .unwrap_or_else(|| label.clone()),
                    AdvancedSlotKind::TapDance => label.clone(),
                };
                (slot, label, tooltip)
            })
            .collect::<Vec<_>>();
        let popup_size = advanced_slot_popup_size(ctx, slot_count);
        crate::ui_style::centered_modal_window(
            ctx,
            title,
            self.popup_state.id(PopupKey::AdvancedSlotWindow),
            &mut open,
            popup_size,
        )
        .show(ctx, |ui| {
            apply_picker_button_visuals(ui);
            let intro = match kind {
                AdvancedSlotKind::Macro => {
                    tr_picker(self.language, "macro_editor.select_macro_slot")
                }
                AdvancedSlotKind::TapDance => {
                    tr_picker(self.language, "tap_dance_editor.select_tap_dance_slot")
                }
            };
            crate::ui_style::modal_intro(ui, intro);
            ui.add_space(crate::ui_style::modal_space_sm());
            let mut picked = None;
            egui::ScrollArea::vertical()
                .id_salt("advanced_slot_choices")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (slot, label, tooltip) in &choices {
                            let response =
                                picker_choice_button(ui, label, tooltip, *slot == selected_slot);
                            if response.clicked() {
                                picked = Some(*slot);
                            }
                        }
                    });
                });
            if let Some(slot) = picked {
                self.close_advanced_slot_picker();
                match kind {
                    AdvancedSlotKind::Macro => self.assign_macro_slot(slot),
                    AdvancedSlotKind::TapDance => self.assign_tap_dance_slot(slot),
                }
            }
        });
        if !open {
            self.close_advanced_slot_picker();
        }
    }
}

fn advanced_slot_popup_size(ctx: &egui::Context, slot_count: usize) -> egui::Vec2 {
    const WIDTH: f32 = 300.0;
    const COLUMNS: usize = 3;
    let rows = slot_count.max(1).div_ceil(COLUMNS);
    let desired_height = (48.0 + rows as f32 * 36.0).max(120.0);
    let available_height = (ctx.content_rect().height() - 64.0).max(120.0);
    egui::vec2(WIDTH, desired_height.min(available_height))
}

fn advanced_slot_label(id: &str, display_name: &str) -> String {
    if display_name == id {
        id.to_owned()
    } else {
        format!("{id}: {display_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_slot_popup_matches_layer_picker_width_and_grows_with_rows() {
        let ctx = egui::Context::default();

        assert_eq!(advanced_slot_popup_size(&ctx, 4).x, 300.0);
        assert!(advanced_slot_popup_size(&ctx, 16).y > advanced_slot_popup_size(&ctx, 4).y);
    }

    #[test]
    fn named_action_slot_uses_the_layer_picker_label_pattern() {
        assert_eq!(advanced_slot_label("M3", "Paste"), "M3: Paste");
        assert_eq!(advanced_slot_label("TD2", "TD2"), "TD2");
    }

    #[test]
    fn escape_closes_only_the_advanced_slot_window() {
        let mut picker = KeycodePicker {
            open: true,
            advanced_slot_picker: Some(AdvancedSlotKind::Macro),
            ..Default::default()
        };

        assert!(picker.close_advanced_slot_picker_on_escape(true));
        assert!(picker.open);
        assert!(picker.advanced_slot_picker.is_none());
    }
}
