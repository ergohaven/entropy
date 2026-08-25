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
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, app_border_color(dark));
    visuals.widgets.inactive.bg_fill = app_surface_fill(dark);
    visuals.widgets.inactive.weak_bg_fill = app_surface_fill(dark);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, app_border_color(dark));
    visuals.widgets.hovered.bg_fill = app_hover_fill(dark);
    visuals.widgets.hovered.weak_bg_fill = app_hover_fill(dark);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, app_border_color(dark));
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals
}

#[cfg(target_os = "windows")]
fn set_windows_window_opacity_by_title(title: &str, opacity: f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes,
        SetWindowLongPtrW, WS_EX_LAYERED,
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

#[cfg(target_os = "macos")]
fn macos_sticky_layout_window_by_title(title: &str) -> Option<*mut objc::runtime::Object> {
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let ns_application = objc::runtime::Class::get("NSApplication")?;
        let app: *mut objc::runtime::Object = msg_send![ns_application, sharedApplication];
        if app.is_null() {
            return None;
        }
        let windows: *mut objc::runtime::Object = msg_send![app, windows];
        if windows.is_null() {
            return None;
        }
        let count: usize = msg_send![windows, count];
        for idx in 0..count {
            let window: *mut objc::runtime::Object = msg_send![windows, objectAtIndex: idx];
            if window.is_null() {
                continue;
            }
            let ns_title: *mut objc::runtime::Object = msg_send![window, title];
            if ns_title.is_null() {
                continue;
            }
            let utf8: *const std::os::raw::c_char = msg_send![ns_title, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let matches = std::ffi::CStr::from_ptr(utf8)
                .to_str()
                .map(|value| value == title)
                .unwrap_or(false);
            if matches {
                return Some(window);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn set_macos_window_opacity_by_title(title: &str, opacity: f32) {
    use objc::{msg_send, sel, sel_impl};

    let Some(window) = macos_sticky_layout_window_by_title(title) else {
        return;
    };
    let opacity = clamp_sticky_layout_opacity(opacity) as f64;
    unsafe {
        let _: () = msg_send![window, setAlphaValue: opacity];
    }
}

#[cfg(target_os = "macos")]
fn bring_macos_sticky_layout_window_to_front_by_title(title: &str) {
    use objc::{msg_send, sel, sel_impl};

    // Winit creates an NSWindow and leaves hidesOnDeactivate at its false default. Pinning only
    // changes level/resizability, so there is no AppKit property to invert when unpinning.
    let Some(window) = macos_sticky_layout_window_by_title(title) else {
        return;
    };
    unsafe {
        let _: () = msg_send![window, orderFrontRegardless];
    }
}

const STICKY_LAYOUT_WINDOW_W: f32 = 720.0_f32;
const STICKY_LAYOUT_WINDOW_H: f32 = 360.0_f32;
const STICKY_LAYOUT_WINDOW_MARGIN: f32 = 1.0_f32;
const STICKY_LAYOUT_WINDOW_TITLE_H: f32 = 42.0_f32;
const STICKY_LAYOUT_WINDOW_FOOTER_H: f32 = 34.0_f32;

#[derive(Clone, Copy, Debug, PartialEq)]
struct StickyLayoutViewportControls {
    dark_mode: bool,
    opacity: f32,
    visibility_mode: StickyLayoutVisibilityMode,
    always_on_top: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum StickyLayoutViewportEvent {
    Close,
    Controls(StickyLayoutViewportControls),
    WindowSize(Vec2),
    ResizeActive,
}

#[derive(Clone, Default)]
pub(crate) struct StickyLayoutViewportEventQueue(
    std::sync::Arc<std::sync::Mutex<Vec<StickyLayoutViewportEvent>>>,
);

fn push_sticky_layout_viewport_event(
    events: &StickyLayoutViewportEventQueue,
    event: StickyLayoutViewportEvent,
) {
    events
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(event);
}

fn drain_sticky_layout_viewport_events(
    events: &StickyLayoutViewportEventQueue,
) -> Vec<StickyLayoutViewportEvent> {
    std::mem::take(
        &mut *events
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

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
    visibility_mode: &mut StickyLayoutVisibilityMode,
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
    let pressed_only_text = crate::i18n::tr_catalog(lang, "ui.sticky_layout_pressed_only");
    let selected_text = if matches!(visibility_mode, StickyLayoutVisibilityMode::PressedOnly) {
        pressed_only_text.to_string()
    } else {
        format!(
            "{} {}%",
            label_prefix,
            (OPACITY_VALUES[selected_idx] * 100.0).round() as i32
        )
    };
    let dropdown_id = ui.id().with("sticky_layout_transparency_dropdown");
    let width = 136.0;
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
    crate::ui_style::popup_below_widget(
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
                    let (pressed_rect, pressed_resp) =
                        ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::click());
                    if pressed_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let pressed_selected =
                        matches!(visibility_mode, StickyLayoutVisibilityMode::PressedOnly);
                    let pressed_fill = if pressed_selected || pressed_resp.hovered() {
                        app_hover_fill(dark)
                    } else {
                        app_surface_fill(dark)
                    };
                    ui.painter().rect_filled(pressed_rect, 7.0, pressed_fill);
                    ui.painter().text(
                        egui::pos2(pressed_rect.left() + 10.0, pressed_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        pressed_only_text.to_string(),
                        FontId::proportional(11.0),
                        if pressed_selected {
                            if dark {
                                Color32::from_rgb(235, 235, 235)
                            } else {
                                Color32::from_rgb(42, 42, 44)
                            }
                        } else {
                            app_muted_text(dark)
                        },
                    );
                    if pressed_resp.clicked() {
                        *visibility_mode = StickyLayoutVisibilityMode::PressedOnly;
                        changed = true;
                        egui::Popup::close_all(ui.ctx());
                    }

                    for (idx, value) in OPACITY_VALUES.iter().copied().enumerate() {
                        let option_text =
                            format!("{} {}%", label_prefix, (value * 100.0).round() as i32);
                        let selected = idx == selected_idx
                            && matches!(
                                visibility_mode,
                                StickyLayoutVisibilityMode::LayoutAndPresses
                            );
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
                            *visibility_mode = StickyLayoutVisibilityMode::LayoutAndPresses;
                            *opacity = value;
                            changed = true;
                            egui::Popup::close_all(ui.ctx());
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
        Stroke::new(if active { 1.2_f32 } else { 0.8_f32 }, stroke_color),
        egui::StrokeKind::Inside,
    );

    let color = if active {
        app_accent()
    } else {
        app_muted_text(dark)
    };
    let stroke = Stroke::new(1.7_f32, color);
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

pub(super) fn sticky_layout_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("entropy_sticky_layout_window")
}

fn sticky_layout_saved_window_size(settings: &AppSettings) -> Vec2 {
    settings
        .sticky_layout_window_size
        .map(|[w, h]| egui::vec2(w.max(STICKY_LAYOUT_WINDOW_W), h.max(STICKY_LAYOUT_WINDOW_H)))
        .unwrap_or_else(sticky_layout_default_window_size)
}

fn sticky_layout_window_level(always_on_top: bool) -> egui::WindowLevel {
    if always_on_top {
        egui::WindowLevel::AlwaysOnTop
    } else {
        egui::WindowLevel::Normal
    }
}

fn sticky_layout_window_resizable(always_on_top: bool) -> bool {
    !always_on_top
}

fn sticky_layout_viewport_builder(
    window_title: String,
    always_on_top: bool,
) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(window_title)
        .with_min_inner_size(sticky_layout_default_window_size())
        .with_resizable(sticky_layout_window_resizable(always_on_top))
        .with_decorations(false)
        .with_taskbar(false)
        .with_window_type(egui::X11WindowType::Utility)
        .with_window_level(sticky_layout_window_level(always_on_top))
}

fn sticky_layout_pin_viewport_commands(always_on_top: bool) -> [egui::ViewportCommand; 2] {
    [
        egui::ViewportCommand::WindowLevel(sticky_layout_window_level(always_on_top)),
        egui::ViewportCommand::Resizable(sticky_layout_window_resizable(always_on_top)),
    ]
}

#[cfg(any(target_os = "macos", test))]
fn sticky_layout_should_bring_window_forward(
    window_opening: bool,
    was_always_on_top: bool,
    is_always_on_top: bool,
) -> bool {
    (window_opening && is_always_on_top) || (!was_always_on_top && is_always_on_top)
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_sticky_layout_background(&mut self, ctx: &egui::Context) {
        if !self.app_settings.sticky_layout_window || self.is_vial_locked() {
            return;
        }

        if let Some((rows, cols)) = self
            .layout
            .as_ref()
            .map(|layout| (layout.rows, layout.cols))
        {
            self.poll_switch_matrix_state(ctx, rows, cols, false);
        }
    }

    fn apply_sticky_layout_viewport_events(&mut self) -> bool {
        let events = drain_sticky_layout_viewport_events(&self.sticky_layout_viewport_events);
        let mut should_close = false;
        let mut should_save_settings = false;

        for event in events {
            match event {
                StickyLayoutViewportEvent::Close => {
                    should_close = true;
                    should_save_settings = true;
                }
                StickyLayoutViewportEvent::Controls(controls) => {
                    if self.app_settings.sticky_layout_dark_mode != controls.dark_mode {
                        self.app_settings.sticky_layout_dark_mode = controls.dark_mode;
                        should_save_settings = true;
                    }
                    let opacity = clamp_sticky_layout_opacity(controls.opacity);
                    if (self.app_settings.sticky_layout_opacity - opacity).abs() > f32::EPSILON {
                        self.app_settings.sticky_layout_opacity = opacity;
                        should_save_settings = true;
                    }
                    if self.app_settings.sticky_layout_visibility_mode != controls.visibility_mode {
                        self.app_settings.sticky_layout_visibility_mode = controls.visibility_mode;
                        should_save_settings = true;
                    }
                    if self.app_settings.sticky_layout_always_on_top != controls.always_on_top {
                        self.app_settings.sticky_layout_always_on_top = controls.always_on_top;
                        should_save_settings = true;
                    }
                }
                StickyLayoutViewportEvent::WindowSize(size) => {
                    if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 {
                        let resized = self
                            .sticky_layout_last_size
                            .map(|last_size| {
                                (last_size.x - size.x).abs() > 0.5
                                    || (last_size.y - size.y).abs() > 0.5
                            })
                            .unwrap_or(false);
                        self.sticky_layout_last_size = Some(size);
                        if resized {
                            self.sticky_layout_resize_opacity_hold_frames = 8;
                        }
                        let saved_size = sticky_layout_saved_window_size(&self.app_settings);
                        if (saved_size.x - size.x).abs() > 1.0
                            || (saved_size.y - size.y).abs() > 1.0
                        {
                            self.app_settings.sticky_layout_window_size = Some([size.x, size.y]);
                            should_save_settings = true;
                        }
                    }
                }
                StickyLayoutViewportEvent::ResizeActive => {
                    self.sticky_layout_resize_opacity_hold_frames = 8;
                }
            }
        }

        if self.sticky_layout_resize_opacity_hold_frames > 0 {
            self.sticky_layout_resize_opacity_hold_frames = self
                .sticky_layout_resize_opacity_hold_frames
                .saturating_sub(1);
        }
        if should_close {
            self.app_settings.sticky_layout_window = false;
            self.sticky_layout_last_size = None;
            self.sticky_layout_resize_opacity_hold_frames = 0;
        }
        if should_save_settings {
            save_app_settings(&self.app_settings);
        }

        should_close
    }

    pub(super) fn draw_sticky_layout_window(&mut self, ctx: &egui::Context) {
        if !self.app_settings.sticky_layout_window {
            let _ = drain_sticky_layout_viewport_events(&self.sticky_layout_viewport_events);
            self.sticky_layout_last_size = None;
            self.sticky_layout_resize_opacity_hold_frames = 0;
            return;
        }

        if self.apply_sticky_layout_viewport_events() {
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

        let viewport_id = sticky_layout_viewport_id();
        let lang = self.app_settings.language;
        let layout = self.layout.clone();
        let selected_device_name = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(|device| {
                let display_name = self
                    .device_display_names
                    .get(&device.display_name_cache_key())
                    .map(String::as_str)
                    .unwrap_or(device.name.as_str());
                device.display_name_with_transport(display_name)
            });
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
        self.sticky_layout_active_layer = sticky_layer;
        let layer_names = self.layer_names.clone();
        let macro_names = self.keycode_picker.macro_names.clone();
        let tap_dance_names = self.keycode_picker.tap_dance_names.clone();
        let key_legend_layout = self.app_settings.key_legend_layout;
        let show_shifted_number_symbols = self.app_settings.show_shifted_number_symbols;
        let layout_options_value = self.layout_options_value;
        let encoder_visibility = self.encoder_visibility.clone();
        let module_settings = self.module_settings.clone();
        let matrix_pressed = self.matrix_tester_pressed.clone();
        let pressed_key_layers = self.sticky_layout_pressed_key_layers.clone();
        let controls = StickyLayoutViewportControls {
            dark_mode: self.app_settings.sticky_layout_dark_mode,
            opacity: clamp_sticky_layout_opacity(self.app_settings.sticky_layout_opacity),
            visibility_mode: self.app_settings.sticky_layout_visibility_mode,
            always_on_top: self.app_settings.sticky_layout_always_on_top,
        };
        let sticky_window_size = sticky_layout_saved_window_size(&self.app_settings);
        let sticky_layout_last_size = self.sticky_layout_last_size;
        let resize_opacity_hold_frames = self.sticky_layout_resize_opacity_hold_frames;
        let viewport_events = self.sticky_layout_viewport_events.clone();

        let mut viewport_builder =
            sticky_layout_viewport_builder(window_title.clone(), controls.always_on_top);
        if self.sticky_layout_last_size.is_none() {
            viewport_builder = viewport_builder.with_inner_size(sticky_window_size);
        }

        ctx.show_viewport_deferred(
            viewport_id,
            viewport_builder,
            move |viewport_ui, viewport_class| {
                let viewport_ctx = viewport_ui.ctx().clone();
                let dark = controls.dark_mode;
                let mut sticky_dark_mode = controls.dark_mode;
                let mut sticky_opacity = controls.opacity;
                let mut sticky_visibility_mode = controls.visibility_mode;
                let mut sticky_always_on_top = controls.always_on_top;
                let mut observed_sticky_size: Option<Vec2> = None;
                let mut resize_opacity_hold_frames = resize_opacity_hold_frames;
                let mut should_close = false;
                let mut resize_active = false;

                if viewport_ctx.input(|i| i.viewport().close_requested()) {
                    push_sticky_layout_viewport_event(
                        &viewport_events,
                        StickyLayoutViewportEvent::Close,
                    );
                    viewport_ctx.request_repaint_of(egui::ViewportId::ROOT);
                    return;
                }

                #[cfg(target_os = "macos")]
                if sticky_layout_should_bring_window_forward(
                    sticky_layout_last_size.is_none(),
                    controls.always_on_top,
                    controls.always_on_top,
                ) {
                    bring_macos_sticky_layout_window_to_front_by_title(&window_title);
                }

                if let Some(current_rect) = viewport_ctx.input(|i| i.viewport().inner_rect) {
                    let current_size = current_rect.size();
                    if current_size.x.is_finite()
                        && current_size.y.is_finite()
                        && current_size.x > 0.0
                        && current_size.y > 0.0
                    {
                        let size_changed = sticky_layout_last_size
                            .map(|last_size| {
                                (last_size.x - current_size.x).abs() > 0.5
                                    || (last_size.y - current_size.y).abs() > 0.5
                            })
                            .unwrap_or(true);
                        if size_changed {
                            if sticky_layout_last_size.is_some() {
                                resize_opacity_hold_frames = 8;
                                resize_active = true;
                            }
                            observed_sticky_size = Some(current_size);
                        }
                    }
                }

                let mut draw_contents = |ui: &mut egui::Ui, should_close: &mut bool| {
                    *ui.visuals_mut() = sticky_layout_visuals(dark);
                    let effective_sticky_opacity = if resize_opacity_hold_frames > 0 {
                        1.0
                    } else {
                        sticky_opacity
                    };
                    #[cfg(target_os = "windows")]
                    set_windows_window_opacity_by_title(&window_title, effective_sticky_opacity);
                    #[cfg(target_os = "macos")]
                    set_macos_window_opacity_by_title(&window_title, effective_sticky_opacity);
                    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
                    ui.set_opacity(effective_sticky_opacity);
                    let panel_bg = app_panel_fill(dark);
                    let full_rect = ui.max_rect();
                    ui.painter().rect_filled(full_rect, 0.0, panel_bg);
                    ui.painter().rect(
                        full_rect.shrink(0.5),
                        0.0,
                        Color32::TRANSPARENT,
                        Stroke::new(1.0_f32, app_border_color(dark)),
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
                        Stroke::new(1.0_f32, app_border_color(dark)),
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

                    crate::ui_style::allocate_ui_at_rect(
                        ui,
                        title_rect.shrink2(Vec2::new(6.0, 4.0)),
                        |ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
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
                                        #[cfg(target_os = "macos")]
                                        let was_always_on_top = sticky_always_on_top;
                                        sticky_always_on_top = !sticky_always_on_top;
                                        for command in sticky_layout_pin_viewport_commands(
                                            sticky_always_on_top,
                                        ) {
                                            viewport_ctx.send_viewport_cmd(command);
                                        }
                                        #[cfg(target_os = "macos")]
                                        if sticky_layout_should_bring_window_forward(
                                            false,
                                            was_always_on_top,
                                            sticky_always_on_top,
                                        ) {
                                            bring_macos_sticky_layout_window_to_front_by_title(
                                                &window_title,
                                            );
                                        }
                                    }
                                },
                            );
                        },
                    );

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
                        Stroke::new(1.0_f32, app_border_color(dark)),
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
                            &module_settings,
                            &matrix_pressed,
                            &pressed_key_layers,
                            sticky_visibility_mode,
                            1.0,
                            dark,
                            rect,
                        );
                    } else {
                        ui.painter().rect(
                            rect,
                            16.0,
                            app_surface_fill(dark),
                            Stroke::new(1.0_f32, app_border_color(dark)),
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
                    crate::ui_style::allocate_ui_at_rect(ui, transparency_rect, |ui| {
                        let _ = draw_sticky_layout_transparency_dropdown(
                            ui,
                            lang,
                            dark,
                            &mut sticky_opacity,
                            &mut sticky_visibility_mode,
                        );
                    });

                    let theme_rect = egui::Rect::from_min_size(
                        egui::pos2(footer_rect.right() - 150.0, footer_rect.center().y - 11.0),
                        egui::vec2(118.0, 22.0),
                    );
                    crate::ui_style::allocate_ui_at_rect(ui, theme_rect, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_theme_selector_labels(ui, lang, &mut sticky_dark_mode, true);
                        });
                    });

                    if sticky_layout_window_resizable(sticky_always_on_top) {
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
                            resize_active = true;
                            viewport_ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                                egui::ResizeDirection::SouthEast,
                            ));
                        }
                        if resize_resp.dragged() {
                            resize_opacity_hold_frames = 8;
                            resize_active = true;
                        }
                        if resize_resp.drag_stopped() {
                            resize_active = true;
                        }
                        let grip_color = app_muted_text(dark);
                        for offset in [7.0, 12.0, 17.0] {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(
                                        full_rect.right() - offset,
                                        full_rect.bottom() - 5.0,
                                    ),
                                    egui::pos2(
                                        full_rect.right() - 5.0,
                                        full_rect.bottom() - offset,
                                    ),
                                ],
                                Stroke::new(1.0_f32, grip_color),
                            );
                        }
                    }
                };

                if matches!(viewport_class, egui::ViewportClass::EmbeddedWindow) {
                    draw_contents(viewport_ui, &mut should_close);
                } else {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE.fill(app_panel_fill(dark)))
                        .show_inside(viewport_ui, |ui| {
                            draw_contents(ui, &mut should_close);
                        });
                }

                let updated_controls = StickyLayoutViewportControls {
                    dark_mode: sticky_dark_mode,
                    opacity: clamp_sticky_layout_opacity(sticky_opacity),
                    visibility_mode: sticky_visibility_mode,
                    always_on_top: sticky_always_on_top,
                };
                let mut root_repaint_required = false;
                if updated_controls != controls {
                    push_sticky_layout_viewport_event(
                        &viewport_events,
                        StickyLayoutViewportEvent::Controls(updated_controls),
                    );
                    root_repaint_required = true;
                }
                if let Some(size) = observed_sticky_size {
                    push_sticky_layout_viewport_event(
                        &viewport_events,
                        StickyLayoutViewportEvent::WindowSize(size),
                    );
                    root_repaint_required = true;
                }
                if resize_active {
                    push_sticky_layout_viewport_event(
                        &viewport_events,
                        StickyLayoutViewportEvent::ResizeActive,
                    );
                    root_repaint_required = true;
                }
                if should_close {
                    push_sticky_layout_viewport_event(
                        &viewport_events,
                        StickyLayoutViewportEvent::Close,
                    );
                    viewport_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    root_repaint_required = true;
                }
                if root_repaint_required {
                    viewport_ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pin_commands(
        always_on_top: bool,
        expected_level: egui::WindowLevel,
        expected_resizable: bool,
    ) {
        let [
            egui::ViewportCommand::WindowLevel(level),
            egui::ViewportCommand::Resizable(resizable),
        ] = sticky_layout_pin_viewport_commands(always_on_top)
        else {
            panic!("pin transition must update window level and resizability");
        };
        assert_eq!(level, expected_level);
        assert_eq!(resizable, expected_resizable);
    }

    #[test]
    fn layout_indicator_uses_an_independent_deferred_viewport() {
        for (always_on_top, expected_level, expected_resizable) in [
            (true, egui::WindowLevel::AlwaysOnTop, false),
            (false, egui::WindowLevel::Normal, true),
        ] {
            let ctx = egui::Context::default();
            ctx.set_embed_viewports(false);
            let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new(&creation_context);
            app.app_settings.sticky_layout_window = true;
            app.app_settings.sticky_layout_always_on_top = always_on_top;
            app.vial_unlocked = Some(true);

            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                app.draw_sticky_layout_window(ui.ctx());
            });
            let viewport = output
                .viewport_output
                .get(&sticky_layout_viewport_id())
                .expect("layout indicator viewport should be submitted");

            assert!(matches!(viewport.class, egui::ViewportClass::Deferred));
            assert!(viewport.viewport_ui_cb.is_some());
            assert_eq!(viewport.builder.window_level, Some(expected_level));
            assert_eq!(viewport.builder.resizable, Some(expected_resizable));
        }
    }

    #[test]
    fn pin_transition_commands_are_symmetric() {
        assert_pin_commands(true, egui::WindowLevel::AlwaysOnTop, false);
        assert_pin_commands(false, egui::WindowLevel::Normal, true);
    }

    #[test]
    fn macos_order_front_policy_is_edge_triggered() {
        assert!(sticky_layout_should_bring_window_forward(true, true, true));
        assert!(!sticky_layout_should_bring_window_forward(
            true, false, false
        ));
        assert!(sticky_layout_should_bring_window_forward(
            false, false, true
        ));
        assert!(!sticky_layout_should_bring_window_forward(
            false, true, false
        ));
        assert!(!sticky_layout_should_bring_window_forward(
            false, true, true
        ));
        assert!(!sticky_layout_should_bring_window_forward(
            false, false, false
        ));
    }
}
