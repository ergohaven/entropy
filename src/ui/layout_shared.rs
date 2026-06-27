use super::*;

#[derive(Clone, Copy)]
pub(crate) struct LayoutGeometry {
    pub(crate) offset_x: f32,
    pub(crate) offset_y: f32,
    pub(crate) unit: f32,
    pub(crate) padding: f32,
    pub(crate) layout_h: f32,
}

pub(crate) fn responsive_layout_max_scale(ctx: &egui::Context, viewport: egui::Rect) -> f32 {
    let native_scale = ctx
        .native_pixels_per_point()
        .unwrap_or_else(|| ctx.pixels_per_point() / ctx.zoom_factor().max(0.1))
        .max(1.0);
    let physical_short_side = (viewport.width().min(viewport.height()) * native_scale).max(0.0);
    let t = ((physical_short_side - 1_080.0) / (2_160.0 - 1_080.0)).clamp(0.0, 1.0);
    1.0 + 0.35 * t
}

pub(crate) fn layout_geometry(
    ctx: &egui::Context,
    layout: &KeyboardLayout,
    viewport: egui::Rect,
    ui_scale: f32,
) -> LayoutGeometry {
    layout_geometry_with_reserved(
        ctx,
        layout,
        viewport,
        ui_scale,
        LAYOUT_TOP_RESERVED_H,
        LAYOUT_BOTTOM_RESERVED_H,
        LAYOUT_FIT_MARGIN,
        None,
    )
}

pub(crate) fn layout_geometry_with_reserved(
    ctx: &egui::Context,
    layout: &KeyboardLayout,
    viewport: egui::Rect,
    ui_scale: f32,
    top_reserved: f32,
    bottom_reserved: f32,
    fit_margin: f32,
    max_scale_override: Option<f32>,
) -> LayoutGeometry {
    layout_geometry_with_reserved_and_filter(
        ctx,
        layout,
        viewport,
        ui_scale,
        top_reserved,
        bottom_reserved,
        fit_margin,
        max_scale_override,
        |_| true,
        |_| true,
    )
}

pub(crate) fn layout_geometry_with_reserved_and_filter(
    ctx: &egui::Context,
    layout: &KeyboardLayout,
    viewport: egui::Rect,
    ui_scale: f32,
    top_reserved: f32,
    bottom_reserved: f32,
    fit_margin: f32,
    max_scale_override: Option<f32>,
    include_key: impl Fn(&PhysicalKey) -> bool,
    include_encoder: impl Fn(&PhysicalEncoder) -> bool,
) -> LayoutGeometry {
    let mut min_x: f32 = f32::MAX;
    let mut min_y: f32 = f32::MAX;
    let mut max_x: f32 = f32::MIN;
    let mut max_y: f32 = f32::MIN;
    for key in &layout.keys {
        if !include_key(key) {
            continue;
        }
        let (x1, y1, x2, y2) = rotated_item_aabb(
            key.x,
            key.y,
            key.w,
            key.h,
            key.rotation,
            key.rotation_x,
            key.rotation_y,
        );
        min_x = min_x.min(x1);
        min_y = min_y.min(y1);
        max_x = max_x.max(x2);
        max_y = max_y.max(y2);
    }
    for encoder in &layout.encoders {
        if !include_encoder(encoder) {
            continue;
        }
        let (x1, y1, x2, y2) = rotated_item_aabb(
            encoder.x,
            encoder.y,
            encoder.w,
            encoder.h,
            encoder.rotation,
            encoder.rotation_x,
            encoder.rotation_y,
        );
        min_x = min_x.min(x1);
        min_y = min_y.min(y1);
        max_x = max_x.max(x2);
        max_y = max_y.max(y2);
    }
    if min_x == f32::MAX {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 1.0;
        max_y = 1.0;
    }

    let span_x = max_x - min_x;
    let span_y = max_y - min_y;
    let fit_width = viewport.width() * ui_scale;
    let fit_height = viewport.height() * ui_scale;
    let scale_x = (fit_width - fit_margin) / (span_x * LAYOUT_BASE_UNIT).max(1.0);
    let scale_y = (fit_height - fit_margin) / (span_y * LAYOUT_BASE_UNIT).max(1.0);
    let max_scale =
        max_scale_override.unwrap_or_else(|| responsive_layout_max_scale(ctx, viewport));
    let scale = scale_x.min(scale_y).min(max_scale);
    let unit = LAYOUT_BASE_UNIT * scale;
    let layout_w = span_x * unit;
    let layout_h = span_y * unit;
    let content_top = viewport.top() + top_reserved;
    let content_bottom = viewport.bottom() - bottom_reserved;

    LayoutGeometry {
        offset_x: viewport.center().x - layout_w / 2.0 - min_x * unit,
        offset_y: ((content_top + content_bottom) - layout_h) / 2.0 - min_y * unit,
        unit,
        padding: LAYOUT_KEY_PADDING,
        layout_h,
    }
}

