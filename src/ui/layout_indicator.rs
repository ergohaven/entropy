use super::*;

fn sticky_momentary_layer_target(kc: u16) -> Option<usize> {
    if let Some((op, target)) = vial_layer_op_target(kc) {
        return matches!(op, 1 | 4 | 6).then_some(target); // MO / OSL / TT while held
    }
    if kc & 0xF000 == 0x4000 {
        return Some(((kc >> 8) & 0xF) as usize); // LT
    }
    if (0x5000..0x5200).contains(&kc) {
        return Some(((kc >> 4) & 0xF) as usize); // LM
    }
    None
}

fn sticky_toggle_layer_target(kc: u16) -> Option<usize> {
    // TG
    vial_layer_op_target(kc).and_then(|(op, target)| (op == 3).then_some(target))
}

fn sticky_base_layer_target(kc: u16) -> Option<usize> {
    // TO / DF / PDF
    vial_layer_op_target(kc).and_then(|(op, target)| matches!(op, 0 | 2 | 7).then_some(target))
}

fn layout_effective_keycode(layout: &KeyboardLayout, layer: usize, key_idx: usize) -> u16 {
    let kc = layout.get_keycode(layer, key_idx);
    if kc != 0x0001 {
        return kc;
    }

    (0..layer)
        .rev()
        .map(|fallback_layer| layout.get_keycode(fallback_layer, key_idx))
        .find(|fallback| *fallback != 0x0001)
        .unwrap_or(0x0000)
}

fn sticky_apply_persistent_layer_action(
    keycode: u16,
    layer_count: usize,
    toggled_layers: &mut [bool],
    base_layer: &mut usize,
) {
    if let Some(target) = sticky_toggle_layer_target(keycode).filter(|target| *target < layer_count)
    {
        if let Some(enabled) = toggled_layers.get_mut(target) {
            *enabled = !*enabled;
        }
    } else if let Some(target) =
        sticky_base_layer_target(keycode).filter(|target| *target < layer_count)
    {
        *base_layer = target;
        toggled_layers.fill(false);
    }
}

fn sticky_combo_is_active(combo: &ComboEntry, pressed_keycodes: &[u16]) -> bool {
    let mut used = vec![false; pressed_keycodes.len()];
    let mut trigger_count = 0;

    for trigger in combo.keys.iter().copied().filter(|keycode| *keycode != 0) {
        trigger_count += 1;
        let Some((idx, _)) = pressed_keycodes
            .iter()
            .enumerate()
            .find(|(idx, keycode)| !used[*idx] && **keycode == trigger)
        else {
            return false;
        };
        used[idx] = true;
    }

    trigger_count > 0 && !combo.output.is_no()
}

fn sticky_combo_is_active_on_layer(
    combo: &ComboEntry,
    pressed_keycodes: &[u16],
    active_layer: usize,
    was_active: bool,
) -> bool {
    sticky_combo_is_active(combo, pressed_keycodes)
        && (was_active
            || combo
                .layer
                .is_none_or(|combo_layer| combo_layer as usize == active_layer))
}

fn sticky_pressed_keycodes(
    layout: &KeyboardLayout,
    matrix_pressed: &[bool],
    pressed_key_layers: &[Option<usize>],
    fallback_layer: usize,
) -> Vec<u16> {
    let layer_count = layout.layers.len().max(1);
    layout
        .keys
        .iter()
        .enumerate()
        .filter_map(|(key_idx, key)| {
            if !layout_matrix_key_pressed(layout, matrix_pressed, key.row, key.col) {
                return None;
            }

            let matrix_idx = key.row as usize * layout.cols + key.col as usize;
            let source_layer = pressed_key_layers
                .get(matrix_idx)
                .and_then(|layer| *layer)
                .filter(|layer| *layer < layer_count)
                .unwrap_or(fallback_layer);
            Some(layout_effective_keycode(layout, source_layer, key_idx))
        })
        .collect()
}

fn sticky_tap_dance_index(keycode: u16) -> Option<usize> {
    (0x5700..=0x57FF)
        .contains(&keycode)
        .then_some((keycode - 0x5700) as usize)
}

fn sticky_tap_dance_term(
    entries: &[crate::keycode_picker::TapDanceEntry],
    entry: usize,
) -> Option<std::time::Duration> {
    entries
        .get(entry)
        .map(|entry| std::time::Duration::from_millis(entry.tapping_term as u64))
}

