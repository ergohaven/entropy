use super::*;

fn clamp_sticky_layout_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.50, 1.0)
    } else {
        default_sticky_layout_opacity()
    }
}

fn sticky_layout_visuals(dark: bool) -> egui::Visuals {
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = app_panel_fill(dark);
    visuals.window_fill = app_window_fill(dark);
    visuals.faint_bg_color = app_panel_fill(dark);
    visuals.extreme_bg_color = app_panel_fill(dark);
    visuals.widgets.noninteractive.bg_fill = app_panel_fill(dark);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, app_border_color(dark));
    visuals.widgets.inactive.bg_fill = app_surface_fill(dark);
    visuals.widgets.inactive.weak_bg_fill = app_surface_fill(dark);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, app_border_color(dark));
    visuals.widgets.hovered.bg_fill = app_hover_fill(dark);
    visuals.widgets.hovered.weak_bg_fill = app_hover_fill(dark);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, app_border_color(dark));
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals
}

#[cfg(target_os = "windows")]
fn set_windows_window_opacity_by_title(title: &str, opacity: f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE,
        LWA_ALPHA, WS_EX_LAYERED,
    };

    let opacity = clamp_sticky_layout_opacity(opacity);
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
        if hwnd.is_null() {
            return;
        }
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if opacity >= 0.999 {
            if (ex_style & WS_EX_LAYERED as isize) != 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_LAYERED as isize));
            }
            return;
        }

        let alpha = (opacity * 255.0).round() as u8;
        if (ex_style & WS_EX_LAYERED as isize) == 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED as isize);
        }
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
    }
}

const STICKY_LAYOUT_WINDOW_W: f32 = 720.0_f32;
const STICKY_LAYOUT_WINDOW_H: f32 = 360.0_f32;
const STICKY_LAYOUT_WINDOW_MARGIN: f32 = 1.0_f32;
const STICKY_LAYOUT_WINDOW_TITLE_H: f32 = 42.0_f32;
const STICKY_LAYOUT_WINDOW_FOOTER_H: f32 = 34.0_f32;

#[derive(Clone, Copy)]
enum StickyLayoutWindowButton {
    Pin,
    Close,
}

fn draw_sticky_layout_transparency_dropdown(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    dark: bool,
    opacity: &mut f32,
) -> bool {
    const OPACITY_VALUES: [f32; 6] = [1.0, 0.90, 0.80, 0.70, 0.60, 0.50];

    let current = clamp_sticky_layout_opacity(*opacity);
    let selected_idx = OPACITY_VALUES
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - current)
                .abs()
                .partial_cmp(&(*b - current).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    let label_prefix = crate::i18n::tr_catalog(lang, "ui.sticky_layout_transparency_short");
    let selected_text = format!(
        "{} {}%",
        label_prefix,
        (OPACITY_VALUES[selected_idx] * 100.0).round() as i32
    );
    let dropdown_id = ui.id().with("sticky_layout_transparency_dropdown");
    let width = 128.0;
    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
        ui,
        dropdown_id,
        &selected_text,
        if dark {
            Color32::from_rgb(235, 235, 235)
        } else {
            Color32::from_rgb(42, 42, 44)
        },
        width,
        24.0,
        11.0,
    );

    let mut changed = false;
    egui::popup_below_widget(
        ui,
        dropdown_id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            *ui.visuals_mut() = sticky_layout_visuals(dark);
            egui::Frame::NONE
                .fill(app_surface_fill(dark))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.set_min_width(width);
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    for (idx, value) in OPACITY_VALUES.iter().copied().enumerate() {
                        let option_text =
                            format!("{} {}%", label_prefix, (value * 100.0).round() as i32);
                        let selected = idx == selected_idx;
                        let (option_rect, option_resp) =
                            ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::click());
                        if option_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let option_fill = if selected || option_resp.hovered() {
                            app_hover_fill(dark)
                        } else {
                            app_surface_fill(dark)
                        };
                        ui.painter().rect_filled(option_rect, 7.0, option_fill);
                        ui.painter().text(
                            egui::pos2(option_rect.left() + 10.0, option_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            option_text,
                            FontId::proportional(11.0),
                            if selected {
                                if dark {
                                    Color32::from_rgb(235, 235, 235)
                                } else {
                                    Color32::from_rgb(42, 42, 44)
                                }
                            } else {
                                app_muted_text(dark)
                            },
                        );
                        if option_resp.clicked() {
                            *opacity = value;
                            changed = true;
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                });
        },
    );

    changed
}