pub(crate) fn layout_keycap_rect(
    offset_x: f32,
    offset_y: f32,
    unit: f32,
    padding: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(offset_x + x * unit + padding, offset_y + y * unit + padding),
        Vec2::new(w * unit - padding * 2.0, h * unit - padding * 2.0),
    )
}

pub(crate) fn rotate_layout_point(
    x: f32,
    y: f32,
    origin_x: f32,
    origin_y: f32,
    rotation_deg: f32,
) -> (f32, f32) {
    if rotation_deg == 0.0 {
        return (x, y);
    }
    let angle = rotation_deg.to_radians();
    let dx = x - origin_x;
    let dy = y - origin_y;
    (
        origin_x + dx * angle.cos() - dy * angle.sin(),
        origin_y + dx * angle.sin() + dy * angle.cos(),
    )
}

fn rotated_item_center(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
) -> (f32, f32) {
    rotate_layout_point(x + w * 0.5, y + h * 0.5, rotation_x, rotation_y, rotation)
}

pub(crate) fn rotated_item_aabb(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
) -> (f32, f32, f32, f32) {
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (cx, cy) in corners {
        let (rx, ry) = rotate_layout_point(cx, cy, rotation_x, rotation_y, rotation);
        min_x = min_x.min(rx);
        min_y = min_y.min(ry);
        max_x = max_x.max(rx);
        max_y = max_y.max(ry);
    }
    (min_x, min_y, max_x, max_y)
}

pub(crate) fn layout_physical_key_rect(key: &PhysicalKey, geometry: LayoutGeometry) -> egui::Rect {
    let (center_x, center_y) = rotated_item_center(
        key.x,
        key.y,
        key.w,
        key.h,
        key.rotation,
        key.rotation_x,
        key.rotation_y,
    );
    egui::Rect::from_center_size(
        egui::pos2(
            geometry.offset_x + center_x * geometry.unit,
            geometry.offset_y + center_y * geometry.unit,
        ),
        Vec2::new(
            key.w * geometry.unit - geometry.padding * 2.0,
            key.h * geometry.unit - geometry.padding * 2.0,
        ),
    )
}

pub(crate) fn layout_physical_encoder_rect(
    encoder: &PhysicalEncoder,
    geometry: LayoutGeometry,
) -> egui::Rect {
    let (center_x, center_y) = rotated_item_center(
        encoder.x,
        encoder.y,
        encoder.w,
        encoder.h,
        encoder.rotation,
        encoder.rotation_x,
        encoder.rotation_y,
    );
    egui::Rect::from_center_size(
        egui::pos2(
            geometry.offset_x + center_x * geometry.unit,
            geometry.offset_y + center_y * geometry.unit,
        ),
        Vec2::new(
            encoder.w * geometry.unit - geometry.padding * 2.0,
            encoder.h * geometry.unit - geometry.padding * 2.0,
        ),
    )
}

