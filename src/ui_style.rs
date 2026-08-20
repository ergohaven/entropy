use egui::{Color32, FontId, RichText, Sense, Stroke, Ui, Vec2};
use std::sync::atomic::{AtomicU32, Ordering};

const DEFAULT_ACCENT_RGB: u32 = (196 << 16) | (132 << 8) | 144;
static ACCENT_RGB: AtomicU32 = AtomicU32::new(DEFAULT_ACCENT_RGB);

pub fn set_accent(color: Color32) {
    let rgb = ((color.r() as u32) << 16) | ((color.g() as u32) << 8) | color.b() as u32;
    ACCENT_RGB.store(rgb, Ordering::Relaxed);
}

pub fn accent() -> Color32 {
    let rgb = ACCENT_RGB.load(Ordering::Relaxed);
    Color32::from_rgb(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    )
}

pub fn popup_below_widget<R>(
    parent_ui: &Ui,
    popup_id: egui::Id,
    widget_response: &egui::Response,
    close_behavior: egui::PopupCloseBehavior,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    popup_below_widget_with_width(
        parent_ui,
        popup_id,
        widget_response,
        close_behavior,
        widget_response.rect.width(),
        add_contents,
    )
}

pub fn popup_below_widget_with_width<R>(
    _parent_ui: &Ui,
    popup_id: egui::Id,
    widget_response: &egui::Response,
    close_behavior: egui::PopupCloseBehavior,
    width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let response = egui::Popup::from_response(widget_response)
        .layout(egui::Layout::top_down_justified(egui::Align::LEFT))
        .open_memory(None)
        .close_behavior(close_behavior)
        .id(popup_id)
        .align(egui::RectAlign::BOTTOM_START)
        .width(width)
        .show(|ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui)
        })?;
    Some(response.inner)
}

pub fn panel_fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(30, 30, 30)
    } else {
        Color32::from_rgb(245, 245, 245)
    }
}

pub fn window_fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(37, 37, 38)
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}

pub fn surface_fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(45, 45, 48)
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix_channel = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round() as u8 };
    Color32::from_rgb(
        mix_channel(a.r(), b.r()),
        mix_channel(a.g(), b.g()),
        mix_channel(a.b(), b.b()),
    )
}

pub fn hover_fill(dark: bool) -> Color32 {
    if dark {
        mix(Color32::from_rgb(45, 45, 48), accent(), 0.22)
    } else {
        mix(Color32::from_rgb(255, 255, 255), accent(), 0.16)
    }
}

pub fn border_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(54, 54, 58)
    } else {
        Color32::from_rgb(236, 236, 238)
    }
}

pub fn muted_text(dark: bool) -> Color32 {
    if dark {
        Color32::from_gray(150)
    } else {
        Color32::from_gray(120)
    }
}

pub const BATTERY_ICON_WIDTH: f32 = 16.0;

pub fn paint_battery_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    percent: u8,
    color: Color32,
) {
    let body = egui::Rect::from_min_size(
        egui::pos2(center.x - BATTERY_ICON_WIDTH * 0.5, center.y - 4.5),
        Vec2::new(14.0, 9.0),
    );
    painter.rect_stroke(
        body,
        2.0,
        Stroke::new(1.0_f32, color),
        egui::StrokeKind::Inside,
    );

    let terminal = egui::Rect::from_min_max(
        egui::pos2(body.right(), center.y - 2.5),
        egui::pos2(body.right() + 2.0, center.y + 2.5),
    );
    painter.rect_filled(terminal, 1.0, color);

    let fill_bounds = body.shrink(2.0);
    let fill_width = fill_bounds.width() * f32::from(percent.min(100)) / 100.0;
    if fill_width > 0.0 {
        painter.rect_filled(
            egui::Rect::from_min_max(
                fill_bounds.min,
                egui::pos2(fill_bounds.left() + fill_width, fill_bounds.bottom()),
            ),
            0.8,
            color,
        );
    }
}