fn sticky_layout_window_icon_button(
    ui: &mut egui::Ui,
    dark: bool,
    kind: StickyLayoutWindowButton,
    active: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let response = response.on_hover_text(tooltip);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let fill = if active || response.hovered() {
        app_hover_fill(dark)
    } else {
        Color32::TRANSPARENT
    };
    let stroke_color = if active {
        app_accent()
    } else {
        app_border_color(dark)
    };
    ui.painter().rect(
        rect,
        8.0,
        fill,
        Stroke::new(if active { 1.2 } else { 0.8 }, stroke_color),
        egui::StrokeKind::Inside,
    );

    let color = if active {
        app_accent()
    } else {
        app_muted_text(dark)
    };
    let stroke = Stroke::new(1.7, color);
    match kind {
        StickyLayoutWindowButton::Close => {
            let a = rect.center() + egui::vec2(-4.5, -4.5);
            let b = rect.center() + egui::vec2(4.5, 4.5);
            let c = rect.center() + egui::vec2(4.5, -4.5);
            let d = rect.center() + egui::vec2(-4.5, 4.5);
            ui.painter().line_segment([a, b], stroke);
            ui.painter().line_segment([c, d], stroke);
        }
        StickyLayoutWindowButton::Pin => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "📌",
                FontId::proportional(14.0),
                color,
            );
        }
    }

    response
}

fn sticky_layout_default_window_size() -> Vec2 {
    egui::vec2(STICKY_LAYOUT_WINDOW_W, STICKY_LAYOUT_WINDOW_H)
}

fn sticky_layout_saved_window_size(settings: &AppSettings) -> Vec2 {
    settings
        .sticky_layout_window_size
        .map(|[w, h]| egui::vec2(w.max(STICKY_LAYOUT_WINDOW_W), h.max(STICKY_LAYOUT_WINDOW_H)))
        .unwrap_or_else(sticky_layout_default_window_size)
}

