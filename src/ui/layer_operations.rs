use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LayerSnapshot {
    pub(super) keycodes: Vec<crate::keyboard::KeyBinding>,
    pub(super) encoder_keycodes: Vec<u16>,
}

#[derive(Clone, Debug)]
pub(super) struct LayerClipboard {
    snapshot: LayerSnapshot,
    key_geometry: Vec<VisualGeometry>,
    encoder_geometry: Vec<VisualGeometry>,
}

#[derive(Clone, Copy, Debug)]
struct VisualGeometry {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LayerOperationError {
    MissingLayer,
    KeyCountMismatch { source: usize, target: usize },
    IncompatibleGeometry,
    UnevenKeyCount { count: usize },
    UnbalancedGeometry,
}

fn capture_layer(layout: &KeyboardLayout, layer: usize) -> Option<LayerSnapshot> {
    layout.layers.get(layer)?;
    Some(LayerSnapshot {
        keycodes: (0..layout.keys.len())
            .map(|key_idx| layout.get_key_binding(layer, key_idx))
            .collect(),
        encoder_keycodes: (0..layout.encoders.len())
            .map(|encoder_idx| layout.get_encoder_keycode(layer, encoder_idx))
            .collect(),
    })
}

fn key_geometry(key: &PhysicalKey) -> VisualGeometry {
    let (x, y) = rotate_layout_point(
        key.x + key.w * 0.5,
        key.y + key.h * 0.5,
        key.rotation_x,
        key.rotation_y,
        key.rotation,
    );
    VisualGeometry {
        x,
        y,
        w: key.w,
        h: key.h,
        rotation: key.rotation,
    }
}

fn encoder_geometry(encoder: &PhysicalEncoder) -> VisualGeometry {
    let (x, y) = rotate_layout_point(
        encoder.x + encoder.w * 0.5,
        encoder.y + encoder.h * 0.5,
        encoder.rotation_x,
        encoder.rotation_y,
        encoder.rotation,
    );
    VisualGeometry {
        x,
        y,
        w: encoder.w,
        h: encoder.h,
        rotation: encoder.rotation,
    }
}

fn capture_clipboard(layout: &KeyboardLayout, layer: usize) -> Option<LayerClipboard> {
    Some(LayerClipboard {
        snapshot: capture_layer(layout, layer)?,
        key_geometry: layout.keys.iter().map(key_geometry).collect(),
        encoder_geometry: layout.encoders.iter().map(encoder_geometry).collect(),
    })
}

fn normalized_geometry(items: &[VisualGeometry]) -> Vec<VisualGeometry> {
    if items.is_empty() {
        return vec![];
    }
    let min_x = items
        .iter()
        .map(|item| item.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = items
        .iter()
        .map(|item| item.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = items
        .iter()
        .map(|item| item.y)
        .fold(f32::INFINITY, f32::min);
    let max_y = items
        .iter()
        .map(|item| item.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let span_x = max_x - min_x;
    let span_y = max_y - min_y;
    let average_width = items.iter().map(|item| item.w).sum::<f32>() / items.len() as f32;
    let average_height = items.iter().map(|item| item.h).sum::<f32>() / items.len() as f32;

    items
        .iter()
        .map(|item| VisualGeometry {
            x: if span_x > f32::EPSILON {
                (item.x - min_x) / span_x
            } else {
                0.5
            },
            y: if span_y > f32::EPSILON {
                (item.y - min_y) / span_y
            } else {
                0.5
            },
            w: item.w / average_width.max(f32::EPSILON),
            h: item.h / average_height.max(f32::EPSILON),
            rotation: item.rotation,
        })
        .collect()
}

fn angle_distance(left: f32, right: f32) -> f32 {
    ((left - right + 180.0).rem_euclid(360.0) - 180.0).abs()
}

/// Returns a target-index -> source-index mapping for visually compatible layouts.
fn geometry_mapping(
    source: &[VisualGeometry],
    target: &[VisualGeometry],
) -> Result<Vec<usize>, LayerOperationError> {
    if source.len() != target.len() {
        return Err(LayerOperationError::KeyCountMismatch {
            source: source.len(),
            target: target.len(),
        });
    }
    if source.is_empty() {
        return Ok(vec![]);
    }

    let source = normalized_geometry(source);
    let target = normalized_geometry(target);
    let mut candidates = Vec::with_capacity(source.len() * target.len());
    for (target_idx, target_item) in target.iter().enumerate() {
        for (source_idx, source_item) in source.iter().enumerate() {
            let dx = target_item.x - source_item.x;
            let dy = target_item.y - source_item.y;
            let dw = target_item.w - source_item.w;
            let dh = target_item.h - source_item.h;
            let rotation = angle_distance(target_item.rotation, source_item.rotation) / 180.0;
            let cost = dx * dx + dy * dy + (dw * dw + dh * dh) * 0.04 + rotation * rotation;
            candidates.push((cost, target_idx, source_idx));
        }
    }
    candidates.sort_by(|left, right| left.0.total_cmp(&right.0));

    let mut mapping = vec![usize::MAX; target.len()];
    let mut source_used = vec![false; source.len()];
    for (_, target_idx, source_idx) in candidates {
        if mapping[target_idx] == usize::MAX && !source_used[source_idx] {
            mapping[target_idx] = source_idx;
            source_used[source_idx] = true;
        }
    }

    for (target_idx, &source_idx) in mapping.iter().enumerate() {
        if source_idx == usize::MAX {
            return Err(LayerOperationError::IncompatibleGeometry);
        }
        let source_item = source[source_idx];
        let target_item = target[target_idx];
        if (target_item.x - source_item.x).abs() > 0.16
            || (target_item.y - source_item.y).abs() > 0.16
            || (target_item.w - source_item.w).abs() > 0.35
            || (target_item.h - source_item.h).abs() > 0.35
            || angle_distance(target_item.rotation, source_item.rotation) > 15.0
        {
            return Err(LayerOperationError::IncompatibleGeometry);
        }
    }
    Ok(mapping)
}

fn layer_snapshot_for_paste(
    copied: &LayerClipboard,
    target: &KeyboardLayout,
    layer: usize,
) -> Result<LayerSnapshot, LayerOperationError> {
    let current = capture_layer(target, layer).ok_or(LayerOperationError::MissingLayer)?;
    let target_key_geometry: Vec<_> = target.keys.iter().map(key_geometry).collect();
    let key_mapping = geometry_mapping(&copied.key_geometry, &target_key_geometry)?;
    let keycodes = key_mapping
        .into_iter()
        .map(|source_idx| copied.snapshot.keycodes[source_idx])
        .collect();

    let target_encoder_geometry: Vec<_> = target.encoders.iter().map(encoder_geometry).collect();
    let encoder_keycodes = geometry_mapping(&copied.encoder_geometry, &target_encoder_geometry)
        .ok()
        .map(|mapping| {
            mapping
                .into_iter()
                .map(|source_idx| copied.snapshot.encoder_keycodes[source_idx])
                .collect()
        })
        .unwrap_or(current.encoder_keycodes);

    Ok(LayerSnapshot {
        keycodes,
        encoder_keycodes,
    })
}

fn filled_layer_snapshot(
    layout: &KeyboardLayout,
    layer: usize,
    keycode: u16,
) -> Option<LayerSnapshot> {
    let mut snapshot = capture_layer(layout, layer)?;
    snapshot.keycodes.fill(keycode.into());
    Some(snapshot)
}

fn apply_layer_updates(
    layout: &mut KeyboardLayout,
    layer: usize,
    key_updates: &[(usize, crate::keyboard::KeyBinding)],
    encoder_updates: &[(usize, u16)],
) {
    for &(key_idx, binding) in key_updates {
        layout.set_key_binding(layer, key_idx, binding);
    }
    for &(encoder_idx, keycode) in encoder_updates {
        layout.set_encoder_keycode(layer, encoder_idx, keycode);
    }
}

fn mirror_key_mapping(keys: &[PhysicalKey]) -> Result<Vec<usize>, LayerOperationError> {
    let count = keys.len();
    if count == 0 || !count.is_multiple_of(2) {
        return Err(LayerOperationError::UnevenKeyCount { count });
    }

    let geometry: Vec<_> = keys.iter().map(key_geometry).collect();
    let mut by_x: Vec<usize> = (0..count).collect();
    by_x.sort_by(|&a, &b| {
        geometry[a]
            .x
            .total_cmp(&geometry[b].x)
            .then_with(|| geometry[a].y.total_cmp(&geometry[b].y))
    });
    let (left, right) = by_x.split_at(count / 2);
    let left_edge = left
        .iter()
        .map(|&idx| geometry[idx].x)
        .fold(f32::NEG_INFINITY, f32::max);
    let right_edge = right
        .iter()
        .map(|&idx| geometry[idx].x)
        .fold(f32::INFINITY, f32::min);
    if !left_edge.is_finite() || !right_edge.is_finite() || left_edge >= right_edge {
        return Err(LayerOperationError::UnbalancedGeometry);
    }
    let axis_x = (left_edge + right_edge) * 0.5;

    let mut candidates = Vec::with_capacity(left.len() * right.len());
    for &left_idx in left {
        let left_key = &keys[left_idx];
        let left_x = geometry[left_idx].x;
        let left_y = geometry[left_idx].y;
        let reflected_x = axis_x * 2.0 - left_x;
        for &right_idx in right {
            let right_key = &keys[right_idx];
            let right_x = geometry[right_idx].x;
            let right_y = geometry[right_idx].y;
            let dx = reflected_x - right_x;
            let dy = left_y - right_y;
            let dw = left_key.w - right_key.w;
            let dh = left_key.h - right_key.h;
            let rotation_sum = left_key.rotation + right_key.rotation;
            let cost =
                dx * dx + dy * dy * 4.0 + dw * dw + dh * dh + rotation_sum * rotation_sum * 0.001;
            candidates.push((cost, left_idx, right_idx));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut mapping = vec![usize::MAX; count];
    for (_, left_idx, right_idx) in candidates {
        if mapping[left_idx] == usize::MAX && mapping[right_idx] == usize::MAX {
            mapping[left_idx] = right_idx;
            mapping[right_idx] = left_idx;
        }
    }
    if mapping.contains(&usize::MAX) {
        return Err(LayerOperationError::UnbalancedGeometry);
    }
    for &left_idx in left {
        let right_idx = mapping[left_idx];
        let left_item = geometry[left_idx];
        let right_item = geometry[right_idx];
        let reflected_x = axis_x * 2.0 - left_item.x;
        if (reflected_x - right_item.x).abs() > 0.55
            || (left_item.y - right_item.y).abs() > 0.55
            || (left_item.w - right_item.w).abs() > 0.25
            || (left_item.h - right_item.h).abs() > 0.25
            || angle_distance(-left_item.rotation, right_item.rotation) > 10.0
        {
            return Err(LayerOperationError::UnbalancedGeometry);
        }
    }
    Ok(mapping)
}

#[cfg(test)]
fn mirrored_layer_snapshot(
    layout: &KeyboardLayout,
    layer: usize,
) -> Result<LayerSnapshot, LayerOperationError> {
    let mapping = mirror_key_mapping(&layout.keys)?;
    mirrored_layer_snapshot_with_mapping(layout, layer, &mapping)
}

fn mirrored_layer_snapshot_with_mapping(
    layout: &KeyboardLayout,
    layer: usize,
    mapping: &[usize],
) -> Result<LayerSnapshot, LayerOperationError> {
    let current = capture_layer(layout, layer).ok_or(LayerOperationError::MissingLayer)?;
    let keycodes = mapping
        .iter()
        .map(|&source_idx| match current.keycodes[source_idx] {
            crate::keyboard::KeyBinding::Vial(keycode) => {
                toggle_handed_modifier(keycode).unwrap_or(keycode).into()
            }
            crate::keyboard::KeyBinding::Rmk(action) => {
                crate::rmk_native::toggle_handed_key_action(action).into()
            }
        })
        .collect();
    Ok(LayerSnapshot {
        keycodes,
        encoder_keycodes: current.encoder_keycodes,
    })
}

enum LayerUiAction {
    Copy,
    Paste(LayerSnapshot),
    Mirror(Vec<usize>),
    FillNone,
    FillInherit,
}

pub(super) const LAYER_OPERATIONS_SUBMENU_HEIGHT: f32 = 174.0;

fn layer_operation_menu_labels(language: crate::i18n::Language) -> [String; 5] {
    [
        crate::i18n::tr_catalog(language, "layer_actions.copy").to_owned(),
        crate::i18n::tr_catalog(language, "layer_actions.paste").to_owned(),
        crate::i18n::tr_catalog(language, "layer_actions.mirror").to_owned(),
        crate::i18n::tr_catalog(language, "layer_actions.fill_none").to_owned(),
        crate::i18n::tr_catalog(language, "layer_actions.fill_inherit").to_owned(),
    ]
}

#[cfg(not(target_arch = "wasm32"))]
type KeyChange = (usize, u8, u8, crate::keyboard::KeyBinding);

#[cfg(not(target_arch = "wasm32"))]
type EncoderChange = (usize, u8, u8, u16);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerUndoBehavior {
    RecordOld,
    RetryDesired { requires_firmware: bool },
}

#[cfg(not(target_arch = "wasm32"))]
struct LayerWriteContext {
    layer: usize,
    old: LayerSnapshot,
    desired: LayerSnapshot,
    action: String,
    total: usize,
    undo_behavior: LayerUndoBehavior,
    is_default_layout_sync: bool,
    default_target: Option<DefaultLayerWriteTarget>,
}

// Bind completion to the profile and control mapping present when the write
// started, not whichever application/device happens to be selected on completion.
#[cfg(not(target_arch = "wasm32"))]
struct DefaultLayerWriteTarget {
    device_key: String,
    before: [u16; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT],
    key_controls: Vec<Option<usize>>,
    encoder_controls: Vec<Option<usize>>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct LayerWriteTask {
    receiver: std::sync::mpsc::Receiver<LayerWriteResult>,
    fallback: LayerWriteFallback,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct PendingLayerWrite {
    layer: usize,
    desired: LayerSnapshot,
    action_key: &'static str,
    undo_behavior: LayerUndoBehavior,
}

#[cfg(not(target_arch = "wasm32"))]
struct LayerWriteFallback {
    layer: usize,
    desired: LayerSnapshot,
    action: String,
    undo_behavior: LayerUndoBehavior,
}

#[cfg(not(target_arch = "wasm32"))]
struct LayerWriteResult {
    hid_device: Option<crate::hid::HidDevice>,
    context: LayerWriteContext,
    progress: LayerWriteProgress,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq, Eq)]
struct LayerWriteProgress {
    key_updates: Vec<(usize, crate::keyboard::KeyBinding)>,
    encoder_updates: Vec<(usize, u16)>,
    written: usize,
    error: Option<String>,
    disconnect: bool,
}

#[cfg(not(target_arch = "wasm32"))]
enum KeyWriteOutcome {
    Saved,
    Mismatch {
        readback: crate::keyboard::KeyBinding,
        error: String,
    },
    Failed(String),
}

#[cfg(not(target_arch = "wasm32"))]
fn run_layer_writes(
    key_changes: &[KeyChange],
    encoder_changes: &[EncoderChange],
    mut write_key: impl FnMut(u8, u8, crate::keyboard::KeyBinding) -> KeyWriteOutcome,
    mut write_encoder: impl FnMut(u8, u8, u16) -> Result<(), String>,
) -> LayerWriteProgress {
    let mut progress = LayerWriteProgress {
        key_updates: Vec::with_capacity(key_changes.len()),
        encoder_updates: Vec::with_capacity(encoder_changes.len()),
        written: 0,
        error: None,
        disconnect: false,
    };

    for &(key_idx, row, col, binding) in key_changes {
        match write_key(row, col, binding) {
            KeyWriteOutcome::Saved => {
                progress.key_updates.push((key_idx, binding));
                progress.written += 1;
            }
            KeyWriteOutcome::Mismatch { readback, error } => {
                progress.key_updates.push((key_idx, readback));
                progress.error = Some(error);
                return progress;
            }
            KeyWriteOutcome::Failed(error) => {
                progress.error = Some(error);
                progress.disconnect = true;
                return progress;
            }
        }
    }

    for &(encoder_idx, device_idx, direction, keycode) in encoder_changes {
        match write_encoder(device_idx, direction, keycode) {
            Ok(()) => {
                progress.encoder_updates.push((encoder_idx, keycode));
                progress.written += 1;
            }
            Err(error) => {
                progress.error = Some(error);
                progress.disconnect = true;
                return progress;
            }
        }
    }

    progress
}

#[cfg(not(target_arch = "wasm32"))]
fn undo_after_layer_write(
    context: &LayerWriteContext,
    touched: bool,
    failed: bool,
) -> Option<(LayerSnapshot, bool)> {
    match context.undo_behavior {
        LayerUndoBehavior::RecordOld if touched => Some((context.old.clone(), true)),
        LayerUndoBehavior::RetryDesired { requires_firmware } if failed => {
            Some((context.desired.clone(), requires_firmware))
        }
        LayerUndoBehavior::RecordOld | LayerUndoBehavior::RetryDesired { .. } => None,
    }
}

impl EntropyApp {
    fn copy_selected_layer(&mut self) {
        let Some(layout) = self.layout.as_ref() else {
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "layer_actions.no_connected_layer",
            )
            .into();
            return;
        };
        let rendered = self
            .application_layouts_supported()
            .then(|| self.application_layout_rendered_copy(layout));
        let layout = rendered.as_ref().unwrap_or(layout);
        let Some(clipboard) = capture_clipboard(layout, self.selected_layer) else {
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "layer_actions.no_connected_layer",
            )
            .into();
            return;
        };
        let layer = self.selected_layer.to_string();
        let keys = clipboard.snapshot.keycodes.len().to_string();
        self.layer_clipboard = Some(clipboard);
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "layer_actions.copied_status",
            &[("layer", &layer), ("keys", &keys)],
        );
    }

    fn paste_selected_layer(&mut self, desired: LayerSnapshot) {
        self.apply_layer_snapshot(self.selected_layer, desired, "layer_actions.paste");
    }

    fn layer_operation_source(&self) -> Option<KeyboardLayout> {
        let layout = self.layout.as_ref()?;
        let profile_selected = self.application_layout_editor_active
            && self.application_layouts_supported()
            && self.application_layout_settings().is_some_and(|settings| {
                settings.active_layout_id
                    != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
            });
        Some(if profile_selected {
            self.application_layout_rendered_copy(layout)
        } else {
            layout.clone()
        })
    }

    fn fill_selected_layer(&mut self, keycode: u16) {
        let target_layer = self.selected_layer;
        let desired = self
            .layer_operation_source()
            .as_ref()
            .and_then(|layout| filled_layer_snapshot(layout, target_layer, keycode));
        let Some(desired) = desired else {
            self.report_layer_operation_error(LayerOperationError::MissingLayer);
            return;
        };
        let action_key = if keycode == 0x0001 {
            "layer_actions.fill_inherit"
        } else {
            "layer_actions.fill_none"
        };
        self.apply_layer_snapshot(target_layer, desired, action_key);
    }

    fn mirror_selected_layer(&mut self, mapping: &[usize]) {
        let desired = self
            .layer_operation_source()
            .as_ref()
            .ok_or(LayerOperationError::MissingLayer)
            .and_then(|layout| {
                mirrored_layer_snapshot_with_mapping(layout, self.selected_layer, mapping)
            });
        match desired {
            Ok(desired) => {
                self.apply_layer_snapshot(self.selected_layer, desired, "layer_actions.mirror")
            }
            Err(error) => self.report_layer_operation_error(error),
        }
    }

    fn layer_paste_snapshot(&self) -> Result<LayerSnapshot, LayerOperationError> {
        let copied = self
            .layer_clipboard
            .as_ref()
            .ok_or(LayerOperationError::MissingLayer)?;
        let layout = self
            .layout
            .as_ref()
            .ok_or(LayerOperationError::MissingLayer)?;
        if layout.layers.get(self.selected_layer).is_none() {
            return Err(LayerOperationError::MissingLayer);
        }
        layer_snapshot_for_paste(copied, layout, self.selected_layer)
    }

    pub(super) fn layer_operations_submenu_width(&self, ui: &egui::Ui) -> f32 {
        let language = self.app_settings.language;
        let labels = layer_operation_menu_labels(language);
        adaptive_top_dropdown_width(ui, labels.iter().map(String::as_str), 210.0)
    }

    pub(super) fn draw_layer_operations_submenu(
        &mut self,
        ui: &mut egui::Ui,
        menu_width: f32,
    ) -> bool {
        let language = self.app_settings.language;
        let [
            copy_label,
            paste_label,
            mirror_label,
            fill_none_label,
            fill_inherit_label,
        ] = layer_operation_menu_labels(language);
        let paste_snapshot = self.layer_paste_snapshot();
        let mirror_mapping = self
            .layout
            .as_ref()
            .ok_or(LayerOperationError::MissingLayer)
            .and_then(|layout| mirror_key_mapping(&layout.keys));
        #[cfg(not(target_arch = "wasm32"))]
        let layer_mutation_available = self.pending_layer_write.is_none()
            && !self.pending_layout_undo
            && !self.hid_user_action_busy();
        #[cfg(target_arch = "wasm32")]
        let layer_mutation_available = true;
        let mut requested_action = None;

        ui.set_min_width(menu_width);
        ui.set_max_width(menu_width);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

        let copy_response = top_dropdown_item(ui, menu_width, &copy_label, true, false);
        let copy_clicked = copy_response.clicked();
        copy_response.on_hover_text(crate::i18n::tr_catalog(
            language,
            "layer_actions.copy_tooltip",
        ));
        if copy_clicked {
            requested_action = Some(LayerUiAction::Copy);
        }

        let paste_enabled = layer_mutation_available && paste_snapshot.is_ok();
        let paste_response = top_dropdown_item(ui, menu_width, &paste_label, paste_enabled, false);
        let paste_clicked = paste_response.clicked();
        let paste_tooltip = if self.layer_clipboard.is_none() {
            crate::i18n::tr_catalog(language, "layer_actions.clipboard_empty").to_owned()
        } else if let Err(error) = paste_snapshot.as_ref() {
            self.layer_operation_error_message(error.clone())
        } else {
            crate::i18n::tr_catalog(language, "layer_actions.paste_tooltip").to_owned()
        };
        paste_response.on_hover_text(paste_tooltip);
        if paste_clicked {
            requested_action = paste_snapshot
                .as_ref()
                .ok()
                .cloned()
                .map(LayerUiAction::Paste);
        }

        ui.add_space(6.0);

        let mirror_enabled = layer_mutation_available && mirror_mapping.is_ok();
        let mirror_response =
            top_dropdown_item(ui, menu_width, &mirror_label, mirror_enabled, false);
        let mirror_clicked = mirror_response.clicked();
        let mirror_tooltip = if let Err(error) = mirror_mapping.as_ref() {
            self.layer_operation_error_message(error.clone())
        } else {
            crate::i18n::tr_catalog(language, "layer_actions.mirror_tooltip").to_owned()
        };
        mirror_response.on_hover_text(mirror_tooltip);
        if mirror_clicked {
            requested_action = mirror_mapping
                .as_ref()
                .ok()
                .map(|mapping| LayerUiAction::Mirror(mapping.clone()));
        }

        ui.add_space(6.0);

        let none_response = top_dropdown_item(
            ui,
            menu_width,
            &fill_none_label,
            layer_mutation_available,
            false,
        );
        let none_clicked = none_response.clicked();
        none_response.on_hover_text(crate::i18n::tr_catalog(
            language,
            "layer_actions.fill_none_tooltip",
        ));
        if none_clicked {
            requested_action = Some(LayerUiAction::FillNone);
        }

        let inherit_response = top_dropdown_item(
            ui,
            menu_width,
            &fill_inherit_label,
            layer_mutation_available,
            false,
        );
        let inherit_clicked = inherit_response.clicked();
        inherit_response.on_hover_text(crate::i18n::tr_catalog(
            language,
            "layer_actions.fill_inherit_tooltip",
        ));
        if inherit_clicked {
            requested_action = Some(LayerUiAction::FillInherit);
        }

        let action_requested = requested_action.is_some();
        match requested_action {
            Some(LayerUiAction::Copy) => self.copy_selected_layer(),
            Some(LayerUiAction::Paste(desired)) => self.paste_selected_layer(desired),
            Some(LayerUiAction::Mirror(mapping)) => self.mirror_selected_layer(&mapping),
            Some(LayerUiAction::FillNone) => self.fill_selected_layer(0x0000),
            Some(LayerUiAction::FillInherit) => self.fill_selected_layer(0x0001),
            None => {}
        }
        action_requested
    }

    fn report_layer_operation_error(&mut self, error: LayerOperationError) {
        self.status_msg = self.layer_operation_error_message(error);
    }

    fn layer_operation_error_message(&self, error: LayerOperationError) -> String {
        match error {
            LayerOperationError::KeyCountMismatch { source, target } => {
                crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "layer_actions.key_count_mismatch",
                    &[
                        ("source", &source.to_string()),
                        ("target", &target.to_string()),
                    ],
                )
            }
            LayerOperationError::UnevenKeyCount { .. }
            | LayerOperationError::UnbalancedGeometry => crate::i18n::tr_catalog(
                self.app_settings.language,
                "layer_actions.mirror_unavailable",
            )
            .into(),
            LayerOperationError::IncompatibleGeometry => crate::i18n::tr_catalog(
                self.app_settings.language,
                "layer_actions.paste_geometry_mismatch",
            )
            .into(),
            LayerOperationError::MissingLayer => crate::i18n::tr_catalog(
                self.app_settings.language,
                "layer_actions.no_connected_layer",
            )
            .into(),
        }
    }

    // Application profiles live in host settings, not in the firmware-owned
    // Default keymap. Route the complete operation before borrowing a HID owner.
    fn apply_application_profile_layer_snapshot(
        &mut self,
        layer: usize,
        desired: &LayerSnapshot,
        action_key: &'static str,
    ) -> bool {
        use crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
        if action_key == "layer_actions.save_default_to_device"
            || !self.application_layout_editor_active
            || !self.application_layouts_supported()
        {
            return false;
        }
        let Some(settings) = self.application_layout_settings() else {
            return false;
        };
        let layout_id = settings.active_layout_id.clone();
        if layout_id == DEFAULT_APPLICATION_LAYOUT_ID {
            return false;
        }
        let Some(device_key) = self.application_layout_device_key() else {
            return true;
        };
        let Some(layout) = self.layout.as_ref() else {
            return true;
        };
        if desired.keycodes.len() != layout.keys.len()
            || desired.encoder_keycodes.len() != layout.encoders.len()
        {
            self.report_layer_operation_error(LayerOperationError::IncompatibleGeometry);
            return true;
        }
        let updates =
            desired
                .keycodes
                .iter()
                .enumerate()
                .filter_map(|(index, binding)| {
                    Self::application_control_for_key(layout, index)
                        .map(|control| (control, binding.vial_keycode()))
                })
                .chain(desired.encoder_keycodes.iter().enumerate().filter_map(
                    |(index, keycode)| {
                        Self::application_control_for_encoder(layout, index)
                            .map(|control| (control, *keycode))
                    },
                ))
                .collect::<Vec<_>>();
        let Some(profile) = self
            .app_settings
            .application_layouts
            .get_mut(&device_key)
            .and_then(|settings| settings.layouts.get_mut(&layout_id))
        else {
            return true;
        };
        let Some(old_keycodes) = profile.layers.get(layer).copied() else {
            return true;
        };
        let mut count = 0;
        for (control, keycode) in updates {
            if profile.set_keycode(layer, control, keycode) {
                count += 1;
            }
        }
        let changed = count != 0;
        let count = count.to_string();
        if changed {
            self.undo_stack.push(UndoAction::ApplicationLayoutLayer {
                device_key,
                layout_id,
                layer,
                old_keycodes,
            });
            save_app_settings(&self.app_settings);
            self.refresh_layer_picker_content_flags();
        }
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            if changed {
                "layer_actions.saved_status"
            } else {
                "layer_actions.no_changes"
            },
            &[
                ("layer", &layer.to_string()),
                ("count", &count),
                (
                    "action",
                    crate::i18n::tr_catalog(self.app_settings.language, action_key),
                ),
            ],
        );
        true
    }

    pub(super) fn apply_layer_snapshot(
        &mut self,
        layer: usize,
        desired: LayerSnapshot,
        action_key: &'static str,
    ) {
        if self.apply_application_profile_layer_snapshot(layer, &desired, action_key) {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.apply_layer_snapshot_with_behavior(
            layer,
            desired,
            action_key,
            LayerUndoBehavior::RecordOld,
        );

        #[cfg(target_arch = "wasm32")]
        self.apply_layer_snapshot_locally(layer, desired, action_key);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn undo_layer_snapshot(
        &mut self,
        layer: usize,
        desired: LayerSnapshot,
        requires_firmware: bool,
    ) {
        self.apply_layer_snapshot_with_behavior(
            layer,
            desired,
            "layer_actions.undo",
            LayerUndoBehavior::RetryDesired { requires_firmware },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn default_layer_write_target(&self, layer: usize) -> Option<DefaultLayerWriteTarget> {
        let device_key = self.application_layout_device_key()?;
        let profile = self
            .app_settings
            .application_layouts
            .get(&device_key)?
            .layouts
            .get(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)?;
        let layout = self.layout.as_ref()?;
        Some(DefaultLayerWriteTarget {
            device_key,
            before: *profile.layers.get(layer)?,
            key_controls: (0..layout.keys.len())
                .map(|i| Self::application_control_for_key(layout, i))
                .collect(),
            encoder_controls: (0..layout.encoders.len())
                .map(|i| Self::application_control_for_encoder(layout, i))
                .collect(),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reconcile_layer_write_default(
        &mut self,
        target: Option<&DefaultLayerWriteTarget>,
        layer: usize,
        key_updates: &[(usize, crate::keyboard::KeyBinding)],
        encoder_updates: &[(usize, u16)],
    ) {
        let Some(target) = target else {
            return;
        };
        let Some(profile) = self
            .app_settings
            .application_layouts
            .get_mut(&target.device_key)
            .and_then(|s| {
                s.layouts
                    .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            })
        else {
            return;
        };
        let mut changed = false;
        for (control, keycode) in key_updates
            .iter()
            .filter_map(|(i, binding)| {
                target
                    .key_controls
                    .get(*i)
                    .copied()
                    .flatten()
                    .map(|c| (c, binding.vial_keycode()))
            })
            .chain(encoder_updates.iter().filter_map(|(i, code)| {
                target
                    .encoder_controls
                    .get(*i)
                    .copied()
                    .flatten()
                    .map(|c| (c, *code))
            }))
        {
            // An edit made while the HID owner was borrowed remains authoritative.
            if profile
                .layers
                .get(layer)
                .is_some_and(|codes| codes[control] == target.before[control])
            {
                changed |= profile.set_keycode(layer, control, keycode);
            }
        }
        if changed {
            save_app_settings(&self.app_settings);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_layer_snapshot_with_behavior(
        &mut self,
        layer: usize,
        desired: LayerSnapshot,
        action_key: &'static str,
        undo_behavior: LayerUndoBehavior,
    ) {
        self.drop_queued_middle_click_assignments_for_layer(layer);

        if self.vial_hid_background_layer_active() {
            self.pending_layer_write = Some(PendingLayerWrite {
                layer,
                desired,
                action_key,
                undo_behavior,
            });
            self.deferred_device_load.defer_background_for_user_input();
            return;
        }

        if self.hid_write_task_active() {
            if let LayerUndoBehavior::RetryDesired { requires_firmware } = undo_behavior {
                self.undo_stack.push(UndoAction::Layer {
                    layer,
                    old: desired,
                    requires_firmware,
                });
            }
            return;
        }

        let Some(layout) = self.layout.as_ref() else {
            self.report_layer_operation_error(LayerOperationError::MissingLayer);
            return;
        };
        let Some(old) = capture_layer(layout, layer) else {
            self.report_layer_operation_error(LayerOperationError::MissingLayer);
            return;
        };
        if desired.keycodes.len() != old.keycodes.len() {
            self.report_layer_operation_error(LayerOperationError::KeyCountMismatch {
                source: desired.keycodes.len(),
                target: old.keycodes.len(),
            });
            return;
        }

        let key_changes: Vec<KeyChange> = desired
            .keycodes
            .iter()
            .copied()
            .enumerate()
            .filter(|(idx, keycode)| old.keycodes.get(*idx).copied() != Some(*keycode))
            .filter_map(|(idx, keycode)| {
                layout
                    .keys
                    .get(idx)
                    .map(|key| (idx, key.row, key.col, keycode))
            })
            .collect();
        let encoder_changes: Vec<EncoderChange> = desired
            .encoder_keycodes
            .iter()
            .copied()
            .enumerate()
            .filter(|(idx, keycode)| old.encoder_keycodes.get(*idx).copied() != Some(*keycode))
            .filter_map(|(idx, keycode)| {
                layout
                    .encoders
                    .get(idx)
                    .map(|encoder| (idx, encoder.encoder_idx, encoder.direction, keycode))
            })
            .collect();
        let is_default_layout_sync = action_key == "layer_actions.save_default_to_device";
        let default_target = (!is_default_layout_sync)
            .then(|| self.default_layer_write_target(layer))
            .flatten();
        // Undo restores what Default displayed, which can include pending edits.
        let undo_old = default_target
            .as_ref()
            .map(|target| {
                let profile = &self.app_settings.application_layouts[&target.device_key].layouts
                    [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID];
                capture_layer(
                    &Self::application_layout_rendered_copy_for_profile(layout, Some(profile)),
                    layer,
                )
                .unwrap()
            })
            .unwrap_or_else(|| old.clone());
        // Values already equal in firmware are confirmed without another write.
        let unchanged_keys = desired
            .keycodes
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, code)| old.keycodes.get(*i) == Some(code))
            .collect::<Vec<_>>();
        let unchanged_encoders = desired
            .encoder_keycodes
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, code)| old.encoder_keycodes.get(*i) == Some(code))
            .collect::<Vec<_>>();
        let total = key_changes.len() + encoder_changes.len();
        let action = crate::i18n::tr_catalog(self.app_settings.language, action_key).to_owned();
        let layer_text = layer.to_string();
        if total == 0 {
            let profile_changed = undo_old != desired;
            self.reconcile_layer_write_default(
                default_target.as_ref(),
                layer,
                &unchanged_keys,
                &unchanged_encoders,
            );
            if profile_changed && matches!(undo_behavior, LayerUndoBehavior::RecordOld) {
                self.undo_stack.push(UndoAction::Layer {
                    layer,
                    old: undo_old,
                    requires_firmware: false,
                });
            }
            self.refresh_layer_picker_content_flags();
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "layer_actions.no_changes",
                &[("layer", &layer_text)],
            );
            return;
        }

        if matches!(
            undo_behavior,
            LayerUndoBehavior::RetryDesired {
                requires_firmware: true
            }
        ) && self.hid_device.is_none()
        {
            self.undo_stack.push(UndoAction::Layer {
                layer,
                old: desired,
                requires_firmware: true,
            });
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "layer_actions.undo_requires_connection",
            )
            .into();
            return;
        }

        let Some(hid_device) = self.hid_device.take() else {
            let key_updates = key_changes
                .iter()
                .map(|(key_idx, _, _, keycode)| (*key_idx, *keycode))
                .collect::<Vec<_>>();
            let encoder_updates = encoder_changes
                .iter()
                .map(|(encoder_idx, _, _, keycode)| (*encoder_idx, *keycode))
                .collect::<Vec<_>>();
            if let Some(layout) = &mut self.layout {
                apply_layer_updates(layout, layer, &key_updates, &encoder_updates);
            }
            if matches!(undo_behavior, LayerUndoBehavior::RecordOld) {
                self.undo_stack.push(UndoAction::Layer {
                    layer,
                    old: undo_old,
                    requires_firmware: false,
                });
            }
            self.reconcile_layer_write_default(
                default_target.as_ref(),
                layer,
                &key_updates,
                &encoder_updates,
            );
            self.reconcile_layer_write_default(
                default_target.as_ref(),
                layer,
                &unchanged_keys,
                &unchanged_encoders,
            );
            self.refresh_layer_picker_content_flags();
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "layer_actions.local_only_status",
                &[
                    ("action", &action),
                    ("layer", &layer_text),
                    ("count", &total.to_string()),
                ],
            );
            return;
        };

        let saving_status = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "layer_actions.saving_status",
            &[
                ("action", &action),
                ("layer", &layer_text),
                ("count", &total.to_string()),
            ],
        );
        let fallback = LayerWriteFallback {
            layer,
            desired: desired.clone(),
            action: action.clone(),
            undo_behavior,
        };
        let context = LayerWriteContext {
            layer,
            old: undo_old,
            desired,
            action,
            total,
            undo_behavior,
            is_default_layout_sync,
            default_target,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let layer_u8 = layer as u8;
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = hid_device.macos_hid_operation_lock();

            let mut progress = run_layer_writes(
                &key_changes,
                &encoder_changes,
                |row, col, binding| match binding {
                    crate::keyboard::KeyBinding::Vial(keycode) => {
                        match hid_device.set_keycode(layer_u8, row, col, keycode) {
                            Ok(()) => KeyWriteOutcome::Saved,
                            Err(error) => match crate::hid::keycode_writeback_readback(&error) {
                                Some(readback) => KeyWriteOutcome::Mismatch {
                                    readback: readback.into(),
                                    error: error.to_string(),
                                },
                                None => KeyWriteOutcome::Failed(error.to_string()),
                            },
                        }
                    }
                    crate::keyboard::KeyBinding::Rmk(action) => hid_device
                        .set_rmk_key_action(layer_u8, row, col, action)
                        .map(|()| KeyWriteOutcome::Saved)
                        .unwrap_or_else(|error| KeyWriteOutcome::Failed(error.to_string())),
                },
                |device_idx, direction, keycode| {
                    hid_device
                        .set_encoder(layer_u8, device_idx, direction, keycode)
                        .map_err(|error| error.to_string())
                },
            );
            if context.default_target.is_some() {
                progress.key_updates.extend(unchanged_keys);
                progress.encoder_updates.extend(unchanged_encoders);
            }
            let hid_device = if progress.disconnect {
                None
            } else {
                Some(hid_device)
            };
            let _ = sender.send(LayerWriteResult {
                hid_device,
                context,
                progress,
            });
        });
        self.layer_write_task = Some(LayerWriteTask { receiver, fallback });
        self.status_msg = saving_status;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_pending_layer_write(&mut self) {
        if self.pending_layer_write.is_none()
            || self.vial_hid_background_layer_active()
            || self.hid_user_action_busy()
        {
            return;
        }

        let pending = self
            .pending_layer_write
            .take()
            .expect("pending layer write checked above");
        self.apply_layer_snapshot_with_behavior(
            pending.layer,
            pending.desired,
            pending.action_key,
            pending.undo_behavior,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_layer_write(&mut self, ctx: &egui::Context) {
        let result = match self.layer_write_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };

        match result {
            Ok(result) => {
                self.layer_write_task = None;
                self.finish_layer_write(result);
                self.continue_pending_settings_writes(ctx);
                self.resume_pending_device_connect();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .layer_write_task
                    .take()
                    .expect("layer write task checked above");
                self.hid_device = None;
                if let LayerUndoBehavior::RetryDesired { requires_firmware } =
                    task.fallback.undo_behavior
                {
                    self.undo_stack.push(UndoAction::Layer {
                        layer: task.fallback.layer,
                        old: task.fallback.desired,
                        requires_firmware,
                    });
                }
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "layer_actions.write_task_failed",
                    &[
                        ("action", &task.fallback.action),
                        ("layer", &task.fallback.layer.to_string()),
                    ],
                );
                self.continue_pending_settings_writes(ctx);
                self.resume_pending_device_connect();
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_layer_write(&mut self, result: LayerWriteResult) {
        if result.context.is_default_layout_sync {
            if result.progress.error.is_none() {
                self.default_layout_pending_layers &= !(1u16 << result.context.layer);
            }
            self.default_layout_sync_retry_after = result
                .progress
                .error
                .as_ref()
                .map(|_| std::time::Instant::now() + std::time::Duration::from_secs(15));
        }
        self.hid_device = result.hid_device;
        if let Some(layout) = &mut self.layout {
            apply_layer_updates(
                layout,
                result.context.layer,
                &result.progress.key_updates,
                &result.progress.encoder_updates,
            );
        }

        self.reconcile_layer_write_default(
            result.context.default_target.as_ref(),
            result.context.layer,
            &result.progress.key_updates,
            &result.progress.encoder_updates,
        );
        let touched = if result.context.default_target.is_some() {
            result
                .progress
                .key_updates
                .iter()
                .any(|(i, code)| result.context.old.keycodes.get(*i) != Some(code))
                || result
                    .progress
                    .encoder_updates
                    .iter()
                    .any(|(i, code)| result.context.old.encoder_keycodes.get(*i) != Some(code))
        } else {
            !result.progress.key_updates.is_empty() || !result.progress.encoder_updates.is_empty()
        };
        let failed = result.progress.error.is_some();
        if let Some((old, requires_firmware)) =
            undo_after_layer_write(&result.context, touched, failed)
        {
            self.undo_stack.push(UndoAction::Layer {
                layer: result.context.layer,
                old,
                requires_firmware,
            });
        }
        if touched {
            self.refresh_layer_picker_content_flags();
        }

        let layer = result.context.layer.to_string();
        let written = result.progress.written.to_string();
        self.status_msg = if let Some(error) = result.progress.error {
            crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "layer_actions.partial_status",
                &[
                    ("action", &result.context.action),
                    ("layer", &layer),
                    ("applied", &written),
                    ("total", &result.context.total.to_string()),
                    ("error", &error),
                ],
            )
        } else {
            crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "layer_actions.saved_status",
                &[
                    ("action", &result.context.action),
                    ("layer", &layer),
                    ("count", &written),
                ],
            )
        };
    }

    #[cfg(target_arch = "wasm32")]
    fn apply_layer_snapshot_locally(
        &mut self,
        layer: usize,
        desired: LayerSnapshot,
        action_key: &'static str,
    ) {
        let Some(old) = self
            .layout
            .as_ref()
            .and_then(|layout| capture_layer(layout, layer))
        else {
            self.report_layer_operation_error(LayerOperationError::MissingLayer);
            return;
        };
        if desired.keycodes.len() != old.keycodes.len() {
            self.report_layer_operation_error(LayerOperationError::KeyCountMismatch {
                source: desired.keycodes.len(),
                target: old.keycodes.len(),
            });
            return;
        }
        let changed = desired
            .keycodes
            .iter()
            .zip(&old.keycodes)
            .filter(|(desired, old)| desired != old)
            .count()
            + desired
                .encoder_keycodes
                .iter()
                .zip(&old.encoder_keycodes)
                .filter(|(desired, old)| desired != old)
                .count();
        if changed == 0 {
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "layer_actions.no_changes",
                &[("layer", &layer.to_string())],
            );
            return;
        }
        if let Some(layout) = &mut self.layout {
            for (key_idx, binding) in desired.keycodes.into_iter().enumerate() {
                layout.set_key_binding(layer, key_idx, binding);
            }
            for (encoder_idx, keycode) in desired.encoder_keycodes.into_iter().enumerate() {
                layout.set_encoder_keycode(layer, encoder_idx, keycode);
            }
        }
        self.undo_stack.push(UndoAction::Layer {
            layer,
            old,
            requires_firmware: false,
        });
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "layer_actions.local_only_status",
            &[
                (
                    "action",
                    crate::i18n::tr_catalog(self.app_settings.language, action_key),
                ),
                ("layer", &layer.to_string()),
                ("count", &changed.to_string()),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;

    fn bindings(keycodes: &[u16]) -> Vec<KeyBinding> {
        keycodes.iter().copied().map(Into::into).collect()
    }

    fn physical_key(index: usize, x: f32, y: f32) -> PhysicalKey {
        PhysicalKey {
            x,
            y,
            w: 1.0,
            h: 1.0,
            row: (index / 8) as u8,
            col: (index % 8) as u8,
            label: index.to_string(),
            rotation: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            layout_condition: None,
        }
    }

    fn physical_encoder(index: usize, x: f32, y: f32) -> PhysicalEncoder {
        PhysicalEncoder {
            x,
            y,
            w: 1.0,
            h: 1.0,
            label: index.to_string(),
            encoder_idx: index as u8,
            direction: 0,
            rotation: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            layout_condition: None,
        }
    }

    fn layout(
        key_positions: &[(f32, f32)],
        keycodes: &[u16],
        encoder_keycodes: &[u16],
    ) -> KeyboardLayout {
        KeyboardLayout {
            name: "Test".into(),
            rows: 8,
            cols: 8,
            keys: key_positions
                .iter()
                .enumerate()
                .map(|(idx, &(x, y))| physical_key(idx, x, y))
                .collect(),
            encoders: encoder_keycodes
                .iter()
                .enumerate()
                .map(|(idx, _)| physical_encoder(idx, idx as f32, 0.0))
                .collect(),
            layers: vec![bindings(keycodes)],
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

    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_fixture() -> (EntropyApp, crate::hid::TestHidRecorder, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let device = crate::device::Device {
            name: "M4CR0Pad v3".into(),
            vendor_id: 0xe126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".into(),
            serial_number: "layer-paste-test".into(),
            bus_type: "Usb".into(),
            path: "inert-layer-paste".into(),
            instance_token: "layer-paste-test".into(),
            firmware: FirmwareProtocol::Vial,
        };
        app.device_about_info = Some(DeviceAboutInfo {
            supports_application_layouts: true,
            vendor_id: device.vendor_id,
            product_id: device.product_id,
            path: device.path.clone(),
            ..Default::default()
        });
        app.device_manager.replace_devices(vec![device]);
        app.selected_device = Some(0);
        app.current_keyboard_id = Some(42);
        // The Macropad definition used by production connection/geometry tests:
        // 12 keys, encoder press and both directions in visual storage order.
        let mut board = KeyboardLayout::from_vial_json(&serde_json::json!({
            "name": "M4CR0Pad v3", "matrix": { "rows": 5, "cols": 3 },
            "layouts": { "keymap": [
                [{ "x": 1.25 }, "1,0", "1,1", "1,2"],
                ["0,1\n\n\n\n\n\n\n\n\ne", { "x": 0.25 }, "2,0", "2,1", "2,2"],
                ["0,2", { "x": 0.25 }, "3,0", "3,1", "3,2"],
                ["0,0\n\n\n\n\n\n\n\n\ne", { "x": 0.25 }, "4,0", "4,1", "4,2"]
            ] }
        }))
        .unwrap();
        assert_eq!(board.keys.len(), 13);
        assert_eq!(board.encoders.len(), 2);
        board.layers = vec![bindings(&[4; 13]), bindings(&[5; 13])];
        board.encoder_layers = vec![vec![6, 7], vec![8, 9]];
        let key = app.application_layout_device_key().unwrap();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        settings.create_for_application(&crate::application_layouts::DetectedApplication {
            executable: "fixture-app".into(),
            display_name: "Fixture".into(),
            ..Default::default()
        });
        let profile = settings
            .layouts
            .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap();
        profile.layers[0] = [4; 15];
        profile.layers[1] = [5; 15];
        for (i, _) in board.encoders.iter().enumerate() {
            let control = EntropyApp::application_control_for_encoder(&board, i).unwrap();
            profile.layers[0][control] = board.encoder_layers[0][i];
            profile.layers[1][control] = board.encoder_layers[1][i];
        }
        // Whole-layer operations retain profile metadata and encoder stacks.
        for profile in settings.layouts.values_mut() {
            profile.set_encoder_stack(
                0,
                vec![
                    crate::application_layouts::EncoderStackAction {
                        name: "Volume".into(),
                        counter_clockwise: 6,
                        clockwise: 7,
                    },
                    crate::application_layouts::EncoderStackAction {
                        name: "Scroll".into(),
                        counter_clockwise: 16,
                        clockwise: 17,
                    },
                ],
            );
        }
        settings.editor_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.into();
        app.app_settings.application_layouts.insert(key, settings);
        app.layout = Some(board);
        app.default_layout_device_reconciled = true;
        app.application_layout_editor_active = true;
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut codes = std::collections::BTreeMap::new();
        recorder.respond_by(move |request| match request[0] {
            0x05 => {
                codes.insert(
                    (request[1], request[2], request[3]),
                    [request[4], request[5]],
                );
                Some(Ok(*request))
            }
            0x04 => {
                let mut reply = *request;
                reply[4..6]
                    .copy_from_slice(codes.get(&(request[1], request[2], request[3])).unwrap());
                Some(Ok(reply))
            }
            _ => None,
        });
        app.hid_device = Some(hid);
        (app, recorder, ctx)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drain_layer_write(app: &mut EntropyApp, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.layer_write_task.is_some() {
            app.poll_layer_write(ctx);
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn click_layer_menu(app: &mut EntropyApp, ctx: &egui::Context, item: usize) {
        let draw = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.draw_layer_operations_submenu(ui, 240.0);
                },
            )
        };
        let _ = draw(app, vec![]);
        let output = draw(app, vec![]);
        let label = layer_operation_menu_labels(app.app_settings.language)[item].clone();
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("visible production submenu item");
        for pressed in [true, false] {
            let _ = draw(
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_default_paste_updates_rendered_layer_after_confirmed_hid_write() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        let before = app.application_layout_settings().unwrap().clone();
        app.selected_layer = 0;
        click_layer_menu(&mut app, &ctx, 0);
        assert!(app.layer_clipboard.is_some());
        app.selected_layer = 1;
        click_layer_menu(&mut app, &ctx, 1);
        assert!(app.layer_write_task.is_some());
        drain_layer_write(&mut app, &ctx);
        assert!(
            recorder
                .requests()
                .iter()
                .any(|p| p[..6] == [0x05, 1, 1, 0, 0, 4])
        );
        assert_eq!(
            app.layout.as_ref().unwrap().get_keycode(1, 0),
            4,
            "confirmed firmware map"
        );
        let visible = app.application_layout_rendered_copy(app.layout.as_ref().unwrap());
        assert_eq!(
            visible.get_keycode(1, 0),
            4,
            "Default still renders old assignment after successful paste"
        );
        assert_eq!(visible.get_encoder_keycode(1, 0), 6);
        let key = app.application_layout_device_key().unwrap();
        let reloaded = crate::app::load_app_settings();
        assert_eq!(
            reloaded.application_layouts[&key],
            *app.application_layout_settings().unwrap()
        );
        let after = app.application_layout_settings().unwrap();
        assert_eq!(
            after.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].encoder_stacks,
            before.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                .encoder_stacks
        );
        for (id, profile) in &before.layouts {
            if id != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID {
                assert_eq!(&after.layouts[id], profile);
            }
        }
        assert_eq!(
            after.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layers[0],
            before.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layers[0]
        );
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 4);
        let UndoAction::Layer {
            layer,
            old,
            requires_firmware,
        } = app.undo_stack.pop().unwrap()
        else {
            panic!("layer undo");
        };
        app.undo_layer_snapshot(layer, old, requires_firmware);
        drain_layer_write(&mut app, &ctx);
        let mut restored = app.application_layout_settings().unwrap().clone();
        let id = crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
        assert!(restored.layouts[id].revision > before.layouts[id].revision);
        restored.layouts.get_mut(id).unwrap().revision = before.layouts[id].revision;
        assert_eq!(restored, before);
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(1, 0), 5);
        assert_eq!(
            crate::app::load_app_settings().application_layouts[&key],
            *app.application_layout_settings().unwrap()
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_profile_paste_matrix_persists_without_firmware_or_default_mutation() {
        use crate::application_layouts::{
            DEFAULT_APPLICATION_LAYOUT_ID as DEFAULT, DetectedApplication,
        };
        for source in [DEFAULT, "fixture_app_1", "other_app_2"] {
            for target in ["fixture_app_1", "other_app_2"] {
                let _storage = crate::app::app_storage::ScopedTempConfig::new();
                let (mut app, recorder, ctx) = macropad_fixture();
                let device_key = app.application_layout_device_key().unwrap();
                let settings = app.application_layout_settings_mut().unwrap();
                settings.create_for_application(&DetectedApplication {
                    executable: "other-app".into(),
                    display_name: "Other".into(),
                    ..Default::default()
                });
                assert!(settings.layouts.contains_key("other_app_2"));
                for (index, profile) in settings.layouts.values_mut().enumerate() {
                    profile.layers[0] = [40 + index as u16; 15];
                    profile.layers[1] = [70 + index as u16; 15];
                }
                settings.active_layout_id = source.into();
                settings.editor_layout_id = source.into();
                let before = settings.clone();
                let raw = app.layout.clone().unwrap();
                app.selected_layer = 0;
                click_layer_menu(&mut app, &ctx, 0);
                let desired = app.layer_clipboard.as_ref().unwrap().snapshot.clone();
                let settings = app.application_layout_settings_mut().unwrap();
                settings.active_layout_id = target.into();
                settings.editor_layout_id = target.into();
                app.selected_layer = 1;
                click_layer_menu(&mut app, &ctx, 1);
                drain_layer_write(&mut app, &ctx);
                let visible = app.application_layout_rendered_copy(app.layout.as_ref().unwrap());
                assert_eq!(
                    capture_layer(&visible, 1).unwrap(),
                    desired,
                    "source={source} target={target}"
                );
                assert!(
                    recorder.requests().is_empty(),
                    "profile paste must not write firmware Default"
                );
                assert_eq!(app.layout.as_ref().unwrap().layers, raw.layers);
                assert_eq!(
                    app.layout.as_ref().unwrap().encoder_layers,
                    raw.encoder_layers
                );
                let after = app.application_layout_settings().unwrap();
                assert_eq!(after.layouts[DEFAULT], before.layouts[DEFAULT]);
                for (id, profile) in &before.layouts {
                    if id != target {
                        assert_eq!(after.layouts[id], *profile);
                    }
                }
                assert_eq!(
                    after.layouts[target].encoder_stacks,
                    before.layouts[target].encoder_stacks
                );
                assert_eq!(
                    crate::app::load_app_settings().application_layouts[&device_key],
                    *after
                );
                let (mut restarted, _, _) = macropad_fixture();
                restarted.app_settings = crate::app::load_app_settings();
                let reloaded =
                    restarted.application_layout_rendered_copy(restarted.layout.as_ref().unwrap());
                assert_eq!(capture_layer(&reloaded, 1).unwrap(), desired);
                // Changing the selected profile after Paste cannot redirect Undo.
                app.application_layout_settings_mut()
                    .unwrap()
                    .active_layout_id = DEFAULT.into();
                app.undo(&ctx);
                let after_undo = app.application_layout_settings().unwrap();
                assert_eq!(
                    after_undo.layouts[target].layers,
                    before.layouts[target].layers
                );
                assert_eq!(after_undo.active_layout_id, DEFAULT);
                assert!(recorder.requests().is_empty());
                assert_eq!(
                    crate::app::load_app_settings().application_layouts[&device_key],
                    *after_undo
                );
            }
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_profile_to_default_paste_preserves_source_and_encoders() {
        use crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID as DEFAULT;
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        let settings = app.application_layout_settings_mut().unwrap();
        let source = settings.layouts.get_mut("fixture_app_1").unwrap();
        source.layers[0] = [42; 15];
        let source = source.clone();
        settings.active_layout_id = "fixture_app_1".into();
        app.selected_layer = 0;
        click_layer_menu(&mut app, &ctx, 0);
        let desired = app.layer_clipboard.as_ref().unwrap().snapshot.clone();
        app.application_layout_settings_mut()
            .unwrap()
            .active_layout_id = DEFAULT.into();
        app.selected_layer = 1;
        click_layer_menu(&mut app, &ctx, 1);
        drain_layer_write(&mut app, &ctx);
        let visible = app.application_layout_rendered_copy(app.layout.as_ref().unwrap());
        assert_eq!(capture_layer(&visible, 1).unwrap(), desired);
        assert_eq!(
            app.application_layout_settings().unwrap().layouts["fixture_app_1"],
            source
        );
        assert!(!recorder.requests().is_empty());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_profile_fill_keeps_visible_encoder_bindings() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        let settings = app.application_layout_settings_mut().unwrap();
        settings.active_layout_id = "fixture_app_1".into();
        settings.layouts.get_mut("fixture_app_1").unwrap().layers[0] = [42; 15];
        app.selected_layer = 0;
        click_layer_menu(&mut app, &ctx, 3);
        let visible = app.application_layout_rendered_copy(app.layout.as_ref().unwrap());
        assert_eq!(visible.get_keycode(0, 0), 0);
        assert_eq!(visible.get_encoder_keycode(0, 0), 42);
        assert_eq!(visible.get_encoder_keycode(0, 1), 42);
        assert!(recorder.requests().is_empty());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_layer_copy_captures_visible_profile_not_stale_firmware() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, _, _) = macropad_fixture();
        app.application_layout_settings_mut()
            .unwrap()
            .layouts
            .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .layers[0][0] = 42;
        app.copy_selected_layer();
        assert_eq!(
            app.layer_clipboard.as_ref().unwrap().snapshot.keycodes[0],
            42.into()
        );
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 4);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_partial_layer_paste_preserves_unwritten_controls_on_mismatch_and_failure() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for disconnect in [false, true] {
            let (mut app, recorder, ctx) = macropad_fixture();
            let mut codes = std::collections::BTreeMap::new();
            recorder.respond_by(move |request| match request[0] {
                0x05 if request[2..4] == [1, 1] && disconnect => {
                    Some(Err("injected write failure".into()))
                }
                0x05 => {
                    codes.insert(
                        (request[1], request[2], request[3]),
                        [request[4], request[5]],
                    );
                    Some(Ok(*request))
                }
                0x04 => {
                    let mut reply = *request;
                    reply[4..6]
                        .copy_from_slice(codes.get(&(request[1], request[2], request[3])).unwrap());
                    if request[2..4] == [1, 1] {
                        reply[4..6].copy_from_slice(&99u16.to_be_bytes());
                    }
                    Some(Ok(reply))
                }
                _ => None,
            });
            app.copy_selected_layer();
            app.selected_layer = 1;
            app.paste_selected_layer(app.layer_paste_snapshot().unwrap());
            drain_layer_write(&mut app, &ctx);
            let expected_second = if disconnect { 5 } else { 99 };
            let visible = app.application_layout_rendered_copy(app.layout.as_ref().unwrap());
            assert_eq!(visible.get_keycode(1, 0), 4);
            assert_eq!(visible.get_keycode(1, 1), expected_second);
            assert_eq!(visible.get_keycode(1, 2), 5);
            assert_eq!(visible.get_encoder_keycode(1, 0), 8);
            assert_eq!(
                app.layout.as_ref().unwrap().get_keycode(1, 1),
                expected_second
            );
            assert_eq!(app.hid_device.is_none(), disconnect);
            assert!(
                !recorder
                    .requests()
                    .iter()
                    .any(|r| r[0] == 0x05 && r[2..4] == [0, 2])
            );
            let key = app.application_layout_device_key().unwrap();
            assert_eq!(
                crate::app::load_app_settings().application_layouts[&key],
                *app.application_layout_settings().unwrap()
            );
            let UndoAction::Layer {
                layer,
                old,
                requires_firmware,
            } = app.undo_stack.pop().unwrap()
            else {
                panic!("partial write undo");
            };
            assert_eq!(layer, 1);
            assert_eq!(old.keycodes, bindings(&[5; 13]));
            assert!(requires_firmware);
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_layer_completion_preserves_later_pending_edit_and_profile_selection() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, _, ctx) = macropad_fixture();
        app.default_layout_pending_layers = 1 << 1;
        app.copy_selected_layer();
        app.selected_layer = 1;
        app.paste_selected_layer(app.layer_paste_snapshot().unwrap());
        // The completion is not polled until the following edit and selection.
        let settings = app.application_layout_settings_mut().unwrap();
        settings
            .layouts
            .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .set_keycode(1, 0, 42);
        settings.active_layout_id = "fixture_app_1".into();
        settings.editor_layout_id = "fixture_app_1".into();
        let other = settings.layouts["fixture_app_1"].clone();
        drain_layer_write(&mut app, &ctx);
        let settings = app.application_layout_settings().unwrap();
        assert_eq!(
            settings.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layers[1]
                [0],
            42
        );
        assert_eq!(
            settings.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layers[1]
                [1],
            4
        );
        assert_eq!(settings.layouts["fixture_app_1"], other);
        assert_eq!(settings.active_layout_id, "fixture_app_1");
        assert_eq!(app.default_layout_pending_layers, 1 << 1);
        let key = app.application_layout_device_key().unwrap();
        assert_eq!(
            crate::app::load_app_settings().application_layouts[&key],
            *settings
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_noop_firmware_paste_reconciles_stale_default_and_undo() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        app.layout.as_mut().unwrap().layers[1] = bindings(&[4; 13]);
        app.layout.as_mut().unwrap().encoder_layers[1] = vec![6, 7];
        app.copy_selected_layer();
        app.selected_layer = 1;
        app.paste_selected_layer(app.layer_paste_snapshot().unwrap());
        assert!(app.layer_write_task.is_none());
        assert!(recorder.requests().is_empty());
        assert_eq!(
            app.application_layout_settings().unwrap().layouts
                [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                .layers[1][0],
            4
        );
        let UndoAction::Layer {
            layer,
            old,
            requires_firmware,
        } = app.undo_stack.pop().unwrap()
        else {
            panic!("host change undo");
        };
        app.undo_layer_snapshot(layer, old, requires_firmware);
        drain_layer_write(&mut app, &ctx);
        assert_eq!(
            app.application_layout_settings().unwrap().layouts
                [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                .layers[1][0],
            5
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn non_macropad_layer_paste_keeps_raw_path_and_profiles_unchanged() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, _, ctx) = macropad_fixture();
        let mut device = app.device_manager.devices()[0].clone();
        device.product_id = 0x0074;
        device.name = "K:04".into();
        app.device_manager.replace_devices(vec![device]);
        assert!(!app.application_layouts_supported());
        let before = app.app_settings.application_layouts.clone();
        app.copy_selected_layer();
        app.selected_layer = 1;
        app.paste_selected_layer(app.layer_paste_snapshot().unwrap());
        drain_layer_write(&mut app, &ctx);
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(1, 0), 4);
        assert_eq!(app.app_settings.application_layouts, before);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_layer_paste_survives_native_background_reconciliation() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        // Keep desktop foreground discovery from routing this Default paste to
        // an installed application's profile during the native background tick.
        let settings = app.application_layout_settings_mut().unwrap();
        settings.automatic_switching_enabled = false;
        settings.active_layout_id = crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.into();
        // Make every non-target binding explicit: runtime UNSET seeding is not
        // a layer-paste mutation. Allow normal preset migration before snapshot.
        for profile in app
            .application_layout_settings_mut()
            .unwrap()
            .layouts
            .values_mut()
        {
            for codes in &mut profile.layers {
                for code in codes {
                    if *code == u16::MAX {
                        *code = 0;
                    }
                }
            }
        }
        app.tick_layout_import_lifecycle_for_test(&ctx, 0.1);
        let before = app.application_layout_settings().unwrap().clone();
        app.selected_layer = 0;
        click_layer_menu(&mut app, &ctx, 0);
        assert!(app.layer_clipboard.is_some(), "Copy menu action did not run");
        app.selected_layer = 1;
        click_layer_menu(&mut app, &ctx, 1);
        assert!(
            app.layer_write_task.is_some(),
            "Paste must start a firmware write"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.layer_write_task.is_some() {
            app.tick_layout_import_lifecycle_for_test(&ctx, 0.2);
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.tick_layout_import_lifecycle_for_test(&ctx, 0.3);
        let after = app.application_layout_settings().unwrap();
        let id = crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
        assert_eq!(after.layouts[id].layers[1], before.layouts[id].layers[0]);
        assert_eq!(after.layouts[id].layers[0], before.layouts[id].layers[0]);
        for (profile_id, profile) in &before.layouts {
            if profile_id != id {
                assert_eq!(&after.layouts[profile_id], profile);
            }
        }
        assert!(
            app.layer_write_task.is_none(),
            "background did not replay stale Default"
        );
        assert_eq!(
            app.application_layout_rendered_copy(app.layout.as_ref().unwrap())
                .get_keycode(1, 0),
            4
        );
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(1, 0), 4);
        assert!(
            recorder
                .requests()
                .iter()
                .any(|r| r[..6] == [5, 1, 1, 0, 0, 4])
        );
        let key = app.application_layout_device_key().unwrap();
        assert_eq!(
            crate::app::load_app_settings().application_layouts[&key],
            *after
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn macropad_default_sync_mismatch_keeps_pending_desired_profile() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        app.default_layout_pending_layers = 1 << 1;
        let desired = capture_layer(app.layout.as_ref().unwrap(), 0).unwrap();
        let profile = app
            .application_layout_settings_mut()
            .unwrap()
            .layouts
            .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap();
        profile.layers[1] = profile.layers[0];
        let before = profile.clone();
        recorder.respond_by(|request| match request[0] {
            5 => Some(Ok(*request)),
            4 => {
                let mut reply = *request;
                reply[4..6].copy_from_slice(&99u16.to_be_bytes());
                Some(Ok(reply))
            }
            _ => None,
        });
        app.apply_layer_snapshot(1, desired, "layer_actions.save_default_to_device");
        drain_layer_write(&mut app, &ctx);
        assert_eq!(
            app.application_layout_settings().unwrap().layouts
                [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID],
            before
        );
        assert_eq!(app.default_layout_pending_layers, 1 << 1);
        assert!(app.default_layout_sync_retry_after.is_some());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(1, 0), 99);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn k04_native_layer_paste_keeps_rmk_actions_and_profiles_unchanged() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, ctx) = macropad_fixture();
        let mut device = app.device_manager.devices()[0].clone();
        device.product_id = 0x0074;
        device.name = "K:04".into();
        app.device_manager.replace_devices(vec![device]);
        let action = rmk_types::action::KeyAction::Single(rmk_types::action::Action::LayerOn(2));
        let board = app.layout.as_mut().unwrap();
        board.layers[0] = vec![KeyBinding::Rmk(action); 13];
        board.encoder_layers[1] = board.encoder_layers[0].clone();
        let before = app.app_settings.application_layouts.clone();
        let mut native_codes = std::collections::BTreeMap::new();
        recorder.respond_by(move |request| {
            if request[1..4] != [0xe8, 3, 1] {
                return None;
            }
            let mut reply = *request;
            match request[0] {
                7 => {
                    native_codes.insert(
                        (request[4], request[5], request[6]),
                        request[8..8 + request[7] as usize].to_vec(),
                    );
                    reply[4] = 0;
                }
                8 => {
                    let payload = &native_codes[&(request[4], request[5], request[6])];
                    reply[4] = 0;
                    reply[5] = payload.len() as u8;
                    reply[6..6 + payload.len()].copy_from_slice(payload);
                }
                _ => return None,
            }
            Some(Ok(reply))
        });
        app.copy_selected_layer();
        assert_eq!(
            app.layer_clipboard.as_ref().unwrap().snapshot.keycodes,
            vec![KeyBinding::Rmk(action); 13]
        );
        app.selected_layer = 1;
        app.paste_selected_layer(app.layer_paste_snapshot().unwrap());
        drain_layer_write(&mut app, &ctx);
        assert!(app.hid_device.is_some());
        assert_eq!(
            app.layout.as_ref().unwrap().layers[1],
            vec![KeyBinding::Rmk(action); 13]
        );
        assert_eq!(recorder.requests().iter().filter(|r| r[0] == 7).count(), 13);
        assert_eq!(recorder.requests().iter().filter(|r| r[0] == 8).count(), 13);
        assert_eq!(app.app_settings.application_layouts, before);
    }

    #[test]
    fn captures_whole_layer_for_same_keyboard_paste() {
        let source = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
            &[10, 11, 12, 13],
            &[20, 21],
        );
        let target = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
            &[1, 1, 1, 1],
            &[2, 2],
        );

        let copied = capture_clipboard(&source, 0).expect("source layer");
        let pasted = layer_snapshot_for_paste(&copied, &target, 0).expect("compatible layer");

        assert_eq!(pasted.keycodes, bindings(&[10, 11, 12, 13]));
        assert_eq!(pasted.encoder_keycodes, vec![20, 21]);
    }

    #[test]
    fn paste_maps_keycodes_by_geometry_when_storage_order_differs() {
        let source = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
            &[10, 11, 12, 13],
            &[],
        );
        let target = layout(
            &[(5.0, 0.0), (0.0, 0.0), (4.0, 0.0), (1.0, 0.0)],
            &[1, 1, 1, 1],
            &[],
        );

        let copied = capture_clipboard(&source, 0).expect("source layer");
        let pasted = layer_snapshot_for_paste(&copied, &target, 0).expect("compatible geometry");

        assert_eq!(pasted.keycodes, bindings(&[13, 10, 12, 11]));
    }

    #[test]
    fn paste_rejects_equal_key_count_with_different_geometry() {
        let source = layout(
            &[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)],
            &[10, 11, 12, 13],
            &[],
        );
        let target = layout(
            &[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)],
            &[1, 1, 1, 1],
            &[],
        );

        let copied = capture_clipboard(&source, 0).expect("source layer");

        assert_eq!(
            layer_snapshot_for_paste(&copied, &target, 0),
            Err(LayerOperationError::IncompatibleGeometry)
        );
    }

    #[test]
    fn paste_requires_equal_key_count_but_preserves_incompatible_encoders() {
        let source = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
            &[10, 11, 12, 13],
            &[20],
        );
        let target = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
            &[1, 1, 1, 1],
            &[30, 31],
        );
        let copied = capture_clipboard(&source, 0).expect("source layer");

        let pasted = layer_snapshot_for_paste(&copied, &target, 0).expect("same key count");

        assert_eq!(pasted.keycodes, bindings(&[10, 11, 12, 13]));
        assert_eq!(pasted.encoder_keycodes, vec![30, 31]);

        let different_target = layout(
            &[
                (0.0, 0.0),
                (1.0, 0.0),
                (2.0, 0.0),
                (4.0, 0.0),
                (5.0, 0.0),
                (6.0, 0.0),
            ],
            &[1; 6],
            &[],
        );
        assert_eq!(
            layer_snapshot_for_paste(&copied, &different_target, 0),
            Err(LayerOperationError::KeyCountMismatch {
                source: 4,
                target: 6,
            })
        );
    }

    #[test]
    fn fill_changes_every_key_but_preserves_encoders() {
        let target = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
            &[10, 11, 12, 13],
            &[20, 21],
        );

        let none = filled_layer_snapshot(&target, 0, 0x0000).expect("layer");
        let inherit = filled_layer_snapshot(&target, 0, 0x0001).expect("layer");

        assert_eq!(none.keycodes, bindings(&[0x0000; 4]));
        assert_eq!(inherit.keycodes, bindings(&[0x0001; 4]));
        assert_eq!(none.encoder_keycodes, vec![20, 21]);
        assert_eq!(inherit.encoder_keycodes, vec![20, 21]);
    }

    #[test]
    fn fill_updates_stay_on_the_selected_raw_layer() {
        let mut target = layout(&[(0.0, 0.0), (1.0, 0.0)], &[10, 11], &[20]);
        target.layers = vec![
            bindings(&[10, 11]),
            bindings(&[12, 13]),
            bindings(&[14, 15]),
        ];
        target.encoder_layers = vec![vec![20], vec![21], vec![22]];

        apply_layer_updates(
            &mut target,
            1,
            &[(0, 0x0001.into()), (1, 0x0001.into())],
            &[],
        );

        assert_eq!(target.layers[0], bindings(&[10, 11]));
        assert_eq!(target.layers[1], bindings(&[0x0001, 0x0001]));
        assert_eq!(target.layers[2], bindings(&[14, 15]));
        assert_eq!(target.encoder_layers, vec![vec![20], vec![21], vec![22]]);
    }

    #[test]
    fn mirror_preserves_rows_and_swaps_handed_modifiers() {
        let target = layout(
            &[
                (0.0, 0.0),
                (1.0, 0.0),
                (4.0, 0.0),
                (5.0, 0.0),
                (0.0, 1.0),
                (1.0, 1.0),
                (4.0, 1.0),
                (5.0, 1.0),
            ],
            &[0x00E0, 11, 12, 0x00E6, 20, 21, 22, 23],
            &[30],
        );

        let mirrored = mirrored_layer_snapshot(&target, 0).expect("balanced layout");

        assert_eq!(
            mirrored.keycodes,
            bindings(&[0x00E2, 12, 11, 0x00E4, 23, 22, 21, 20])
        );
        assert_eq!(mirrored.encoder_keycodes, vec![30]);
    }

    #[test]
    fn mirror_rejects_keyboards_without_equal_halves() {
        let target = layout(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)], &[10, 11, 12], &[]);

        assert_eq!(
            mirrored_layer_snapshot(&target, 0),
            Err(LayerOperationError::UnevenKeyCount { count: 3 })
        );
    }

