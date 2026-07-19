use super::*;

impl EntropyApp {
    pub(super) fn apply_picker_results(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.hid_write_task_active() {
            return;
        }

        if let Some(kc_value) = self.keycode_picker.result.take() {
            if let Some((combo_idx, field)) = self.combo_pick_target.take() {
                self.push_combo_undo();
                if let Some(combo) = self.combo_entries.get_mut(combo_idx) {
                    match field {
                        ComboPickField::Trigger(key_idx) => combo.keys[key_idx] = kc_value,
                        ComboPickField::Output => {
                            combo.output =
                                crate::keycode::normalize_output_symbol_keycode(kc_value);
                        }
                    }
                    self.mark_combo_dirty();
                }
            } else if let Some(field) = self.key_override_pick_target.take() {
                let idx = self
                    .selected_key_override
                    .min(self.key_override_entries.len().saturating_sub(1));
                self.push_key_override_undo();
                if let Some(entry) = self.key_override_entries.get_mut(idx) {
                    match field {
                        KeyOverridePickField::Trigger => entry.trigger = kc_value,
                        KeyOverridePickField::Replacement => entry.replacement = kc_value,
                    }
                    Self::normalize_key_override_entry(entry);
                }
                self.write_key_override(idx);
            } else if let Some(field) = self.alt_repeat_pick_target.take() {
                let idx = self
                    .selected_alt_repeat
                    .min(self.alt_repeat_entries.len().saturating_sub(1));
                if let Some(entry) = self.alt_repeat_entries.get_mut(idx) {
                    match field {
                        AltRepeatPickField::LastKey => entry.keycode = kc_value,
                        AltRepeatPickField::AltKey => entry.alt_keycode = kc_value,
                    }
                }
                self.write_alt_repeat_entry(idx);
            } else if let Some((layer, encoder_visual_idx)) = self.selected_encoder {
                #[cfg(not(target_arch = "wasm32"))]
                self.assign_encoder_keycode(layer, encoder_visual_idx, kc_value);
                #[cfg(target_arch = "wasm32")]
                if let Some(layout) = &mut self.layout {
                    layout.set_encoder_keycode(layer, encoder_visual_idx, kc_value);
                }
                if is_alt_repeat_keycode(kc_value) {
                    self.open_alt_repeat_window_compact();
                }
            } else if let Some((layer, ki)) = self.selected_key {
                #[cfg(not(target_arch = "wasm32"))]
                self.assign_keycode(layer, ki, kc_value);
                #[cfg(target_arch = "wasm32")]
                if let Some(layout) = &mut self.layout {
                    layout.set_keycode(layer, ki, kc_value);
                }
                if is_alt_repeat_keycode(kc_value) {
                    self.open_alt_repeat_window_compact();
                }
            }
            self.selected_key = None;
            self.selected_encoder = None;
        }
    }

    pub(super) fn assign_encoder_keycode(
        &mut self,
        layer: usize,
        encoder_visual_idx: usize,
        kc_value: u16,
    ) {
        if self.qmk_settings_write_busy() {
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "settings_write.busy")
                    .to_owned();
            return;
        }
        let encoder = match self
            .layout
            .as_ref()
            .and_then(|l| l.encoders.get(encoder_visual_idx))
        {
            Some(e) => e.clone(),
            None => return,
        };
        let old_kc = self
            .layout
            .as_ref()
            .map(|l| l.get_encoder_keycode(layer, encoder_visual_idx))
            .unwrap_or(0);
        self.undo_stack.push(UndoAction::Encoder {
            layer,
            encoder_visual_idx,
            old_kc,
        });

        if let Some(layout) = &mut self.layout {
            layout.set_encoder_keycode(layer, encoder_visual_idx, kc_value);
        }

        let Some(conn) = &self.hid_device else {
            self.status_msg =
                "Read-only: encoder changed locally, firmware write disabled for this device"
                    .into();
            return;
        };
        let result = conn.set_encoder(
            layer as u8,
            encoder.encoder_idx,
            encoder.direction,
            kc_value,
        );

