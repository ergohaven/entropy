use crate::keyboard::KeyBinding;

use super::*;

fn combo_tap_hold_keycode(binding: &KeyBinding) -> Option<u16> {
    let value = binding.vial_keycode();
    let is_layer_tap = value & 0xF000 == 0x4000;
    let is_mod_tap = value & 0xE000 == 0x2000;
    (is_layer_tap || is_mod_tap).then_some(value & 0x00FF)
}

fn unique_combo_tap_hold_match<'a>(
    picked: u16,
    keycodes: impl IntoIterator<Item = &'a KeyBinding>,
) -> Option<KeyBinding> {
    let mut matched = None;

    for keycode in keycodes {
        if combo_tap_hold_keycode(keycode) != Some(picked) {
            continue;
        }

        match &matched {
            Some(previous) if previous != keycode => return None,
            Some(_) => {}
            None => matched = Some(*keycode),
        }
    }

    matched
}

fn resolve_combo_trigger_keycode(
    picked: &KeyBinding,
    selected_layer: usize,
    layers: &[Vec<KeyBinding>],
) -> KeyBinding {
    // QMK compares complete keycodes for combo triggers. If a basic key was
    // picked but the current layer contains only one matching MT/LT keycode,
    // use that assigned keycode so the combo can match it.
    let KeyBinding::Vial(picked_value) = picked else {
        return *picked;
    };

    if *picked_value <= 0x0001 || *picked_value > 0x00FF {
        return *picked;
    }

    if let Some(layer) = layers.get(selected_layer) {
        if layer.contains(picked) {
            return *picked;
        }
        if let Some(keycode) = unique_combo_tap_hold_match(*picked_value, layer) {
            return keycode;
        }
    }

    if layers.iter().flatten().any(|keycode| keycode == picked) {
        return *picked;
    }

    unique_combo_tap_hold_match(*picked_value, layers.iter().flatten()).unwrap_or(*picked)
}

