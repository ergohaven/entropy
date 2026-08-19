use super::*;

impl KeycodePicker {
    pub(super) fn show_vial_advanced(
        &mut self,
        ui: &mut egui::Ui,
        macro_data_state: DeferredPickerDataState,
        tap_dance_data_state: DeferredPickerDataState,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            crate::ui_style::modal_intro(ui, tr_picker(self.language, "key_picker.advanced_intro"));
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if self.supports_macro {
                    self.show_advanced_kind_button(
                        ui,
                        AdvancedSlotKind::Macro,
                        tr_picker(self.language, "macro_editor.picker_item"),
                        tr_picker(self.language, "key_picker.advanced_macro_tooltip"),
                        macro_data_state,
                    );
                }
                if self.supports_tap_dance {
                    self.show_advanced_kind_button(
                        ui,
                        AdvancedSlotKind::TapDance,
                        tr_picker(self.language, "tap_dance_editor.picker_item"),
                        tr_picker(self.language, "key_picker.advanced_tap_dance_tooltip"),
                        tap_dance_data_state,
                    );
                }
            });
        });
    }

    fn show_advanced_kind_button(
        &mut self,
        ui: &mut egui::Ui,
        kind: AdvancedSlotKind,
        label: &str,
        tooltip: &str,
        data_state: DeferredPickerDataState,
    ) {
        let ready = data_state == DeferredPickerDataState::Ready;
        let response = crate::ui_style::modern_button_with_font(
            ui,
            label,
            egui::vec2(190.0, 54.0),
            14.0,
            ready,
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
        let popup_size = crate::ui_style::ResponsiveMetrics::from_ctx(ctx).size(470.0, 260.0);
        crate::ui_style::centered_modal_window(
            ctx,
            title,
            self.popup_state.id(PopupKey::AdvancedSlotWindow),
            &mut open,
            popup_size,
        )
        .show(ctx, |ui| {
            apply_picker_button_visuals(ui);
            crate::ui_style::modal_intro(ui, title);
            ui.add_space(crate::ui_style::modal_space_sm());
            let picked = match kind {
                AdvancedSlotKind::Macro => self.show_macro_slot_grid(
                    ui,
                    self.macro_inline_selected.unwrap_or(0),
                    "advanced_macro_slots",
                ),
                AdvancedSlotKind::TapDance => self.show_tap_dance_slot_grid(
                    ui,
                    self.tap_dance_editor_open.unwrap_or(0),
                    "advanced_tap_dance_slots",
                ),
            };
            if let Some(slot) = picked {
                self.advanced_slot_picker = None;
                match kind {
                    AdvancedSlotKind::Macro => self.assign_macro_slot(slot),
                    AdvancedSlotKind::TapDance => self.assign_tap_dance_slot(slot),
                }
            }
            ui.add_space(crate::ui_style::modal_space_sm());
            ui.horizontal_centered(|ui| {
                if crate::ui_style::modern_button(
                    ui,
                    tr_picker(self.language, "key_picker.cancel"),
                    crate::ui_style::modal_action_button_size(),
                    true,
                )
                .clicked()
                {
                    self.advanced_slot_picker = None;
                }
            });
        });
        if !open {
            self.advanced_slot_picker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_tab_is_visible_for_either_supported_action_kind() {
        let macro_only = KeycodePicker {
            supports_macro: true,
            supports_tap_dance: false,
            ..Default::default()
        };
        assert!(macro_only.vial_tab_supported(KeycodeTab::Advanced));

        let tap_dance_only = KeycodePicker {
            supports_macro: false,
            supports_tap_dance: true,
            ..Default::default()
        };
        assert!(tap_dance_only.vial_tab_supported(KeycodeTab::Advanced));
    }
}
