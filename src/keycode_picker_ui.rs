use super::*;

fn inactive_picker_entry_text(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_gray(55)
    } else {
        egui::Color32::from_gray(175)
    }
}

fn picker_keycap_fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(48, 48, 52)
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}

fn picker_keycap_text(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(239, 233, 232)
    } else {
        Color32::from_rgb(26, 26, 30)
    }
}

fn picker_keycap_secondary_text(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(130, 130, 145)
    } else {
        Color32::from_rgb(130, 130, 150)
    }
}

pub(super) fn apply_picker_button_visuals(ui: &mut egui::Ui) {
    let dark_mode = ui.visuals().dark_mode;
    let visuals = ui.visuals_mut();
    let key_fill = picker_keycap_fill(dark_mode);
    visuals.widgets.inactive.bg_fill = key_fill;
    visuals.widgets.inactive.weak_bg_fill = key_fill;
    let picker_hover_fill = crate::ui_style::hover_fill(dark_mode);
    visuals.widgets.hovered.bg_fill = picker_hover_fill;
    visuals.widgets.hovered.weak_bg_fill = picker_hover_fill;
    visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.active.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.open.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.open.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);
    if dark_mode {
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(54, 54, 58));
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(54, 54, 58));
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, crate::ui_style::accent());
        visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, crate::ui_style::accent());
    } else {
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(230, 230, 233));
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(230, 230, 233));
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, crate::ui_style::accent());
        visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, crate::ui_style::accent());
    }
}

pub(super) fn popup_key_button_size(ui: &egui::Ui, _label: &str) -> Vec2 {
    responsive_picker_key_size(ui.ctx())
}

