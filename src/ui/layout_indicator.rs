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

fn sticky_layout_active_layer(
    layout: &KeyboardLayout,
    matrix_pressed: &[bool],
    toggled_layers: &[bool],
    base_layer: usize,
) -> usize {
    let layer_count = layout.layers.len().max(1);
    let mut active_layer = toggled_layers
        .iter()
        .enumerate()
        .rev()
        .find_map(|(layer, enabled)| (*enabled && layer < layer_count).then_some(layer))
        .unwrap_or_else(|| base_layer.min(layer_count - 1));

    for _ in 0..layer_count {
        let next_layer = layout.keys.iter().enumerate().find_map(|(key_idx, key)| {
            if !layout_matrix_key_pressed(layout, matrix_pressed, key.row, key.col) {
                return None;
            }
            sticky_momentary_layer_target(layout_effective_keycode(layout, active_layer, key_idx))
                .filter(|target| *target < layer_count)
        });

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

        if self.sticky_layout_prev_pressed.len() != pressed.len() {
            self.sticky_layout_prev_pressed = vec![false; pressed.len()];
        }
        if self.sticky_layout_pressed_key_layers.len() != pressed.len() {
            self.sticky_layout_pressed_key_layers = vec![None; pressed.len()];
        }
        if self.sticky_layout_toggled_layers.len() != layer_count {
            self.sticky_layout_toggled_layers = vec![false; layer_count];
        }
        self.sticky_layout_base_layer = self.sticky_layout_base_layer.min(layer_count - 1);

        for (key_idx, key) in layout.keys.iter().enumerate() {
            let matrix_idx = key.row as usize * layout.cols + key.col as usize;
            let is_pressed = pressed.get(matrix_idx).copied().unwrap_or(false);
            let was_pressed = self
                .sticky_layout_prev_pressed
                .get(matrix_idx)
                .copied()
                .unwrap_or(false);
            if !is_pressed {
                if let Some(source_layer) =
                    self.sticky_layout_pressed_key_layers.get_mut(matrix_idx)
                {
                    *source_layer = None;
                }
                continue;
            }
            if was_pressed {
                continue;
            }

            let layer_before = sticky_layout_active_layer(
                layout,
                &self.sticky_layout_prev_pressed,
                &self.sticky_layout_toggled_layers,
                self.sticky_layout_base_layer,
            );
            let kc = layout_effective_keycode(layout, layer_before, key_idx);
            if sticky_momentary_layer_target(kc).is_some()
                || sticky_toggle_layer_target(kc).is_some()
                || sticky_base_layer_target(kc).is_some()
            {
                if let Some(source_layer) =
                    self.sticky_layout_pressed_key_layers.get_mut(matrix_idx)
                {
                    *source_layer = Some(layer_before);
                }
            }
            if let Some(target) =
                sticky_toggle_layer_target(kc).filter(|target| *target < layer_count)
            {
                if let Some(enabled) = self.sticky_layout_toggled_layers.get_mut(target) {
                    *enabled = !*enabled;
                }
            } else if let Some(target) =
                sticky_base_layer_target(kc).filter(|target| *target < layer_count)
            {
                self.sticky_layout_base_layer = target;
                self.sticky_layout_toggled_layers.fill(false);
            }
        }

        self.sticky_layout_prev_pressed = pressed;
        sticky_layout_active_layer(
            layout,
            &self.matrix_tester_pressed,
            &self.sticky_layout_toggled_layers,
            self.sticky_layout_base_layer,
        )
    }
}
