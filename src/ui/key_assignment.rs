use super::*;

impl EntropyApp {
    pub(super) fn apply_picker_results(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.hid_user_action_busy() {
            return;
        }

        if let Some(binding) = self.keycode_picker.result.take() {
            let kc_value = binding.vial_keycode();
            let native_target_supported = match self.combo_pick_target {
                Some((_, ComboPickField::Output)) => {
                    self.keycode_picker.supports_rmk_native_combo_output
                }
                Some((_, ComboPickField::Trigger(_))) => false,
                None => self.selected_key.is_some(),
            };
            if binding.rmk_action().is_some() && !native_target_supported {
                self.status_msg =
                    "This lossless RMK action is not supported for this target".into();
                return;
            }
            if let Some((combo_idx, field)) = self.combo_pick_target.take() {
                self.push_combo_undo();
                if let Some(combo) = self.combo_entries.get_mut(combo_idx) {
                    match field {
                        ComboPickField::Trigger(key_idx) => combo.keys[key_idx] = kc_value,
                        ComboPickField::Output => {
                            combo.output = match binding {
                                crate::keyboard::KeyBinding::Vial(value) => {
                                    crate::keyboard::KeyBinding::Vial(
                                        crate::keycode::normalize_output_symbol_keycode(value),
                                    )
                                }
                                crate::keyboard::KeyBinding::Rmk(_) => binding,
                            };
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
                    Self::initialize_key_override_entry(entry);
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
                        self.drop_queued_middle_click_assignment(self.selected_layer, ki);
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
                    self.drop_queued_middle_click_assignment(self.selected_layer, ki);
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
            self.keycode_picker.macro_inline_selected = Some(macro_n);
            self.open_macro_settings_page();
            self.secondary_click_handled = true;
            return;
        }
        if (0x5700..=0x57FF).contains(&kc) {
            let td_n = (kc - 0x5700) as u8;
            self.keycode_picker.tap_dance_editor_open = Some(td_n);
            self.open_tap_dance_settings_page();
            self.secondary_click_handled = true;
            return;
        }
        if is_mouse_keycode(kc) && self.mouse_keys_settings.supported {
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

    fn middle_click_binding(&self) -> crate::keyboard::KeyBinding {
        crate::keyboard::KeyBinding::Vial(if self.app_settings.middle_click_assigns_transparent {
            0x0001
        } else {
            0x0000
        })
    }

    /// Assign the configured middle-click value on the selected layer without
    /// opening the picker. If a HID write is already in flight, the assignment
    /// is queued so rapid clicks are not lost.
    pub(super) fn request_middle_click_key_assignment(&mut self, ctx: &egui::Context, ki: usize) {
        let layer = self.selected_layer;
        let binding = self.middle_click_binding();
        // The assignment supersedes older deferred edits of this key: a picker
        // result not yet applied and a handed swap waiting for Ctrl release.
        if self.keycode_picker.result.is_some()
            && self.combo_pick_target.is_none()
            && self.key_override_pick_target.is_none()
            && self.alt_repeat_pick_target.is_none()
            && self.selected_encoder.is_none()
            && self.selected_key == Some((layer, ki))
        {
            self.keycode_picker.result = None;
            self.selected_key = None;
        }
        if self
            .pending_handed_swap
            .is_some_and(|(l, k, _)| l == layer && k == ki)
        {
            self.pending_handed_swap = None;
        }
        let already_assigned = self
            .layout
            .as_ref()
            .map(|l| l.get_key_binding(layer, ki) == binding)
            .unwrap_or(true);
        if already_assigned {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.assign_key_binding(ctx, layer, ki, binding) {
            self.queue_pending_middle_click_assignment(PendingMiddleClickAssignment::Key {
                layer,
                key_idx: ki,
                binding,
                generation: self.connection_generation,
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            if let Some(layout) = &mut self.layout {
                layout.set_key_binding(layer, ki, binding);
            }
        }
    }

    /// Assign the configured middle-click value to an encoder slot.
    pub(super) fn request_middle_click_encoder_assignment(
        &mut self,
        ctx: &egui::Context,
        encoder_visual_idx: usize,
    ) {
        let layer = self.selected_layer;
        let keycode = self.middle_click_binding().vial_keycode();
        // The assignment supersedes a picker result not yet applied to this slot.
        if self.keycode_picker.result.is_some()
            && self.combo_pick_target.is_none()
            && self.key_override_pick_target.is_none()
            && self.alt_repeat_pick_target.is_none()
            && self.selected_encoder == Some((layer, encoder_visual_idx))
        {
            self.keycode_picker.result = None;
            self.selected_encoder = None;
        }
        let already_assigned = self
            .layout
            .as_ref()
            .map(|l| l.get_encoder_keycode(layer, encoder_visual_idx) == keycode)
            .unwrap_or(true);
        if already_assigned {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.assign_encoder_keycode(ctx, layer, encoder_visual_idx, keycode) {
            self.queue_pending_middle_click_assignment(PendingMiddleClickAssignment::Encoder {
                layer,
                encoder_visual_idx,
                keycode,
                generation: self.connection_generation,
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            if let Some(layout) = &mut self.layout {
                layout.set_encoder_keycode(layer, encoder_visual_idx, keycode);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn queue_pending_middle_click_assignment(&mut self, assignment: PendingMiddleClickAssignment) {
        self.pending_middle_click_assignments
            .retain(|pending| !same_middle_click_target(*pending, assignment));
        self.pending_middle_click_assignments.push(assignment);
    }

    /// A newer deferred edit of this key supersedes an older queued assignment.
    fn drop_queued_middle_click_assignment(&mut self, layer: usize, key_idx: usize) {
        self.pending_middle_click_assignments.retain(|assignment| {
            !matches!(
                assignment,
                PendingMiddleClickAssignment::Key { layer: l, key_idx: k, .. }
                    if *l == layer && *k == key_idx
            )
        });
    }

    /// A whole-layer write (paste, fill, undo) supersedes queued middle-click
    /// assignments of keys and encoders on that layer.
    pub(super) fn drop_queued_middle_click_assignments_for_layer(&mut self, layer: usize) {
        self.pending_middle_click_assignments
            .retain(|assignment| match assignment {
                PendingMiddleClickAssignment::Key { layer: l, .. }
                | PendingMiddleClickAssignment::Encoder { layer: l, .. } => *l != layer,
            });
    }

    /// Apply queued middle-click assignments once the HID handle is free again.
    /// Assignments from a previous connection are dropped; targets that already
    /// have the intended value are skipped without a write.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn flush_pending_middle_click_assignments(&mut self, ctx: &egui::Context) {
        if self.pending_middle_click_assignments.is_empty() {
            return;
        }
        let generation = self.connection_generation;
        self.pending_middle_click_assignments
            .retain(|assignment| match assignment {
                PendingMiddleClickAssignment::Key { generation: g, .. }
                | PendingMiddleClickAssignment::Encoder { generation: g, .. } => *g == generation,
            });
        while !self.pending_middle_click_assignments.is_empty() {
            if self.hid_write_task_active() {
                return;
            }
            let applied = match self.pending_middle_click_assignments[0] {
                PendingMiddleClickAssignment::Key {
                    layer,
                    key_idx,
                    binding,
                    ..
                } => {
                    let already_assigned = self
                        .layout
                        .as_ref()
                        .map(|l| l.get_key_binding(layer, key_idx) == binding)
                        .unwrap_or(true);
                    already_assigned || self.assign_key_binding(ctx, layer, key_idx, binding)
                }
                PendingMiddleClickAssignment::Encoder {
                    layer,
                    encoder_visual_idx,
                    keycode,
                    ..
                } => {
                    let already_assigned = self
                        .layout
                        .as_ref()
                        .map(|l| l.get_encoder_keycode(layer, encoder_visual_idx) == keycode)
                        .unwrap_or(true);
                    already_assigned
                        || self.assign_encoder_keycode(ctx, layer, encoder_visual_idx, keycode)
                }
            };
            if !applied {
                return;
            }
            self.pending_middle_click_assignments.remove(0);
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

fn same_middle_click_target(
    left: PendingMiddleClickAssignment,
    right: PendingMiddleClickAssignment,
) -> bool {
    match (left, right) {
        (
            PendingMiddleClickAssignment::Key {
                layer: left_layer,
                key_idx: left_key,
                ..
            },
            PendingMiddleClickAssignment::Key {
                layer: right_layer,
                key_idx: right_key,
                ..
            },
        ) => left_layer == right_layer && left_key == right_key,
        (
            PendingMiddleClickAssignment::Encoder {
                layer: left_layer,
                encoder_visual_idx: left_encoder,
                ..
            },
            PendingMiddleClickAssignment::Encoder {
                layer: right_layer,
                encoder_visual_idx: right_encoder,
                ..
            },
        ) => left_layer == right_layer && left_encoder == right_encoder,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn universal_symbol_can_be_assigned_to_combo_output() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let binding =
            crate::universal_symbols::binding(crate::universal_symbols::USER_SYMBOL_START);
        app.combo_entries = vec![ComboEntry::default()];
        app.combo_pick_target = Some((0, ComboPickField::Output));
        app.keycode_picker.supports_rmk_native_combo_output = true;
        app.keycode_picker.result = Some(binding);

        app.apply_picker_results(&ctx);

        assert_eq!(app.combo_entries[0].output, binding);
        assert!(app.combo_dirty);
    }

    #[test]
    fn native_action_is_rejected_for_combo_trigger_even_with_a_selected_key() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let binding =
            crate::universal_symbols::binding(crate::universal_symbols::USER_SYMBOL_START);
        app.combo_entries = vec![ComboEntry::default()];
        app.combo_pick_target = Some((0, ComboPickField::Trigger(0)));
        app.selected_key = Some((0, 0));
        app.keycode_picker.supports_rmk_native_combo_output = true;
        app.keycode_picker.result = Some(binding);

        app.apply_picker_results(&ctx);

        assert_eq!(app.combo_entries[0].keys[0], 0);
        assert!(!app.combo_dirty);
        assert!(app.status_msg.contains("not supported"));
    }

    #[test]
    fn mouse_key_secondary_action_requires_exposed_mouse_settings() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);

        app.handle_secondary_target(
            &ctx,
            false,
            crate::keyboard::KeyBinding::Vial(0x00CD),
            Some(0),
            None,
        );
        assert!(!app.secondary_click_handled);
        assert!(!matches!(app.settings_tab, SettingsTab::MouseKeys));

        app.mouse_keys_settings.supported = true;
        app.handle_secondary_target(
            &ctx,
            false,
            crate::keyboard::KeyBinding::Vial(0x00CD),
            Some(0),
            None,
        );
        assert!(app.secondary_click_handled);
        assert!(matches!(app.settings_tab, SettingsTab::MouseKeys));
    }

    fn test_layout(keycodes: &[u16], encoder_keycodes: &[u16]) -> KeyboardLayout {
        KeyboardLayout {
            name: "Test".into(),
            rows: 8,
            cols: 8,
            keys: keycodes
                .iter()
                .enumerate()
                .map(|(idx, _)| PhysicalKey {
                    x: idx as f32,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    row: (idx / 8) as u8,
                    col: (idx % 8) as u8,
                    label: idx.to_string(),
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            encoders: encoder_keycodes
                .iter()
                .enumerate()
                .map(|(idx, _)| PhysicalEncoder {
                    x: idx as f32,
                    y: 2.0,
                    w: 1.0,
                    h: 1.0,
                    label: idx.to_string(),
                    encoder_idx: idx as u8,
                    direction: 0,
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            layers: vec![keycodes.iter().copied().map(Into::into).collect()],
            encoder_layers: vec![encoder_keycodes.to_vec()],
            layer_names: vec![],
            custom_keycodes: vec![],
            layout_options: vec![],
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
            live_features: Default::default(),
        }
    }

    #[test]
    fn middle_click_clear_writes_kc_no_and_records_undo() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x0004, 0x0005], &[]));

        app.request_middle_click_key_assignment(&ctx, 0);

        assert_eq!(
            app.layout.as_ref().unwrap().get_key_binding(0, 0),
            crate::keyboard::KeyBinding::Vial(0)
        );
        assert!(matches!(
            app.undo_stack.last(),
            Some(UndoAction::Key {
                layer: 0,
                key_idx: 0,
                old_binding: crate::keyboard::KeyBinding::Vial(0x0004),
            })
        ));
        assert!(app.pending_middle_click_assignments.is_empty());
    }

    #[test]
    fn middle_click_clear_skips_already_empty_key() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x0000], &[]));

        app.request_middle_click_key_assignment(&ctx, 0);

        assert!(app.undo_stack.is_empty());
        assert!(app.pending_middle_click_assignments.is_empty());
    }

    #[test]
    fn middle_click_clears_encoder_slot_and_records_undo() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[], &[0x00E9]));

        app.request_middle_click_encoder_assignment(&ctx, 0);

        assert_eq!(app.layout.as_ref().unwrap().get_encoder_keycode(0, 0), 0);
        assert!(matches!(
            app.undo_stack.last(),
            Some(UndoAction::Encoder {
                layer: 0,
                encoder_visual_idx: 0,
                old_kc: 0x00E9,
            })
        ));
        assert!(app.pending_middle_click_assignments.is_empty());
    }

    #[test]
    fn middle_click_transparent_setting_assigns_inherit_to_keys_and_encoders() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.app_settings.middle_click_assigns_transparent = true;
        app.layout = Some(test_layout(&[0x0004], &[0x00E9]));

        app.request_middle_click_key_assignment(&ctx, 0);
        app.request_middle_click_encoder_assignment(&ctx, 0);

        let layout = app.layout.as_ref().unwrap();
        assert_eq!(
            layout.get_key_binding(0, 0),
            crate::keyboard::KeyBinding::Vial(0x0001)
        );
        assert_eq!(layout.get_encoder_keycode(0, 0), 0x0001);
        assert_eq!(app.undo_stack.len(), 2);
    }

    #[test]
    fn middle_click_clear_supersedes_parked_picker_result() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x0004], &[]));
        app.selected_key = Some((0, 0));
        app.keycode_picker.result = Some(crate::keyboard::KeyBinding::Vial(0x0006));

        app.request_middle_click_key_assignment(&ctx, 0);
        app.apply_picker_results(&ctx);

        assert!(app.keycode_picker.result.is_none());
        assert_eq!(
            app.layout.as_ref().unwrap().get_key_binding(0, 0),
            crate::keyboard::KeyBinding::Vial(0)
        );
    }

    #[test]
    fn middle_click_clear_cancels_pending_handed_swap() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x00E1], &[]));
        app.pending_handed_swap = Some((0, 0, crate::keyboard::KeyBinding::Vial(0x00E5)));

        app.request_middle_click_key_assignment(&ctx, 0);

        assert!(app.pending_handed_swap.is_none());
        assert_eq!(
            app.layout.as_ref().unwrap().get_key_binding(0, 0),
            crate::keyboard::KeyBinding::Vial(0)
        );
    }

    #[test]
    fn handed_swap_drops_queued_middle_click_assignment_for_same_key() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x00E1], &[]));
        app.pending_middle_click_assignments
            .push(PendingMiddleClickAssignment::Key {
                layer: 0,
                key_idx: 0,
                binding: crate::keyboard::KeyBinding::Vial(0),
                generation: app.connection_generation,
            });

        app.handle_secondary_target(
            &ctx,
            true,
            crate::keyboard::KeyBinding::Vial(0x00E1),
            Some(0),
            None,
        );

        assert!(app.pending_middle_click_assignments.is_empty());
        assert!(app.pending_handed_swap.is_some());
    }

    #[test]
    fn layer_snapshot_write_drops_queued_middle_click_assignments_for_that_layer() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x0004, 0x0005], &[]));
        let generation = app.connection_generation;
        app.pending_middle_click_assignments = vec![
            PendingMiddleClickAssignment::Key {
                layer: 0,
                key_idx: 0,
                binding: crate::keyboard::KeyBinding::Vial(0),
                generation,
            },
            PendingMiddleClickAssignment::Encoder {
                layer: 0,
                encoder_visual_idx: 0,
                keycode: 0,
                generation,
            },
            PendingMiddleClickAssignment::Key {
                layer: 1,
                key_idx: 0,
                binding: crate::keyboard::KeyBinding::Vial(0),
                generation,
            },
        ];

        app.apply_layer_snapshot(
            0,
            LayerSnapshot {
                keycodes: vec![
                    crate::keyboard::KeyBinding::Vial(0x0007),
                    crate::keyboard::KeyBinding::Vial(0x0008),
                ],
                encoder_keycodes: vec![],
            },
            "layer_actions.paste",
        );

        // Only the pasted layer's assignments are superseded.
        assert_eq!(
            app.pending_middle_click_assignments,
            vec![PendingMiddleClickAssignment::Key {
                layer: 1,
                key_idx: 0,
                binding: crate::keyboard::KeyBinding::Vial(0),
                generation,
            }]
        );
    }

    #[test]
    fn busy_middle_click_assignments_are_queued_deduped_and_flushed() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.layout = Some(test_layout(&[0x0004, 0x0005], &[]));
        let (hid_device, _recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);

        // First assignment takes the HID handle for its write task.
        app.request_middle_click_key_assignment(&ctx, 0);
        assert!(app.hid_write_task_active());

        // Clicks while the write is in flight queue once per target.
        app.request_middle_click_key_assignment(&ctx, 1);
        app.request_middle_click_key_assignment(&ctx, 1);
        assert_eq!(app.pending_middle_click_assignments.len(), 1);

        for _ in 0..400 {
            app.poll_vial_hid_task(&ctx);
            app.flush_pending_middle_click_assignments(&ctx);
            if !app.hid_write_task_active() && app.pending_middle_click_assignments.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(app.pending_middle_click_assignments.is_empty());
        assert!(!app.hid_write_task_active());
        let layout = app.layout.as_ref().unwrap();
        assert_eq!(
            layout.get_key_binding(0, 0),
            crate::keyboard::KeyBinding::Vial(0)
        );
        assert_eq!(
            layout.get_key_binding(0, 1),
            crate::keyboard::KeyBinding::Vial(0)
        );
        assert_eq!(app.undo_stack.len(), 2);
    }
}
