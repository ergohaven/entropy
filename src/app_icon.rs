fn blend(dst: &mut [f32; 4], src: [f32; 4]) {
    let a = src[3].clamp(0.0, 1.0);
    let inv = 1.0 - a;
    dst[0] = src[0] * a + dst[0] * inv;
    dst[1] = src[1] * a + dst[1] * inv;
    dst[2] = src[2] * a + dst[2] * inv;
    dst[3] = a + dst[3] * inv;
}

fn smooth_alpha(distance: f32, softness: f32) -> f32 {
    (0.5 - distance / softness).clamp(0.0, 1.0)
}

fn sd_round_box(px: f32, py: f32, hx: f32, hy: f32, radius: f32) -> f32 {
    let qx = px.abs() - hx + radius;
    let qy = py.abs() - hy + radius;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - radius
}

fn sd_capsule(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, radius: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    let dx = pax - bax * h;
    let dy = pay - bay * h;
    (dx * dx + dy * dy).sqrt() - radius
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn keycap_color(x: f32, y: f32) -> [f32; 3] {
    let rose = [0.95, 0.39, 0.53];
    let violet = [0.66, 0.42, 0.88];
    let blue = [0.34, 0.63, 0.95];
    let t = ((x + 0.68) * 0.68 + (0.66 - y) * 0.32).clamp(0.0, 1.0);
    if t < 0.55 {
        mix(rose, violet, t / 0.55)
    } else {
        mix(violet, blue, (t - 0.55) / 0.45)
    }
}

fn draw_capsule(
    pixel: &mut [f32; 4],
    x: f32,
    y: f32,
    start: (f32, f32),
    end: (f32, f32),
    radius: f32,
    color: [f32; 4],
) {
    let distance = sd_capsule(x, y, start.0, start.1, end.0, end.1, radius);
    let alpha = smooth_alpha(distance, 0.020) * color[3];
    if alpha > 0.0 {
        blend(pixel, [color[0], color[1], color[2], alpha]);
    }
}

fn draw_keycap(pixel: &mut [f32; 4], x: f32, y: f32) {
    let shadow = sd_round_box(x - 0.045, y - 0.060, 0.68, 0.68, 0.185);
    blend(pixel, [0.0, 0.0, 0.0, smooth_alpha(shadow, 0.034) * 0.26]);

    let rim = sd_round_box(x, y, 0.70, 0.70, 0.195);
    let rim_alpha = smooth_alpha(rim, 0.026);
    if rim_alpha > 0.0 {
        blend(pixel, [0.93, 0.88, 0.81, rim_alpha]);
    }

    let face = sd_round_box(x, y, 0.595, 0.595, 0.155);
    let face_alpha = smooth_alpha(face, 0.022);
    if face_alpha > 0.0 {
        let c = keycap_color(x, y);
        blend(pixel, [c[0], c[1], c[2], face_alpha]);
    }

    let top_highlight = sd_round_box(x + 0.030, y + 0.070, 0.505, 0.430, 0.125);
    let top_highlight_alpha = smooth_alpha(top_highlight, 0.020) * face_alpha * 0.14;
    if top_highlight_alpha > 0.0 {
        blend(pixel, [1.0, 1.0, 1.0, top_highlight_alpha]);
    }
}

fn draw_letter_e(pixel: &mut [f32; 4], x: f32, y: f32) {
    let shadow = [0.0, 0.0, 0.0, 0.18];
    let cream = [0.99, 0.96, 0.90, 1.0];
    let strokes = [
        (-0.245, -0.340, -0.245, 0.340, 0.060),
        (-0.225, -0.330, 0.285, -0.330, 0.060),
        (-0.225, 0.000, 0.230, 0.000, 0.054),
        (-0.225, 0.330, 0.285, 0.330, 0.060),
    ];

    for (ax, ay, bx, by, radius) in strokes {
        draw_capsule(
            pixel,
            x,
            y,
            (ax + 0.030, ay + 0.030),
            (bx + 0.030, by + 0.030),
            radius,
            shadow,
        );
    }

    for (ax, ay, bx, by, radius) in strokes {
        draw_capsule(pixel, x, y, (ax, ay), (bx, by), radius, cream);
    }
}

pub(crate) fn rgba_icon(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let nx = ((x as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let ny = ((y as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let mut pixel = [0.0, 0.0, 0.0, 0.0];

            draw_keycap(&mut pixel, nx, ny);
            draw_letter_e(&mut pixel, nx, ny);

            rgba.push((pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8);
            rgba.push((pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8);
            rgba.push((pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8);
            rgba.push((pixel[3].clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }

    rgba
}

pub(crate) fn egui_icon(size: u32) -> egui::IconData {
    egui::IconData {
        rgba: rgba_icon(size),
        width: size,
        height: size,
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn tray_icon(size: u32) -> Option<tray_icon::Icon> {
    tray_icon::Icon::from_rgba(rgba_icon(size), size, size).ok()
}