pub fn modal_outline_stroke(dark: bool) -> Stroke {
    if dark {
        Stroke::new(1.0_f32, Color32::from_rgb(54, 54, 58))
    } else {
        Stroke::new(1.0_f32, Color32::from_rgb(230, 230, 233))
    }
}

pub fn allocate_ui_at_rect<R>(
    ui: &mut Ui,
    rect: egui::Rect,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    #[allow(deprecated)]
    ui.allocate_ui_at_rect(rect, add_contents)
}

#[derive(Debug, Clone, Copy)]
pub struct ResponsiveMetrics {
    pub scale: f32,
}

impl ResponsiveMetrics {
    pub fn from_ctx(ctx: &egui::Context) -> Self {
        let native_scale = ctx
            .native_pixels_per_point()
            .unwrap_or_else(|| ctx.pixels_per_point() / ctx.zoom_factor().max(0.1))
            .max(1.0);
        let short_side = ctx.content_rect().width().min(ctx.content_rect().height()) * native_scale;
        let t = ((short_side - 1_500.0) / (2_160.0 - 1_500.0)).clamp(0.0, 1.0);
        Self {
            scale: 1.0 + 0.12 * t,
        }
    }

    pub fn value(self, base: f32) -> f32 {
        base * self.scale
    }

    pub fn size(self, width: f32, height: f32) -> Vec2 {
        Vec2::new(self.value(width), self.value(height))
    }

    pub fn settings_content_width(self) -> f32 {
        self.value(470.0)
    }

    pub fn settings_row_content_width(self) -> f32 {
        self.value(452.0)
    }

    pub fn settings_row_height(self) -> f32 {
        self.value(54.0)
    }

    pub fn settings_control_width(self) -> f32 {
        self.value(168.0)
    }

    pub fn settings_control_height(self) -> f32 {
        self.value(32.0)
    }

    pub fn settings_control_font_size(self) -> f32 {
        self.value(12.5)
    }
}

pub fn modern_button(ui: &mut Ui, label: &str, size: Vec2, enabled: bool) -> egui::Response {
    modern_button_with_font(ui, label, size, 12.5, enabled)
}

