//! Validation helpers for embedded Ergohaven layout fixtures.
use crate::keyboard::PhysicalKey;

const K03_JSON: &str = include_str!("layouts/k03.json");
const IMPERIAL44_JSON: &str = include_str!("layouts/imperial44.json");
const OP36_JSON: &str = include_str!("layouts/op36.json");

struct EmbeddedLayout {
    name: &'static str,
    json: &'static str,
}

static LAYOUTS: &[EmbeddedLayout] = &[
    EmbeddedLayout {
        name: "K:03",
        json: K03_JSON,
    },
    EmbeddedLayout {
        name: "Imperial44",
        json: IMPERIAL44_JSON,
    },
    EmbeddedLayout {
        name: "Omega Point 36",
        json: OP36_JSON,
    },
];

/// Parse the embedded JSON format: `layouts.default_transform.layout` is an array of
/// `{ row, col, x, y, r? }` with absolute coordinates in KLE units.
fn parse_embedded_json(json: &str) -> Option<Vec<PhysicalKey>> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let layout_arr = root
        .get("layouts")?
        .get("default_transform")?
        .get("layout")?
        .as_array()?;

    let keys = layout_arr
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let row = entry.get("row").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
            let col = entry.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
            let x = entry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let y = entry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let r = entry.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            // Embedded layouts use absolute per-key coordinates. Rotate around the
            // key center unless an explicit KLE group origin is provided.
            let rx = entry
                .get("rx")
                .and_then(|v| v.as_f64())
                .unwrap_or(x as f64 + 0.5) as f32;
            let ry = entry
                .get("ry")
                .and_then(|v| v.as_f64())
                .unwrap_or(y as f64 + 0.5) as f32;

            PhysicalKey {
                x,
                y,
                w: 1.0,
                h: 1.0,
                row,
                col,
                label: format!("{i}"),
                rotation: r,
                rotation_x: rx,
                rotation_y: ry,
                layout_condition: None,
            }
        })
        .collect();

    Some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYMMETRY_EPSILON: f32 = 0.001;

    fn rendered_center(key: &PhysicalKey) -> (f32, f32) {
        let angle = key.rotation.to_radians();
        let dx = key.x + key.w * 0.5 - key.rotation_x;
        let dy = key.y + key.h * 0.5 - key.rotation_y;
        (
            key.rotation_x + dx * angle.cos() - dy * angle.sin(),
            key.rotation_y + dx * angle.sin() + dy * angle.cos(),
        )
    }

    #[test]
    fn rotated_key_without_origin_uses_center_anchor() {
        let json = r#"{
            "layouts": {
                "default_transform": {
                    "layout": [{ "row": 0, "col": 0, "x": 2.0, "y": 3.0, "r": 20 }]
                }
            }
        }"#;
        let keys = parse_embedded_json(json).expect("inline layout should parse");
        let key = &keys[0];
        assert!((key.rotation_x - 2.5).abs() < SYMMETRY_EPSILON);
        assert!((key.rotation_y - 3.5).abs() < SYMMETRY_EPSILON);
    }

    #[test]
    fn embedded_layouts_are_visually_symmetric() {
        for layout in LAYOUTS {
            let keys = parse_embedded_json(layout.json).expect("embedded layout should parse");
            let centers: Vec<_> = keys.iter().map(rendered_center).collect();
            let min_x = centers
                .iter()
                .map(|(x, _)| *x)
                .fold(f32::INFINITY, f32::min);
            let max_x = centers
                .iter()
                .map(|(x, _)| *x)
                .fold(f32::NEG_INFINITY, f32::max);
            let symmetry_axis = (min_x + max_x) * 0.5;

            for (key, &(x, y)) in keys.iter().zip(&centers) {
                let reflected_x = symmetry_axis * 2.0 - x;
                let has_counterpart =
                    keys.iter()
                        .zip(&centers)
                        .any(|(candidate, &(candidate_x, candidate_y))| {
                            (candidate_x - reflected_x).abs() < SYMMETRY_EPSILON
                                && (candidate_y - y).abs() < SYMMETRY_EPSILON
                                && (candidate.rotation + key.rotation).abs() < SYMMETRY_EPSILON
                                && (candidate.w - key.w).abs() < SYMMETRY_EPSILON
                                && (candidate.h - key.h).abs() < SYMMETRY_EPSILON
                        });
                assert!(
                    has_counterpart,
                    "{} key ({}, {}) at ({x:.3}, {y:.3}) has no reflected counterpart around x={symmetry_axis:.3}",
                    layout.name,
                    key.row,
                    key.col
                );
            }
        }
    }
}