impl EntropyApp {
    pub(super) fn draw_sticky_layout_window(&mut self, ctx: &egui::Context) {
        if !self.app_settings.sticky_layout_window {
            self.sticky_layout_last_size = None;
            return;
        }

        #[cfg(not(target_arch = "wasm32"))]
        if self.is_vial_locked() {
            self.app_settings.sticky_layout_window = false;
            self.pending_layout_indicator_open_after_unlock = true;
            self.unlock_open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "matrix_tester.keyboard_is_locked_unlock_it_to_use_matrix_tester",
            )
            .into();
            save_app_settings(&self.app_settings);
            return;
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some((rows, cols)) = self
            .layout
            .as_ref()
            .map(|layout| (layout.rows, layout.cols))
        {
            self.poll_switch_matrix_state(ctx, rows, cols, false);
        }

        let viewport_id = egui::ViewportId::from_hash_of("entropy_sticky_layout_window");
        let lang = self.app_settings.language;
        let layout = self.layout.clone();
        let selected_device_name = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(|device| device.name.clone());
        let indicator_title =
            crate::i18n::tr_catalog(lang, "ui.sticky_layout_window_title").to_string();
        let device_title = selected_device_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                layout
                    .as_ref()
                    .map(|layout| layout.name.trim())
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
            });
        let window_title = device_title
            .as_deref()
            .map(|device_title| format!("{indicator_title} — {device_title}"))
            .unwrap_or_else(|| indicator_title.clone());
        let sticky_layer = layout
            .as_ref()
            .map(|layout| self.sync_sticky_layout_layer_state(layout))
            .unwrap_or(0);
        let layer_names = self.layer_names.clone();
        let macro_names = self.keycode_picker.macro_names.clone();
        let tap_dance_names = self.keycode_picker.tap_dance_names.clone();
        let key_legend_layout = self.app_settings.key_legend_layout;
        let show_shifted_number_symbols = self.app_settings.show_shifted_number_symbols;
        let layout_options_value = self.layout_options_value;
        let encoder_visibility = self.encoder_visibility.clone();
        let matrix_pressed = self.matrix_tester_pressed.clone();
        let pressed_key_layers = self.sticky_layout_pressed_key_layers.clone();
        let dark = self.app_settings.sticky_layout_dark_mode;
        let mut sticky_dark_mode = self.app_settings.sticky_layout_dark_mode;
        let mut sticky_opacity =
            clamp_sticky_layout_opacity(self.app_settings.sticky_layout_opacity);
        let mut sticky_always_on_top = self.app_settings.sticky_layout_always_on_top;
        let sticky_window_size = sticky_layout_saved_window_size(&self.app_settings);
        let mut observed_sticky_size: Option<Vec2> = None;
        let mut resize_opacity_hold_frames = self.sticky_layout_resize_opacity_hold_frames;
        let mut should_close = false;
        let mut should_save_settings = false;

        let mut viewport_builder = egui::ViewportBuilder::default()
            .with_title(window_title.clone())
            .with_min_inner_size(sticky_layout_default_window_size())
            .with_resizable(true)
            .with_decorations(false)
            .with_taskbar(false)
            .with_window_type(egui::X11WindowType::Utility)
            .with_window_level(if sticky_always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            });
        if self.sticky_layout_last_size.is_none() {
            viewport_builder = viewport_builder.with_inner_size(sticky_window_size);
        }

        ctx.show_viewport_immediate(
            viewport_id,
            viewport_builder,
            |viewport_ctx, viewport_class| {
                if viewport_ctx.input(|i| i.viewport().close_requested()) {
                    should_close = true;
                    return;
                }

                if let Some(current_rect) = viewport_ctx.input(|i| i.viewport().inner_rect) {
                    let current_size = current_rect.size();
                    if current_size.x.is_finite()
                        && current_size.y.is_finite()
                        && current_size.x > 0.0
                        && current_size.y > 0.0
                    {
                        if self
                            .sticky_layout_last_size
                            .map(|last_size| {
                                (last_size.x - current_size.x).abs() > 0.5
                                    || (last_size.y - current_size.y).abs() > 0.5
                            })
                            .unwrap_or(false)
                        {
                            resize_opacity_hold_frames = 8;
                        }
                        observed_sticky_size = Some(current_size);
                    }
                }

                let viewport_default_size = sticky_window_size;

                let mut draw_contents = |ui: &mut egui::Ui, should_close: &mut bool| {
                    *ui.visuals_mut() = sticky_layout_visuals(dark);
                    let effective_sticky_opacity = if resize_opacity_hold_frames > 0 {
                        1.0
                    } else {
                        sticky_opacity
                    };
                    #[cfg(not(target_os = "windows"))]
                    ui.set_opacity(effective_sticky_opacity);
                    #[cfg(target_os = "windows")]
                    set_windows_window_opacity_by_title(&window_title, effective_sticky_opacity);
                    let panel_bg = app_panel_fill(dark);
                    let full_rect = ui.max_rect();
                    ui.painter().rect_filled(full_rect, 0.0, panel_bg);
                    ui.painter().rect(
                        full_rect.shrink(0.5),
                        0.0,
                        Color32::TRANSPARENT,
                        Stroke::new(1.0, app_border_color(dark)),
                        egui::StrokeKind::Inside,
                    );
                    let title_rect = egui::Rect::from_min_max(
                        full_rect.min,
                        egui::pos2(
                            full_rect.right(),
                            full_rect.top() + STICKY_LAYOUT_WINDOW_TITLE_H,
                        ),
                    );
                    let buttons_w = 60.0;
                    let title_drag_rect = egui::Rect::from_min_max(
                        title_rect.min,
                        egui::pos2(title_rect.right() - buttons_w, title_rect.bottom()),
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(title_rect.left(), title_rect.bottom()),
                            egui::pos2(title_rect.right(), title_rect.bottom()),
                        ],
                        Stroke::new(1.0, app_border_color(dark)),
                    );

                    let title_x = title_rect.left() + 12.0;
                    if let Some(device_title) = &device_title {
                        ui.painter().text(
                            egui::pos2(title_x, title_rect.top() + 14.0),
                            egui::Align2::LEFT_CENTER,
                            indicator_title.as_str(),
                            FontId::proportional(13.0),
                            if dark {
                                Color32::from_gray(238)
                            } else {
                                Color32::from_gray(32)
                            },
                        );
                        ui.painter().text(
                            egui::pos2(title_x, title_rect.top() + 30.0),
                            egui::Align2::LEFT_CENTER,
                            device_title.as_str(),
                            FontId::proportional(11.0),
                            app_muted_text(dark),
                        );
                    } else {
                        ui.painter().text(
                            egui::pos2(title_x, title_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            indicator_title.as_str(),
                            FontId::proportional(13.0),
                            if dark {
                                Color32::from_gray(238)
                            } else {
                                Color32::from_gray(32)
                            },
                        );
                    }

                    ui.allocate_ui_at_rect(title_rect.shrink2(Vec2::new(6.0, 4.0)), |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if sticky_layout_window_icon_button(
                                ui,
                                dark,
                                StickyLayoutWindowButton::Close,
                                false,
                                crate::i18n::tr_catalog(
                                    lang,
                                    "ui.sticky_layout_window_close_tooltip",
                                ),
                            )
                            .clicked()
                            {
                                *should_close = true;
                            }
                            ui.add_space(4.0);
                            if sticky_layout_window_icon_button(
                                ui,
                                dark,
                                StickyLayoutWindowButton::Pin,
                                sticky_always_on_top,
                                crate::i18n::tr_catalog(
                                    lang,
                                    "ui.sticky_layout_window_pin_tooltip",
                                ),
                            )
                            .clicked()
                            {
                                sticky_always_on_top = !sticky_always_on_top;
                                should_save_settings = true;
                                viewport_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                    if sticky_always_on_top {
                                        egui::WindowLevel::AlwaysOnTop
                                    } else {
                                        egui::WindowLevel::Normal
                                    },
                                ));
                            }
                        });
                    });

                    let footer_rect = egui::Rect::from_min_max(
                        egui::pos2(
                            full_rect.left(),
                            full_rect.bottom() - STICKY_LAYOUT_WINDOW_FOOTER_H,
                        ),
                        full_rect.right_bottom(),
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(footer_rect.left(), footer_rect.top()),
                            egui::pos2(footer_rect.right(), footer_rect.top()),
                        ],
                        Stroke::new(1.0, app_border_color(dark)),
                    );
                    let footer_drag_rect = egui::Rect::from_min_max(
                        egui::pos2(footer_rect.left() + 124.0, footer_rect.top()),
                        egui::pos2(footer_rect.right() - 154.0, footer_rect.bottom()),
                    );
                    let title_drag_response = ui.interact(
                        title_drag_rect,
                        ui.id().with("sticky_layout_window_title_drag"),
                        Sense::click_and_drag(),
                    );
                    let footer_drag_response = ui.interact(
                        footer_drag_rect,
                        ui.id().with("sticky_layout_window_footer_drag"),
                        Sense::click_and_drag(),
                    );
                    if title_drag_response.drag_started() || footer_drag_response.drag_started() {
                        viewport_ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    let preview_rect = egui::Rect::from_min_max(
                        egui::pos2(full_rect.left(), title_rect.bottom()),
                        egui::pos2(full_rect.right(), footer_rect.top()),
                    );
                    let rect = preview_rect.shrink(STICKY_LAYOUT_WINDOW_MARGIN);
                    if let Some(layout) = &layout {
                        Self::paint_sticky_layout_preview(
                            ui,
                            layout,
                            sticky_layer,
                            &layer_names,
                            &macro_names,
                            &tap_dance_names,
                            key_legend_layout,
                            show_shifted_number_symbols,
                            layout_options_value,
                            &encoder_visibility,
                            &matrix_pressed,
                            &pressed_key_layers,
                            1.0,
                            dark,
                            rect,
                        );
                    } else {
                        ui.painter().rect(
                            rect,
                            16.0,
                            app_surface_fill(dark),
                            Stroke::new(1.0, app_border_color(dark)),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            crate::i18n::tr_catalog(lang, "ui.sticky_layout_no_keyboard"),
                            FontId::proportional(14.0),
                            app_muted_text(dark),
                        );
                    }

                    let transparency_rect = egui::Rect::from_min_size(
                        egui::pos2(footer_rect.left() + 8.0, footer_rect.center().y - 12.0),
                        egui::vec2(132.0, 24.0),
                    );
                    ui.allocate_ui_at_rect(transparency_rect, |ui| {
                        if draw_sticky_layout_transparency_dropdown(
                            ui,
                            lang,
                            dark,
                            &mut sticky_opacity,
                        ) {
                            should_save_settings = true;
                        }
                    });

                    let theme_rect = egui::Rect::from_min_size(
                        egui::pos2(footer_rect.right() - 150.0, footer_rect.center().y - 11.0),
                        egui::vec2(118.0, 22.0),
                    );
                    ui.allocate_ui_at_rect(theme_rect, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_theme_selector_labels(ui, lang, &mut sticky_dark_mode, true);
                        });
                    });

                    let resize_rect = egui::Rect::from_min_size(
                        egui::pos2(footer_rect.right() - 26.0, footer_rect.bottom() - 26.0),
                        egui::vec2(26.0, 26.0),
                    );
                    let resize_resp = ui.interact(
                        resize_rect,
                        ui.id().with("sticky_layout_resize_grip"),
                        Sense::click_and_drag(),
                    );
                    if resize_resp.hovered() || resize_resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeSouthEast);
                    }
                    if resize_resp.drag_started() {
                        resize_opacity_hold_frames = 8;
                        viewport_ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                            egui::ResizeDirection::SouthEast,
                        ));
                    }
                    if resize_resp.dragged() {
                        resize_opacity_hold_frames = 8;
                    }
                    if resize_resp.drag_stopped() {
                        should_save_settings = true;
                    }
                    let grip_color = app_muted_text(dark);
                    for offset in [7.0, 12.0, 17.0] {
                        ui.painter().line_segment(
                            [
                                egui::pos2(full_rect.right() - offset, full_rect.bottom() - 5.0),
                                egui::pos2(full_rect.right() - 5.0, full_rect.bottom() - offset),
                            ],
                            Stroke::new(1.0, grip_color),
                        );
                    }
                };

                if matches!(viewport_class, egui::ViewportClass::Embedded) {
                    let mut open = true;
                    egui::Window::new(window_title.as_str())
                        .open(&mut open)
                        .default_size(viewport_default_size)
                        .min_size(sticky_layout_default_window_size())
                        .resizable(true)
                        .show(viewport_ctx, |ui| {
                            draw_contents(ui, &mut should_close);
                        });
                    if !open {
                        should_close = true;
                    }
                } else {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE.fill(app_panel_fill(dark)))
                        .show(viewport_ctx, |ui| {
                            draw_contents(ui, &mut should_close);
                        });
                }
            },
        );

        if resize_opacity_hold_frames > 0 {
            resize_opacity_hold_frames = resize_opacity_hold_frames.saturating_sub(1);
        }
        self.sticky_layout_resize_opacity_hold_frames = resize_opacity_hold_frames;

        if let Some(size) = observed_sticky_size {
            self.sticky_layout_last_size = Some(size);
            let saved_size = sticky_layout_saved_window_size(&self.app_settings);
            if (saved_size.x - size.x).abs() > 1.0 || (saved_size.y - size.y).abs() > 1.0 {
                self.app_settings.sticky_layout_window_size = Some([size.x, size.y]);
                should_save_settings = true;
            }
        }

        if should_close {
            self.app_settings.sticky_layout_window = false;
            self.sticky_layout_last_size = None;
            should_save_settings = true;
        }
        if self.app_settings.sticky_layout_dark_mode != sticky_dark_mode {
            self.app_settings.sticky_layout_dark_mode = sticky_dark_mode;
            should_save_settings = true;
        }
        sticky_opacity = clamp_sticky_layout_opacity(sticky_opacity);
        if (self.app_settings.sticky_layout_opacity - sticky_opacity).abs() > f32::EPSILON {
            self.app_settings.sticky_layout_opacity = sticky_opacity;
            should_save_settings = true;
        }
        if self.app_settings.sticky_layout_always_on_top != sticky_always_on_top {
            self.app_settings.sticky_layout_always_on_top = sticky_always_on_top;
            should_save_settings = true;
        }
        if should_save_settings {
            save_app_settings(&self.app_settings);
        }
    }
}