pub fn modern_keycap_button(ui: &mut Ui, label: &str, size: Vec2, enabled: bool) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let active = enabled && resp.is_pointer_button_down_on();
    let hovered = enabled && resp.hovered();
    let fill = if active {
        if dark {
            Color32::from_rgb(56, 56, 59)
        } else {
            Color32::from_rgb(232, 232, 235)
        }
    } else if hovered {
        hover_fill(dark)
    } else {
        surface_fill(dark)
    };
    ui.painter().rect(
        rect,
        9.0,
        fill,
        modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );

    let text_color = if enabled {
        ui.visuals().text_color()
    } else {
        muted_text(dark)
    };
    let label_scale = (rect.height() / 54.0).clamp(0.88, 1.22);
    let (top_size, bottom_size) = crate::keycode::key_label_font_sizes(label);
    if let Some((top, bottom)) = label.split_once('\n') {
        let top_color = text_color.gamma_multiply(0.75);
        let top_galley = ui.painter().layout_no_wrap(
            top.to_owned(),
            egui::FontId::proportional(top_size.unwrap_or(9.0) * label_scale),
            top_color,
        );
        let bottom_galley = ui.painter().layout_no_wrap(
            bottom.to_owned(),
            egui::FontId::proportional(bottom_size * label_scale),
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
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(bottom_size * label_scale),
            text_color,
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[allow(dead_code)]
pub fn settings_segmented_control(
    ui: &mut Ui,
    id_source: impl std::hash::Hash,
    labels: &[String],
    selected: usize,
    size: Vec2,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }

    let dark = ui.visuals().dark_mode;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let segment_width = rect.width() / labels.len() as f32;
    let base_id = ui.id().with(id_source);
    let mut picked = None;

    ui.painter().rect(
        rect,
        10.0,
        if dark {
            Color32::from_rgb(38, 38, 41)
        } else {
            Color32::from_rgb(248, 248, 249)
        },
        modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );

    for (idx, label) in labels.iter().enumerate() {
        let left = rect.left() + idx as f32 * segment_width;
        let right = if idx + 1 == labels.len() {
            rect.right()
        } else {
            left + segment_width
        };
        let segment_rect = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(right, rect.bottom()),
        );
        let resp = ui.interact(segment_rect, base_id.with(idx), Sense::click());
        let is_selected = idx == selected.min(labels.len() - 1);
        let fill = if is_selected {
            if dark {
                Color32::from_rgb(58, 58, 62)
            } else {
                Color32::from_rgb(235, 235, 238)
            }
        } else if resp.hovered() {
            hover_fill(dark)
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            let rounding = if idx == 0 && idx + 1 == labels.len() {
                9.0
            } else if idx == 0 || idx + 1 == labels.len() {
                8.0
            } else {
                4.0
            };
            ui.painter()
                .rect_filled(segment_rect.shrink(2.0), rounding, fill);
        }
        if idx > 0 {
            let x = segment_rect.left();
            ui.painter().line_segment(
                [
                    egui::pos2(x, rect.top() + 7.0),
                    egui::pos2(x, rect.bottom() - 7.0),
                ],
                Stroke::new(1.0_f32, border_color(dark).gamma_multiply(0.75)),
            );
        }
        let text_color = if is_selected {
            ui.visuals().text_color()
        } else {
            muted_text(dark)
        };
        let text_rect = segment_rect.shrink2(Vec2::new(8.0, 0.0));
        let mut font_size = 12.5_f32;
        let mut galley =
            ui.painter()
                .layout_no_wrap(label.clone(), FontId::proportional(font_size), text_color);
        while galley.size().x > text_rect.width() && font_size > 8.5 {
            font_size -= 0.5;
            galley = ui.painter().layout_no_wrap(
                label.clone(),
                FontId::proportional(font_size),
                text_color,
            );
        }
        ui.painter().with_clip_rect(text_rect).galley(
            egui::pos2(
                text_rect.center().x - galley.size().x * 0.5,
                text_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            text_color,
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && !is_selected {
            picked = Some(idx);
        }
    }

    picked
}

pub fn modern_button_with_font(
    ui: &mut Ui,
    label: &str,
    size: Vec2,
    font_size: f32,
    enabled: bool,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let active = enabled && resp.is_pointer_button_down_on();
    let hovered = enabled && resp.hovered();
    let fill = if active {
        if dark {
            Color32::from_rgb(56, 56, 59)
        } else {
            Color32::from_rgb(232, 232, 235)
        }
    } else if hovered {
        hover_fill(dark)
    } else {
        surface_fill(dark)
    };
    ui.painter().rect(
        rect,
        9.0,
        fill,
        modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );
    let text_color = if enabled {
        ui.visuals().text_color()
    } else {
        muted_text(dark)
    };
    let text_rect = rect.shrink2(Vec2::new(6.0, 2.0));
    let mut fitted_font_size = font_size;
    let mut galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(fitted_font_size),
        text_color,
    );
    while galley.size().x > text_rect.width() && fitted_font_size > 8.5 {
        fitted_font_size -= 0.5;
        galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            FontId::proportional(fitted_font_size),
            text_color,
        );
    }
    ui.painter().with_clip_rect(text_rect).galley(
        egui::pos2(
            text_rect.center().x - galley.size().x * 0.5,
            text_rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn modern_text_field_sized(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    width: f32,
    height: f32,
    hint: &str,
    char_limit: usize,
    horizontal_align: egui::Align,
) -> egui::Response {
    let font_size = 12.5 * (height / 32.0).clamp(1.0, 1.3);
    modern_text_field_impl(
        ui,
        id,
        text,
        width,
        height,
        font_size,
        hint,
        char_limit,
        horizontal_align,
        true,
    )
}

pub fn modern_text_field_interactive(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    width: f32,
    hint: &str,
    char_limit: usize,
    horizontal_align: egui::Align,
    interactive: bool,
) -> egui::Response {
    modern_text_field_impl(
        ui,
        id,
        text,
        width,
        32.0,
        12.5,
        hint,
        char_limit,
        horizontal_align,
        interactive,
    )
}

fn modern_text_field_impl(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    width: f32,
    height: f32,
    font_size: f32,
    hint: &str,
    char_limit: usize,
    horizontal_align: egui::Align,
    interactive: bool,
) -> egui::Response {
    let field_rect = paint_modern_text_field_frame(ui, id, width, height, interactive);

    let resp = allocate_ui_at_rect(ui, field_rect.shrink2(Vec2::new(10.0, 0.0)), |ui| {
        ui.add_sized(
            [width - 20.0, height],
            egui::TextEdit::singleline(text)
                .id(id)
                .desired_width(width - 20.0)
                .hint_text(hint)
                .font(FontId::proportional(font_size))
                .char_limit(char_limit)
                .frame(egui::Frame::NONE)
                .interactive(interactive)
                .horizontal_align(horizontal_align)
                .vertical_align(egui::Align::Center),
        )
    })
    .inner;
    if resp.hovered() && interactive {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    resp
}

#[allow(clippy::too_many_arguments)]
pub fn modern_text_field_sized_with_layouter(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    width: f32,
    height: f32,
    hint: &str,
    char_limit: usize,
    horizontal_align: egui::Align,
    layouter: &mut dyn FnMut(&Ui, &dyn egui::TextBuffer, f32) -> std::sync::Arc<egui::Galley>,
) -> egui::widgets::text_edit::TextEditOutput {
    let font_size = 12.5 * (height / 32.0).clamp(1.0, 1.3);
    let field_rect = paint_modern_text_field_frame(ui, id, width, height, true);
    let inner_size = Vec2::new(width - 20.0, height);
    let output = allocate_ui_at_rect(ui, field_rect.shrink2(Vec2::new(10.0, 0.0)), |ui| {
        egui::TextEdit::singleline(text)
            .id(id)
            .desired_width(inner_size.x)
            .min_size(inner_size)
            .hint_text(hint)
            .font(FontId::proportional(font_size))
            .char_limit(char_limit)
            .frame(egui::Frame::NONE)
            .horizontal_align(horizontal_align)
            .vertical_align(egui::Align::Center)
            .layouter(layouter)
            .show(ui)
    })
    .inner;
    if output.response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    output
}

fn paint_modern_text_field_frame(
    ui: &mut Ui,
    id: egui::Id,
    width: f32,
    height: f32,
    interactive: bool,
) -> egui::Rect {
    let dark = ui.visuals().dark_mode;
    let field_size = Vec2::new(width, height);
    let (field_rect, _) = ui.allocate_exact_size(field_size, Sense::hover());
    let field_hovered = ui.input(|i| {
        i.pointer
            .hover_pos()
            .map(|pos| field_rect.contains(pos))
            .unwrap_or(false)
    });
    let field_focused = ui.memory(|m| m.has_focus(id));
    let field_fill = if interactive && field_focused {
        if dark {
            Color32::from_rgb(52, 52, 55)
        } else {
            Color32::from_rgb(244, 244, 246)
        }
    } else if interactive && field_hovered {
        hover_fill(dark)
    } else {
        surface_fill(dark)
    };
    ui.painter().rect(
        field_rect,
        9.0,
        field_fill,
        modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );
    field_rect
}

pub fn modern_dropdown_button(
    ui: &mut Ui,
    id: egui::Id,
    selected_text: &str,
    text_color: Color32,
    width: f32,
) -> egui::Response {
    modern_dropdown_button_sized(ui, id, selected_text, text_color, width, 32.0, 12.5)
}

pub fn modern_dropdown_button_sized(
    ui: &mut Ui,
    id: egui::Id,
    selected_text: &str,
    text_color: Color32,
    width: f32,
    height: f32,
    font_size: f32,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let dropdown_open = egui::Popup::is_id_open(ui.ctx(), id);
    let (dropdown_rect, dropdown_resp) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if dropdown_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if dropdown_resp.clicked() {
        egui::Popup::toggle_id(ui.ctx(), id);
    }

    let dropdown_fill = if dropdown_open || dropdown_resp.hovered() {
        hover_fill(dark)
    } else {
        surface_fill(dark)
    };
    ui.painter().rect(
        dropdown_rect,
        9.0,
        dropdown_fill,
        modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(dropdown_rect.left() + 12.0, dropdown_rect.center().y),
        egui::Align2::LEFT_CENTER,
        selected_text,
        FontId::proportional(font_size),
        text_color,
    );
    let chevron_x = dropdown_rect.right() - 15.0;
    let chevron_y = dropdown_rect.center().y + 1.0;
    let chevron_color = muted_text(dark);
    ui.painter().line_segment(
        [
            egui::pos2(chevron_x - 4.5, chevron_y - 2.0),
            egui::pos2(chevron_x, chevron_y + 2.5),
        ],
        Stroke::new(1.4_f32, chevron_color),
    );
    ui.painter().line_segment(
        [
            egui::pos2(chevron_x, chevron_y + 2.5),
            egui::pos2(chevron_x + 4.5, chevron_y - 2.0),
        ],
        Stroke::new(1.4_f32, chevron_color),
    );
    dropdown_resp
}

pub fn modern_dropdown_select_sized(
    ui: &mut Ui,
    id: egui::Id,
    labels: &[String],
    selected: usize,
    width: f32,
    height: f32,
    font_size: f32,
) -> (egui::Response, Option<usize>) {
    let dark = ui.visuals().dark_mode;
    let selected_text = labels.get(selected).map(String::as_str).unwrap_or("");
    let dropdown_resp = modern_dropdown_button_sized(
        ui,
        id,
        selected_text,
        ui.visuals().text_color(),
        width,
        height,
        font_size,
    );
    let mut picked = None;
    crate::ui_style::popup_below_widget(
        ui,
        id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(width);
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            let option_height = 28.0;
            let max_height = (labels.len() as f32 * (option_height + 2.0))
                .min(142.0)
                .max(option_height);
            egui::ScrollArea::vertical()
                .id_salt(("modern_dropdown_select_scroll", id))
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (idx, label) in labels.iter().enumerate() {
                        let is_selected = idx == selected;
                        let (option_rect, option_resp) =
                            ui.allocate_exact_size(Vec2::new(width, option_height), Sense::click());
                        if option_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let option_fill = if is_selected {
                            if dark {
                                Color32::from_rgb(58, 58, 61)
                            } else {
                                Color32::from_rgb(236, 236, 238)
                            }
                        } else if option_resp.hovered() {
                            hover_fill(dark)
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(option_rect, 7.0, option_fill);
                        ui.painter().text(
                            egui::pos2(option_rect.left() + 10.0, option_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            FontId::proportional(12.0),
                            if is_selected {
                                ui.visuals().text_color()
                            } else {
                                muted_text(dark)
                            },
                        );
                        if option_resp.clicked() {
                            picked = Some(idx);
                            egui::Popup::close_all(ui.ctx());
                        }
                    }
                });
        },
    );
    (dropdown_resp, picked)
}

pub fn modern_toggle_pill(
    ui: &mut Ui,
    glyph: &str,
    label: &str,
    tooltip: &str,
    size: Vec2,
    selected: bool,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let fill = if selected {
        if dark {
            mix(surface_fill(dark), accent(), 0.16)
        } else {
            mix(surface_fill(dark), accent(), 0.10)
        }
    } else if resp.hovered() {
        hover_fill(dark)
    } else {
        surface_fill(dark)
    };
    let stroke = if selected {
        Stroke::new(
            1.1_f32,
            mix(modal_outline_stroke(dark).color, accent(), 0.42),
        )
    } else {
        modal_outline_stroke(dark)
    };
    ui.painter()
        .rect(rect, 9.0, fill, stroke, egui::StrokeKind::Inside);

    let text_color = if selected {
        ui.visuals().text_color()
    } else {
        muted_text(dark)
    };
    let badge_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 26.0, rect.center().y),
        Vec2::new(34.0, 20.0),
    );
    let badge_fill = if selected {
        mix(surface_fill(dark), accent(), if dark { 0.34 } else { 0.20 })
    } else if resp.hovered() {
        if dark {
            Color32::from_rgb(58, 58, 61)
        } else {
            Color32::from_rgb(236, 236, 238)
        }
    } else if dark {
        Color32::from_rgb(52, 52, 55)
    } else {
        Color32::from_rgb(244, 244, 246)
    };
    ui.painter().rect_filled(badge_rect, 6.0, badge_fill);
    ui.painter().text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        FontId::monospace(11.0),
        text_color,
    );

    let text_rect = egui::Rect::from_min_max(
        egui::pos2(badge_rect.right() + 8.0, rect.top()),
        egui::pos2(rect.right() - 13.0, rect.bottom()),
    );
    let mut font_size = 12.0_f32;
    let mut galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(font_size),
        text_color,
    );
    while galley.size().x > text_rect.width() && font_size > 8.5 {
        font_size -= 0.5;
        galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            FontId::proportional(font_size),
            text_color,
        );
    }
    ui.painter().with_clip_rect(text_rect).galley(
        egui::pos2(
            text_rect.left(),
            text_rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );

    resp.on_hover_text(tooltip)
}

