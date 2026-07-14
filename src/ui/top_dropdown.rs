use super::*;

pub(super) fn top_dropdown_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(app_surface_fill(dark))
        .stroke(crate::ui_style::modal_outline_stroke(dark))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

pub(super) fn top_dropdown_item(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    top_dropdown_item_with_indicator(ui, width, label, enabled, selected, false)
}

pub(super) fn top_dropdown_item_with_indicator(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    selected: bool,
    show_indicator: bool,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 30.0), sense);
    let hovered = resp.hovered() && enabled;
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        if selected || hovered {
            let fill = app_hover_fill(dark);
            ui.painter().rect_filled(rect, 8.0, fill);
        }

        let text_color = if !enabled {
            app_muted_text(dark)
        } else if selected {
            app_accent()
        } else {
            ui.visuals().text_color()
        };
        let text_clip = if selected {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.right() - 24.0, rect.bottom()))
        } else {
            rect
        };
        ui.painter().with_clip_rect(text_clip).text(
            egui::pos2(rect.left() + 10.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            text_color,
        );

        if show_indicator {
            let label_width = top_menu_text_width(ui, label, 13.0);
            let max_dot_x = rect.right() - if selected { 28.0 } else { 10.0 };
            let dot_x = (rect.left() + 10.0 + label_width + 8.0).min(max_dot_x);
            ui.painter()
                .circle_filled(egui::pos2(dot_x, rect.center().y), 2.5, app_accent());
        }

        if selected {
            ui.painter().circle_filled(
                egui::pos2(rect.right() - 12.0, rect.center().y),
                2.5,
                app_accent(),
            );
        }
    }

    resp
}

pub(super) fn top_menu_text_width(ui: &egui::Ui, label: &str, font_size: f32) -> f32 {
    ui.fonts_mut(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(font_size),
            ui.visuals().widgets.inactive.fg_stroke.color,
        )
        .size()
        .x
    })
}

pub(super) fn adaptive_top_dropdown_width<'a>(
    ui: &egui::Ui,
    labels: impl IntoIterator<Item = &'a str>,
    min_width: f32,
) -> f32 {
    let text_width = labels
        .into_iter()
        .filter(|label| !label.is_empty())
        .map(|label| top_menu_text_width(ui, label, 13.0))
        .fold(0.0, f32::max);

    // 16px frame margins + 10px left text inset + selected-dot reserve + breathing room.
    (text_width + 56.0).max(min_width).min(360.0)
}

impl EntropyApp {
    pub(super) fn close_top_dropdowns(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("device_dropdown_open"), false);
            d.insert_temp(egui::Id::new("advanced_dropdown_open"), false);
            d.insert_temp(egui::Id::new("settings_dropdown_open"), false);
        });
    }

    pub(super) fn top_dropdown_open(&self, ctx: &egui::Context) -> bool {
        ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("device_dropdown_open"))
                .unwrap_or(false)
                || d.get_temp::<bool>(egui::Id::new("advanced_dropdown_open"))
                    .unwrap_or(false)
                || d.get_temp::<bool>(egui::Id::new("settings_dropdown_open"))
                    .unwrap_or(false)
        })
    }
}
