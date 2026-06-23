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

#[derive(Clone, Copy)]
enum LayerKeyOsdKind {
    Hold,
    OneShot,
    ToggleOn,
    ToggleOff,
    Default,
}

fn layer_key_osd_momentary_kind(kc: u16) -> Option<(LayerKeyOsdKind, usize)> {
    if let Some((op, target)) = vial_layer_op_target(kc) {
        return match op {
            1 | 6 => Some((LayerKeyOsdKind::Hold, target)),
            4 => Some((LayerKeyOsdKind::OneShot, target)),
            _ => None,
        };
    }
    if kc & 0xF000 == 0x4000 {
        return Some((LayerKeyOsdKind::Hold, ((kc >> 8) & 0xF) as usize));
    }
    if (0x5000..0x5200).contains(&kc) {
        return Some((LayerKeyOsdKind::Hold, ((kc >> 4) & 0xF) as usize));
    }
    None
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
    fn layer_key_osd_layer_enabled(&self, target_layer: usize) -> bool {
        self.app_settings
            .layer_key_osd_layers
            .get(target_layer)
            .copied()
            .unwrap_or(true)
    }

    fn queue_layer_key_osd(&mut self, kind: LayerKeyOsdKind, target_layer: usize) {
        if !self.app_settings.layer_key_osd || !self.layer_key_osd_layer_enabled(target_layer) {
            return;
        }

        let lang = self.app_settings.language;
        let fallback_layer = crate::i18n::tr_catalog_format(
            lang,
            "ui.layer_key_osd_layer",
            &[("layer", &target_layer.to_string())],
        );
        let custom_layer_name = self
            .layer_names
            .get(target_layer)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty() && *name != target_layer.to_string());
        let layer_name = custom_layer_name.unwrap_or(fallback_layer.as_str());
        let title_key = match kind {
            LayerKeyOsdKind::Hold => "ui.layer_key_osd_hold",
            LayerKeyOsdKind::OneShot => "ui.layer_key_osd_one_shot",
            LayerKeyOsdKind::ToggleOn => "ui.layer_key_osd_toggle",
            LayerKeyOsdKind::ToggleOff => "ui.layer_key_osd_toggle_off",
            LayerKeyOsdKind::Default => "ui.layer_key_osd_default",
        };
        self.layer_key_osd_title =
            crate::i18n::tr_catalog_format(lang, title_key, &[("layer", layer_name)]);
        self.layer_key_osd_detail = if custom_layer_name.is_some() {
            fallback_layer
        } else {
            String::new()
        };
        let timeout_ms = clamp_notification_timeout_ms(self.app_settings.layer_key_osd_timeout_ms);
        self.layer_key_osd_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64));
    }

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
            if let Some((kind, target)) =
                layer_key_osd_momentary_kind(kc).filter(|(_, target)| *target < layer_count)
            {
                self.queue_layer_key_osd(kind, target);
            }
            if let Some(target) =
                sticky_toggle_layer_target(kc).filter(|target| *target < layer_count)
            {
                if let Some(enabled) = self.sticky_layout_toggled_layers.get_mut(target) {
                    *enabled = !*enabled;
                    let kind = if *enabled {
                        LayerKeyOsdKind::ToggleOn
                    } else {
                        LayerKeyOsdKind::ToggleOff
                    };
                    self.queue_layer_key_osd(kind, target);
                }
            } else if let Some(target) =
                sticky_base_layer_target(kc).filter(|target| *target < layer_count)
            {
                self.sticky_layout_base_layer = target;
                self.sticky_layout_toggled_layers.fill(false);
                self.queue_layer_key_osd(LayerKeyOsdKind::Default, target);
            }
        }

        self.sticky_layout_prev_pressed = pressed;
        self.sticky_layout_active_layer = sticky_layout_active_layer(
            layout,
            &self.matrix_tester_pressed,
            &self.sticky_layout_toggled_layers,
            self.sticky_layout_base_layer,
        );
        self.sticky_layout_active_layer
    }
}