pub fn paint_floating_scrollbar_handle(
    ui: &mut Ui,
    track_rect: egui::Rect,
    handle_height: f32,
    t: f32,
    hovered: bool,
) {
    let dark = ui.visuals().dark_mode;
    let handle_top = egui::lerp(
        track_rect.top()..=(track_rect.bottom() - handle_height),
        t.clamp(0.0, 1.0),
    );
    let handle_rect = egui::Rect::from_min_max(
        egui::pos2(track_rect.left(), handle_top),
        egui::pos2(track_rect.right(), handle_top + handle_height),
    );
    let handle_fill = if dark {
        if hovered {
            Color32::from_rgb(74, 74, 78)
        } else {
            Color32::from_rgb(62, 62, 66)
        }
    } else if hovered {
        Color32::from_rgb(198, 198, 202)
    } else {
        Color32::from_rgb(212, 212, 216)
    };
    ui.painter().rect_filled(handle_rect, 3.0, handle_fill);
}

pub fn modal_action_button_size() -> Vec2 {
    Vec2::new(104.0, 32.0)
}

pub fn modal_small_button_size(width: f32) -> Vec2 {
    Vec2::new(width, 32.0)
}

pub fn modal_space_xs() -> f32 {
    4.0
}

pub fn modal_space_sm() -> f32 {
    8.0
}