fn sticky_tap_dance_tap_action(
    entries: &[crate::keycode_picker::TapDanceEntry],
    entry: usize,
) -> Option<u16> {
    entries.get(entry).map(|entry| entry.on_tap.vial_keycode())
}

fn sticky_tap_dance_hold_action(
    entries: &[crate::keycode_picker::TapDanceEntry],
    entry: usize,
    tap_count: u8,
) -> Option<u16> {
    entries.get(entry).map(|entry| {
        if tap_count >= 2 {
            entry.on_tap_hold.vial_keycode()
        } else {
            entry.on_hold.vial_keycode()
        }
    })
}

fn sticky_update_tap_dance_state(
    state: &mut StickyLayoutTapDanceState,
    is_pressed: bool,
    pressed_entry: Option<usize>,
    entries: &[crate::keycode_picker::TapDanceEntry],
    now: std::time::Instant,
) -> Vec<u16> {
    let previous = std::mem::take(state);
    let mut actions = Vec::new();

    *state = match previous {
        StickyLayoutTapDanceState::Idle => {
            if is_pressed {
                if let Some(entry) = pressed_entry.filter(|entry| *entry < entries.len()) {
                    StickyLayoutTapDanceState::Pressed {
                        entry,
                        pressed_at: now,
                        tap_count: 1,
                        hold_active: false,
                    }
                } else {
                    StickyLayoutTapDanceState::Idle
                }
            } else {
                StickyLayoutTapDanceState::Idle
            }
        }
        StickyLayoutTapDanceState::WaitingForSecondTap { entry, released_at } => {
            let term = sticky_tap_dance_term(entries, entry).unwrap_or_default();
            if is_pressed {
                if pressed_entry == Some(entry) && now.duration_since(released_at) < term {
                    StickyLayoutTapDanceState::Pressed {
                        entry,
                        pressed_at: now,
                        tap_count: 2,
                        hold_active: false,
                    }
                } else {
                    actions.extend(sticky_tap_dance_tap_action(entries, entry));
                    if let Some(next_entry) = pressed_entry.filter(|entry| *entry < entries.len()) {
                        StickyLayoutTapDanceState::Pressed {
                            entry: next_entry,
                            pressed_at: now,
                            tap_count: 1,
                            hold_active: false,
                        }
                    } else {
                        StickyLayoutTapDanceState::Idle
                    }
                }
            } else if now.duration_since(released_at) >= term {
                actions.extend(sticky_tap_dance_tap_action(entries, entry));
                StickyLayoutTapDanceState::Idle
            } else {
                StickyLayoutTapDanceState::WaitingForSecondTap { entry, released_at }
            }
        }
        StickyLayoutTapDanceState::Pressed {
            entry,
            pressed_at,
            tap_count,
            hold_active,
        } => {
            let term = sticky_tap_dance_term(entries, entry).unwrap_or_default();
            let held_long_enough = now.duration_since(pressed_at) >= term;
            if !is_pressed {
                if hold_active || held_long_enough {
                    if !hold_active {
                        actions.extend(sticky_tap_dance_hold_action(entries, entry, tap_count));
                    }
                    StickyLayoutTapDanceState::Idle
                } else if tap_count >= 2 {
                    if let Some(entry) = entries.get(entry) {
                        actions.push(entry.on_double_tap.vial_keycode());
                    }
                    StickyLayoutTapDanceState::Idle
                } else {
                    StickyLayoutTapDanceState::WaitingForSecondTap {
                        entry,
                        released_at: now,
                    }
                }
            } else if !hold_active && held_long_enough {
                actions.extend(sticky_tap_dance_hold_action(entries, entry, tap_count));
                StickyLayoutTapDanceState::Pressed {
                    entry,
                    pressed_at,
                    tap_count,
                    hold_active: true,
                }
            } else {
                StickyLayoutTapDanceState::Pressed {
                    entry,
                    pressed_at,
                    tap_count,
                    hold_active,
                }
            }
        }
    };

    actions
        .into_iter()
        .filter(|keycode| *keycode != 0)
        .collect()
}

fn sticky_tap_dance_active_keycode(
    state: &StickyLayoutTapDanceState,
    entries: &[crate::keycode_picker::TapDanceEntry],
) -> Option<u16> {
    let StickyLayoutTapDanceState::Pressed {
        entry,
        tap_count,
        hold_active: true,
        ..
    } = state
    else {
        return None;
    };
    sticky_tap_dance_hold_action(entries, *entry, *tap_count).filter(|keycode| *keycode != 0)
}

