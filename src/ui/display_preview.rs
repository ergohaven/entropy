use super::*;

const DISPLAY_WIDTH: f32 = 240.0;
const DISPLAY_HEIGHT: f32 = 280.0;
const DISPLAY_CONTENT_INSET: f32 = 5.0;
const DISPLAY_DIAGONAL_INCHES: f32 = 1.69;
const DISPLAY_CORNER_RADIUS_MM: f32 = 3.0;
const MILLIMETERS_PER_INCH: f32 = 25.4;

// Mirrors keyboards/ergohaven/macropad/screen_layout.c in 240×280 source pixels.
const MAIN_HEADER_X: f32 = 23.0;
const MAIN_HEADER_WIDTH: f32 = 194.0;
const MAIN_GRID_X: f32 = 9.0;
const MAIN_GRID_Y: f32 = 47.0;
const MAIN_CELL_WIDTH: f32 = 74.0;
const MAIN_CELL_HEIGHT: f32 = 43.0;
const MAIN_SHAPE_WIDTH: f32 = 73.0;
const MAIN_SHAPE_HEIGHT: f32 = 42.0;
const MAIN_ENCODER_ROW_Y: f32 = 224.0;

fn display_corner_radius(rendered_width: f32) -> f32 {
    let diagonal_pixels = (DISPLAY_WIDTH * DISPLAY_WIDTH + DISPLAY_HEIGHT * DISPLAY_HEIGHT).sqrt();
    let diagonal_mm = DISPLAY_DIAGONAL_INCHES * MILLIMETERS_PER_INCH;
    let source_radius_pixels = DISPLAY_CORNER_RADIUS_MM * diagonal_pixels / diagonal_mm;
    source_radius_pixels * rendered_width / DISPLAY_WIDTH
}

fn fitted_display_size(available_width: f32, available_height: f32) -> egui::Vec2 {
    let width = available_width
        .min(available_height * DISPLAY_WIDTH / DISPLAY_HEIGHT)
        .max(1.0);
    egui::vec2(width, width * DISPLAY_HEIGHT / DISPLAY_WIDTH)
}

fn animation_frame_index(
    delays_ms: &[u16],
    frame_count: usize,
    elapsed_ms: u64,
    speed_percent: u16,
) -> usize {
    if frame_count < 2 {
        return 0;
    }
    let total = (0..frame_count)
        .map(|index| u64::from(delays_ms.get(index).copied().unwrap_or(100).max(20)))
        .sum::<u64>();
    let scaled_elapsed = elapsed_ms.saturating_mul(u64::from(speed_percent.max(1))) / 100;
    let mut position = scaled_elapsed % total;
    for index in 0..frame_count {
        let delay = u64::from(delays_ms.get(index).copied().unwrap_or(100).max(20));
        if position < delay {
            return index;
        }
        position -= delay;
    }
    0
}

fn screen_rect(screen: egui::Rect, x: f32, y: f32, width: f32, height: f32) -> egui::Rect {
    let scale = screen.width() / DISPLAY_WIDTH;
    egui::Rect::from_min_size(
        screen.min + egui::vec2(x * scale, y * scale),
        egui::vec2(width * scale, height * scale),
    )
}

fn display_preview_font(source_size: f32, scale: f32) -> egui::FontId {
    egui::FontId::new(
        source_size * scale,
        egui::FontFamily::Name("display_preview".into()),
    )
}

fn layer_name_needs_scrolling(
    name_width: f32,
    header_width: f32,
    icon_width: f32,
    gap: f32,
) -> bool {
    name_width > header_width - icon_width - gap
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewKeyIcon {
    LayerPrevious,
    LayerNext,
    VolumeDown,
    VolumeMute,
    VolumeUp,
}

fn preview_key_icon(keycode: u16) -> Option<PreviewKeyIcon> {
    match keycode {
        0x7E05 => Some(PreviewKeyIcon::LayerNext),
        0x7E06 => Some(PreviewKeyIcon::LayerPrevious),
        0x00A8 => Some(PreviewKeyIcon::VolumeMute),
        0x00A9 => Some(PreviewKeyIcon::VolumeUp),
        0x00AA => Some(PreviewKeyIcon::VolumeDown),
        _ => None,
    }
}

// U+E257 (EH_SYMBOL_LAYER), exported from the actual firmware LVGL fonts.
// Coverage includes the cut-outs in the lower layers, not three full diamonds.
fn layer_icon_glyph(size: f32, source_size: u8) -> (usize, usize, f32, &'static [u8]) {
    let (font_size, width, height, alpha) = if source_size == 20 {
        (
            20,
            19,
            16,
            include_bytes!("../../assets/display-layer-20.alpha").as_slice(),
        )
    } else {
        (
            28,
            27,
            21,
            include_bytes!("../../assets/display-layer-28.alpha").as_slice(),
        )
    };
    (width, height, size / font_size as f32, alpha)
}

fn paint_layer_icon(ui: &egui::Ui, center: egui::Pos2, size: f32, color: Color32, source_size: u8) {
    let (width, height, pixel, alpha) = layer_icon_glyph(size, source_size);
    let origin = center - egui::vec2(width as f32, height as f32) * (pixel * 0.5);
    let mut mesh = egui::epaint::Mesh::default();
    for (index, coverage) in alpha.iter().enumerate() {
        if *coverage == 0 {
            continue;
        }
        let rect = egui::Rect::from_min_size(
            origin + egui::vec2((index % width) as f32, (index / width) as f32) * pixel,
            egui::vec2(pixel, pixel),
        );
        mesh.add_colored_rect(rect, color.gamma_multiply(*coverage as f32 / 255.0));
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn paint_chevrons(
    ui: &egui::Ui,
    center: egui::Pos2,
    size: f32,
    points_right: bool,
    color: Color32,
) {
    let direction = if points_right { 1.0 } else { -1.0 };
    let stroke = Stroke::new((size * 0.13).max(1.0), color);
    for offset in [-size * 0.18, size * 0.18] {
        let x = center.x + offset * direction;
        ui.painter().line_segment(
            [
                egui::pos2(x - direction * size * 0.18, center.y - size * 0.28),
                egui::pos2(x + direction * size * 0.10, center.y),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                egui::pos2(x + direction * size * 0.10, center.y),
                egui::pos2(x - direction * size * 0.18, center.y + size * 0.28),
            ],
            stroke,
        );
    }
}

fn paint_speaker_icon(
    ui: &egui::Ui,
    center: egui::Pos2,
    size: f32,
    icon: PreviewKeyIcon,
    color: Color32,
) {
    let left = center.x - size * 0.43;
    let body_right = center.x - size * 0.08;
    let top = center.y - size * 0.30;
    let bottom = center.y + size * 0.30;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(left, center.y - size * 0.14),
            egui::pos2(left + size * 0.17, center.y - size * 0.14),
            egui::pos2(body_right, top),
            egui::pos2(body_right, bottom),
            egui::pos2(left + size * 0.17, center.y + size * 0.14),
            egui::pos2(left, center.y + size * 0.14),
        ],
        color,
        Stroke::NONE,
    ));
    let stroke = Stroke::new((size * 0.10).max(1.0), color);
    match icon {
        PreviewKeyIcon::VolumeMute => {
            let x = center.x + size * 0.20;
            let half = size * 0.18;
            ui.painter().line_segment(
                [
                    egui::pos2(x - half, center.y - half),
                    egui::pos2(x + half, center.y + half),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(x + half, center.y - half),
                    egui::pos2(x - half, center.y + half),
                ],
                stroke,
            );
        }
        PreviewKeyIcon::VolumeDown | PreviewKeyIcon::VolumeUp => {
            let wave_count = if icon == PreviewKeyIcon::VolumeDown {
                1
            } else {
                2
            };
            for wave in 0..wave_count {
                let x = center.x + size * (0.05 + wave as f32 * 0.20);
                let height = size * (0.20 + wave as f32 * 0.12);
                ui.painter().add(egui::Shape::line(
                    vec![
                        egui::pos2(x, center.y - height),
                        egui::pos2(x + size * 0.10, center.y - height * 0.48),
                        egui::pos2(x + size * 0.13, center.y),
                        egui::pos2(x + size * 0.10, center.y + height * 0.48),
                        egui::pos2(x, center.y + height),
                    ],
                    stroke,
                ));
            }
        }
        _ => {}
    }
}