pub fn modal_window_frame(style: &egui::Style, dark: bool) -> egui::Frame {
    egui::Frame::window(style)
        .fill(window_fill(dark))
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::same(10))
}

pub fn modal_backdrop_alpha(dark: bool) -> u8 {
    if dark {
        96
    } else {
        48
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModalLayout {
    pub content_width: f32,
    pub top_padding: f32,
}

impl ModalLayout {
    pub fn new(content_width: f32) -> Self {
        Self {
            content_width,
            top_padding: 6.0,
        }
    }

    pub fn with_top_padding(mut self, top_padding: f32) -> Self {
        self.top_padding = top_padding;
        self
    }
}

pub fn centered_modal_window<'a>(
    ctx: &egui::Context,
    title: &'a str,
    id: egui::Id,
    open: &'a mut bool,
    size: Vec2,
) -> egui::Window<'a> {
    egui::Window::new(title)
        .id(id)
        .open(open)
        .collapsible(false)
        .resizable(false)
        .movable(true)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size(size)
        .frame(modal_window_frame(
            ctx.global_style().as_ref(),
            ctx.global_style().visuals.dark_mode,
        ))
}

pub fn modal_content(ui: &mut Ui, layout: ModalLayout, add_contents: impl FnOnce(&mut Ui)) {
    ui.vertical_centered(|ui| {
        if layout.top_padding > 0.0 {
            ui.add_space(layout.top_padding);
        }
        ui.allocate_ui_with_layout(
            Vec2::new(layout.content_width, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            add_contents,
        );
    });
}

pub fn modal_intro(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.0)
            .color(Color32::from_gray(140)),
    );
}