fn sticky_virtual_layer_keycodes(
    active_combos: &[bool],
    combos: &[ComboEntry],
    tap_dance_states: &[StickyLayoutTapDanceState],
    tap_dance_entries: &[crate::keycode_picker::TapDanceEntry],
) -> Vec<u16> {
    active_combos
        .iter()
        .zip(combos)
        .filter_map(|(active, combo)| {
            let output = combo.output.vial_keycode();
            (*active && output != 0).then_some(output)
        })
        .chain(
            tap_dance_states
                .iter()
                .filter_map(|state| sticky_tap_dance_active_keycode(state, tap_dance_entries)),
        )
        .collect()
}

fn sticky_layout_active_layer(
    layout: &KeyboardLayout,
    matrix_pressed: &[bool],
    pressed_key_layers: &[Option<usize>],
    toggled_layers: &[bool],
    base_layer: usize,
    virtual_layer_keycodes: &[u16],
) -> usize {
    let layer_count = layout.layers.len().max(1);
    let mut active_layer = toggled_layers
        .iter()
        .enumerate()
        .rev()
        .find_map(|(layer, enabled)| (*enabled && layer < layer_count).then_some(layer))
        .unwrap_or_else(|| base_layer.min(layer_count - 1));

    for _ in 0..layer_count {
        let virtual_target = virtual_layer_keycodes
            .iter()
            .filter_map(|keycode| sticky_momentary_layer_target(*keycode))
            .filter(|target| *target < layer_count)
            .max();
        let physical_target = layout
            .keys
            .iter()
            .enumerate()
            .filter_map(|(key_idx, key)| {
                if !layout_matrix_key_pressed(layout, matrix_pressed, key.row, key.col) {
                    return None;
                }

                let matrix_idx = key.row as usize * layout.cols + key.col as usize;
                let source_layer = pressed_key_layers
                    .get(matrix_idx)
                    .and_then(|layer| *layer)
                    .filter(|layer| *layer < layer_count)
                    .unwrap_or(active_layer);

                sticky_momentary_layer_target(layout_effective_keycode(
                    layout,
                    source_layer,
                    key_idx,
                ))
                .filter(|target| *target < layer_count)
            })
            .max();
        let next_layer = virtual_target.into_iter().chain(physical_target).max();

        match next_layer {
            Some(next_layer) if next_layer != active_layer => active_layer = next_layer,
            _ => break,
        }
    }

    active_layer
}

