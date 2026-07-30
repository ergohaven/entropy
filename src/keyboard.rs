use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::firmware::FirmwareProtocol;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyBinding {
    Vial(u16),
    Rmk(rmk_types::action::KeyAction),
}

impl Default for KeyBinding {
    fn default() -> Self {
        Self::Vial(0)
    }
}

impl From<u16> for KeyBinding {
    fn from(value: u16) -> Self {
        Self::Vial(value)
    }
}

impl From<rmk_types::action::KeyAction> for KeyBinding {
    fn from(action: rmk_types::action::KeyAction) -> Self {
        Self::Rmk(action)
    }
}

impl KeyBinding {
    pub fn vial_keycode(self) -> u16 {
        match self {
            Self::Vial(value) => value,
            Self::Rmk(_) => 0,
        }
    }

    pub fn rmk_action(self) -> Option<rmk_types::action::KeyAction> {
        match self {
            Self::Vial(_) => None,
            Self::Rmk(action) => Some(action),
        }
    }

    pub fn is_no(self) -> bool {
        match self {
            Self::Vial(value) => value == 0,
            Self::Rmk(action) => matches!(action, rmk_types::action::KeyAction::No),
        }
    }

    pub fn is_transparent(self) -> bool {
        matches!(self, Self::Vial(0x0001))
    }
}

/// A physical key on the keyboard with position and matrix mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalKey {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub row: u8,
    pub col: u8,
    pub label: String,
    /// Rotation angle in degrees (for drawing key shape)
    pub rotation: f32,
    /// Rotation anchor X (in KLE units)
    pub rotation_x: f32,
    /// Rotation anchor Y (in KLE units)
    pub rotation_y: f32,
    /// Optional Vial layout-display condition from KLE label position 8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_condition: Option<LayoutCondition>,
}

/// A visual encoder slot on the keyboard layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalEncoder {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
    pub encoder_idx: u8,
    pub direction: u8,
    /// Rotation angle in degrees (for drawing key shape)
    pub rotation: f32,
    /// Rotation anchor X (in KLE units)
    pub rotation_x: f32,
    /// Rotation anchor Y (in KLE units)
    pub rotation_y: f32,
    /// Optional Vial layout-display condition from KLE label position 8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_condition: Option<LayoutCondition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutCondition {
    pub option_idx: usize,
    pub value: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomKeycode {
    pub name: String,
    pub label: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutOption {
    pub label: String,
    /// Empty for boolean options; otherwise contains selectable values.
    #[serde(default)]
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeatures {
    #[serde(default)]
    pub time: bool,
    #[serde(default)]
    pub volume: bool,
    #[serde(default)]
    pub layout: bool,
    #[serde(default)]
    pub media: bool,
}

impl LiveFeatures {
    pub fn is_empty(&self) -> bool {
        !self.time && !self.volume && !self.layout && !self.media
    }

    fn enable_named(&mut self, name: &str) {
        let normalized = name.trim().replace(['-', ' '], "_").to_ascii_lowercase();
        match normalized.as_str() {
            "time" | "clock" | "clock_sync" | "time_sync" => self.time = true,
            "volume" | "volume_sync" => self.volume = true,
            "layout" | "layout_sync" | "keyboard_layout" => self.layout = true,
            "media" | "media_info" | "media_sync" => self.media = true,
            _ => {}
        }
    }
}

/// Full keyboard layout with multiple layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    pub keys: Vec<PhysicalKey>,
    pub encoders: Vec<PhysicalEncoder>,
    pub layers: Vec<Vec<KeyBinding>>, // layers[layer][key_idx] = key binding
    pub encoder_layers: Vec<Vec<u16>>, // encoder_layers[layer][encoder_visual_idx] = keycode (Vial)
    /// Layer names from descriptor/firmware when available.
    #[serde(default)]
    pub layer_names: Vec<String>,
    /// Custom keycodes from vial JSON: symbolic name, short button label, readable tooltip title.
    pub custom_keycodes: Vec<CustomKeycode>,
    /// Vial `layouts.labels` options. Boolean entries have no choices; select entries store choices.
    #[serde(default)]
    pub layout_options: Vec<LayoutOption>,
    /// Entropy Live Features advertised by firmware metadata.
    #[serde(default, skip_serializing_if = "LiveFeatures::is_empty")]
    pub live_features: LiveFeatures,
    /// Whether the keyboard definition exposes runtime RGB controls.
    #[serde(default)]
    pub supports_rgb: bool,
    /// Lighting backend from Vial/QMK definition, for example `qmk_rgblight` or `vialrgb`.
    #[serde(default)]
    pub lighting_mode: Option<String>,
    /// Firmware type
    #[serde(default = "default_firmware")]
    pub firmware: FirmwareProtocol,
}

fn default_firmware() -> FirmwareProtocol {
    FirmwareProtocol::Vial
}

/// Parse matrix (row, col) from vial KLE key label.
/// Label first line format: "row,col"
const KLE_LABEL_MAP: [[i8; 12]; 8] = [
    [0, 6, 2, 8, 9, 11, 3, 5, 1, 4, 7, 10],
    [1, 7, -1, -1, 9, 11, 4, -1, -1, -1, -1, 10],
    [3, -1, 5, -1, 9, 11, -1, -1, 4, -1, -1, 10],
    [4, -1, -1, -1, 9, 11, -1, -1, -1, -1, -1, 10],
    [0, 6, 2, 8, 10, -1, 3, 5, 1, 4, 7, -1],
    [1, 7, -1, -1, 10, -1, 4, -1, -1, -1, -1, -1],
    [3, -1, 5, -1, 10, -1, -1, -1, 4, -1, -1, -1],
    [4, -1, -1, -1, 10, -1, -1, -1, -1, -1, -1, -1],
];

fn kle_labels(label: &str, align: usize) -> [String; 12] {
    let mut labels: [String; 12] = std::array::from_fn(|_| String::new());
    let map = KLE_LABEL_MAP.get(align).unwrap_or(&KLE_LABEL_MAP[4]);
    for (raw_idx, line) in label.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let Some(&mapped) = map.get(raw_idx) else {
            continue;
        };
        if mapped >= 0 {
            labels[mapped as usize] = line.to_string();
        }
    }
    labels
}