pub fn modal_hint(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.0)
            .color(muted_text(ui.visuals().dark_mode)),
    );
}

pub fn modal_empty_state(ui: &mut Ui, title: &str, detail: Option<&str>) {
    let dark = ui.visuals().dark_mode;
    ui.vertical_centered(|ui| {
        ui.add_space(72.0);
        ui.label(RichText::new(title).size(13.0).color(muted_text(dark)));
        if let Some(detail) = detail {
            ui.add_space(6.0);
            ui.label(RichText::new(detail).size(11.5).color(muted_text(dark)));
        }
    });
}

#[allow(dead_code)]
pub fn modal_centered_text_block(ui: &mut Ui, width: f32, add_contents: impl FnOnce(&mut Ui)) {
    ui.horizontal_centered(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            add_contents,
        );
    });
}

pub fn settings_list_row(
    ui: &mut Ui,
    content_width: f32,
    row_height: f32,
    label: &str,
    label_enabled: bool,
    control_width: f32,
    add_control: impl FnOnce(&mut Ui),
) {
    settings_list_row_with_tooltip(
        ui,
        content_width,
        row_height,
        label,
        label_enabled,
        None,
        control_width,
        add_control,
    );
}

pub fn settings_list_row_with_tooltip(
    ui: &mut Ui,
    content_width: f32,
    row_height: f32,
    label: &str,
    label_enabled: bool,
    tooltip: Option<&str>,
    control_width: f32,
    add_control: impl FnOnce(&mut Ui),
) {
    let dark = ui.visuals().dark_mode;
    let (row_rect, _) =
        ui.allocate_exact_size(egui::vec2(content_width, row_height), egui::Sense::hover());
    let separator = border_color(dark).gamma_multiply(if dark { 0.72 } else { 0.9 });
    ui.painter().line_segment(
        [row_rect.left_bottom(), row_rect.right_bottom()],
        Stroke::new(1.0_f32, separator),
    );

    let label_scale = (content_width / 452.0).clamp(1.0, 1.12);
    let label_color = if label_enabled {
        ui.visuals().text_color()
    } else {
        muted_text(dark)
    };
    ui.painter().text(
        egui::pos2(row_rect.left() + 2.0, row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0 * label_scale),
        label_color,
    );

    if let Some(tooltip) = tooltip {
        let text_width = (label.chars().count() as f32 * 7.4 * label_scale + 8.0)
            .min((content_width - control_width - 20.0).max(0.0));
        let label_rect = egui::Rect::from_center_size(
            egui::pos2(
                row_rect.left() + 2.0 + text_width / 2.0,
                row_rect.center().y,
            ),
            egui::vec2(text_width, 22.0 * label_scale),
        );
        let label_resp = ui.interact(
            label_rect,
            ui.id().with(("settings_label", label)),
            egui::Sense::hover(),
        );
        if label_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Help);
        }
        let tooltip = tooltip.trim_end_matches('.');
        label_resp.on_hover_text(tooltip);
    }

    let control_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.right() - control_width, row_rect.top()),
        egui::vec2(control_width, row_height),
    );
    crate::ui_style::allocate_ui_at_rect(ui, control_rect, |ui| {
        ui.set_min_size(egui::vec2(control_width, row_height));
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center),
            add_control,
        );
    });
}

