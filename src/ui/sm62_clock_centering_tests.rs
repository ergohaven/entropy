// Actual egui painter/atlas regression, not a substitute geometry renderer.
#[cfg(test)]
mod clock_centering_tests {
    use super::*;

    fn clock_meshes(ctx: &egui::Context, time: &str, scale: f32, fallback: bool) -> Vec<egui::Mesh> {
        let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| {
            let screen = egui::Rect::from_min_size(egui::pos2(11.0, 13.0), egui::vec2(240.0, 280.0) * scale);
            if fallback { paint_sm62_clock_fallback(ui, screen); }
            else { paint_sm62_clock_time(ui, screen, time); }
        }));
        output.shapes.into_iter().filter_map(|s| match s.shape {
            egui::Shape::Mesh(mesh) => Some((*mesh).clone()), _ => None,
        }).collect()
    }

    #[test]
    fn sm62_clock_actual_egui_centers_all_1440_times_before_scaling() {
        let ctx = egui::Context::default();
        for scale in [0.5, 0.825, 1.0] {
            for hour in 0..24 {
                for minute in 0..60 {
                    let time = format!("{hour:02}:{minute:02}");
                    let meshes = clock_meshes(&ctx, &time, scale, false);
                    assert_eq!(meshes.len(), 5, "{time}: two digits + colon + two digits");
                    let left = meshes.iter().flat_map(|m| &m.vertices).map(|v| (v.pos.x - 11.0) / scale).fold(f32::INFINITY, f32::min);
                    let right = meshes.iter().flat_map(|m| &m.vertices).map(|v| (v.pos.x - 11.0) / scale).fold(f32::NEG_INFINITY, f32::max);
                    assert!((left - (240.0 - right)).abs() <= 1.001,
                        "live clock off-center: time={time} scale={scale} left={left} right={right}");
                    assert!(meshes.iter().all(|m| m.texture_id == meshes[0].texture_id));
                    assert!(meshes.iter().all(|m| m.vertices.iter().all(|v| v.uv.x >= 0.0 && v.uv.x <= 1.0 && v.uv.y >= 0.0 && v.uv.y <= 1.0)));
                }
            }
        }
    }

    #[test]
    fn sm62_clock_actual_egui_invalid_time_keeps_unshifted_placeholder() {
        let ctx = egui::Context::default();
        for time in ["SM:62", "24:00", "12:60", "no-time"] {
            let meshes = clock_meshes(&ctx, time, 1.0, false);
            let (hours, minutes) = time.split_once(':').unwrap_or(("SM", "62"));
            let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| {
                let screen = egui::Rect::from_min_size(egui::pos2(11.0, 13.0), egui::vec2(240.0, 280.0));
                paint_sm62_clock_label(ui, screen, hours, 48, 7.0, 124.0, 98.0, 2, Color32::from_rgb(255,248,231));
                paint_sm62_clock_label(ui, screen, ":", 48, 110.0, 124.0, 20.0, 1, Color32::from_rgb(242,219,149));
                paint_sm62_clock_label(ui, screen, minutes, 48, 132.0, 124.0, 84.0, 0, Color32::from_rgb(255,248,231));
            }));
            let expected: Vec<_> = output.shapes.into_iter().filter_map(|s| match s.shape {
                egui::Shape::Mesh(m) => Some((*m).clone()), _ => None,
            }).collect();
            assert_eq!(meshes, expected, "invalid time must not get a live centering shift: {time}");
        }
        let fallback = clock_meshes(&ctx, "", 1.0, true);
        assert_eq!(fallback.len(), 5);
        let min = fallback.iter().flat_map(|m| &m.vertices).map(|v| v.pos.x).fold(f32::INFINITY, f32::min);
        let max = fallback.iter().flat_map(|m| &m.vertices).map(|v| v.pos.x).fold(f32::NEG_INFINITY, f32::max);
        assert!(((min - 11.0) - (251.0 - max)).abs() <= 1.0);
    }
}