fn parse_matrix_from_label(label: &str, align: usize) -> Option<(u8, u8)> {
    let labels = kle_labels(label, align);
    let first_line = labels[0].trim();
    let (r, c) = first_line.split_once(',')?;
    let row = r.trim().parse::<u8>().ok()?;
    let col = c.trim().parse::<u8>().ok()?;
    Some((row, col))
}

/// Parse encoder metadata from a Vial KLE label.
/// Vial marks encoders with label position 4 == "e" and label 0 == "idx,dir".
fn parse_encoder_from_label(label: &str, align: usize) -> Option<(u8, u8)> {
    let labels = kle_labels(label, align);
    if labels[4].trim() != "e" {
        return None;
    }
    let first_line = labels[0].trim();
    let (idx, dir) = first_line.split_once(',')?;
    Some((idx.trim().parse().ok()?, dir.trim().parse().ok()?))
}

/// Parse Vial KLE layout-display condition from label position 8.
///
/// Labels use "option,value", for example "0,0" means the item is visible when
/// the first `layouts.labels` option is set to value 0.
fn parse_layout_condition_from_label(label: &str, align: usize) -> Option<LayoutCondition> {
    let labels = kle_labels(label, align);
    let condition = labels[8].trim();
    let (option_idx, value) = condition.split_once(',')?;
    Some(LayoutCondition {
        option_idx: option_idx.trim().parse().ok()?,
        value: value.trim().parse().ok()?,
    })
}

fn parse_layer_name_value(value: &serde_json::Value) -> Option<String> {
    if let Some(name) = value.as_str() {
        let name = name.trim();
        return (!name.is_empty()).then(|| name.to_string());
    }

    if let Some(obj) = value.as_object() {
        for key in ["name", "label", "title"] {
            if let Some(name) = obj.get(key).and_then(parse_layer_name_value) {
                return Some(name);
            }
        }
    }

    if let Some(arr) = value.as_array() {
        return arr.first().and_then(parse_layer_name_value);
    }

    None
}

fn parse_layer_names_candidate(candidate: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = candidate.as_array() {
        return arr.iter().filter_map(parse_layer_name_value).collect();
    }

    if let Some(obj) = candidate.as_object() {
        let mut indexed_names: Vec<(usize, String)> = obj
            .iter()
            .filter_map(|(key, value)| {
                let index = key.parse::<usize>().ok()?;
                Some((index, parse_layer_name_value(value)?))
            })
            .collect();
        indexed_names.sort_by_key(|(index, _)| *index);
        if !indexed_names.is_empty() {
            return indexed_names.into_iter().map(|(_, name)| name).collect();
        }

        for key in ["names", "layer_names", "layerNames", "layers"] {
            if let Some(names) = obj.get(key).map(parse_layer_names_candidate) {
                if !names.is_empty() {
                    return names;
                }
            }
        }
    }

    vec![]
}