    #[test]
    fn mirror_handles_symmetric_mixed_width_rotated_keys_and_is_involutive() {
        let mut target = layout(
            &[(0.0, 0.0), (1.5, 1.0), (5.0, 1.0), (7.0, 0.0)],
            &[0x00E0, 11, 12, 0x00E6],
            &[],
        );
        for (key, (width, rotation)) in
            target
                .keys
                .iter_mut()
                .zip([(1.0, -6.0), (1.5, 8.0), (1.5, -8.0), (1.0, 6.0)])
        {
            key.w = width;
            key.rotation = rotation;
            key.rotation_x = key.x + width * 0.5;
            key.rotation_y = key.y + key.h * 0.5;
        }

        let mapping = mirror_key_mapping(&target.keys).expect("symmetric geometry");
        assert_eq!(mapping, vec![3, 2, 1, 0]);

        let original = target.layers[0].clone();
        let once = mirrored_layer_snapshot(&target, 0).expect("first mirror");
        target.layers[0] = once.keycodes;
        let twice = mirrored_layer_snapshot(&target, 0).expect("second mirror");
        assert_eq!(twice.keycodes, original);
    }

    #[test]
    fn mirror_rejects_even_but_asymmetric_geometry() {
        let target = layout(
            &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0), (8.0, 0.0)],
            &[10, 11, 12, 13],
            &[],
        );

        assert_eq!(
            mirrored_layer_snapshot(&target, 0),
            Err(LayerOperationError::UnbalancedGeometry)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layer_write_runner_records_successful_keys_and_encoders() {
        let keys = vec![(0, 0, 0, 10.into()), (1, 0, 1, 11.into())];
        let encoders = vec![(0, 0, 1, 20)];

        let progress = run_layer_writes(
            &keys,
            &encoders,
            |_, _, _| KeyWriteOutcome::Saved,
            |_, _, _| Ok(()),
        );

        assert_eq!(
            progress,
            LayerWriteProgress {
                key_updates: vec![(0, 10.into()), (1, 11.into())],
                encoder_updates: vec![(0, 20)],
                written: 3,
                error: None,
                disconnect: false,
            }
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layer_write_runner_stops_after_disconnect_and_skips_encoders() {
        let keys = vec![
            (0, 0, 0, 10.into()),
            (1, 0, 1, 11.into()),
            (2, 0, 2, 12.into()),
        ];
        let encoders = vec![(0, 0, 1, 20)];
        let mut encoder_calls = 0;

        let progress = run_layer_writes(
            &keys,
            &encoders,
            |_, _, keycode| {
                if keycode == 11.into() {
                    KeyWriteOutcome::Failed("device disconnected".into())
                } else {
                    KeyWriteOutcome::Saved
                }
            },
            |_, _, _| {
                encoder_calls += 1;
                Ok(())
            },
        );

        assert_eq!(progress.key_updates, vec![(0, 10.into())]);
        assert!(progress.encoder_updates.is_empty());
        assert_eq!(progress.written, 1);
        assert_eq!(progress.error.as_deref(), Some("device disconnected"));
        assert!(progress.disconnect);
        assert_eq!(encoder_calls, 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layer_write_runner_reconciles_readback_mismatch_without_disconnect() {
        let keys = vec![(0, 0, 0, 10.into())];

        let progress = run_layer_writes(
            &keys,
            &[],
            |_, _, _| KeyWriteOutcome::Mismatch {
                readback: 99.into(),
                error: "writeback mismatch".into(),
            },
            |_, _, _| Ok(()),
        );

        assert_eq!(progress.key_updates, vec![(0, 99.into())]);
        assert_eq!(progress.written, 0);
        assert_eq!(progress.error.as_deref(), Some("writeback mismatch"));
        assert!(!progress.disconnect);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layer_write_runner_keeps_encoder_successes_before_failure() {
        let encoders = vec![(0, 0, 0, 20), (1, 1, 1, 21)];

        let progress = run_layer_writes(
            &[],
            &encoders,
            |_, _, _| KeyWriteOutcome::Saved,
            |_, _, keycode| {
                if keycode == 21 {
                    Err("encoder write failed".into())
                } else {
                    Ok(())
                }
            },
        );

        assert_eq!(progress.encoder_updates, vec![(0, 20)]);
        assert_eq!(progress.written, 1);
        assert_eq!(progress.error.as_deref(), Some("encoder write failed"));
        assert!(progress.disconnect);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failed_layer_undo_requeues_desired_snapshot() {
        let old = LayerSnapshot {
            keycodes: bindings(&[1, 2]),
            encoder_keycodes: vec![],
        };
        let desired = LayerSnapshot {
            keycodes: bindings(&[3, 4]),
            encoder_keycodes: vec![],
        };
        let context = LayerWriteContext {
            layer: 0,
            old,
            desired: desired.clone(),
            action: "Undo".into(),
            total: 2,
            undo_behavior: LayerUndoBehavior::RetryDesired {
                requires_firmware: true,
            },
            is_default_layout_sync: false,
            default_target: None,
        };

        assert_eq!(
            undo_after_layer_write(&context, true, true),
            Some((desired, true))
        );
        assert_eq!(undo_after_layer_write(&context, true, false), None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn partial_normal_write_records_old_snapshot_for_undo() {
        let old = LayerSnapshot {
            keycodes: bindings(&[1, 2]),
            encoder_keycodes: vec![],
        };
        let context = LayerWriteContext {
            layer: 0,
            old: old.clone(),
            desired: LayerSnapshot {
                keycodes: bindings(&[3, 4]),
                encoder_keycodes: vec![],
            },
            action: "Paste".into(),
            total: 2,
            undo_behavior: LayerUndoBehavior::RecordOld,
            is_default_layout_sync: false,
            default_target: None,
        };

        assert_eq!(
            undo_after_layer_write(&context, true, true),
            Some((old, true))
        );
        assert_eq!(undo_after_layer_write(&context, false, true), None);
    }
}
