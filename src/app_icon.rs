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

fn sd_box(px: f32, py: f32, hx: f32, hy: f32) -> f32 {
    sd_round_box(px, py, hx, hy, 0.0)
}

// Геометрия и палитра повторяют assets/entropy.svg, из которого растут .icns,
// .ico и иконки hicolor: логотип должен быть один во всех местах. Координаты
// viewBox 0..256 переведены в нормализованные -1..1 по формуле svg / 128 - 1.
const NAVY: [f32; 3] = [0.063, 0.094, 0.157];
const TEAL: [f32; 3] = [0.369, 0.918, 0.831];
const ORANGE: [f32; 3] = [0.976, 0.451, 0.086];

// Бирюзовая фигура — объединение трёх прямоугольников, поэтому расстояния
// берутся через min: раздельная отрисовка оставила бы швы на стыках.
fn teal_distance(x: f32, y: f32) -> f32 {
    sd_box(x, y + 0.3359375, 0.5078125, 0.1328125)
        .min(sd_box(x + 0.3515625, y + 0.15625, 0.15625, 0.3125))
        .min(sd_box(x - 0.1171875, y - 0.203125, 0.3125, 0.1328125))
}

fn draw_logo(pixel: &mut [f32; 4], x: f32, y: f32, softness: f32) {
    let paint = |pixel: &mut [f32; 4], distance: f32, color: [f32; 3]| {
        let alpha = smooth_alpha(distance, softness);
        if alpha > 0.0 {
            blend(pixel, [color[0], color[1], color[2], alpha]);
        }
    };

    paint(pixel, sd_round_box(x, y, 1.0, 1.0, 0.375), NAVY);
    paint(pixel, teal_distance(x, y), TEAL);
    paint(
        pixel,
        sd_box(x, y - 0.3359375, 0.5078125, 0.1328125),
        ORANGE,
    );
}

pub(crate) fn rgba_icon(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    // Сглаживание шириной в пиксель: иначе трей-иконка 32x32 идёт лесенкой.
    let softness = 2.0 / size as f32;

    for y in 0..size {
        for x in 0..size {
            let nx = ((x as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let ny = ((y as f32 + 0.5) / size as f32) * 2.0 - 1.0;
            let mut pixel = [0.0, 0.0, 0.0, 0.0];

            draw_logo(&mut pixel, nx, ny, softness);

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
