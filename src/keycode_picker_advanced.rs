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
            ui.vertical_centered(|ui| crate::ui_style::modal_intro(ui, intro));
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
                self.close_advanced_slot_picker();
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
                    self.close_advanced_slot_picker();
                }
            });
        });
        if !open {
            self.close_advanced_slot_picker();
        }
    }
}

fn advanced_slot_popup_size(ctx: &egui::Context, slot_count: usize) -> egui::Vec2 {
    let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ctx);
    let width = metrics
        .settings_content_width()
        .min((ctx.content_rect().width() - metrics.value(32.0)).max(metrics.value(320.0)));
    let columns = ((width - metrics.value(20.0) + metrics.value(4.0)) / metrics.value(52.0))
        .floor()
        .clamp(4.0, 16.0) as usize;
    let visible_rows = slot_count.max(1).div_ceil(columns).min(2);
    let height = metrics.value(172.0 + 46.0 * visible_rows as f32);
    egui::vec2(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_slot_popup_grows_for_a_second_visible_row() {
        let ctx = egui::Context::default();

        assert!(advanced_slot_popup_size(&ctx, 16).y > advanced_slot_popup_size(&ctx, 4).y);
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