#[allow(dead_code)]
pub fn settings_switch(ui: &mut Ui, checked: &mut bool) -> egui::Response {
    settings_switch_interactive(ui, checked, true)
}

pub fn settings_switch_sized(
    ui: &mut Ui,
    checked: &mut bool,
    desired_size: Vec2,
) -> egui::Response {
    settings_switch_impl(ui, checked, desired_size, true, None)
}

pub fn settings_switch_sized_stable(
    ui: &mut Ui,
    id_source: impl std::hash::Hash,
    checked: &mut bool,
    desired_size: Vec2,
) -> egui::Response {
    settings_switch_sized_stable_interactive(ui, id_source, checked, desired_size, true)
}

pub fn settings_switch_sized_stable_interactive(
    ui: &mut Ui,
    id_source: impl std::hash::Hash,
    checked: &mut bool,
    desired_size: Vec2,
    interactive: bool,
) -> egui::Response {
    settings_switch_impl(
        ui,
        checked,
        desired_size,
        interactive,
        Some(ui.id().with(id_source)),
    )
}

pub fn settings_switch_interactive(
    ui: &mut Ui,
    checked: &mut bool,
    interactive: bool,
) -> egui::Response {
    settings_switch_impl(ui, checked, egui::vec2(46.0, 24.0), interactive, None)
}