impl EntropyApp {
    pub(super) fn sync_sticky_layout_layer_state(&mut self, layout: &KeyboardLayout) -> usize {
        let layer_count = layout.layers.len().max(1);
        let pressed = self.matrix_tester_pressed.clone();
        let now = std::time::Instant::now();
        let combo_entries = &self.combo_entries;
        let tap_dance_entries = &self.keycode_picker.tap_dance_entries;

        if self.sticky_layout_prev_pressed.len() != pressed.len() {
            self.sticky_layout_prev_pressed = vec![false; pressed.len()];
        }
        if self.sticky_layout_pressed_key_layers.len() != pressed.len() {
            self.sticky_layout_pressed_key_layers = vec![None; pressed.len()];
        }
        if self.sticky_layout_toggled_layers.len() != layer_count {
            self.sticky_layout_toggled_layers = vec![false; layer_count];
        }
        if self.sticky_layout_active_combos.len() != combo_entries.len() {
            self.sticky_layout_active_combos = vec![false; combo_entries.len()];
        }
        if self.sticky_layout_tap_dance_states.len() != pressed.len() {
            self.sticky_layout_tap_dance_states =
                vec![StickyLayoutTapDanceState::Idle; pressed.len()];
        }
        self.sticky_layout_base_layer = self.sticky_layout_base_layer.min(layer_count - 1);

        let previous_virtual_layer_keycodes = sticky_virtual_layer_keycodes(
            &self.sticky_layout_active_combos,
            combo_entries,
            &self.sticky_layout_tap_dance_states,
            tap_dance_entries,
        );
        let layer_before = sticky_layout_active_layer(
            layout,
            &self.sticky_layout_prev_pressed,
            &self.sticky_layout_pressed_key_layers,
            &self.sticky_layout_toggled_layers,
            self.sticky_layout_base_layer,
            &previous_virtual_layer_keycodes,
        );
        let mut generated_layer_actions = Vec::new();

        for (key_idx, key) in layout.keys.iter().enumerate() {
            let matrix_idx = key.row as usize * layout.cols + key.col as usize;
            let is_pressed = pressed.get(matrix_idx).copied().unwrap_or(false);
            let was_pressed = self
                .sticky_layout_prev_pressed
                .get(matrix_idx)
                .copied()
                .unwrap_or(false);
            if is_pressed && !was_pressed {
                if let Some(source_layer) =
                    self.sticky_layout_pressed_key_layers.get_mut(matrix_idx)
                {
                    *source_layer = Some(layer_before);
                }
            }

            let pressed_keycode = is_pressed.then(|| {
                let source_layer = self
                    .sticky_layout_pressed_key_layers
                    .get(matrix_idx)
                    .and_then(|layer| *layer)
                    .unwrap_or(layer_before);
                layout_effective_keycode(layout, source_layer, key_idx)
            });
            let pressed_tap_dance_entry = pressed_keycode.and_then(sticky_tap_dance_index);
            if let Some(state) = self.sticky_layout_tap_dance_states.get_mut(matrix_idx) {
                generated_layer_actions.extend(sticky_update_tap_dance_state(
                    state,
                    is_pressed,
                    pressed_tap_dance_entry,
                    tap_dance_entries,
                    now,
                ));
            }

            if is_pressed && !was_pressed {
                if let Some(keycode) = pressed_keycode {
                    sticky_apply_persistent_layer_action(
                        keycode,
                        layer_count,
                        &mut self.sticky_layout_toggled_layers,
                        &mut self.sticky_layout_base_layer,
                    );
                }
            }

            if !is_pressed {
                if let Some(source_layer) =
                    self.sticky_layout_pressed_key_layers.get_mut(matrix_idx)
                {
                    *source_layer = None;
                }
            }
        }

        for keycode in generated_layer_actions {
            sticky_apply_persistent_layer_action(
                keycode,
                layer_count,
                &mut self.sticky_layout_toggled_layers,
                &mut self.sticky_layout_base_layer,
            );
        }

        let pressed_keycodes = sticky_pressed_keycodes(
            layout,
            &pressed,
            &self.sticky_layout_pressed_key_layers,
            layer_before,
        );
        let current_active_combos: Vec<bool> = combo_entries
            .iter()
            .enumerate()
            .map(|(idx, combo)| {
                sticky_combo_is_active_on_layer(
                    combo,
                    &pressed_keycodes,
                    layer_before,
                    self.sticky_layout_active_combos
                        .get(idx)
                        .copied()
                        .unwrap_or(false),
                )
            })
            .collect();
        for (idx, active) in current_active_combos.iter().copied().enumerate() {
            let was_active = self
                .sticky_layout_active_combos
                .get(idx)
                .copied()
                .unwrap_or(false);
            if active && !was_active {
                sticky_apply_persistent_layer_action(
                    combo_entries[idx].output.vial_keycode(),
                    layer_count,
                    &mut self.sticky_layout_toggled_layers,
                    &mut self.sticky_layout_base_layer,
                );
            }
        }
        self.sticky_layout_active_combos = current_active_combos;

        self.sticky_layout_prev_pressed = pressed;
        let virtual_layer_keycodes = sticky_virtual_layer_keycodes(
            &self.sticky_layout_active_combos,
            combo_entries,
            &self.sticky_layout_tap_dance_states,
            tap_dance_entries,
        );
        sticky_layout_active_layer(
            layout,
            &self.matrix_tester_pressed,
            &self.sticky_layout_pressed_key_layers,
            &self.sticky_layout_toggled_layers,
            self.sticky_layout_base_layer,
            &virtual_layer_keycodes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::FirmwareProtocol;
    use crate::keyboard::PhysicalKey;

    fn mo(layer: u16) -> u16 {
        0x5200 | (1 << 5) | layer
    }

    fn tg(layer: u16) -> u16 {
        0x5200 | (3 << 5) | layer
    }

    fn test_layout(layers: Vec<Vec<u16>>) -> KeyboardLayout {
        let cols = layers.first().map(Vec::len).unwrap_or(0);
        KeyboardLayout {
            name: "test".to_string(),
            rows: 1,
            cols,
            keys: (0..cols)
                .map(|col| PhysicalKey {
                    x: col as f32,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    row: 0,
                    col: col as u8,
                    label: format!("0,{col}"),
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            encoders: vec![],
            layers: layers
                .into_iter()
                .map(|layer| layer.into_iter().map(Into::into).collect())
                .collect(),
            encoder_layers: vec![],
            layer_names: vec![],
            custom_keycodes: vec![],
            layout_options: vec![],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn held_momentary_key_uses_the_layer_where_it_was_pressed() {
        let layout = test_layout(vec![vec![mo(2)], vec![0], vec![mo(3)], vec![0]]);
        let pressed = vec![true];
        let pressed_key_layers = vec![Some(0)];
        let toggled_layers = vec![false; 4];

        assert_eq!(
            sticky_layout_active_layer(
                &layout,
                &pressed,
                &pressed_key_layers,
                &toggled_layers,
                0,
                &[],
            ),
            2
        );
    }

    #[test]
    fn combo_output_participates_in_active_layer_calculation() {
        let layout = test_layout(vec![vec![0x0004, 0x0005], vec![0, 0], vec![0, 0]]);
        let pressed = vec![true, true];
        let pressed_key_layers = vec![Some(0), Some(0)];
        let pressed_keycodes = sticky_pressed_keycodes(&layout, &pressed, &pressed_key_layers, 0);
        let combo = ComboEntry {
            keys: [0x0004, 0x0005, 0, 0],
            output: mo(2).into(),
            layer: None,
        };

        assert!(sticky_combo_is_active(&combo, &pressed_keycodes));
        assert_eq!(
            sticky_layout_active_layer(
                &layout,
                &pressed,
                &pressed_key_layers,
                &[false; 3],
                0,
                &[combo.output.vial_keycode()],
            ),
            2
        );
    }

    #[test]
    fn layer_specific_combo_activates_only_on_its_layer() {
        let combo = ComboEntry {
            keys: [0x0004, 0x0005, 0, 0],
            output: 0x0006.into(),
            layer: Some(1),
        };
        let pressed_keycodes = [0x0004, 0x0005];

        assert!(!sticky_combo_is_active_on_layer(
            &combo,
            &pressed_keycodes,
            0,
            false,
        ));
        assert!(sticky_combo_is_active_on_layer(
            &combo,
            &pressed_keycodes,
            1,
            false,
        ));
        assert!(sticky_combo_is_active_on_layer(
            &combo,
            &pressed_keycodes,
            2,
            true,
        ));
    }

    #[test]
    fn tap_dance_hold_exposes_its_layer_action_after_tapping_term() {
        let entries = vec![crate::keycode_picker::TapDanceEntry {
            on_hold: mo(2).into(),
            tapping_term: 150,
            ..Default::default()
        }];
        let now = std::time::Instant::now();
        let mut state = StickyLayoutTapDanceState::Idle;

        assert!(sticky_update_tap_dance_state(&mut state, true, Some(0), &entries, now).is_empty());
        assert_eq!(
            sticky_update_tap_dance_state(
                &mut state,
                true,
                Some(0),
                &entries,
                now + std::time::Duration::from_millis(150),
            ),
            vec![mo(2)]
        );
        assert_eq!(
            sticky_tap_dance_active_keycode(&state, &entries),
            Some(mo(2))
        );
        assert!(sticky_update_tap_dance_state(
            &mut state,
            false,
            None,
            &entries,
            now + std::time::Duration::from_millis(160),
        )
        .is_empty());
        assert!(matches!(state, StickyLayoutTapDanceState::Idle));
    }

    #[test]
    fn tap_dance_tap_emits_persistent_layer_action_after_tapping_term() {
        let entries = vec![crate::keycode_picker::TapDanceEntry {
            on_tap: tg(2).into(),
            tapping_term: 150,
            ..Default::default()
        }];
        let now = std::time::Instant::now();
        let mut state = StickyLayoutTapDanceState::Idle;

        sticky_update_tap_dance_state(&mut state, true, Some(0), &entries, now);
        assert!(sticky_update_tap_dance_state(
            &mut state,
            false,
            None,
            &entries,
            now + std::time::Duration::from_millis(50),
        )
        .is_empty());
        assert_eq!(
            sticky_update_tap_dance_state(
                &mut state,
                false,
                None,
                &entries,
                now + std::time::Duration::from_millis(200),
            ),
            vec![tg(2)]
        );
    }
}