fn paint_preview_key_icon(ui: &egui::Ui, rect: egui::Rect, icon: PreviewKeyIcon, color: Color32) {
    let size = rect.height() * 0.52;
    match icon {
        PreviewKeyIcon::LayerPrevious | PreviewKeyIcon::LayerNext => {
            let center = rect.center();
            paint_layer_icon(
                ui,
                egui::pos2(center.x - size * 0.32, center.y),
                size * 0.82,
                color,
                20,
            );
            paint_chevrons(
                ui,
                egui::pos2(center.x + size * 0.38, center.y),
                size,
                icon == PreviewKeyIcon::LayerNext,
                color,
            );
        }
        PreviewKeyIcon::VolumeDown | PreviewKeyIcon::VolumeMute | PreviewKeyIcon::VolumeUp => {
            paint_speaker_icon(ui, rect.center(), size, icon, color);
        }
    }
}

fn compact_label(label: String) -> String {
    label
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(2)
        .map(|line| line.chars().take(9).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn pattern_points_up(variant: u8, index: usize) -> bool {
    let row = index / 3;
    let col = index % 3;
    match variant {
        0 => row % 2 == 0,
        1 => row % 2 != 0,
        2 => col % 2 == 0,
        3 => col % 2 != 0,
        4 => true,
        _ => false,
    }
}

fn paint_button_shape(ui: &egui::Ui, style: u8, index: usize, rect: egui::Rect, color: Color32) {
    use super::display_settings_ui::{
        paint_closed_shape, paint_oval_side_icon, paint_semicircle_icon, paint_trapezoid_icon,
        paint_wave_icon,
    };

    let stroke = Stroke::new((rect.height() / 44.0).clamp(0.85, 1.5), color);
    let wide = rect;
    match style.min(32) {
        0 => {
            ui.painter()
                .rect_stroke(wide, 6.0, stroke, egui::StrokeKind::Inside);
        }
        1 => {
            ui.painter()
                .circle_stroke(rect.center(), (rect.height() - 2.0) / 2.0, stroke);
        }
        2 => {
            ui.painter()
                .rect_stroke(wide, wide.height() / 2.0, stroke, egui::StrokeKind::Inside);
        }
        3 => paint_closed_shape(
            ui,
            &[
                egui::pos2(wide.center().x, wide.top()),
                egui::pos2(wide.right(), wide.center().y),
                egui::pos2(wide.center().x, wide.bottom()),
                egui::pos2(wide.left(), wide.center().y),
            ],
            color,
        ),
        4 => {
            let cut = wide.height() * 0.16;
            paint_closed_shape(
                ui,
                &[
                    egui::pos2(wide.left() + cut, wide.top()),
                    egui::pos2(wide.right() - cut, wide.top()),
                    egui::pos2(wide.right(), wide.top() + cut),
                    egui::pos2(wide.right(), wide.bottom() - cut),
                    egui::pos2(wide.right() - cut, wide.bottom()),
                    egui::pos2(wide.left() + cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.bottom() - cut),
                    egui::pos2(wide.left(), wide.top() + cut),
                ],
                color,
            );
        }
        5 => {
            let cut = wide.height() * 0.23;
            paint_closed_shape(
                ui,
                &[
                    egui::pos2(wide.left() + cut, wide.top()),
                    egui::pos2(wide.right() - cut, wide.top()),
                    egui::pos2(wide.right(), wide.center().y),
                    egui::pos2(wide.right() - cut, wide.bottom()),
                    egui::pos2(wide.left() + cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.center().y),
                ],
                color,
            );
        }
        6 => {
            ui.painter()
                .rect_stroke(wide, wide.height() * 0.32, stroke, egui::StrokeKind::Inside);
        }
        7..=10 | 13..=14 => {
            let row = index / 3;
            let col = index % 3;
            let narrow_top = match style {
                7 => row % 2 == 0,
                8 => row % 2 != 0,
                9 => true,
                10 => false,
                13 => col % 2 == 0,
                _ => col % 2 != 0,
            };
            paint_trapezoid_icon(ui, wide, narrow_top, color);
        }
        11..=12 | 15..=18 => {
            let phase = matches!(style, 12 | 16 | 18) && (index / 3 + index % 3) % 2 != 0;
            paint_wave_icon(
                ui,
                wide,
                matches!(style, 11 | 12 | 17 | 18),
                matches!(style, 15 | 16 | 17 | 18),
                phase,
                color,
            );
        }
        19..=24 => paint_semicircle_icon(ui, wide, pattern_points_up(style - 19, index), color),
        25..=30 => paint_oval_side_icon(ui, wide, pattern_points_up(style - 25, index), color),
        31..=32 => {
            let cut = wide.height() * 0.2;
            let points = if style == 31 {
                vec![
                    egui::pos2(wide.left() + cut, wide.top()),
                    egui::pos2(wide.right(), wide.top()),
                    egui::pos2(wide.right(), wide.bottom() - cut),
                    egui::pos2(wide.right() - cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.bottom()),
                    egui::pos2(wide.left(), wide.top() + cut),
                ]
            } else {
                vec![
                    egui::pos2(wide.left(), wide.top()),
                    egui::pos2(wide.right() - cut, wide.top()),
                    egui::pos2(wide.right(), wide.top() + cut),
                    egui::pos2(wide.right(), wide.bottom()),
                    egui::pos2(wide.left() + cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.bottom() - cut),
                ]
            };
            paint_closed_shape(ui, &points, color);
        }
        _ => {}
    }
}

fn digit_mask(digit: u8) -> u8 {
    [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f][digit.min(9) as usize]
}

fn paint_segment_digit(
    ui: &egui::Ui,
    digit: u8,
    rect: egui::Rect,
    italic: bool,
    color: Color32,
    width: f32,
) {
    let skew = |y: f32| {
        if italic {
            (rect.bottom() - y) * 0.12
        } else {
            0.0
        }
    };
    let (x0, x1, y0, ym, y1) = (
        rect.left(),
        rect.right(),
        rect.top(),
        rect.center().y,
        rect.bottom(),
    );
    let segments = [
        [egui::pos2(x0 + skew(y0), y0), egui::pos2(x1 + skew(y0), y0)],
        [egui::pos2(x1 + skew(y0), y0), egui::pos2(x1 + skew(ym), ym)],
        [egui::pos2(x1 + skew(ym), ym), egui::pos2(x1 + skew(y1), y1)],
        [egui::pos2(x0 + skew(y1), y1), egui::pos2(x1 + skew(y1), y1)],
        [egui::pos2(x0 + skew(ym), ym), egui::pos2(x0 + skew(y1), y1)],
        [egui::pos2(x0 + skew(y0), y0), egui::pos2(x0 + skew(ym), ym)],
        [egui::pos2(x0 + skew(ym), ym), egui::pos2(x1 + skew(ym), ym)],
    ];
    let mask = digit_mask(digit);
    for (index, segment) in segments.into_iter().enumerate() {
        if mask & (1 << index) != 0 {
            ui.painter()
                .line_segment(segment, Stroke::new(width, color));
        }
    }
}

fn paint_hex_segment(
    ui: &egui::Ui,
    start: egui::Pos2,
    end: egui::Pos2,
    thickness: f32,
    horizontal: bool,
    color: Color32,
) {
    let half = (thickness / 2.0).max(1.0);
    let points = if horizontal {
        vec![
            egui::pos2(start.x + half, start.y - half),
            egui::pos2(end.x - half, end.y - half),
            end,
            egui::pos2(end.x - half, end.y + half),
            egui::pos2(start.x + half, start.y + half),
            start,
        ]
    } else {
        vec![
            start,
            egui::pos2(start.x + half, start.y + half),
            egui::pos2(end.x + half, end.y - half),
            end,
            egui::pos2(end.x - half, end.y - half),
            egui::pos2(start.x - half, start.y + half),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn paint_hex_segment_digit(
    ui: &egui::Ui,
    digit: u8,
    rect: egui::Rect,
    italic: bool,
    color: Color32,
) {
    let height = rect.height();
    let rounded = (height / 9.0).max(3.0).round() as i32;
    let thickness = if rounded % 2 == 0 {
        rounded + 1
    } else {
        rounded
    } as f32;
    let half = thickness / 2.0;
    let joint = half + 1.0;
    let mid = height / 2.0;
    let left = rect.left() + half;
    let right = rect.right() - half;
    let skew = |y: f32| if italic { (height - y) / 7.0 } else { 0.0 };
    let y = rect.top();
    let segments = [
        [
            egui::pos2(left + joint + skew(0.0), y),
            egui::pos2(right - joint + skew(0.0), y),
        ],
        [
            egui::pos2(right + skew(joint), y + joint),
            egui::pos2(right + skew(mid - joint), y + mid - joint),
        ],
        [
            egui::pos2(right + skew(mid + joint), y + mid + joint),
            egui::pos2(right + skew(height - joint), y + height - joint),
        ],
        [
            egui::pos2(left + joint + skew(height), y + height),
            egui::pos2(right - joint + skew(height), y + height),
        ],
        [
            egui::pos2(left + skew(mid + joint), y + mid + joint),
            egui::pos2(left + skew(height - joint), y + height - joint),
        ],
        [
            egui::pos2(left + skew(joint), y + joint),
            egui::pos2(left + skew(mid - joint), y + mid - joint),
        ],
        [
            egui::pos2(left + joint + skew(mid), y + mid),
            egui::pos2(right - joint + skew(mid), y + mid),
        ],
    ];
    let mask = digit_mask(digit);
    for (index, segment) in segments.into_iter().enumerate() {
        if mask & (1 << index) != 0 {
            paint_hex_segment(
                ui,
                segment[0],
                segment[1],
                thickness,
                matches!(index, 0 | 3 | 6),
                color,
            );
        }
    }
}

fn paint_dot_digit(ui: &egui::Ui, digit: u8, rect: egui::Rect, color: Color32) {
    let patterns = [
        0b111101101101111u16,
        0b010110010010111,
        0b111001111100111,
        0b111001111001111,
        0b101101111001001,
        0b111100111001111,
        0b111100111101111,
        0b111001001001001,
        0b111101111101111,
        0b111101111001111,
    ];
    let pattern = patterns[digit.min(9) as usize];
    let radius = (rect.width() / 9.0).max(1.0);
    for row in 0..5 {
        for col in 0..3 {
            if pattern & (1 << (14 - (row * 3 + col))) != 0 {
                ui.painter().circle_filled(
                    egui::pos2(
                        rect.left() + (col as f32 + 1.0) * rect.width() / 4.0,
                        rect.top() + (row as f32 + 1.0) * rect.height() / 6.0,
                    ),
                    radius,
                    color,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_clock(
    ui: &egui::Ui,
    screen: egui::Rect,
    style: u8,
    size: u8,
    alignment: u8,
    text_color: Color32,
    background_color: Color32,
    colon_visible: bool,
    digits: [u8; 4],
    center_y: f32,
) {
    let scale = screen.width() / DISPLAY_WIDTH;
    let height = [28.0, 40.0, 48.0, 64.0][size.min(3) as usize] * scale;
    let area = screen_rect(screen, 0.0, 28.0, 240.0, 80.0);
    if style == 0 {
        let font = egui::FontId::proportional(height * 0.82);
        let left = ui.painter().layout_no_wrap(
            format!("{}{}", digits[0], digits[1]),
            font.clone(),
            text_color,
        );
        let colon = ui
            .painter()
            .layout_no_wrap(":".to_owned(), font.clone(), text_color);
        let right =
            ui.painter()
                .layout_no_wrap(format!("{}{}", digits[2], digits[3]), font, text_color);
        let group_width = left.size().x + colon.size().x + right.size().x;
        let x = match alignment.min(2) {
            0 => area.left() + 8.0 * scale,
            2 => area.right() - group_width - 8.0 * scale,
            _ => area.center().x - group_width / 2.0,
        };
        let y = area.center().y - left.size().y / 2.0;
        ui.painter()
            .galley(egui::pos2(x, y), left.clone(), text_color);
        if colon_visible {
            ui.painter()
                .galley(egui::pos2(x + left.size().x, y), colon.clone(), text_color);
        }
        ui.painter().galley(
            egui::pos2(x + left.size().x + colon.size().x, y),
            right,
            text_color,
        );
        return;
    }

    let digit_width = height * 0.52;
    let gap = height * 0.13;
    let colon_width = height * 0.16;
    let total_width = digit_width * 4.0 + gap * 4.0 + colon_width;
    let mut x = match alignment.min(2) {
        0 => area.left() + 8.0 * scale,
        2 => area.right() - total_width - 8.0 * scale,
        _ => area.center().x - total_width / 2.0,
    };
    let y = area.center().y - height / 2.0;
    for (index, digit) in digits.into_iter().enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(digit_width, height));
        match style.min(6) {
            3 => {
                ui.painter()
                    .rect_filled(rect, rect.width() * 0.45, background_color);
                ui.painter().rect_stroke(
                    rect,
                    rect.width() * 0.45,
                    Stroke::new(scale, text_color.gamma_multiply(0.65)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    digit.to_string(),
                    egui::FontId::monospace(height * 0.72),
                    text_color,
                );
            }
            4 => {
                ui.painter()
                    .rect_filled(rect, 3.0 * scale, background_color);
                ui.painter().rect_stroke(
                    rect,
                    3.0 * scale,
                    Stroke::new(scale, text_color.gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left(), rect.center().y),
                        egui::pos2(rect.right(), rect.center().y),
                    ],
                    Stroke::new(scale, Color32::BLACK),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    digit.to_string(),
                    egui::FontId::monospace(height * 0.72),
                    text_color,
                );
            }
            5 => paint_dot_digit(ui, digit, rect, text_color),
            6 => {
                paint_segment_digit(
                    ui,
                    digit,
                    rect,
                    false,
                    text_color.gamma_multiply(0.25),
                    (height / 6.0).max(3.0),
                );
                paint_segment_digit(ui, digit, rect, false, text_color, (height / 18.0).max(1.5));
            }
            _ => paint_hex_segment_digit(ui, digit, rect, style == 2, text_color),
        }
        x += digit_width + gap;
        if index == 1 {
            if colon_visible {
                let radius = (height / 18.0).max(1.0);
                for fraction in [1.0 / 3.0, 2.0 / 3.0] {
                    ui.painter().circle_filled(
                        egui::pos2(x + colon_width / 2.0, y + height * fraction),
                        radius,
                        text_color,
                    );
                }
            }
            x += colon_width + gap;
        }
    }
}

fn paint_clock_font(
    ui: &egui::Ui,
    screen: egui::Rect,
    font_choice: u8,
    size: u8,
    alignment: u8,
    text_color: Color32,
    colon_visible: bool,
    digits: [u8; 4],
    center_y: f32,
) {
    let scale = screen.width() / DISPLAY_WIDTH;
    let height = [28.0, 40.0, 48.0, 64.0][size.min(3) as usize] * scale;
    let family_name = [
        "clock_montserrat",
        "clock_ubuntu_sans",
        "clock_ubuntu_mono",
        "clock_liberation_mono",
        "clock_dejavu_sans",
        "clock_dejavu_serif",
        "clock_dejavu_mono",
        "clock_liberation_sans",
        "clock_liberation_serif",
        "clock_liberation_narrow",
    ][font_choice.min(9) as usize];
    let font = egui::FontId::new(height, egui::FontFamily::Name(family_name.into()));
    let left = ui.painter().layout_no_wrap(
        format!("{}{}", digits[0], digits[1]),
        font.clone(),
        text_color,
    );
    let colon = ui
        .painter()
        .layout_no_wrap(":".to_owned(), font.clone(), text_color);
    let right = ui.painter().layout_no_wrap(
        format!("{}{}", digits[2], digits[3]),
        font.clone(),
        text_color,
    );
    let colon_start = left.size().x;
    let right_start = colon_start + colon.size().x;
    // Advance widths include side bearings; only visible meshes determine the
    // clock's center. Always include the colon, including its blink-off frame.
    let ink = left
        .mesh_bounds
        .union(colon.mesh_bounds.translate(egui::vec2(colon_start, 0.0)))
        .union(right.mesh_bounds.translate(egui::vec2(right_start, 0.0)));
    let area = screen_rect(screen, 15.0, 33.0, 210.0, 80.0);
    let x = match alignment.min(2) {
        0 => area.left() + 8.0 * scale - ink.left(),
        2 => area.right() - 8.0 * scale - ink.right(),
        _ => area.center().x - ink.center().x,
    };
    let y = center_y - ink.center().y;
    ui.painter().galley(egui::pos2(x, y), left, text_color);
    if colon_visible {
        ui.painter()
            .galley(egui::pos2(x + colon_start, y), colon, text_color);
    }
    ui.painter()
        .galley(egui::pos2(x + right_start, y), right, text_color);
}

impl EntropyApp {
    fn display_preview_key(&self, row: u8, col: u8) -> (u16, String) {
        let Some(layout) = self.layout.as_ref() else {
            return (0, String::new());
        };
        let Some(index) = layout
            .keys
            .iter()
            .position(|key| key.row == row && key.col == col)
        else {
            return (0, String::new());
        };
        let layer = (0..=self.selected_layer)
            .rev()
            .find(|&layer| layout.get_keycode(layer, index) != 1)
            .unwrap_or(0);
        (
            layout.get_keycode(layer, index),
            compact_label(crate::app::key_binding_label_with_macro_names(
                layout.get_key_binding(layer, index),
                &layout.custom_keycodes,
                &self.layer_names,
                &self.keycode_picker.macro_names,
                &self.keycode_picker.tap_dance_names,
                self.app_settings.key_legend_layout,
            )),
        )
    }

    fn display_preview_encoder(&self, direction: u8) -> (u16, String) {
        let Some(layout) = self.layout.as_ref() else {
            return (0, String::new());
        };
        let Some(index) = layout
            .encoders
            .iter()
            .position(|encoder| encoder.encoder_idx == 0 && encoder.direction == direction)
        else {
            return (0, String::new());
        };
        let layer = (0..=self.selected_layer)
            .rev()
            .find(|&layer| layout.get_encoder_keycode(layer, index) != 1)
            .unwrap_or(0);
        let keycode = layout.get_encoder_keycode(layer, index);
        (
            keycode,
            compact_label(crate::app::keycode_label_with_macro_names(
                keycode,
                &layout.custom_keycodes,
                &self.layer_names,
                &self.keycode_picker.macro_names,
                &self.keycode_picker.tap_dance_names,
                self.app_settings.key_legend_layout,
            )),
        )
    }

    fn standby_preview_texture(&self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let frames = &self.display_settings.clock_background_preview_frames_rgba;
        let frame_index = if frames.is_empty() {
            0
        } else {
            animation_frame_index(
                &self.display_settings.clock_background_preview_delays_ms,
                frames.len(),
                (ctx.input(|input| input.time) * 1000.0).max(0.0) as u64,
                self.display_settings.clock_background_speed_percent,
            )
        };
        let rgba = frames
            .get(frame_index)
            .unwrap_or(&self.display_settings.clock_background_preview_rgba);
        if rgba.len() != 240usize * 280usize * 4 {
            return None;
        }
        if frames.len() > 1 {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
        let id = egui::Id::new((
            "standby_background_preview",
            self.connection_generation,
            self.display_settings.clock_background_preview_revision,
            frame_index,
        ));
        if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
            return Some(texture);
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([240, 280], rgba);
        let texture = ctx.load_texture(
            format!("standby-background-preview-{frame_index}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    }

    fn startup_preview_texture(&self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let rgba = &self.display_settings.startup_image_preview_rgba;
        if rgba.len() != 240usize * 280usize * 4 {
            return None;
        }
        let id = egui::Id::new((
            "startup_image_preview",
            self.connection_generation,
            self.display_settings.startup_image_preview_revision,
        ));
        if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
            return Some(texture);
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([240, 280], rgba);
        let texture =
            ctx.load_texture("startup-image-preview", image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    }

    fn builtin_startup_logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let id = egui::Id::new("m4cr0pad_builtin_startup_logo");
        if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
            return Some(texture);
        }
        let image = image::load_from_memory(include_bytes!(
            "../../assets/ergohaven-macropad-startup-logo.png"
        ))
        .ok()?
        .into_rgba8();
        if image.dimensions() != (240, 72) {
            return None;
        }
        let color_image = egui::ColorImage::from_rgba_unmultiplied([240, 72], image.as_raw());
        let texture = ctx.load_texture(
            "m4cr0pad-builtin-startup-logo",
            color_image,
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    }

    fn paint_startup_display_preview(&self, ui: &mut egui::Ui, screen: egui::Rect) {
        let background = Color32::from_rgb(
            self.display_settings.background_color[0],
            self.display_settings.background_color[1],
            self.display_settings.background_color[2],
        );
        let corner_radius = display_corner_radius(screen.width());
        ui.painter().rect_filled(screen, corner_radius, background);
        if let Some(texture) = self.startup_preview_texture(ui.ctx()) {
            ui.put(
                screen,
                egui::Image::new(&texture)
                    .fit_to_exact_size(screen.size())
                    .corner_radius(corner_radius),
            );
        } else {
            if let Some(texture) = Self::builtin_startup_logo_texture(ui.ctx()) {
                ui.put(
                    screen_rect(screen, 0.0, 68.0, 240.0, 72.0),
                    egui::Image::new(&texture)
                        .fit_to_exact_size(screen_rect(screen, 0.0, 68.0, 240.0, 72.0).size()),
                );
            }
            ui.painter().text(
                screen_rect(screen, 0.0, 232.0, 240.0, 24.0).center(),
                egui::Align2::CENTER_CENTER,
                "v4.0.6",
                egui::FontId::new(
                    14.0 * screen.width() / DISPLAY_WIDTH,
                    egui::FontFamily::Name("display_preview".into()),
                ),
                Color32::WHITE,
            );
        }
    }

    fn paint_main_display_preview(&self, ui: &mut egui::Ui, screen: egui::Rect) {
        let accent = Color32::from_rgb(
            self.display_settings.color[0],
            self.display_settings.color[1],
            self.display_settings.color[2],
        );
        let background = Color32::from_rgb(
            self.display_settings.background_color[0],
            self.display_settings.background_color[1],
            self.display_settings.background_color[2],
        );
        let corner_radius = display_corner_radius(screen.width());
        ui.painter().rect_filled(screen, corner_radius, background);
        let scale = screen.width() / DISPLAY_WIDTH;
        let saved_clip = ui.clip_rect();
        ui.set_clip_rect(saved_clip.intersect(screen.shrink(DISPLAY_CONTENT_INSET * scale)));
        let layer_name = self
            .layer_names
            .get(self.selected_layer)
            .cloned()
            .unwrap_or_else(|| format!("Layer {}", self.selected_layer));
        let header = screen_rect(screen, MAIN_HEADER_X, 9.0, MAIN_HEADER_WIDTH, 38.0);
        let header_font = display_preview_font(28.0, scale);
        let name_galley = ui
            .painter()
            .layout_no_wrap(layer_name.clone(), header_font, accent);
        let icon_width = 28.0 * scale;
        let gap = 6.0 * scale;
        if !layer_name_needs_scrolling(name_galley.size().x, header.width(), icon_width, gap) {
            let group_width = icon_width + gap + name_galley.size().x;
            let group_left = header.center().x - group_width / 2.0;
            paint_layer_icon(
                ui,
                egui::pos2(group_left + icon_width / 2.0, header.center().y),
                28.0 * scale,
                accent,
                28,
            );
            ui.painter().galley(
                egui::pos2(
                    group_left + icon_width + gap,
                    header.center().y - name_galley.size().y / 2.0,
                ),
                name_galley,
                accent,
            );
        } else {
            paint_layer_icon(
                ui,
                egui::pos2(header.left() + icon_width / 2.0, header.center().y),
                28.0 * scale,
                accent,
                28,
            );
            let name_rect = egui::Rect::from_min_max(
                egui::pos2(header.left() + icon_width + gap, header.top()),
                header.max,
            );
            let scroll_gap = 24.0 * scale;
            let cycle = name_galley.size().x + scroll_gap;
            let now = ui.ctx().input(|input| input.time.max(0.0));
            let scroll_id = egui::Id::new((
                "display_preview_layer_scroll",
                self.selected_layer,
                layer_name,
            ));
            let started_at = ui.ctx().data_mut(|data| {
                if let Some(started_at) = data.get_temp::<f64>(scroll_id) {
                    started_at
                } else {
                    data.insert_temp(scroll_id, now);
                    now
                }
            });
            let elapsed = (now - started_at).max(0.0) as f32;
            let offset = (elapsed * 30.0 * scale) % cycle;
            let text_y = name_rect.center().y - name_galley.size().y / 2.0;
            let clipped = ui.painter().with_clip_rect(name_rect);
            clipped.galley(
                egui::pos2(name_rect.left() - offset, text_y),
                name_galley.clone(),
                accent,
            );
            clipped.galley(
                egui::pos2(name_rect.left() - offset + cycle, text_y),
                name_galley,
                accent,
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }

        for row in 0..4usize {
            for col in 0..3usize {
                let index = row * 3 + col;
                let cell = screen_rect(
                    screen,
                    MAIN_GRID_X + col as f32 * MAIN_CELL_WIDTH,
                    MAIN_GRID_Y + row as f32 * MAIN_CELL_HEIGHT,
                    MAIN_SHAPE_WIDTH,
                    MAIN_SHAPE_HEIGHT,
                );
                paint_button_shape(ui, self.display_settings.button_style, index, cell, accent);
                let (keycode, label) = self.display_preview_key(row as u8 + 1, col as u8);
                #[cfg(not(target_arch = "wasm32"))]
                if let Some((bitmap, icon_color)) =
                    pictogram_keycode_slot(keycode).and_then(|(kind, slot)| {
                        self.display_settings
                            .pictograms
                            .library
                            .bitmap(kind, slot)
                            .map(|bitmap| (bitmap, self.display_settings.color))
                    })
                {
                    let icon_rect = egui::Rect::from_center_size(
                        cell.center(),
                        egui::vec2(35.0 * scale, 35.0 * scale),
                    );
                    super::display_settings_ui::paint_monochrome_pictogram(
                        ui,
                        icon_rect,
                        bitmap,
                        Color32::from_rgb(icon_color[0], icon_color[1], icon_color[2]),
                        Color32::TRANSPARENT,
                    );
                    continue;
                }
                if let Some(icon) = preview_key_icon(keycode) {
                    paint_preview_key_icon(ui, cell, icon, accent);
                } else {
                    ui.painter().text(
                        cell.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        display_preview_font(20.0, scale),
                        accent,
                    );
                }
            }
        }

        let encoder_keys = [
            self.display_preview_encoder(0),
            self.display_preview_key(0, 2),
            self.display_preview_encoder(1),
        ];
        for (index, (keycode, label)) in encoder_keys.into_iter().enumerate() {
            let cell = screen_rect(
                screen,
                MAIN_GRID_X + index as f32 * MAIN_CELL_WIDTH,
                MAIN_ENCODER_ROW_Y,
                MAIN_CELL_WIDTH,
                MAIN_CELL_HEIGHT,
            );
            #[cfg(not(target_arch = "wasm32"))]
            if let Some((bitmap, icon_color)) =
                pictogram_keycode_slot(keycode).and_then(|(kind, slot)| {
                    self.display_settings
                        .pictograms
                        .library
                        .bitmap(kind, slot)
                        .map(|bitmap| (bitmap, self.display_settings.color))
                })
            {
                let icon_rect = egui::Rect::from_center_size(
                    cell.center(),
                    egui::vec2(35.0 * scale, 35.0 * scale),
                );
                super::display_settings_ui::paint_monochrome_pictogram(
                    ui,
                    icon_rect,
                    bitmap,
                    Color32::from_rgb(icon_color[0], icon_color[1], icon_color[2]),
                    Color32::TRANSPARENT,
                );
                continue;
            }
            if let Some(icon) = preview_key_icon(keycode) {
                paint_preview_key_icon(ui, cell, icon, accent);
            } else {
                ui.painter().text(
                    cell.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    display_preview_font(20.0, scale),
                    accent,
                );
            }
        }

        ui.set_clip_rect(saved_clip);
        let brightness = self.display_settings.brightness.min(100);
        if brightness < 100 {
            ui.painter().rect_filled(
                screen,
                corner_radius,
                Color32::from_black_alpha(((100 - brightness) as u16 * 255 / 100) as u8),
            );
        }
    }

    fn standby_preview_language(&self) -> &'static str {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(language) = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .and_then(|device| self.qmk_hid_hosts.get(&device.path))
            .and_then(|bridge| bridge.layout_label())
        {
            return language;
        }
        "—"
    }

    fn paint_standby_display_preview(&self, ui: &mut egui::Ui, screen: egui::Rect) {
        let background = Color32::from_rgb(
            self.display_settings.clock_background_color[0],
            self.display_settings.clock_background_color[1],
            self.display_settings.clock_background_color[2],
        );
        let text_color = Color32::from_rgba_unmultiplied(
            self.display_settings.clock_text_color[0],
            self.display_settings.clock_text_color[1],
            self.display_settings.clock_text_color[2],
            255,
        );
        let info_color = text_color;
        let corner_radius = display_corner_radius(screen.width());
        ui.painter().rect_filled(screen, corner_radius, background);
        let has_asset = self.display_settings.clock_background_kind != 0;
        if has_asset {
            if let Some(texture) = self.standby_preview_texture(ui.ctx()) {
                ui.put(
                    screen,
                    egui::Image::new(&texture)
                        .fit_to_exact_size(screen.size())
                        .corner_radius(corner_radius),
                );
            }
            let dim = self.display_settings.clock_background_dim.min(100);
            if dim > 0 {
                ui.painter().rect_filled(
                    screen,
                    corner_radius,
                    Color32::from_rgba_unmultiplied(
                        background.r(),
                        background.g(),
                        background.b(),
                        (dim as u16 * 255 / 100) as u8,
                    ),
                );
            }
        }

        let saved_clip = ui.clip_rect();
        ui.set_clip_rect(
            saved_clip
                .intersect(screen.shrink(DISPLAY_CONTENT_INSET * screen.width() / DISPLAY_WIDTH)),
        );
        let time = chrono::Local::now().format("%H%M").to_string();
        let bytes = time.as_bytes();
        let digits = [
            bytes[0] - b'0',
            bytes[1] - b'0',
            bytes[2] - b'0',
            bytes[3] - b'0',
        ];
        let colon_visible = !self.display_settings.clock_colon_blink
            || chrono::Local::now().timestamp_subsec_millis() < 500;
        if self.display_settings.clock_visible && self.display_settings.clock_colon_blink {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        let scale = screen.width() / DISPLAY_WIDTH;
        let family = [
            "clock_montserrat",
            "clock_ubuntu_sans",
            "clock_ubuntu_mono",
            "clock_liberation_mono",
            "clock_dejavu_sans",
            "clock_dejavu_serif",
            "clock_dejavu_mono",
            "clock_liberation_sans",
            "clock_liberation_serif",
            "clock_liberation_narrow",
        ][self.display_settings.clock_style.min(9) as usize];
        let standby_font =
            |size: f32| egui::FontId::new(size * scale, egui::FontFamily::Name(family.into()));
        let layer_name = self
            .layer_names
            .get(self.selected_layer)
            .cloned()
            .unwrap_or_else(|| format!("Layer {}", self.selected_layer));
        // Additional inset for the rounded physical display corners.
        let info = screen_rect(screen, 31.0, 16.0, 178.0, 28.0);
        // Measure visible ink rather than font line-box padding. Keep these
        // reference positions when a row is disabled, avoiding clock jumps.
        let language = self.standby_preview_language();
        let mut header_bottom = info.center().y + 8.0 * scale;
        for (text, font) in [
            (layer_name.clone(), standby_font(20.0)),
            ("EN".to_owned(), standby_font(20.0)),
            ("RU".to_owned(), standby_font(20.0)),
        ] {
            let galley = ui.painter().layout_no_wrap(text, font, info_color);
            if galley.mesh_bounds.is_positive() {
                header_bottom = header_bottom
                    .max(info.center().y - galley.size().y / 2.0 + galley.mesh_bounds.bottom());
            }
        }
        let date_top;
        if self.display_settings.clock_info_visible {
            let painter = ui.painter().with_clip_rect(info.intersect(screen));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            paint_layer_icon(
                ui,
                egui::pos2(info.left() + 10.0 * scale, info.center().y),
                20.0 * scale,
                info_color,
                20,
            );
            let name_rect = screen_rect(screen, 57.0, 16.0, 110.0, 28.0);
            let name_painter = painter.with_clip_rect(name_rect);
            let galley = name_painter.layout_no_wrap(layer_name, standby_font(20.0), info_color);
            let y = name_rect.center().y - galley.size().y / 2.0;
            if galley.size().x > name_rect.width() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
                let gap = name_painter
                    .layout_no_wrap("   ".into(), standby_font(20.0), info_color)
                    .size()
                    .x;
                let period = galley.size().x + gap;
                let offset = (ui.input(|i| i.time) as f32 * 25.0 * scale) % period;
                for x in [
                    name_rect.left() - offset,
                    name_rect.left() - offset + period,
                ] {
                    name_painter.galley(egui::pos2(x, y), galley.clone(), info_color);
                }
            } else {
                name_painter.galley(egui::pos2(name_rect.left(), y), galley, info_color);
            }
            painter.text(
                egui::pos2(info.right(), info.center().y),
                egui::Align2::RIGHT_CENTER,
                language,
                standby_font(20.0),
                info_color,
            );
        }
        {
            let d = self.display_settings.date;
            let text = chrono::Local::now()
                .format(match d[8] {
                    1 => "%m.%d.%Y",
                    2 => "%Y.%m.%d",
                    _ => "%d.%m.%Y",
                })
                .to_string();
            let rect = screen_rect(screen, 15.0, 120.0, 210.0, 24.0);
            let color = text_color;
            let family = [
                "clock_montserrat",
                "clock_ubuntu_sans",
                "clock_ubuntu_mono",
                "clock_liberation_mono",
                "clock_dejavu_sans",
                "clock_dejavu_serif",
                "clock_dejavu_mono",
                "clock_liberation_sans",
                "clock_liberation_serif",
                "clock_liberation_narrow",
            ][self.display_settings.clock_style.min(9) as usize];
            let galley = ui.painter().layout_no_wrap(
                text,
                egui::FontId::new(20.0 * scale, egui::FontFamily::Name(family.into())),
                color,
            );
            let x = rect.center().x - galley.size().x / 2.0;
            let y = rect.center().y - galley.size().y / 2.0;
            date_top = y + galley.mesh_bounds.top();
            if self.display_settings.date_supported && self.display_settings.date[0] != 0 {
                ui.painter()
                    .with_clip_rect(rect)
                    .galley(egui::pos2(x, y), galley, color);
            }
        }
        if self.display_settings.clock_visible {
            paint_clock_font(
                ui,
                screen,
                self.display_settings.clock_style,
                self.display_settings.clock_size,
                self.display_settings.clock_alignment,
                text_color,
                colon_visible,
                digits,
                (header_bottom + date_top) / 2.0,
            );
        }

        let media = if self.display_settings.date[11] != 0 {
            crate::qmk_hid_host::media_snapshot()
        } else {
            None
        };
        let media_color = text_color;
        if let Some((artist, title)) = media.as_ref() {
            let title_rect = screen_rect(screen, 15.0, 142.0, 210.0, 36.0);
            let artist_rect = screen_rect(screen, 15.0, 183.0, 210.0, 30.0);
            for (rect, text, pixels, color) in [
                (title_rect, title, 20.0, media_color),
                (artist_rect, artist, 20.0, media_color),
            ] {
                let painter = ui.painter().with_clip_rect(rect.intersect(screen));
                let galley = painter.layout_no_wrap(text.clone(), standby_font(pixels), color);
                let y = rect.center().y - galley.size().y / 2.0;
                if galley.size().x > rect.width() {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(16));
                    // LVGL circular labels use three space glyphs between copies.
                    let gap = painter
                        .layout_no_wrap("   ".into(), standby_font(pixels), color)
                        .size()
                        .x;
                    let period = galley.size().x + gap;
                    let offset = (ui.input(|i| i.time) as f32 * 25.0 * scale) % period;
                    let x = rect.left() - offset;
                    painter.galley(egui::pos2(x, y), galley.clone(), color);
                    painter.galley(egui::pos2(x + period, y), galley, color);
                } else {
                    painter.galley(
                        egui::pos2(rect.center().x - galley.size().x / 2.0, y),
                        galley,
                        color,
                    );
                }
            }
        }
        ui.set_clip_rect(saved_clip);
        let brightness = self.display_settings.date[10].min(100);
        if brightness < 100 {
            ui.painter().rect_filled(
                screen,
                corner_radius,
                Color32::from_black_alpha(((100 - brightness) as u16 * 255 / 100) as u8),
            );
        }
    }

    pub(super) fn draw_display_preview_panel(
        &mut self,
        ui: &mut egui::Ui,
        max_height: f32,
        mode: u8,
    ) {
        let display_size = fitted_display_size(
            (ui.available_width() - 20.0).max(1.0),
            (max_height - 14.0).max(1.0),
        );
        let (outer, _) =
            ui.allocate_exact_size(display_size + egui::vec2(14.0, 14.0), Sense::hover());
        let frame_color = Color32::from_gray(24);
        let corner_radius = display_corner_radius(display_size.x);
        ui.painter().rect(
            outer,
            corner_radius + 7.0,
            frame_color,
            Stroke::new(1.0_f32, Color32::from_gray(75)),
            egui::StrokeKind::Inside,
        );
        let screen = egui::Rect::from_center_size(outer.center(), display_size);
        ui.painter()
            .rect_filled(screen, corner_radius, Color32::BLACK);
        if mode == 0 {
            self.paint_startup_display_preview(ui, screen);
        } else if mode == 2 {
            self.paint_standby_display_preview(ui, screen);
        } else {
            self.paint_main_display_preview(ui, screen);
        }
        ui.painter().rect_stroke(
            screen,
            corner_radius,
            Stroke::new(1.0_f32, Color32::from_gray(8)),
            egui::StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use image::GenericImageView;

    use super::*;

    fn single_key_layout(keycode: u16) -> KeyboardLayout {
        KeyboardLayout {
            name: "Async key write".into(),
            rows: 2,
            cols: 1,
            keys: vec![PhysicalKey {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                row: 1,
                col: 0,
                label: String::new(),
                rotation: 0.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                layout_condition: None,
            }],
            encoders: vec![],
            layers: vec![vec![keycode.into()]],
            encoder_layers: vec![vec![]],
            layer_names: vec!["Layer 0".into()],
            custom_keycodes: vec![],
            layout_options: vec![],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn transparent_preview_inherits_nearest_lower_assignment_and_stops_at_no_key() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&cc);
        let mut layout = single_key_layout(0x7700);
        layout.layers = vec![
            vec![0x7700.into()],
            vec![1.into()],
            vec![1.into()],
            vec![0x00e9.into()],
            vec![1.into()],
        ];
        layout.encoders = vec![PhysicalEncoder {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            label: String::new(),
            encoder_idx: 0,
            direction: 0,
            rotation: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            layout_condition: None,
        }];
        layout.encoder_layers = vec![vec![0x7700], vec![1], vec![1], vec![0x00ea], vec![1]];
        app.layout = Some(layout);
        app.selected_layer = 4;
        assert_eq!(app.display_preview_key(1, 0).0, 0x00e9);
        assert_eq!(app.display_preview_encoder(0).0, 0x00ea);
        app.layout.as_mut().unwrap().layers[3][0] = 0.into();
        app.layout.as_mut().unwrap().encoder_layers[3][0] = 0;
        assert_eq!(app.display_preview_key(1, 0).0, 0);
        assert_eq!(app.display_preview_encoder(0).0, 0);
    }

    #[test]
    fn assigned_icon_preview_follows_live_accent_without_reloading_library() {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.families.insert(
            egui::FontFamily::Name("display_preview".into()),
            fonts.families[&egui::FontFamily::Proportional].clone(),
        );
        ctx.set_fonts(fonts);
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let mut bitmap = vec![0u8; PICTOGRAM_BYTES];
        bitmap[0] = 0x80;
        app.display_settings.pictograms.library.set_colored(
            PictogramKind::Macro,
            0,
            &bitmap,
            [255, 0, 255],
        );
        app.layout = Some(single_key_layout(0x7700));
        for color in [[255, 255, 255], [19, 137, 220]] {
            app.display_settings.color = color;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.paint_main_display_preview(
                        ui,
                        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(240.0, 280.0)),
                    );
                });
            });
            let icons: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.vertices.len() == 4 => Some(mesh),
                    _ => None,
                })
                .collect();
            assert_eq!(icons.len(), 1);
            let expected = Color32::from_rgb(color[0], color[1], color[2]);
            assert!(icons[0]
                .vertices
                .iter()
                .all(|vertex| vertex.color == expected));
        }
    }

    #[test]
    fn builtin_startup_logo_matches_the_firmware_asset_dimensions() {
        let image = image::load_from_memory(include_bytes!(
            "../../assets/ergohaven-macropad-startup-logo.png"
        ))
        .unwrap();
        assert_eq!(image.dimensions(), (240, 72));
    }

    #[test]
    fn display_preview_keeps_physical_aspect_ratio() {
        let size = fitted_display_size(240.0, 1000.0);
        assert_eq!(size, egui::vec2(240.0, 280.0));

        let size = fitted_display_size(1000.0, 140.0);
        assert_eq!(size, egui::vec2(120.0, 140.0));
    }

    #[test]
    fn three_millimeter_corner_radius_matches_the_169_inch_panel() {
        let radius = display_corner_radius(240.0);

        assert!((radius - 25.77).abs() < 0.05, "radius={radius}");
    }

    #[test]
    fn compact_labels_are_bounded_to_two_short_lines() {
        assert_eq!(
            compact_label("ABCDEFGHIJK\nsecond line\nthird".to_owned()),
            "ABCDEFGHI\nsecond li"
        );
    }

    #[test]
    fn long_layer_names_switch_to_the_firmware_scroll_layout() {
        assert!(!layer_name_needs_scrolling(170.0, 204.0, 28.0, 6.0));
        assert!(layer_name_needs_scrolling(170.1, 204.0, 28.0, 6.0));
    }

    #[test]
    fn main_screen_geometry_matches_firmware_source_pixels() {
        assert_eq!(MAIN_HEADER_WIDTH, 194.0);
        assert_eq!(MAIN_GRID_X, 9.0);
        assert_eq!(MAIN_GRID_Y, 47.0);
        assert_eq!((MAIN_CELL_WIDTH, MAIN_CELL_HEIGHT), (74.0, 43.0));
        assert_eq!((MAIN_SHAPE_WIDTH, MAIN_SHAPE_HEIGHT), (73.0, 42.0));
        assert_eq!(MAIN_ENCODER_ROW_Y, 224.0);
        assert_eq!(
            DISPLAY_HEIGHT - (MAIN_ENCODER_ROW_Y + MAIN_CELL_HEIGHT),
            13.0
        );
    }

    #[test]
    fn button_pattern_direction_matches_firmware_grid_rules() {
        assert!(pattern_points_up(0, 0));
        assert!(!pattern_points_up(0, 3));
        assert!(pattern_points_up(2, 0));
        assert!(!pattern_points_up(2, 1));
    }

    #[test]
    fn firmware_main_screen_icons_use_vector_previews() {
        assert_eq!(preview_key_icon(0x7E05), Some(PreviewKeyIcon::LayerNext));
        assert_eq!(
            preview_key_icon(0x7E06),
            Some(PreviewKeyIcon::LayerPrevious)
        );
        assert_eq!(preview_key_icon(0x00A8), Some(PreviewKeyIcon::VolumeMute));
        assert_eq!(preview_key_icon(0x00A9), Some(PreviewKeyIcon::VolumeUp));
        assert_eq!(preview_key_icon(0x00AA), Some(PreviewKeyIcon::VolumeDown));
        assert_eq!(preview_key_icon(0x0007), None);
    }

    #[test]
    fn gif_preview_uses_the_firmware_frame_delays() {
        let delays = [100, 200];

        assert_eq!(animation_frame_index(&delays, 2, 0, 100), 0);
        assert_eq!(animation_frame_index(&delays, 2, 99, 100), 0);
        assert_eq!(animation_frame_index(&delays, 2, 100, 100), 1);
        assert_eq!(animation_frame_index(&delays, 2, 299, 100), 1);
        assert_eq!(animation_frame_index(&delays, 2, 300, 100), 0);
        assert_eq!(animation_frame_index(&delays, 2, 50, 200), 1);
        assert_eq!(animation_frame_index(&delays, 2, 200, 50), 1);
    }
}