fn settings_switch_impl(
    ui: &mut Ui,
    checked: &mut bool,
    desired_size: Vec2,
    interactive: bool,
    stable_id: Option<egui::Id>,
) -> egui::Response {
    let sense = if interactive {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, mut response) = if let Some(id) = stable_id {
        let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
        (rect, ui.interact(rect, id, sense))
    } else {
        ui.allocate_exact_size(desired_size, sense)
    };
    if interactive && response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    if response.hovered() && interactive {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        let dark = ui.visuals().dark_mode;
        let t = ui.ctx().animate_bool_responsive(response.id, *checked);
        let hover_t = ui
            .ctx()
            .animate_bool_responsive(response.id.with("hover"), response.hovered() && interactive);
        let radius = rect.height() / 2.0;
        let blend = |from: Color32, to: Color32, t: f32| -> Color32 {
            let mix = |a: u8, b: u8| egui::lerp((a as f32)..=(b as f32), t).round() as u8;
            Color32::from_rgba_unmultiplied(
                mix(from.r(), to.r()),
                mix(from.g(), to.g()),
                mix(from.b(), to.b()),
                mix(from.a(), to.a()),
            )
        };
        let base_track_fill = if !interactive {
            if dark {
                Color32::from_rgb(43, 43, 46)
            } else {
                Color32::from_rgb(226, 226, 229)
            }
        } else if *checked {
            if dark {
                Color32::from_rgb(66, 66, 70)
            } else {
                Color32::from_rgb(214, 214, 218)
            }
        } else if dark {
            Color32::from_rgb(46, 46, 49)
        } else {
            Color32::from_rgb(232, 232, 235)
        };
        let hover_track_fill = if *checked {
            if dark {
                Color32::from_rgb(78, 78, 84)
            } else {
                Color32::from_rgb(204, 204, 210)
            }
        } else if dark {
            Color32::from_rgb(56, 56, 61)
        } else {
            Color32::from_rgb(222, 222, 228)
        };
        let track_fill = blend(base_track_fill, hover_track_fill, hover_t);
        let stroke_color = blend(
            Color32::TRANSPARENT,
            if dark {
                Color32::from_rgba_unmultiplied(125, 125, 132, 145)
            } else {
                Color32::from_rgba_unmultiplied(128, 128, 138, 130)
            },
            hover_t,
        );
        let stroke = Stroke::new(hover_t, stroke_color);
        ui.painter()
            .rect(rect, radius, track_fill, stroke, egui::StrokeKind::Inside);

        let knob_radius = radius - 4.0 + hover_t;
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        let base_knob_fill = if !interactive {
            if dark {
                Color32::from_rgb(102, 102, 106)
            } else {
                Color32::from_rgb(168, 168, 172)
            }
        } else if *checked {
            if dark {
                Color32::from_rgb(220, 220, 224)
            } else {
                Color32::from_rgb(74, 74, 78)
            }
        } else if dark {
            Color32::from_rgb(82, 82, 86)
        } else {
            Color32::from_rgb(188, 188, 192)
        };
        let hover_knob_fill = if *checked {
            if dark {
                Color32::from_rgb(235, 235, 239)
            } else {
                Color32::from_rgb(58, 58, 64)
            }
        } else if dark {
            Color32::from_rgb(104, 104, 110)
        } else {
            Color32::from_rgb(168, 168, 176)
        };
        let knob_fill = blend(base_knob_fill, hover_knob_fill, hover_t);
        ui.painter()
            .circle_filled(egui::pos2(x, rect.center().y), knob_radius, knob_fill);
    }

    response
}