        match result {
            Ok(()) => {
                self.status_msg = format!(
                    "Assigned encoder {} direction {} on layer {}",
                    encoder.encoder_idx,
                    encoder.direction,
                    layer + 1
                );
            }
            Err(e) => {
                self.status_msg = format!("Set encoder failed: {e}");
            }
        }
    }

    pub(super) fn open_picker_for_target(
        &mut self,
        key_target: Option<usize>,
        encoder_target: Option<usize>,
    ) {
        let current_keycode = self.layout.as_ref().and_then(|layout| {
            key_target
                .map(|ki| layout.get_keycode(self.selected_layer, ki))
                .or_else(|| {
                    encoder_target.map(|ei| layout.get_encoder_keycode(self.selected_layer, ei))
                })
        });
        self.selected_key = key_target.map(|ki| (self.selected_layer, ki));
        self.selected_encoder = encoder_target.map(|ei| (self.selected_layer, ei));
        self.keycode_picker.open = true;
        self.keycode_picker.result = None;
        self.keycode_picker.search_query.clear();
        self.keycode_picker.layer_names = self.layer_names.clone();
        self.keycode_picker.vial_quantum_pending_mod = None;
        self.keycode_picker.vial_quantum_pending_mt = None;
        self.keycode_picker.vial_layer_pending = None;
        self.keycode_picker.tap_dance_editor_open = None;
        self.keycode_picker.macro_inline_selected = None;
        self.keycode_picker.td_key_pick = None;
        self.keycode_picker.td_mod_key_pick = None;
        if let Some(current_keycode) = current_keycode {
            self.keycode_picker.select_tab_for_keycode(current_keycode);
        } else {
            self.keycode_picker.selected_tab = crate::keycode_picker::KeycodeTab::Basic;
        }
    }

    pub(super) fn handle_secondary_target(
        &mut self,
        ctrl_held: bool,
        kc: u16,
        key_target: Option<usize>,
        encoder_target: Option<usize>,
    ) {
        if !ctrl_held {
            if let Some(target_layer) = vial_layer_target(kc) {
                if target_layer != self.selected_layer {
                    self.jump_back_stack.push(self.selected_layer);
                    self.selected_layer = target_layer;
                    self.hover_layer = None;
                }
                self.secondary_click_handled = true;
                return;
            }
        }
        if ctrl_held {
            if kc & 0xF000 == 0x4000 {
                self.open_picker_for_target(key_target, encoder_target);
                self.keycode_picker.vial_quantum_pending_mt = Some(kc & 0xFF00);
                self.keycode_picker.vial_quantum_pending_mod = None;
                self.secondary_click_handled = true;
            } else if let Some(swapped) = toggle_handed_modifier(kc) {
                if let Some(visual_idx) = encoder_target {
                    self.assign_encoder_keycode(self.selected_layer, visual_idx, swapped);
                } else if let Some(ki) = key_target {
                    self.pending_handed_swap = Some((self.selected_layer, ki, swapped));
                }
                self.secondary_click_handled = true;
            } else {
                if let Some(base) = vial_layer_retarget_base(kc) {
                    self.open_picker_for_target(key_target, encoder_target);
                    self.keycode_picker.vial_layer_pending = Some(base);
                    self.secondary_click_handled = true;
                }
            }
            if self.secondary_click_handled {
                return;
            }
        }
        if (0x7700..=0x77FF).contains(&kc) {
            let macro_n = (kc - 0x7700) as u8;
            self.open_picker_for_target(key_target, encoder_target);
            self.keycode_picker.selected_tab = crate::keycode_picker::KeycodeTab::Macro;
            self.keycode_picker.macro_inline_selected = Some(macro_n);
            self.secondary_click_handled = true;
            return;
        }
        if (0x5700..=0x57FF).contains(&kc) {
            let td_n = (kc - 0x5700) as u8;
            self.open_picker_for_target(key_target, encoder_target);
            self.keycode_picker.selected_tab = crate::keycode_picker::KeycodeTab::TapDance;
            self.keycode_picker.tap_dance_editor_open = Some(td_n);
            self.secondary_click_handled = true;
            return;
        }
        if is_mouse_keycode(kc) {
            self.open_mouse_keys_settings_page();
            self.secondary_click_handled = true;
            return;
        }
        if is_alt_repeat_keycode(kc) {
            self.open_alt_repeat_window_compact();
            self.secondary_click_handled = true;
            return;
        }
        let is_layer_key = vial_layer_target(kc).is_some();
        let pending_base: Option<u16> = if is_layer_key {
            None
        } else if (0x2000..0x4000).contains(&kc)
            || ((0x0100..0x2000).contains(&kc) && (kc & 0xFF) != 0)
        {
            Some(kc & 0xFF00)
        } else {
            None
        };
        if let Some(base) = pending_base {
            self.open_picker_for_target(key_target, encoder_target);
            if kc >= 0x2000 {
                self.keycode_picker.vial_quantum_pending_mt = Some(base);
                self.keycode_picker.vial_quantum_pending_mod = None;
            } else {
                self.keycode_picker.vial_quantum_pending_mod = Some(base);
                self.keycode_picker.vial_quantum_pending_mt = None;
            }
            self.secondary_click_handled = true;
        }
    }

    pub(super) fn assign_keycode(&mut self, layer: usize, ki: usize, kc_value: u16) {
        if self.qmk_settings_write_busy() {
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "settings_write.busy")
                    .to_owned();
            return;
        }
        let old_kc = self
            .layout
            .as_ref()
            .map(|l| l.get_keycode(layer, ki))
            .unwrap_or(0);

        let key = match self.layout.as_ref().and_then(|l| l.keys.get(ki)) {
            Some(k) => k.clone(),
            None => return,
        };

        let commit_local_assignment = |this: &mut Self| {
            this.undo_stack.push(UndoAction::Key {
                layer,
                key_idx: ki,
                old_kc,
            });
            if let Some(layout) = &mut this.layout {
                layout.set_keycode(layer, ki, kc_value);
            }
            this.refresh_layer_picker_content_flags();
        };

        // Never open a fresh HID handle synchronously from the UI thread.
        // Connect keeps the active Vial handle alive; if it is unavailable,
        // keep the edit local/read-only instead of freezing the whole app.
        let Some(conn) = &self.hid_device else {
            commit_local_assignment(self);
            self.status_msg =
                "Read-only: key changed locally, firmware write disabled for this device".into();
            return;
        };
        let result = conn.set_keycode(layer as u8, key.row, key.col, kc_value);

        match result {
            Ok(()) => {
                commit_local_assignment(self);
                self.status_msg = "✓ Saved".into();
            }
            Err(e) => {
                self.status_msg = format!("Write error: {e}");
                if !crate::hid::is_keycode_writeback_mismatch(&e) {
                    // Connection lost — reopen
                    self.hid_device = None;
                }
            }
        }
    }

    /// Reload all keycodes from device in background.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn load_from_device(&mut self) {
        if let Some(idx) = self.selected_device {
            self.start_connect(idx);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            return;
        };
        match action {
            UndoAction::Key {
                layer,
                key_idx,
                old_kc,
            } => {
                self.assign_keycode(layer, key_idx, old_kc);
                self.undo_stack.pop();
            }
            UndoAction::Encoder {
                layer,
                encoder_visual_idx,
                old_kc,
            } => {
                self.assign_encoder_keycode(layer, encoder_visual_idx, old_kc);
                self.undo_stack.pop();
            }
            UndoAction::Layer {
                layer,
                old,
                requires_firmware,
            } => {
                self.undo_layer_snapshot(layer, old, requires_firmware);
            }
        }
    }
}
