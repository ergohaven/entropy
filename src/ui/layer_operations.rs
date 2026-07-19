use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LayerSnapshot {
    pub(super) keycodes: Vec<u16>,
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
            .map(|key_idx| layout.get_keycode(layer, key_idx))
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
    snapshot.keycodes.fill(keycode);
    Some(snapshot)
}

fn apply_layer_updates(
    layout: &mut KeyboardLayout,
    layer: usize,
    key_updates: &[(usize, u16)],
    encoder_updates: &[(usize, u16)],
) {
    for &(key_idx, keycode) in key_updates {
        layout.set_keycode(layer, key_idx, keycode);
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
        .map(|&source_idx| {
            let keycode = current.keycodes[source_idx];
            toggle_handed_modifier(keycode).unwrap_or(keycode)
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

#[cfg(not(target_arch = "wasm32"))]
type KeyChange = (usize, u8, u8, u16);

#[cfg(not(target_arch = "wasm32"))]
type EncoderChange = (usize, u8, u8, u16);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LayerUndoBehavior {
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
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct LayerWriteTask {
    receiver: std::sync::mpsc::Receiver<LayerWriteResult>,
    fallback: LayerWriteFallback,
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
    key_updates: Vec<(usize, u16)>,
    encoder_updates: Vec<(usize, u16)>,
    written: usize,
    error: Option<String>,
    disconnect: bool,
}

#[cfg(not(target_arch = "wasm32"))]
enum KeyWriteOutcome {
    Saved,
    Mismatch { readback: u16, error: String },
    Failed(String),
}

#[cfg(not(target_arch = "wasm32"))]
fn run_layer_writes(
    key_changes: &[KeyChange],
    encoder_changes: &[EncoderChange],
    mut write_key: impl FnMut(u8, u8, u16) -> KeyWriteOutcome,
    mut write_encoder: impl FnMut(u8, u8, u16) -> Result<(), String>,
) -> LayerWriteProgress {
    let mut progress = LayerWriteProgress {
        key_updates: Vec::with_capacity(key_changes.len()),
        encoder_updates: Vec::with_capacity(encoder_changes.len()),
        written: 0,
        error: None,
        disconnect: false,
    };

    for &(key_idx, row, col, keycode) in key_changes {
        match write_key(row, col, keycode) {
            KeyWriteOutcome::Saved => {
                progress.key_updates.push((key_idx, keycode));
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

    fn fill_selected_layer(&mut self, keycode: u16) {
        let target_layer = self.selected_layer;
        let desired = self
            .layout
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
            .layout
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

    pub(super) fn draw_layer_actions_menu(&mut self, ui: &mut egui::Ui, center: egui::Pos2) {
        let language = self.app_settings.language;
        let popup_id = ui.make_persistent_id("layout_layer_actions_popup");
        let button_rect = egui::Rect::from_center_size(center, egui::vec2(34.0, 36.0));
        let button_response = ui.interact(
            button_rect,
            ui.make_persistent_id("layout_layer_actions_button"),
            Sense::click(),
        );
        if button_response.clicked() {
            egui::Popup::toggle_id(ui.ctx(), popup_id);
        }
        if button_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        if button_response.hovered() || popup_open {
            ui.painter()
                .rect_filled(button_rect, 8.0, app_hover_fill(self.dark_mode));
        }
        ui.painter().text(
            button_rect.center() + egui::vec2(0.0, -3.0),
            egui::Align2::CENTER_CENTER,
            "⋯",
            FontId::proportional(24.0),
            if button_response.hovered() || popup_open {
                app_accent()
            } else {
                app_muted_text(self.dark_mode)
            },
        );
        let button_response = button_response.on_hover_text(crate::i18n::tr_catalog(
            language,
            "layer_actions.menu_tooltip",
        ));

        if !popup_open {
            return;
        }

        let copy_label = crate::i18n::tr_catalog(language, "layer_actions.copy").to_owned();
        let paste_label = crate::i18n::tr_catalog(language, "layer_actions.paste").to_owned();
        let mirror_label = crate::i18n::tr_catalog(language, "layer_actions.mirror").to_owned();
        let fill_none_label =
            crate::i18n::tr_catalog(language, "layer_actions.fill_none").to_owned();
        let fill_inherit_label =
            crate::i18n::tr_catalog(language, "layer_actions.fill_inherit").to_owned();
        let menu_width = adaptive_top_dropdown_width(
            ui,
            [
                copy_label.as_str(),
                paste_label.as_str(),
                mirror_label.as_str(),
                fill_none_label.as_str(),
                fill_inherit_label.as_str(),
            ],
            210.0,
        );
        let paste_snapshot = self.layer_paste_snapshot();
        let mirror_mapping = self
            .layout
            .as_ref()
            .ok_or(LayerOperationError::MissingLayer)
            .and_then(|layout| mirror_key_mapping(&layout.keys));
        let mut requested_action = None;

        ui.style_mut().visuals.window_stroke =
            crate::ui_style::modal_outline_stroke(self.dark_mode);
        ui.style_mut().visuals.window_fill = app_surface_fill(self.dark_mode);
        crate::ui_style::popup_below_widget(
            ui,
            popup_id,
            &button_response,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(menu_width);
                ui.set_max_width(menu_width);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);

                let copy_response = top_dropdown_item(ui, menu_width, &copy_label, true, false);
                let copy_clicked = copy_response.clicked();
                copy_response.on_hover_text(crate::i18n::tr_catalog(
                    language,
                    "layer_actions.copy_tooltip",
                ));
                if copy_clicked {
                    requested_action = Some(LayerUiAction::Copy);
                }

                let paste_enabled = paste_snapshot.is_ok();
                let paste_response =
                    top_dropdown_item(ui, menu_width, &paste_label, paste_enabled, false);
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

                ui.separator();

                let mirror_enabled = mirror_mapping.is_ok();
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

                ui.separator();

                let none_response =
                    top_dropdown_item(ui, menu_width, &fill_none_label, true, false);
                let none_clicked = none_response.clicked();
                none_response.on_hover_text(crate::i18n::tr_catalog(
                    language,
                    "layer_actions.fill_none_tooltip",
                ));
                if none_clicked {
                    requested_action = Some(LayerUiAction::FillNone);
                }

                let inherit_response =
                    top_dropdown_item(ui, menu_width, &fill_inherit_label, true, false);
                let inherit_clicked = inherit_response.clicked();
                inherit_response.on_hover_text(crate::i18n::tr_catalog(
                    language,
                    "layer_actions.fill_inherit_tooltip",
                ));
                if inherit_clicked {
                    requested_action = Some(LayerUiAction::FillInherit);
                }

                if requested_action.is_some() {
                    egui::Popup::close_all(ui.ctx());
                }
            },
        );

        match requested_action {
            Some(LayerUiAction::Copy) => self.copy_selected_layer(),
            Some(LayerUiAction::Paste(desired)) => self.paste_selected_layer(desired),
            Some(LayerUiAction::Mirror(mapping)) => self.mirror_selected_layer(&mapping),
            Some(LayerUiAction::FillNone) => self.fill_selected_layer(0x0000),
            Some(LayerUiAction::FillInherit) => self.fill_selected_layer(0x0001),
            None => {}
        }
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

    pub(super) fn apply_layer_snapshot(
        &mut self,
        layer: usize,
        desired: LayerSnapshot,
        action_key: &'static str,
    ) {
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
    pub(super) fn apply_layer_snapshot_with_behavior(
        &mut self,
        layer: usize,
        desired: LayerSnapshot,
        action_key: &'static str,
        undo_behavior: LayerUndoBehavior,
    ) {
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
        let total = key_changes.len() + encoder_changes.len();
        let action = crate::i18n::tr_catalog(self.app_settings.language, action_key).to_owned();
        let layer_text = layer.to_string();
        if total == 0 {
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
                    old,
                    requires_firmware: false,
                });
            }
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
            old,
            desired,
            action,
            total,
            undo_behavior,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let layer_u8 = layer as u8;
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = crate::hid::macos_hid_operation_lock();

            let progress = run_layer_writes(
                &key_changes,
                &encoder_changes,
                |row, col, keycode| match hid_device.set_keycode(layer_u8, row, col, keycode) {
                    Ok(()) => KeyWriteOutcome::Saved,
                    Err(error) => match crate::hid::keycode_writeback_readback(&error) {
                        Some(readback) => KeyWriteOutcome::Mismatch {
                            readback,
                            error: error.to_string(),
                        },
                        None => KeyWriteOutcome::Failed(error.to_string()),
                    },
                },
                |device_idx, direction, keycode| {
                    hid_device
                        .set_encoder(layer_u8, device_idx, direction, keycode)
                        .map_err(|error| error.to_string())
                },
            );
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
                let status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "layer_actions.write_task_failed",
                    &[
                        ("action", &task.fallback.action),
                        ("layer", &task.fallback.layer.to_string()),
                    ],
                );
                self.pending_layer_write = Some(DeferredLayerWrite {
                    layer: task.fallback.layer,
                    keycodes: task.fallback.desired.keycodes.clone(),
                    encoder_keycodes: task.fallback.desired.encoder_keycodes.clone(),
                });
                self.handoff_hid_worker_disconnect(status_msg);
                if let LayerUndoBehavior::RetryDesired { requires_firmware } =
                    task.fallback.undo_behavior
                {
                    self.undo_stack.push(UndoAction::Layer {
                        layer: task.fallback.layer,
                        old: task.fallback.desired,
                        requires_firmware,
                    });
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_layer_write(&mut self, result: LayerWriteResult) {
        if result.hid_device.is_none() {
            self.pending_layer_write = Some(DeferredLayerWrite {
                layer: result.context.layer,
                keycodes: result.context.desired.keycodes.clone(),
                encoder_keycodes: result.context.desired.encoder_keycodes.clone(),
            });
            self.handoff_hid_worker_disconnect("Layer write disconnected");
            return;
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

        let touched =
            !result.progress.key_updates.is_empty() || !result.progress.encoder_updates.is_empty();
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
            for (key_idx, keycode) in desired.keycodes.into_iter().enumerate() {
                layout.set_keycode(layer, key_idx, keycode);
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
            layers: vec![keycodes.to_vec()],
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

        assert_eq!(pasted.keycodes, vec![10, 11, 12, 13]);
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

        assert_eq!(pasted.keycodes, vec![13, 10, 12, 11]);
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

        assert_eq!(pasted.keycodes, vec![10, 11, 12, 13]);
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

        assert_eq!(none.keycodes, vec![0x0000; 4]);
        assert_eq!(inherit.keycodes, vec![0x0001; 4]);
        assert_eq!(none.encoder_keycodes, vec![20, 21]);
        assert_eq!(inherit.encoder_keycodes, vec![20, 21]);
    }

    #[test]
    fn fill_updates_stay_on_the_selected_raw_layer() {
        let mut target = layout(&[(0.0, 0.0), (1.0, 0.0)], &[10, 11], &[20]);
        target.layers = vec![vec![10, 11], vec![12, 13], vec![14, 15]];
        target.encoder_layers = vec![vec![20], vec![21], vec![22]];

        apply_layer_updates(&mut target, 1, &[(0, 0x0001), (1, 0x0001)], &[]);

        assert_eq!(target.layers[0], vec![10, 11]);
        assert_eq!(target.layers[1], vec![0x0001, 0x0001]);
        assert_eq!(target.layers[2], vec![14, 15]);
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
            vec![0x00E2, 12, 11, 0x00E4, 23, 22, 21, 20]
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
        let keys = vec![(0, 0, 0, 10), (1, 0, 1, 11)];
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
                key_updates: vec![(0, 10), (1, 11)],
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
        let keys = vec![(0, 0, 0, 10), (1, 0, 1, 11), (2, 0, 2, 12)];
        let encoders = vec![(0, 0, 1, 20)];
        let mut encoder_calls = 0;

        let progress = run_layer_writes(
            &keys,
            &encoders,
            |_, _, keycode| {
                if keycode == 11 {
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

        assert_eq!(progress.key_updates, vec![(0, 10)]);
        assert!(progress.encoder_updates.is_empty());
        assert_eq!(progress.written, 1);
        assert_eq!(progress.error.as_deref(), Some("device disconnected"));
        assert!(progress.disconnect);
        assert_eq!(encoder_calls, 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layer_disconnect_reconnect_replays_only_remaining_keys_and_encoders() {
        // Each key is a set/readback pair; fault after two requests interrupts
        // after key 0, while fault after four interrupts before encoder 0.
        for (fault_after, fresh_keys, expected_requests) in
            [(2, vec![30, 2], 3), (4, vec![30, 31], 1)]
        {
            let ctx = egui::Context::default();
            let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new(&creation_context);
            let about = DeviceAboutInfo {
                path: format!("usb:test-{fault_after}"),
                keyboard_id: 42,
                ..Default::default()
            };
            app.device_about_info = Some(about.clone());
            app.layout = Some(layout(&[(0.0, 0.0), (1.0, 0.0)], &[1, 2], &[3]));
            let (hid, _) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
                fault_after,
                crate::hid::TestHidFault::Disconnect,
            )));
            app.hid_device = Some(hid);
            app.apply_layer_snapshot(
                0,
                LayerSnapshot {
                    keycodes: vec![30, 31],
                    encoder_keycodes: vec![32],
                },
                "layer_actions.paste",
            );
            for _ in 0..100 {
                app.poll_layer_write(&ctx);
                if app.layer_write_task.is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(app.deferred_hid_settings.len(), 1);

            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.device_about_info = Some(about.clone());
            app.layout = Some(layout(&[(0.0, 0.0), (1.0, 0.0)], &fresh_keys, &[3]));
            app.hid_device = Some(hid);
            app.restore_deferred_hid_settings_after_connect(&about);
            for _ in 0..100 {
                app.poll_layer_write(&ctx);
                if app.layer_write_task.is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }

            let layout = app.layout.as_ref().expect("reconnected layout");
            assert_eq!(layout.layers[0], vec![30, 31]);
            assert_eq!(layout.encoder_layers[0], vec![32]);
            assert_eq!(recorder.requests().len(), expected_requests);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layer_write_runner_reconciles_readback_mismatch_without_disconnect() {
        let keys = vec![(0, 0, 0, 10)];

        let progress = run_layer_writes(
            &keys,
            &[],
            |_, _, _| KeyWriteOutcome::Mismatch {
                readback: 99,
                error: "writeback mismatch".into(),
            },
            |_, _, _| Ok(()),
        );

        assert_eq!(progress.key_updates, vec![(0, 99)]);
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
            keycodes: vec![1, 2],
            encoder_keycodes: vec![],
        };
        let desired = LayerSnapshot {
            keycodes: vec![3, 4],
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
            keycodes: vec![1, 2],
            encoder_keycodes: vec![],
        };
        let context = LayerWriteContext {
            layer: 0,
            old: old.clone(),
            desired: LayerSnapshot {
                keycodes: vec![3, 4],
                encoder_keycodes: vec![],
            },
            action: "Paste".into(),
            total: 2,
            undo_behavior: LayerUndoBehavior::RecordOld,
        };

        assert_eq!(
            undo_after_layer_write(&context, true, true),
            Some((old, true))
        );
        assert_eq!(undo_after_layer_write(&context, false, true), None);
    }
}