pub(super) fn picker_keycap_button_in_rect(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    enabled: bool,
    active: bool,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let resp = ui.allocate_rect(rect, sense);
    let dark = ui.visuals().dark_mode;
    let hovered = enabled && resp.hovered();
    let pressed = enabled && resp.is_pointer_button_down_on();
    let stroke = crate::ui_style::modal_outline_stroke(dark);
    let fill = if active {
        crate::ui_style::accent()
    } else if pressed {
        if dark {
            Color32::from_rgb(56, 56, 59)
        } else {
            Color32::from_rgb(232, 232, 235)
        }
    } else if hovered {
        crate::ui_style::hover_fill(dark)
    } else {
        picker_keycap_fill(dark)
    };
    ui.painter()
        .rect(rect, 9.0, fill, stroke, egui::StrokeKind::Inside);

    let text_color = if active {
        Color32::WHITE
    } else if enabled {
        picker_keycap_text(dark)
    } else {
        crate::ui_style::muted_text(dark)
    };
    let label_scale = (rect.height() / 54.0).clamp(1.0, 1.22);
    let (top_size, bottom_size) = key_label_font_sizes(label);
    let top_size = top_size.map(|size| size * label_scale);
    let bottom_size = bottom_size * label_scale;
    if let Some((top, bottom)) = label.split_once('\n') {
        let top_color = if active {
            text_color.gamma_multiply(0.75)
        } else if enabled {
            picker_keycap_secondary_text(dark)
        } else {
            crate::ui_style::muted_text(dark).gamma_multiply(0.75)
        };
        let top_galley = ui.painter().layout_no_wrap(
            top.to_owned(),
            egui::FontId::proportional(top_size.unwrap_or(9.0)),
            top_color,
        );
        let bottom_galley = ui.painter().layout_no_wrap(
            bottom.to_owned(),
            egui::FontId::proportional(bottom_size),
            text_color,
        );
        ui.painter().galley(
            egui::pos2(
                rect.center().x - top_galley.size().x / 2.0,
                rect.center().y - 7.0 * label_scale - top_galley.size().y / 2.0,
            ),
            top_galley,
            top_color,
        );
        ui.painter().galley(
            egui::pos2(
                rect.center().x - bottom_galley.size().x / 2.0,
                rect.center().y + 6.0 * label_scale - bottom_galley.size().y / 2.0,
            ),
            bottom_galley,
            text_color,
        );
    } else {
        let galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(bottom_size),
            text_color,
        );
        ui.painter().galley(
            egui::pos2(
                rect.center().x - galley.size().x / 2.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            text_color,
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(super) fn picker_keycap_button(
    ui: &mut egui::Ui,
    label: &str,
    size: Vec2,
    enabled: bool,
    active: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    picker_keycap_button_in_rect(ui, rect, label, enabled, active)
}

const KEY_PICKER_POPUP_WIDTH: f32 = 760.0;
const KEY_PICKER_POPUP_HEIGHT: f32 = 560.0;
const KEY_PICKER_SCROLL_HEIGHT: f32 = 430.0;
const KEY_PICKER_MAIN_WIDTH: f32 = 920.0;
const KEY_PICKER_MAIN_HEIGHT: f32 = 560.0;
const KEY_PICKER_MAIN_CONTENT_HEIGHT: f32 = 455.0;

pub(super) fn responsive_window_size(ctx: &egui::Context, base: Vec2, max: Vec2) -> Vec2 {
    let screen = ctx.content_rect().size();
    Vec2::new(
        base.x.max((screen.x * 0.82).min(max.x)),
        base.y.max((screen.y * 0.78).min(max.y)),
    )
}

pub(super) fn key_picker_main_size(ctx: &egui::Context) -> Vec2 {
    responsive_window_size(
        ctx,
        Vec2::new(KEY_PICKER_MAIN_WIDTH, KEY_PICKER_MAIN_HEIGHT),
        Vec2::new(1_260.0, 820.0),
    )
}

pub(super) fn key_picker_main_content_height(picker_size: Vec2) -> f32 {
    (picker_size.y - 105.0).clamp(KEY_PICKER_MAIN_CONTENT_HEIGHT, 700.0)
}

pub(super) fn key_picker_popup_size(ctx: &egui::Context) -> Vec2 {
    responsive_window_size(
        ctx,
        Vec2::new(KEY_PICKER_POPUP_WIDTH, KEY_PICKER_POPUP_HEIGHT),
        Vec2::new(1_050.0, 760.0),
    )
}

pub(super) fn key_picker_popup_scroll_height(popup_size: Vec2) -> f32 {
    (popup_size.y - 130.0).clamp(KEY_PICKER_SCROLL_HEIGHT, 620.0)
}

pub(super) fn responsive_picker_element_scale(ctx: &egui::Context) -> f32 {
    crate::ui_style::ResponsiveMetrics::from_ctx(ctx).scale
}

pub(super) fn responsive_picker_key_size(ctx: &egui::Context) -> Vec2 {
    Vec2::splat(54.0 * responsive_picker_element_scale(ctx))
}

pub(super) fn picker_scaled_size(ctx: &egui::Context, width: f32, height: f32) -> Vec2 {
    crate::ui_style::ResponsiveMetrics::from_ctx(ctx).size(width, height)
}

pub(super) fn picker_paint_centered_label(
    ui: &egui::Ui,
    rect: egui::Rect,
    label: &str,
    font_size: f32,
    color: Color32,
) {
    let lines: Vec<&str> = label.split('\n').collect();
    if lines.len() <= 1 {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(font_size),
            color,
        );
        return;
    }

    let line_h = font_size + 2.0;
    let total_h = line_h * lines.len() as f32;
    let start_y = rect.center().y - total_h * 0.5 + line_h * 0.5;
    for (idx, line) in lines.iter().enumerate() {
        ui.painter().text(
            egui::pos2(rect.center().x, start_y + idx as f32 * line_h),
            egui::Align2::CENTER_CENTER,
            *line,
            egui::FontId::proportional(font_size),
            color,
        );
    }
}

pub(super) fn picker_button(
    ui: &mut egui::Ui,
    label: &str,
    size: Vec2,
    enabled: bool,
    active: bool,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let dark = ui.visuals().dark_mode;
    let hovered = enabled && resp.hovered();
    let pressed = enabled && resp.is_pointer_button_down_on();
    let fill = if active {
        crate::ui_style::accent()
    } else if pressed {
        if dark {
            Color32::from_rgb(56, 56, 59)
        } else {
            Color32::from_rgb(232, 232, 235)
        }
    } else if hovered {
        crate::ui_style::hover_fill(dark)
    } else {
        crate::ui_style::surface_fill(dark)
    };
    ui.painter().rect(
        rect,
        9.0,
        fill,
        crate::ui_style::modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );
    let color = if active {
        Color32::WHITE
    } else if enabled {
        ui.visuals().text_color()
    } else {
        crate::ui_style::muted_text(dark)
    };
    let label_scale = (size.y / 54.0).clamp(1.0, 1.22);
    picker_paint_centered_label(ui, rect, label, 12.0 * label_scale, color);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(super) fn picker_tab_width(label: &str) -> f32 {
    (label.chars().count() as f32 * 7.0 + 24.0).clamp(52.0, 132.0)
}

pub(super) fn picker_tab_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    picker_button(
        ui,
        label,
        Vec2::new(picker_tab_width(label), 30.0),
        true,
        active,
    )
}

pub(super) fn picker_slot_button(
    ui: &mut egui::Ui,
    id_text: &str,
    display_name: &str,
    active: bool,
    has_content: bool,
) -> egui::Response {
    let scale = responsive_picker_element_scale(ui.ctx());
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(48.0 * scale, 30.0 * scale), egui::Sense::click());
    let dark = ui.visuals().dark_mode;
    let hovered = resp.hovered();
    let fill = if active {
        crate::ui_style::accent()
    } else if hovered {
        crate::ui_style::hover_fill(dark)
    } else {
        crate::ui_style::surface_fill(dark)
    };
    ui.painter().rect(
        rect,
        8.0,
        fill,
        crate::ui_style::modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let text_color = if active {
        Color32::WHITE
    } else if has_content {
        ui.visuals().text_color()
    } else {
        inactive_picker_entry_text(dark)
    };
    if display_name != id_text {
        ui.painter().text(
            egui::pos2(rect.center().x, rect.top() + 9.0 * scale),
            egui::Align2::CENTER_CENTER,
            id_text,
            egui::FontId::proportional(10.5 * scale),
            text_color,
        );
        ui.painter().text(
            egui::pos2(rect.center().x, rect.bottom() - 8.5 * scale),
            egui::Align2::CENTER_CENTER,
            display_name,
            egui::FontId::proportional(10.5 * scale),
            text_color,
        );
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            id_text,
            egui::FontId::proportional(12.0 * scale),
            text_color,
        );
    }
    resp
}