impl EntropyApp {
    pub(super) fn apply_picker_results(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.hid_user_action_busy() {
            return;
        }

        if let Some(binding) = self.keycode_picker.result.take() {
            let kc_value = binding.vial_keycode();
            if binding.rmk_action().is_some() && self.selected_key.is_none() {
                self.status_msg =
                    "This lossless RMK action can only be assigned to a keyboard key".into();
                return;
            }
            if let Some((combo_idx, field)) = self.combo_pick_target.take() {
                let combo_trigger_keycode = if matches!(&field, ComboPickField::Trigger(_)) {
                    self.layout
                        .as_ref()
                        .map(|layout| {
                            resolve_combo_trigger_keycode(
                                &binding,
                                self.selected_layer,
                                &layout.layers,
                            )
                            .vial_keycode()
                        })
                        .unwrap_or(kc_value)
                } else {
                    kc_value
                };
                self.push_combo_undo();
                if let Some(combo) = self.combo_entries.get_mut(combo_idx) {
                    match field {
                        ComboPickField::Trigger(key_idx) => {
                            combo.keys[key_idx] = combo_trigger_keycode
                        }
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
                if !self.assign_encoder_keycode(ctx, layer, encoder_visual_idx, kc_value) {
                    self.keycode_picker.result = Some(binding);
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                if let Some(layout) = &mut self.layout {
                    layout.set_encoder_keycode(layer, encoder_visual_idx, kc_value);
                }
                if is_alt_repeat_keycode(kc_value) {
                    self.open_alt_repeat_window_compact();
                }
            } else if let Some((layer, ki)) = self.selected_key {
                #[cfg(not(target_arch = "wasm32"))]
                if !self.assign_key_binding(ctx, layer, ki, binding) {
                    self.keycode_picker.result = Some(binding);
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                if let Some(layout) = &mut self.layout {
                    layout.set_key_binding(layer, ki, binding);
                }
                if is_alt_repeat_keycode(kc_value) {
                    self.open_alt_repeat_window_compact();
                }
            }
            self.selected_key = None;
            self.selected_encoder = None;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn assign_encoder_keycode(
        &mut self,
        ctx: &egui::Context,
        layer: usize,
        encoder_visual_idx: usize,
        kc_value: u16,
    ) -> bool {
        self.assign_encoder_keycode_with_mode(ctx, layer, encoder_visual_idx, kc_value, false)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn assign_encoder_keycode_with_mode(
        &mut self,
        ctx: &egui::Context,
        layer: usize,
        encoder_visual_idx: usize,
        kc_value: u16,
        is_undo: bool,
    ) -> bool {
        if self.qmk_settings_write_busy() {
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "settings_write.busy")
                    .to_owned();
            return false;
        }
        let encoder = match self
            .layout
            .as_ref()
            .and_then(|l| l.encoders.get(encoder_visual_idx))
        {
            Some(e) => e.clone(),
            None => return true,
        };
        let old_kc = self
            .layout
            .as_ref()
            .map(|l| l.get_encoder_keycode(layer, encoder_visual_idx))
            .unwrap_or(0);

        let operation = super::vial_hid_task::VialHidOperation::EncoderWrite {
            layer,
            encoder_visual_index: encoder_visual_idx,
            encoder_index: encoder.encoder_idx,
            direction: encoder.direction,
            old_keycode: old_kc,
            keycode: kc_value,
            is_undo,
        };
        match self.start_vial_hid_operation(ctx, operation) {
            super::vial_hid_task::VialHidTaskStart::Started => {
                if let Some(layout) = &mut self.layout {
                    layout.set_encoder_keycode(layer, encoder_visual_idx, kc_value);
                }
                self.status_msg = "Saving…".into();
                true
            }
            super::vial_hid_task::VialHidTaskStart::Busy => false,
            super::vial_hid_task::VialHidTaskStart::NoDevice => {
                if !is_undo {
                    self.undo_stack.push(UndoAction::Encoder {
                        layer,
                        encoder_visual_idx,
                        old_kc,
                    });
                }
                if let Some(layout) = &mut self.layout {
                    layout.set_encoder_keycode(layer, encoder_visual_idx, kc_value);
                }
                self.status_msg =
                    "Read-only: encoder changed locally, firmware write disabled for this device"
                        .into();
                true
            }
        }
    }

    pub(super) fn open_picker_for_target(
        &mut self,
        key_target: Option<usize>,
        encoder_target: Option<usize>,
    ) {
        let current_binding = self.layout.as_ref().and_then(|layout| {
            key_target.map(|key_index| layout.get_key_binding(self.selected_layer, key_index))
        });
        let current_encoder_keycode = self.layout.as_ref().and_then(|layout| {
            encoder_target
                .map(|encoder_index| layout.get_encoder_keycode(self.selected_layer, encoder_index))
        });
        self.selected_key = key_target.map(|ki| (self.selected_layer, ki));
        self.selected_encoder = encoder_target.map(|ei| (self.selected_layer, ei));
        self.keycode_picker.open = true;
        self.keycode_picker.result = None;
        self.keycode_picker
            .rmk_native_key_actions_allowed_for_target = key_target.is_some();
        self.keycode_picker.search_query.clear();
        self.keycode_picker.layer_names = self.layer_names.clone();
        self.keycode_picker.vial_quantum_pending_mod = None;
        self.keycode_picker.vial_quantum_pending_mt = None;
        self.keycode_picker.vial_layer_pending = None;
        self.keycode_picker.tap_dance_editor_open = None;
        self.keycode_picker.macro_inline_selected = None;
        self.keycode_picker.td_key_pick = None;
        self.keycode_picker.td_mod_key_pick = None;
        if let Some(current_binding) = current_binding {
            self.keycode_picker.select_tab_for_binding(current_binding);
        } else if let Some(current_keycode) = current_encoder_keycode {
            self.keycode_picker.select_tab_for_keycode(current_keycode);
        } else {
            self.keycode_picker.selected_tab = crate::keycode_picker::KeycodeTab::Basic;
        }
    }

    pub(super) fn handle_secondary_target(
        &mut self,
        ctx: &egui::Context,
        ctrl_held: bool,
        binding: crate::keyboard::KeyBinding,
        key_target: Option<usize>,
        encoder_target: Option<usize>,
    ) {
        let kc = binding.vial_keycode();
        if let crate::keyboard::KeyBinding::Rmk(action) = binding {
            if let Some(parts) = crate::rmk_native::rmk_mod_tap_parts(action) {
                if ctrl_held {
                    if let Some(ki) = key_target {
                        let swapped = crate::rmk_native::toggle_handed_key_action(action);
                        self.pending_handed_swap = Some((
                            self.selected_layer,
                            ki,
                            crate::keyboard::KeyBinding::Rmk(swapped),
                        ));
                    }
                } else {
                    self.open_picker_for_target(key_target, encoder_target);
                    self.keycode_picker.vial_quantum_pending_mt = Some(parts.vial_base());
                    self.keycode_picker.vial_quantum_pending_mod = None;
                }
                self.secondary_click_handled = true;
                return;
            }
        }
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
                    #[cfg(not(target_arch = "wasm32"))]
                    if self.hid_write_task_active() {
                        self.selected_key = None;
                        self.selected_encoder = Some((self.selected_layer, visual_idx));
                        self.keycode_picker.result = Some(swapped.into());
                    } else {
                        self.assign_encoder_keycode(ctx, self.selected_layer, visual_idx, swapped);
                    }
                    #[cfg(target_arch = "wasm32")]
                    if let Some(layout) = &mut self.layout {
                        layout.set_encoder_keycode(self.selected_layer, visual_idx, swapped);
                    }
                } else if let Some(ki) = key_target {
                    self.pending_handed_swap = Some((
                        self.selected_layer,
                        ki,
                        crate::keyboard::KeyBinding::Vial(swapped),
                    ));
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

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn assign_keycode(
        &mut self,
        ctx: &egui::Context,
        layer: usize,
        ki: usize,
        kc_value: u16,
    ) -> bool {
        self.assign_key_binding(ctx, layer, ki, kc_value.into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn assign_key_binding(
        &mut self,
        ctx: &egui::Context,
        layer: usize,
        ki: usize,
        binding: crate::keyboard::KeyBinding,
    ) -> bool {
        self.assign_key_binding_with_mode(ctx, layer, ki, binding, false)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn assign_key_binding_with_mode(
        &mut self,
        ctx: &egui::Context,
        layer: usize,
        ki: usize,
        binding: crate::keyboard::KeyBinding,
        is_undo: bool,
    ) -> bool {
        if self.qmk_settings_write_busy() {
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "settings_write.busy")
                    .to_owned();
            return false;
        }
        let old_binding = self
            .layout
            .as_ref()
            .map(|l| l.get_key_binding(layer, ki))
            .unwrap_or_default();

        let key = match self.layout.as_ref().and_then(|l| l.keys.get(ki)) {
            Some(k) => k.clone(),
            None => return true,
        };

        let commit_local_assignment = |this: &mut Self, record_undo: bool| {
            if record_undo {
                this.undo_stack.push(UndoAction::Key {
                    layer,
                    key_idx: ki,
                    old_binding,
                });
            }
            if let Some(layout) = &mut this.layout {
                layout.set_key_binding(layer, ki, binding);
            }
            this.refresh_layer_picker_content_flags();
        };

        let operation = super::vial_hid_task::VialHidOperation::KeyWrite {
            layer,
            key_index: ki,
            row: key.row,
            col: key.col,
            old_binding,
            binding,
            is_undo,
        };
        match self.start_vial_hid_operation(ctx, operation) {
            super::vial_hid_task::VialHidTaskStart::Started => {
                // Apply the edit immediately in memory. The worker owns the HID
                // round trip; failures reconcile this optimistic value.
                commit_local_assignment(self, false);
                self.status_msg = "Saving…".into();
                true
            }
            super::vial_hid_task::VialHidTaskStart::Busy => false,
            super::vial_hid_task::VialHidTaskStart::NoDevice => {
                commit_local_assignment(self, !is_undo);
                self.status_msg =
                    "Read-only: key changed locally, firmware write disabled for this device"
                        .into();
                true
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
    pub(super) fn undo(&mut self, ctx: &egui::Context) {
        if self.vial_hid_background_layer_active() {
            self.pending_layout_undo = true;
            self.deferred_device_load.defer_background_for_user_input();
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
            return;
        }
        self.perform_layout_undo(ctx);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_pending_layout_undo(&mut self, ctx: &egui::Context) {
        if !self.pending_layout_undo
            || self.vial_hid_background_layer_active()
            || self.hid_user_action_busy()
        {
            return;
        }
        self.pending_layout_undo = false;
        self.perform_layout_undo(ctx);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn perform_layout_undo(&mut self, ctx: &egui::Context) {
        let Some(action) = self.undo_stack.pop() else {
            return;
        };
        match action {
            UndoAction::Key {
                layer,
                key_idx,
                old_binding,
            } => {
                if !self.assign_key_binding_with_mode(ctx, layer, key_idx, old_binding, true) {
                    self.undo_stack.push(UndoAction::Key {
                        layer,
                        key_idx,
                        old_binding,
                    });
                }
            }
            UndoAction::Encoder {
                layer,
                encoder_visual_idx,
                old_kc,
            } => {
                if !self.assign_encoder_keycode_with_mode(
                    ctx,
                    layer,
                    encoder_visual_idx,
                    old_kc,
                    true,
                ) {
                    self.undo_stack.push(UndoAction::Encoder {
                        layer,
                        encoder_visual_idx,
                        old_kc,
                    });
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;
    use rmk_types::action::{Action, KeyAction};
    use rmk_types::keycode::HidKeyCode;
    use rmk_types::modifier::ModifierCombination;

    #[test]
    fn right_click_reopens_native_mod_tap_tap_key_picker() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::LeftBracket, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::RCTRL),
            Default::default(),
        );

        app.handle_secondary_target(
            &ctx,
            false,
            crate::keyboard::KeyBinding::Rmk(action),
            Some(0),
            None,
        );

        assert!(app.secondary_click_handled);
        assert!(app.keycode_picker.open);
        assert_eq!(
            app.keycode_picker.vial_quantum_pending_mt,
            Some(0x2000 | ((ModifierCombination::RCTRL.into_packed_bits() as u16) << 8))
        );
        assert!(app.keycode_picker.rmk_native_key_actions_allowed_for_target);
    }

    #[test]
    fn combo_trigger_uses_unique_mod_tap_keycode_from_selected_layer() {
        let layers = vec![vec![KeyBinding::Vial(0x2416), KeyBinding::Vial(0x000A)]];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x0016), 0, &layers),
            KeyBinding::Vial(0x2416)
        );
    }

    #[test]
    fn combo_trigger_keeps_plain_keycode_when_it_exists_on_selected_layer() {
        let layers = vec![vec![
            KeyBinding::Vial(0x0016),
            KeyBinding::Vial(0x2416),
            KeyBinding::Vial(0x000A),
        ]];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x0016), 0, &layers),
            KeyBinding::Vial(0x0016)
        );
    }

    #[test]
    fn combo_trigger_prefers_selected_layer_tap_hold_over_plain_key_on_another_layer() {
        let layers = vec![
            vec![KeyBinding::Vial(0x2416), KeyBinding::Vial(0x000A)],
            vec![KeyBinding::Vial(0x0016)],
        ];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x0016), 0, &layers),
            KeyBinding::Vial(0x2416)
        );
    }

    #[test]
    fn combo_trigger_does_not_guess_between_distinct_tap_hold_assignments() {
        let layers = vec![vec![
            KeyBinding::Vial(0x2416),
            KeyBinding::Vial(0x2116),
            KeyBinding::Vial(0x000A),
        ]];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x0016), 0, &layers),
            KeyBinding::Vial(0x0016)
        );
    }

    #[test]
    fn combo_trigger_uses_unique_tap_hold_keycode_from_another_layer_as_fallback() {
        let layers = vec![
            vec![KeyBinding::Vial(0x000A)],
            vec![KeyBinding::Vial(0x4116)],
        ];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x0016), 0, &layers),
            KeyBinding::Vial(0x4116)
        );
    }

    #[test]
    fn combo_trigger_keeps_an_explicit_advanced_keycode() {
        let layers = vec![vec![KeyBinding::Vial(0x2416), KeyBinding::Vial(0x000A)]];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x2416), 0, &layers),
            KeyBinding::Vial(0x2416)
        );
    }

    #[test]
    fn combo_trigger_keeps_clear_keycode() {
        let layers = vec![vec![KeyBinding::Vial(0x2400), KeyBinding::Vial(0x000A)]];

        assert_eq!(
            resolve_combo_trigger_keycode(&KeyBinding::Vial(0x0000), 0, &layers),
            KeyBinding::Vial(0x0000)
        );
    }
}