pub(crate) fn paint_layout_keycap(
    painter: &egui::Painter,
    rect: egui::Rect,
    rotation: f32,
    fill: Color32,
    stroke: Stroke,
) {
    if rotation == 0.0 {
        painter.rect(rect, 6.0, fill, stroke, egui::StrokeKind::Inside);
        return;
    }

    let angle = rotation.to_radians();
    let center = rect.center();
    let rotate = |pos: egui::Pos2| {
        let dx = pos.x - center.x;
        let dy = pos.y - center.y;
        egui::pos2(
            center.x + dx * angle.cos() - dy * angle.sin(),
            center.y + dx * angle.sin() + dy * angle.cos(),
        )
    };

    let radius = 6.0_f32.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let corner_segments = 5;
    let mut points = Vec::with_capacity((corner_segments + 1) * 4);
    let corners = [
        (
            egui::pos2(rect.right() - radius, rect.top() + radius),
            -std::f32::consts::FRAC_PI_2,
            0.0,
        ),
        (
            egui::pos2(rect.right() - radius, rect.bottom() - radius),
            0.0,
            std::f32::consts::FRAC_PI_2,
        ),
        (
            egui::pos2(rect.left() + radius, rect.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ),
        (
            egui::pos2(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
        ),
    ];
    for (corner_center, start, end) in corners {
        for step in 0..=corner_segments {
            let t = step as f32 / corner_segments as f32;
            let theta = start + (end - start) * t;
            let point = egui::pos2(
                corner_center.x + radius * theta.cos(),
                corner_center.y + radius * theta.sin(),
            );
            points.push(rotate(point));
        }
    }

    painter.add(egui::Shape::convex_polygon(points, fill, stroke));
}

pub(crate) fn layout_matrix_key_pressed(
    layout: &KeyboardLayout,
    matrix_pressed: &[bool],
    row: u8,
    col: u8,
) -> bool {
    matrix_pressed
        .get(row as usize * layout.cols + col as usize)
        .copied()
        .unwrap_or(false)
}

pub(crate) fn rotated_offset(dx: f32, dy: f32, angle: f32) -> egui::Vec2 {
    egui::vec2(
        dx * angle.cos() - dy * angle.sin(),
        dx * angle.sin() + dy * angle.cos(),
    )
}

pub(crate) fn paint_centered_text_rotated(
    painter: &egui::Painter,
    center: egui::Pos2,
    text: &str,
    font_id: FontId,
    color: Color32,
    rotation: f32,
) {
    if rotation == 0.0 {
        painter.text(center, egui::Align2::CENTER_CENTER, text, font_id, color);
        return;
    }

    let galley = painter.layout_no_wrap(text.to_string(), font_id, color);
    let half = galley.size() * 0.5;
    let pos = center - rotated_offset(half.x, half.y, rotation);
    painter.add(egui::Shape::Text(
        egui::epaint::TextShape::new(pos, galley, color).with_angle(rotation),
    ));
}

fn keycap_label_text_scale(rect: egui::Rect) -> f32 {
    ((rect.width().min(rect.height()) / 54.0) * 0.94).clamp(0.68, 1.18)
}

fn split_keycap_label(label: &str) -> (Option<String>, String) {
    if label.contains('\n') {
        let mut parts = label.splitn(2, '\n');
        return (
            parts.next().map(str::to_string),
            parts.next().unwrap_or(label).to_string(),
        );
    }

    if let Some(pos) = label.rfind('+').filter(|pos| *pos + 1 < label.len()) {
        return (Some(label[..pos].to_string()), label[pos + 1..].to_string());
    }

    if let Some(pos) = label
        .find('/')
        .filter(|pos| *pos > 0 && *pos + 1 < label.len())
    {
        return (Some(label[..pos].to_string()), label[pos + 1..].to_string());
    }

    (None, label.to_string())
}

fn fitted_keycap_font_size(
    painter: &egui::Painter,
    text: &str,
    base_size: f32,
    available_width: f32,
    min_size: f32,
) -> f32 {
    if text.trim().is_empty() {
        return base_size;
    }

    let galley = painter.layout_no_wrap(
        text.to_string(),
        FontId::proportional(base_size),
        Color32::WHITE,
    );
    let width = galley.size().x.max(1.0);
    if width <= available_width {
        base_size
    } else {
        (base_size * available_width.max(1.0) / width).clamp(min_size, base_size)
    }
}

fn draw_key_label_with_colors(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    top_color: Color32,
    bottom_color: Color32,
    rotation: f32,
) {
    let scale = keycap_label_text_scale(rect);
    let clip_rect = rect.shrink2(egui::vec2(4.0, 3.0) * scale);
    let available_width = clip_rect.width().max(1.0);
    let clipped = painter.with_clip_rect(clip_rect);
    let (top, bottom) = split_keycap_label(label);
    let (top_size, bottom_size) = key_label_font_sizes(label);

    let lines: Vec<&str> = label.split('\n').collect();
    if let [top_str, middle, bottom_str] = lines.as_slice() {
        let center = rect.center();
        let is_symbol_line = |line: &str| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && trimmed.chars().count() <= 3
                && trimmed
                    .chars()
                    .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
        };
        let top_base = top_size.unwrap_or(8.3) * scale;
        let middle_base = if is_symbol_line(middle) { 10.2 } else { 9.2 } * scale;
        let bottom_base = bottom_size * scale;
        let top_fit =
            fitted_keycap_font_size(&clipped, top_str, top_base, available_width, 5.2 * scale);
        let middle_fit =
            fitted_keycap_font_size(&clipped, middle, middle_base, available_width, 5.6 * scale);
        let bottom_fit = fitted_keycap_font_size(
            &clipped,
            bottom_str,
            bottom_base,
            available_width,
            5.8 * scale,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, -14.0 * scale, rotation),
            top_str,
            FontId::proportional(top_fit),
            top_color,
            rotation,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, -scale, rotation),
            middle,
            FontId::proportional(middle_fit),
            top_color,
            rotation,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, 12.0 * scale, rotation),
            bottom_str,
            FontId::proportional(bottom_fit),
            bottom_color,
            rotation,
        );
        return;
    }

    if let Some(top_str) = top {
        let top_base = top_size.unwrap_or(9.0) * scale;
        let bottom_base = bottom_size * scale;
        let top_fit =
            fitted_keycap_font_size(&clipped, &top_str, top_base, available_width, 5.8 * scale);
        let bottom_fit =
            fitted_keycap_font_size(&clipped, &bottom, bottom_base, available_width, 6.4 * scale);
        let center = rect.center();
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, -7.0 * scale, rotation),
            &top_str,
            FontId::proportional(top_fit),
            top_color,
            rotation,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, 6.0 * scale, rotation),
            &bottom,
            FontId::proportional(bottom_fit),
            bottom_color,
            rotation,
        );
    } else {
        let base_size = if bottom == "↵" {
            16.0 * scale
        } else {
            bottom_size * scale
        };
        let font_size =
            fitted_keycap_font_size(&clipped, &bottom, base_size, available_width, 6.8 * scale);
        paint_centered_text_rotated(
            &clipped,
            rect.center(),
            &bottom,
            FontId::proportional(font_size),
            bottom_color,
            rotation,
        );
    }
}

