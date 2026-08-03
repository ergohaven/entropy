use super::*;

pub(super) fn device_dropdown_open_id() -> egui::Id {
    egui::Id::new("device_dropdown_open")
}

pub(super) fn device_layer_operations_submenu_open_id() -> egui::Id {
    egui::Id::new("device_layer_operations_submenu_open")
}

pub(super) fn advanced_dropdown_open_id() -> egui::Id {
    egui::Id::new("advanced_dropdown_open")
}

pub(super) fn settings_dropdown_open_id() -> egui::Id {
    egui::Id::new("settings_dropdown_open")
}

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
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        selected,
        TopDropdownItemAccessory::None,
    )
}

pub(super) fn top_dropdown_item_with_indicator(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    selected: bool,
    show_indicator: bool,
) -> egui::Response {
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        selected,
        if show_indicator {
            TopDropdownItemAccessory::Indicator
        } else {
            TopDropdownItemAccessory::None
        },
    )
}

pub(super) fn top_dropdown_submenu_item(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    submenu_open: bool,
) -> egui::Response {
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        submenu_open,
        TopDropdownItemAccessory::Submenu,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TopDropdownItemAccessory {
    None,
    Indicator,
    Submenu,
}

fn top_dropdown_item_with_accessory(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    selected: bool,
    accessory: TopDropdownItemAccessory,
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
        let reserve_right = selected || accessory == TopDropdownItemAccessory::Submenu;
        let text_clip = if reserve_right {
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

        if accessory == TopDropdownItemAccessory::Indicator {
            let label_width = top_menu_text_width(ui, label, 13.0);
            let max_dot_x = rect.right() - if selected { 28.0 } else { 10.0 };
            let dot_x = (rect.left() + 10.0 + label_width + 8.0).min(max_dot_x);
            ui.painter()
                .circle_filled(egui::pos2(dot_x, rect.center().y), 2.5, app_accent());
        }

        if accessory == TopDropdownItemAccessory::Submenu {
            ui.painter().text(
                egui::pos2(rect.right() - 10.0, rect.center().y - 1.0),
                egui::Align2::RIGHT_CENTER,
                "›",
                egui::FontId::proportional(18.0),
                if !enabled {
                    app_muted_text(dark)
                } else if selected || hovered {
                    app_accent()
                } else {
                    ui.visuals().text_color()
                },
            );
        } else if selected {
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

pub(super) fn top_menu_divider_stroke(dark: bool) -> egui::Stroke {
    let color = if dark {
        Color32::from_gray(105)
    } else {
        Color32::from_gray(170)
    };
    egui::Stroke::new(1.5_f32, color)
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
            d.insert_temp(device_dropdown_open_id(), false);
            d.insert_temp(device_layer_operations_submenu_open_id(), false);
            d.insert_temp(advanced_dropdown_open_id(), false);
            d.insert_temp(settings_dropdown_open_id(), false);
        });
    }

    pub(super) fn top_dropdown_open(&self, ctx: &egui::Context) -> bool {
        ctx.data(|d| {
            d.get_temp::<bool>(device_dropdown_open_id())
                .unwrap_or(false)
                || d.get_temp::<bool>(advanced_dropdown_open_id())
                    .unwrap_or(false)
                || d.get_temp::<bool>(settings_dropdown_open_id())
                    .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_dropdown_state_is_visible_to_background_lifecycle() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let app = EntropyApp::new(&creation_context);

        ctx.data_mut(|d| d.insert_temp(device_dropdown_open_id(), true));
        assert!(app.top_dropdown_open(&ctx));

        app.close_top_dropdowns(&ctx);
        assert!(!app.top_dropdown_open(&ctx));
    }
}