fn parse_layer_names_from_json(json: &serde_json::Value) -> Vec<String> {
    let candidates = [
        json.get("layer_names"),
        json.get("layerNames"),
        json.get("layers"),
        json.get("layout").and_then(|v| v.get("layer_names")),
        json.get("layout").and_then(|v| v.get("layerNames")),
        json.get("layout").and_then(|v| v.get("layers")),
        json.get("layouts").and_then(|v| v.get("layer_names")),
        json.get("layouts").and_then(|v| v.get("layerNames")),
        json.get("layouts").and_then(|v| v.get("layers")),
        json.get("vial").and_then(|v| v.get("layer_names")),
        json.get("vial").and_then(|v| v.get("layerNames")),
        json.get("vial").and_then(|v| v.get("layers")),
    ];

    for candidate in candidates.into_iter().flatten() {
        let names = parse_layer_names_candidate(candidate);
        if !names.is_empty() {
            return names;
        }
    }

    vec![]
}

fn parse_layout_options_from_json(json: &serde_json::Value) -> Vec<LayoutOption> {
    let Some(labels) = json
        .get("layouts")
        .and_then(|v| v.get("labels"))
        .and_then(|v| v.as_array())
    else {
        return vec![];
    };

    labels
        .iter()
        .filter_map(|item| {
            if let Some(label) = item.as_str() {
                let label = label.trim();
                if label.is_empty() {
                    None
                } else {
                    Some(LayoutOption {
                        label: label.to_string(),
                        choices: vec![],
                    })
                }
            } else if let Some(values) = item.as_array() {
                let mut strings = values
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty());
                let label = strings.next()?;
                let choices: Vec<String> = strings.collect();
                if choices.is_empty() {
                    None
                } else {
                    Some(LayoutOption { label, choices })
                }
            } else {
                None
            }
        })
        .collect()
}

fn parse_live_features_candidate(candidate: &serde_json::Value, features: &mut LiveFeatures) {
    if let Some(name) = candidate.as_str() {
        features.enable_named(name);
        return;
    }

    if let Some(arr) = candidate.as_array() {
        for item in arr {
            parse_live_features_candidate(item, features);
        }
        return;
    }

    if let Some(obj) = candidate.as_object() {
        for (key, value) in obj {
            if value.as_bool().unwrap_or(false) {
                features.enable_named(key);
            } else if matches!(key.as_str(), "features" | "liveFeatures" | "live_features") {
                parse_live_features_candidate(value, features);
            }
        }
    }
}

fn parse_live_features_from_json(json: &serde_json::Value) -> LiveFeatures {
    let mut features = LiveFeatures::default();
    let candidates = [
        json.get("entropy").and_then(|v| v.get("liveFeatures")),
        json.get("entropy").and_then(|v| v.get("live_features")),
        json.get("features")
            .and_then(|v| v.get("entropyLiveFeatures")),
        json.get("features")
            .and_then(|v| v.get("entropy_live_features")),
        json.get("firmware")
            .and_then(|v| v.get("entropyLiveFeatures")),
        json.get("firmware")
            .and_then(|v| v.get("entropy_live_features")),
    ];

    for candidate in candidates.into_iter().flatten() {
        parse_live_features_candidate(candidate, &mut features);
    }

    features
}

impl KeyboardLayout {
    pub fn get_keycode(&self, layer: usize, key_idx: usize) -> u16 {
        self.layers
            .get(layer)
            .and_then(|l| l.get(key_idx))
            .copied()
            .unwrap_or_default()
            .vial_keycode()
    }

    pub fn get_key_binding(&self, layer: usize, key_idx: usize) -> KeyBinding {
        self.layers
            .get(layer)
            .and_then(|l| l.get(key_idx))
            .copied()
            .unwrap_or_default()
    }

    pub fn get_encoder_keycode(&self, layer: usize, encoder_visual_idx: usize) -> u16 {
        self.encoder_layers
            .get(layer)
            .and_then(|l| l.get(encoder_visual_idx))
            .copied()
            .unwrap_or(0)
    }