pub(crate) fn draw_key_label_dimmed(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    dark: bool,
    rotation: f32,
) {
    let dim = if dark {
        Color32::from_rgb(62, 56, 56)
    } else {
        Color32::from_rgb(200, 200, 208)
    };
    let dim_top = if dark {
        Color32::from_rgb(45, 45, 50)
    } else {
        Color32::from_rgb(215, 215, 220)
    };
    draw_key_label_with_colors(painter, rect, label, dim_top, dim, rotation);
}

pub(crate) fn number_row_shifted_label(
    label: String,
    enabled: bool,
    key_legend_layout: KeyLegendLayout,
) -> String {
    if !enabled {
        return label;
    }

    let Some((digit, english, russian)) = (match label.as_str() {
        "1" => Some(("1", "!", "!")),
        "2" => Some(("2", "@", "\"")),
        "3" => Some(("3", "#", "№")),
        "4" => Some(("4", "$", ";")),
        "5" => Some(("5", "%", "%")),
        "6" => Some(("6", "^", ":")),
        "7" => Some(("7", "&", "?")),
        "8" => Some(("8", "*", "*")),
        "9" => Some(("9", "(", "(")),
        "0" => Some(("0", ")", ")")),
        _ => None,
    }) else {
        return label;
    };

    let shifted = match key_legend_layout {
        KeyLegendLayout::English => english.to_string(),
        KeyLegendLayout::Russian => {
            if english == russian {
                english.to_string()
            } else {
                format!("{}  {}", english, russian)
            }
        }
        KeyLegendLayout::RussianPrimary => {
            if english == russian {
                russian.to_string()
            } else {
                format!("{}  {}", russian, english)
            }
        }
    };
    format!("{}\n{}", shifted, digit)
}

pub(crate) fn draw_key_label(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    dark: bool,
    rotation: f32,
) {
    let top_color = if dark {
        Color32::from_rgb(130, 130, 145)
    } else {
        Color32::from_rgb(130, 130, 150)
    };
    let main_color = if dark {
        Color32::from_rgb(239, 233, 232)
    } else {
        Color32::from_rgb(26, 26, 30)
    };
    draw_key_label_with_colors(painter, rect, label, top_color, main_color, rotation);
}