    pub fn encoder_count(&self) -> usize {
        self.encoders
            .iter()
            .map(|e| e.encoder_idx as usize + 1)
            .max()
            .unwrap_or(0)
    }

    pub fn set_key_binding(&mut self, layer: usize, key_idx: usize, binding: KeyBinding) {
        while self.layers.len() <= layer {
            self.layers
                .push(vec![KeyBinding::default(); self.keys.len()]);
        }
        if let Some(layer_data) = self.layers.get_mut(layer) {
            if let Some(slot) = layer_data.get_mut(key_idx) {
                *slot = binding;
            }
        }
    }

    pub fn set_rmk_key_action(
        &mut self,
        layer: usize,
        key_idx: usize,
        action: rmk_types::action::KeyAction,
    ) {
        self.set_key_binding(layer, key_idx, KeyBinding::Rmk(action));
    }

    pub fn set_encoder_keycode(&mut self, layer: usize, encoder_visual_idx: usize, keycode: u16) {
        if self.encoder_layers.len() <= layer {
            self.encoder_layers
                .resize_with(layer + 1, || vec![0; self.encoders.len()]);
        }
        if let Some(layer_data) = self.encoder_layers.get_mut(layer) {
            if layer_data.len() < self.encoders.len() {
                layer_data.resize(self.encoders.len(), 0);
            }
            if let Some(slot) = layer_data.get_mut(encoder_visual_idx) {
                *slot = keycode;
            }
        }
    }

    /// Parse a Vial JSON descriptor into a KeyboardLayout.
    ///
    /// Vial JSON format:
    /// {
    ///   "name": "...",
    ///   "matrix": {"rows": N, "cols": M},
    ///   "layouts": {
    ///     "keymap": [
    ///       [ {obj_or_string}, "label", ... ],  // KLE rows
    ///       ...
    ///     ]
    ///   }
    /// }
    ///
    /// KLE format: rows are arrays. Items are either:
    /// - A JSON object: modifies properties for the NEXT key (x, y, w, h offsets)
    /// - A string: a key label. The key gets current x/y/w/h, then x advances.
    ///
    /// Matrix indices come from the order keys appear: key_index maps to
    /// "layout" array entries which have [row, col] in the vial JSON.
    pub fn from_vial_json(json: &serde_json::Value) -> Result<Self> {
        let name = json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let matrix = json.get("matrix").context("missing 'matrix' field")?;
        let rows = matrix
            .get("rows")
            .and_then(|v| v.as_u64())
            .context("missing matrix.rows")? as usize;
        let cols = matrix
            .get("cols")
            .and_then(|v| v.as_u64())
            .context("missing matrix.cols")? as usize;
        if rows == 0 || rows > 32 || cols == 0 || cols > 32 {
            anyhow::bail!("invalid matrix dimensions in Vial JSON: rows={rows}, cols={cols}");
        }

        let layouts = json.get("layouts").context("missing 'layouts' field")?;
        let keymap = layouts
            .get("keymap")
            .and_then(|v| v.as_array())
            .context("missing 'layouts.keymap'")?;

        let mut keys = Vec::new();
        let mut encoders = Vec::new();

        // KLE global state
        let mut cur_x: f32 = 0.0;
        let mut cur_y: f32 = 0.0;
        let mut rotation_x: f32 = 0.0;
        let mut rotation_y: f32 = 0.0;
        let mut rotation_angle: f32 = 0.0; // degrees
        let mut align: usize = 4; // KLE default: center front

        for kle_row in keymap {
            let row_items = match kle_row.as_array() {
                Some(arr) => arr,
                None => continue,
            };

            let mut next_w: f32 = 1.0;
            let mut next_h: f32 = 1.0;

            for item in row_items {
                if let Some(obj) = item.as_object() {
                    if let Some(r) = obj.get("r").and_then(|v| v.as_f64()) {
                        rotation_angle = r as f32;
                    }
                    if let Some(rx) = obj.get("rx").and_then(|v| v.as_f64()) {
                        rotation_x = rx as f32;
                        cur_x = rotation_x;
                        cur_y = rotation_y;
                    }
                    if let Some(ry) = obj.get("ry").and_then(|v| v.as_f64()) {
                        rotation_y = ry as f32;
                        cur_x = rotation_x;
                        cur_y = rotation_y;
                    }
                    if let Some(a) = obj.get("a").and_then(|v| v.as_u64()) {
                        align = (a as usize).min(7);
                    }
                    if let Some(x) = obj.get("x").and_then(|v| v.as_f64()) {
                        cur_x += x as f32;
                    }
                    if let Some(y) = obj.get("y").and_then(|v| v.as_f64()) {
                        cur_y += y as f32;
                    }
                    if let Some(w) = obj.get("w").and_then(|v| v.as_f64()) {
                        next_w = w as f32;
                    }
                    if let Some(h) = obj.get("h").and_then(|v| v.as_f64()) {
                        next_h = h as f32;
                    }
                } else if let Some(label) = item.as_str() {
                    if let Some((encoder_idx, direction)) = parse_encoder_from_label(label, align) {
                        encoders.push(PhysicalEncoder {
                            x: cur_x,
                            y: cur_y,
                            w: next_w,
                            h: next_h,
                            label: label.to_string(),
                            encoder_idx,
                            direction,
                            rotation: rotation_angle,
                            rotation_x,
                            rotation_y,
                            layout_condition: parse_layout_condition_from_label(label, align),
                        });
                        cur_x += next_w;
                        next_w = 1.0;
                        next_h = 1.0;
                        continue;
                    }

                    if let Some((mat_row, mat_col)) = parse_matrix_from_label(label, align) {
                        keys.push(PhysicalKey {
                            x: cur_x,
                            y: cur_y,
                            w: next_w,
                            h: next_h,
                            row: mat_row,
                            col: mat_col,
                            label: format!("{},{}", mat_row, mat_col),
                            rotation: rotation_angle,
                            rotation_x,
                            rotation_y,
                            layout_condition: parse_layout_condition_from_label(label, align),
                        });
                    }

                    cur_x += next_w;
                    next_w = 1.0;
                    next_h = 1.0;
                }
            }

            cur_y += 1.0;
            cur_x = rotation_x;
        }

        let layer_names = parse_layer_names_from_json(json);
        let layout_options = parse_layout_options_from_json(json);
        let live_features = parse_live_features_from_json(json);

        // Parse custom keycodes
        let custom_keycodes =
            if let Some(customs) = json.get("customKeycodes").and_then(|v| v.as_array()) {
                customs
                    .iter()
                    .map(|c| {
                        let name = c
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let title = c
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let label =
                            if let Some(short_raw) = c.get("shortName").and_then(|v| v.as_str()) {
                                // Vial keeps an explicitly empty shortName empty; Ergohaven uses
                                // that for reserved EH_RSRV slots that should not show in pickers.
                                let parts: Vec<&str> =
                                    short_raw.lines().filter(|l| !l.trim().is_empty()).collect();
                                match parts.len() {
                                    0 => String::new(),
                                    1 => parts[0].to_string(),
                                    _ => format!("{}\n{}", parts[0], parts[1..].join(" ")),
                                }
                            } else {
                                name.clone()
                            };
                        let title = if title.is_empty() {
                            name.clone()
                        } else {
                            title
                        };
                        CustomKeycode { name, label, title }
                    })
                    .collect()
            } else {
                vec![]
            };

        let num_keys = keys.len();
        let lighting_mode = json
            .get("lighting")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let supports_rgb = matches!(
            lighting_mode.as_deref(),
            Some("qmk_rgblight") | Some("qmk_backlight_rgblight") | Some("vialrgb")
        );
        Ok(Self {
            name,
            rows,
            cols,
            keys,
            encoders,
            layers: vec![vec![KeyBinding::default(); num_keys]; 4],
            encoder_layers: vec![],
            layer_names,
            custom_keycodes,
            layout_options,
            live_features,
            supports_rgb,
            lighting_mode,
            firmware: FirmwareProtocol::Vial,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entropy_live_features_from_vial_json() {
        let json = serde_json::json!({
            "name": "Qube Test",
            "matrix": { "rows": 1, "cols": 1 },
            "entropy": { "liveFeatures": ["time", "media"] },
            "layouts": { "keymap": [["0,0"]] }
        });

        let layout = KeyboardLayout::from_vial_json(&json).unwrap();

        assert!(layout.live_features.time);
        assert!(!layout.live_features.layout);
        assert!(!layout.live_features.volume);
        assert!(layout.live_features.media);
    }
}
