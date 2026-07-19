#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]

use super::*;

#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

fn should_keep_vial_unlock_visible(unlock_open: bool, unlock_polling: bool) -> bool {
    unlock_open || unlock_polling
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn defer_exit_until_hid_write_returns(&mut self, ctx: &egui::Context) -> bool {
        if !self.hid_write_task_active() {
            return false;
        }

        self.exit_after_hid_write = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn finish_deferred_exit_after_hid_write(&mut self, ctx: &egui::Context) {
        if !self.exit_after_hid_write || self.hid_write_task_active() {
            return;
        }

        self.exit_after_hid_write = false;
        self.flush_pending_tap_hold_numeric_writes();
        self.fallback_entropy_display_presets_before_exit();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn keep_vial_unlock_visible(&mut self, ctx: &egui::Context) {
        self.restore_from_tray(ctx);
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            "status_messages.finish_unlock_before_closing",
        )
        .into();
        ctx.request_repaint();
    }

    pub(super) fn remember_main_window_size(&mut self, ctx: &egui::Context) {
        let viewport = ctx.input(|i| i.viewport().clone());
        if viewport.minimized == Some(true)
            || viewport.maximized == Some(true)
            || viewport.fullscreen == Some(true)
        {
            return;
        }

        let Some(size) = viewport.inner_rect.map(|rect| rect.size()) else {
            return;
        };
        if size.x < 800.0 || size.y < 500.0 || !size.x.is_finite() || !size.y.is_finite() {
            return;
        }

        let next = [size.x.round(), size.y.round()];
        let changed = self.app_settings.window_size.is_none_or(|current| {
            (current[0] - next[0]).abs() > 0.5 || (current[1] - next[1]).abs() > 0.5
        });
        if changed {
            self.app_settings.window_size = Some(next);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_single_instance_signal(&mut self, ctx: &egui::Context) {
        let signal = read_single_instance_signal();
        if signal.is_empty() || signal == self.last_single_instance_signal {
            return;
        }
        self.last_single_instance_signal = signal;
        self.status_msg = "Entropy refreshed from a repeated launch".into();
        self.restore_from_tray(ctx);
        if self.hid_device.is_none() && !matches!(self.connect_state, ConnectState::Loading { .. })
        {
            self.start_device_scan();
        }
        ctx.request_repaint();
    }

    pub(super) fn restore_from_tray(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.windows_hwnd {
            self.windows_window_hidden_to_tray = false;
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
                };
                let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
        }
        #[cfg(target_os = "macos")]
        {
            restore_macos_app_window(self.macos_ns_window);
            self.macos_window_hidden_to_menu_bar = false;
            self.macos_hidden_to_menu_bar_at = None;
            self.macos_app_was_hidden = false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    #[cfg(target_os = "windows")]
    pub(super) fn cache_windows_hwnd(&mut self, frame: &eframe::Frame) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if self.windows_hwnd.is_some() {
            return;
        }
        if let Ok(handle) = frame.window_handle() {
            if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                self.windows_hwnd = Some(win32.hwnd.get());
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn cache_macos_ns_window(&mut self, frame: &eframe::Frame) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if self.macos_ns_window.is_some() {
            return;
        }
        if let Ok(handle) = frame.window_handle() {
            if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                self.macos_ns_window = macos_ns_window_from_view(appkit.ns_view.as_ptr());
            }
        }
    }

    fn close_to_tray_prompt_needed(&self) -> bool {
        self.app_settings.text_expander_enabled
    }

    pub(super) fn handle_close_to_tray(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }

        if should_keep_vial_unlock_visible(self.unlock_open, self.vial_unlock_polling) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.keep_vial_unlock_visible(ctx);
            return;
        }

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if self.consume_tray_quit_request() {
            #[cfg(not(target_arch = "wasm32"))]
            if self.defer_exit_until_hid_write_returns(ctx) {
                return;
            }
            return;
        }

        if self.force_close_requested {
            #[cfg(not(target_arch = "wasm32"))]
            if self.defer_exit_until_hid_write_returns(ctx) {
                return;
            }
            return;
        }

        if self.app_settings.minimize_to_tray_on_close
            || matches!(
                self.app_settings.close_to_tray_behavior,
                CloseToTrayBehavior::Tray
            )
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.minimize_window_to_tray(ctx);
            return;
        }

        match self.app_settings.close_to_tray_behavior {
            CloseToTrayBehavior::Ask if self.close_to_tray_prompt_needed() => {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_to_tray_prompt_open = true;
                self.close_to_tray_prompt_remember = false;
                ctx.request_repaint();
            }
            CloseToTrayBehavior::Ask | CloseToTrayBehavior::Close | CloseToTrayBehavior::Tray => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.defer_exit_until_hid_write_returns(ctx);
                }
            }
        }
    }

    pub(super) fn minimize_window_to_tray(&mut self, ctx: &egui::Context) {
        if should_keep_vial_unlock_visible(self.unlock_open, self.vial_unlock_polling) {
            self.keep_vial_unlock_visible(ctx);
            return;
        }

        #[cfg(target_os = "linux")]
        let background_status = "Entropy is running in background";
        #[cfg(target_os = "macos")]
        let background_status = "Entropy is running in the menu bar";
        #[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
        let background_status = "Entropy is running in the tray";

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        self.ensure_tray_icon(ctx);

        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.windows_hwnd {
                unsafe {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
                    ShowWindow(hwnd as windows_sys::Win32::Foundation::HWND, SW_HIDE);
                }
                self.windows_window_hidden_to_tray = true;
                self.status_msg = background_status.into();
                return;
            }
        }
        #[cfg(target_os = "linux")]
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.status_msg = background_status.into();
            return;
        }
        #[cfg(target_os = "macos")]
        {
            self.macos_window_hidden_to_menu_bar = true;
            self.macos_hidden_to_menu_bar_at = Some(std::time::Instant::now());
            self.macos_app_was_hidden = true;
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            #[cfg(target_os = "macos")]
            hide_macos_app();
            self.status_msg = background_status.into();
        }
    }

    fn persist_close_to_tray_behavior(&mut self, behavior: CloseToTrayBehavior) {
        self.app_settings.close_to_tray_behavior = behavior;
        self.app_settings.minimize_to_tray_on_close = matches!(behavior, CloseToTrayBehavior::Tray);
        if !self.app_settings.minimize_to_tray_on_close {
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            {
                self.tray_icon = None;
            }
        }
        save_app_settings(&self.app_settings);
    }

    pub(super) fn draw_close_to_tray_prompt(&mut self, ctx: &egui::Context) {
        if !self.close_to_tray_prompt_open {
            return;
        }

        let dark = ctx.global_style().visuals.dark_mode;
        let screen_rect = ctx.content_rect();
        egui::Area::new("close_to_tray_prompt_backdrop".into())
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
                ui.interact(
                    rect,
                    egui::Id::new("close_to_tray_prompt_blocker"),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    Color32::from_black_alpha(crate::ui_style::modal_backdrop_alpha(dark)),
                );
            });

        let lang = self.app_settings.language;
        #[cfg(target_os = "linux")]
        let (title, body, remember, close_label, tray_label, cancel_label) =
            linux_close_to_tray_prompt_copy(
                lang,
                crate::smart_input::text_expander_runs_outside_entropy_process(),
            );
        #[cfg(target_os = "macos")]
        let (title, body, remember, close_label, tray_label, cancel_label) = match lang {
            crate::i18n::Language::Russian => (
                "Закрыть Entropy?",
                "Text Expander и фоновые функции остановятся, если закрыть приложение",
                "Запомнить выбор",
                "Закрыть",
                "В менюбар",
                "Отмена",
            ),
            crate::i18n::Language::English => (
                "Close Entropy?",
                "Text Expander and background features will stop if Entropy is closed",
                "Remember my choice",
                "Close",
                "Keep in menu bar",
                "Cancel",
            ),
        };
        #[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
        let (title, body, remember, close_label, tray_label, cancel_label) = match lang {
            crate::i18n::Language::Russian => (
                "Закрыть Entropy?",
                "Text Expander и фоновые функции остановятся, если закрыть приложение",
                "Запомнить выбор",
                "Закрыть",
                "Свернуть в трей",
                "Отмена",
            ),
            crate::i18n::Language::English => (
                "Close Entropy?",
                "Text Expander and background features will stop if Entropy is closed",
                "Remember my choice",
                "Close",
                "Minimize to tray",
                "Cancel",
            ),
        };

        let mut close_app = false;
        let mut minimize_to_tray = false;
        let mut cancel = false;

        let panel_size = Vec2::new(460.0, 176.0);
        egui::Window::new("")
            .id(egui::Id::new("close_to_tray_prompt_window"))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .movable(false)
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(panel_size)
            .frame(crate::ui_style::modal_window_frame(
                ctx.global_style().as_ref(),
                dark,
            ))
            .show(ctx, |ui| {
                ui.set_min_size(panel_size);
                let rect = egui::Rect::from_min_size(ui.min_rect().min, panel_size);
                let painter = ui.painter().clone();

                let close_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 22.0, rect.top() + 22.0),
                    Vec2::new(26.0, 26.0),
                );
                let close_resp = ui.interact(
                    close_rect,
                    egui::Id::new("close_to_tray_prompt_x"),
                    egui::Sense::click(),
                );
                if close_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if close_resp.clicked() {
                    cancel = true;
                }
                painter.text(
                    close_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "×",
                    FontId::proportional(24.0),
                    app_muted_text(dark),
                );

                painter.text(
                    egui::pos2(rect.center().x, rect.top() + 25.0),
                    egui::Align2::CENTER_CENTER,
                    title,
                    FontId::proportional(18.0),
                    ui.visuals().text_color(),
                );

                let body_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.center().x, rect.top() + 72.0),
                    Vec2::new(400.0, 28.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, body_rect, |ui| {
                    ui.add_sized(
                        body_rect.size(),
                        egui::Label::new(
                            RichText::new(body)
                                .size(12.5)
                                .color(ui.visuals().text_color()),
                        )
                        .wrap()
                        .halign(egui::Align::Center),
                    );
                });

                let checkbox_size = 13.0;
                let remember_gap = 7.0;
                let remember_font = FontId::proportional(12.5);
                let remember_text_width = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        remember.to_owned(),
                        remember_font.clone(),
                        ui.visuals().text_color(),
                    )
                    .size()
                    .x
                });
                let remember_width = checkbox_size + remember_gap + remember_text_width;
                let remember_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.center().x, rect.top() + 105.0),
                    Vec2::new(remember_width, 24.0),
                );
                let remember_resp = ui.interact(
                    remember_rect,
                    egui::Id::new("close_to_tray_prompt_remember"),
                    egui::Sense::click(),
                );
                if remember_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if remember_resp.clicked() {
                    self.close_to_tray_prompt_remember = !self.close_to_tray_prompt_remember;
                }
                let check_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        remember_rect.left(),
                        remember_rect.center().y - checkbox_size * 0.5,
                    ),
                    Vec2::splat(checkbox_size),
                );
                painter.rect(
                    check_rect,
                    3.0,
                    if self.close_to_tray_prompt_remember {
                        app_accent()
                    } else {
                        app_surface_fill(dark)
                    },
                    crate::ui_style::modal_outline_stroke(dark),
                    egui::StrokeKind::Inside,
                );
                if self.close_to_tray_prompt_remember {
                    painter.text(
                        check_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "✓",
                        FontId::proportional(10.0),
                        app_window_fill(dark),
                    );
                }
                painter.text(
                    egui::pos2(check_rect.right() + remember_gap, remember_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    remember,
                    remember_font,
                    ui.visuals().text_color(),
                );

                let close_size = Vec2::new(104.0, 32.0);
                let tray_size = Vec2::new(142.0, 32.0);
                let cancel_size = Vec2::new(104.0, 32.0);
                let gap = 8.0;
                let total_width = close_size.x + tray_size.x + cancel_size.x + gap * 2.0;
                let top = rect.top() + 132.0;
                let mut left = rect.center().x - total_width * 0.5;

                let close_button_rect =
                    egui::Rect::from_min_size(egui::pos2(left, top), close_size);
                left += close_size.x + gap;
                let tray_rect = egui::Rect::from_min_size(egui::pos2(left, top), tray_size);
                left += tray_size.x + gap;
                let cancel_rect = egui::Rect::from_min_size(egui::pos2(left, top), cancel_size);

                crate::ui_style::allocate_ui_at_rect(ui, close_button_rect, |ui| {
                    if crate::ui_style::modern_button(ui, close_label, close_size, true).clicked() {
                        close_app = true;
                    }
                });
                crate::ui_style::allocate_ui_at_rect(ui, tray_rect, |ui| {
                    if crate::ui_style::modern_button(ui, tray_label, tray_size, true).clicked() {
                        minimize_to_tray = true;
                    }
                });
                crate::ui_style::allocate_ui_at_rect(ui, cancel_rect, |ui| {
                    if crate::ui_style::modern_button(ui, cancel_label, cancel_size, true).clicked()
                    {
                        cancel = true;
                    }
                });
            });

        if close_app {
            if self.close_to_tray_prompt_remember {
                self.persist_close_to_tray_behavior(CloseToTrayBehavior::Close);
            }
            self.close_to_tray_prompt_open = false;
            self.force_close_requested = true;
            save_app_settings(&self.app_settings);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if minimize_to_tray {
            if self.close_to_tray_prompt_remember {
                self.persist_close_to_tray_behavior(CloseToTrayBehavior::Tray);
            }
            self.close_to_tray_prompt_open = false;
            self.minimize_window_to_tray(ctx);
        } else if cancel {
            self.close_to_tray_prompt_open = false;
        }
    }

    pub(super) fn set_launch_at_startup(&mut self, enabled: bool) -> bool {
        #[cfg(target_os = "windows")]
        {
            let Ok(exe) = std::env::current_exe() else {
                return false;
            };
            let exe_arg = format!("\"{}\"", exe.display());
            let status = if enabled {
                std::process::Command::new("reg")
                    .args([
                        "add",
                        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                        "/v",
                        "Entropy",
                        "/t",
                        "REG_SZ",
                        "/d",
                        &exe_arg,
                        "/f",
                    ])
                    .status()
            } else {
                std::process::Command::new("reg")
                    .args([
                        "delete",
                        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                        "/v",
                        "Entropy",
                        "/f",
                    ])
                    .status()
            };
            return status.map(|status| status.success()).unwrap_or(false);
        }
        #[cfg(target_os = "linux")]
        {
            let Some(autostart_dir) = dirs::config_dir().map(|dir| dir.join("autostart")) else {
                return false;
            };
            let desktop_path = autostart_dir.join("entropy.desktop");
            if !enabled {
                return match std::fs::remove_file(&desktop_path) {
                    Ok(()) => true,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                    Err(e) => {
                        log::warn!("failed to remove autostart entry: {e}");
                        false
                    }
                };
            }

            let exe = std::env::var_os("APPIMAGE")
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::current_exe().ok());
            let Some(exe) = exe else {
                return false;
            };

            if let Err(e) = std::fs::create_dir_all(&autostart_dir) {
                log::warn!("failed to create autostart directory: {e}");
                return false;
            }

            let exec = desktop_exec_arg(&exe.to_string_lossy());
            let desktop_entry = format!(
                "[Desktop Entry]\nType=Application\nName=Entropy\nComment=Modern app for programmable keyboards and input devices\nExec={exec}\nTerminal=false\nStartupNotify=false\nX-GNOME-Autostart-enabled=true\n"
            );
            if let Err(e) = std::fs::write(&desktop_path, desktop_entry) {
                log::warn!("failed to write autostart entry: {e}");
                return false;
            }
            true
        }
        #[cfg(target_os = "macos")]
        {
            let Some(launch_agents_dir) =
                dirs::home_dir().map(|dir| dir.join("Library").join("LaunchAgents"))
            else {
                return false;
            };
            let plist_path = launch_agents_dir.join("com.ergohaven.entropy.plist");
            if !enabled {
                return match std::fs::remove_file(&plist_path) {
                    Ok(()) => true,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                    Err(e) => {
                        log::warn!("failed to remove macOS launch agent: {e}");
                        false
                    }
                };
            }

            let Ok(exe) = std::env::current_exe() else {
                return false;
            };
            let program_arguments = macos_launch_agent_program_arguments(&exe);

            if let Err(e) = std::fs::create_dir_all(&launch_agents_dir) {
                log::warn!("failed to create macOS LaunchAgents directory: {e}");
                return false;
            }

            let plist = macos_launch_agent_plist(&program_arguments);
            if let Err(e) = std::fs::write(&plist_path, plist) {
                log::warn!("failed to write macOS launch agent: {e}");
                return false;
            }
            true
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            let _ = enabled;
            false
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(super) fn ensure_tray_icon(&mut self, ctx: &egui::Context) {
        if self.tray_icon.is_some() {
            return;
        }
        let Some(icon) = crate::app_icon::tray_icon(32) else {
            return;
        };
        let ctx_for_handler = ctx.clone();
        #[cfg(target_os = "windows")]
        let hwnd_for_handler = self.windows_hwnd;
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    TRAY_RESTORE_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
                    #[cfg(target_os = "windows")]
                    if let Some(hwnd) = hwnd_for_handler {
                        unsafe {
                            use windows_sys::Win32::UI::WindowsAndMessaging::{
                                SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
                            };
                            let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
                            ShowWindow(hwnd, SW_SHOW);
                            ShowWindow(hwnd, SW_RESTORE);
                            SetForegroundWindow(hwnd);
                        }
                    }
                    ctx_for_handler.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx_for_handler.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx_for_handler.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx_for_handler.request_repaint();
                }
                _ => {}
            }
        }));
        let ctx_for_menu = ctx.clone();
        #[cfg(target_os = "windows")]
        let hwnd_for_menu = self.windows_hwnd;
        tray_icon::menu::MenuEvent::set_event_handler(Some(
            move |event: tray_icon::menu::MenuEvent| {
                if event.id == "entropy_tray_quit" {
                    TRAY_QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
                    #[cfg(target_os = "windows")]
                    if let Some(hwnd) = hwnd_for_menu {
                        request_windows_window_close_from_tray(hwnd);
                    }
                    ctx_for_menu.request_repaint();
                }
            },
        ));
        let tray_menu = tray_icon::menu::Menu::new();
        let quit_item =
            tray_icon::menu::MenuItem::with_id("entropy_tray_quit", "Quit Entropy", true, None);
        if let Err(e) = tray_menu.append(&quit_item) {
            log::warn!("failed to create tray menu: {e}");
        }
        match tray_icon::TrayIconBuilder::new()
            .with_tooltip("Entropy")
            .with_menu(Box::new(tray_menu))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .with_icon(icon)
            .build()
        {
            Ok(icon) => self.tray_icon = Some(icon),
            Err(e) => log::warn!("failed to create tray icon: {e}"),
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn consume_tray_quit_request(&mut self) -> bool {
        if !TRAY_QUIT_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        self.force_close_requested = true;
        save_app_settings(&self.app_settings);
        true
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(super) fn handle_tray_quit_request(&mut self, ctx: &egui::Context) {
        if should_keep_vial_unlock_visible(self.unlock_open, self.vial_unlock_polling)
            && TRAY_QUIT_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.keep_vial_unlock_visible(ctx);
            return;
        }

        if self.consume_tray_quit_request() {
            #[cfg(target_os = "windows")]
            {
                if let Some(hwnd) = self.windows_hwnd {
                    request_windows_window_close_from_tray(hwnd);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            #[cfg(not(target_os = "windows"))]
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(super) fn handle_tray_restore_request(&mut self, ctx: &egui::Context) {
        if TRAY_RESTORE_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.restore_from_tray(ctx);
            ctx.request_repaint();
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn handle_macos_dock_reopen(&mut self, ctx: &egui::Context) {
        let app_is_hidden = macos_app_is_hidden();
        if self.macos_window_hidden_to_menu_bar {
            let recently_hidden = self.macos_hidden_to_menu_bar_at.is_some_and(|hidden_at| {
                hidden_at.elapsed() < std::time::Duration::from_millis(500)
            });
            if recently_hidden {
                self.macos_app_was_hidden = true;
                return;
            }
            if !app_is_hidden && self.macos_app_was_hidden {
                self.restore_from_tray(ctx);
                ctx.request_repaint();
            }
        }
        self.macos_app_was_hidden = app_is_hidden;
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(super) fn poll_tray_events(&mut self, ctx: &egui::Context) {
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => self.restore_from_tray(ctx),
                _ => {}
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn request_windows_window_close_from_tray(hwnd: isize) {
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            PostMessageW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW, WM_CLOSE,
        };

        let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
        ShowWindow(hwnd, SW_SHOW);
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

#[cfg(target_os = "macos")]
fn macos_shared_application() -> Option<*mut objc::runtime::Object> {
    unsafe {
        let ns_application = objc::runtime::Class::get("NSApplication")?;
        let app: *mut objc::runtime::Object = msg_send![ns_application, sharedApplication];
        (!app.is_null()).then_some(app)
    }
}

#[cfg(target_os = "macos")]
fn macos_ns_window_from_view(ns_view: *mut std::ffi::c_void) -> Option<usize> {
    unsafe {
        let ns_view = ns_view.cast::<objc::runtime::Object>();
        if ns_view.is_null() {
            return None;
        }
        let window: *mut objc::runtime::Object = msg_send![ns_view, window];
        (!window.is_null()).then_some(window as usize)
    }
}

#[cfg(target_os = "macos")]
fn restore_macos_app_window(ns_window: Option<usize>) {
    unsafe {
        let Some(app) = macos_shared_application() else {
            return;
        };
        let nil: *mut objc::runtime::Object = std::ptr::null_mut();
        let _: () = msg_send![app, unhide: nil];
        let _: () = msg_send![app, activateIgnoringOtherApps: objc::runtime::YES];

        let Some(ns_window) = ns_window else {
            return;
        };
        let window = ns_window as *mut objc::runtime::Object;
        if window.is_null() {
            return;
        }
        let _: () = msg_send![window, makeKeyAndOrderFront: nil];
        let _: () = msg_send![window, orderFrontRegardless];
    }
}

#[cfg(target_os = "macos")]
fn hide_macos_app() {
    unsafe {
        let Some(app) = macos_shared_application() else {
            return;
        };
        let nil: *mut objc::runtime::Object = std::ptr::null_mut();
        let _: () = msg_send![app, hide: nil];
    }
}

#[cfg(target_os = "macos")]
fn macos_app_is_hidden() -> bool {
    unsafe {
        let Some(app) = macos_shared_application() else {
            return false;
        };
        let hidden: objc::runtime::BOOL = msg_send![app, isHidden];
        hidden == objc::runtime::YES
    }
}

#[cfg(target_os = "linux")]
fn desktop_exec_arg(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_launch_agent_program_arguments(exe: &std::path::Path) -> Vec<String> {
    if let Some(app_bundle) = exe.ancestors().find(|path| {
        path.extension()
            .is_some_and(|ext| ext == std::ffi::OsStr::new("app"))
    }) {
        return vec![
            "/usr/bin/open".to_string(),
            "-n".to_string(),
            app_bundle.to_string_lossy().into_owned(),
        ];
    }

    vec![exe.to_string_lossy().into_owned()]
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_launch_agent_plist(program_arguments: &[String]) -> String {
    let mut arguments_xml = String::new();
    for argument in program_arguments {
        arguments_xml.push_str("        <string>");
        arguments_xml.push_str(&plist_xml_escape(argument));
        arguments_xml.push_str("</string>\n");
    }

    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
            "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\">\n",
            "<dict>\n",
            "    <key>Label</key>\n",
            "    <string>com.ergohaven.entropy</string>\n",
            "    <key>ProgramArguments</key>\n",
            "    <array>\n",
            "{arguments_xml}",
            "    </array>\n",
            "    <key>RunAtLoad</key>\n",
            "    <true/>\n",
            "</dict>\n",
            "</plist>\n"
        ),
        arguments_xml = arguments_xml
    )
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn plist_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "linux")]
fn linux_close_to_tray_prompt_copy(
    lang: crate::i18n::Language,
    input_method_backend: bool,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match (lang, input_method_backend) {
        (crate::i18n::Language::Russian, true) => (
            "Закрыть Entropy?",
            "Text Expander продолжит работать через IBus/Fcitx после закрытия Entropy",
            "Запомнить выбор",
            "Закрыть",
            "В фон",
            "Отмена",
        ),
        (crate::i18n::Language::English, true) => (
            "Close Entropy?",
            "Text Expander keeps running through IBus/Fcitx after Entropy closes",
            "Remember my choice",
            "Close",
            "Keep running",
            "Cancel",
        ),
        (crate::i18n::Language::Russian, false) => (
            "Закрыть Entropy?",
            "X11 Text Expander остановится; оставьте Entropy работать в фоне",
            "Запомнить выбор",
            "Закрыть",
            "В фон",
            "Отмена",
        ),
        (crate::i18n::Language::English, false) => (
            "Close Entropy?",
            "X11 Text Expander will stop; keep Entropy running in background",
            "Remember my choice",
            "Close",
            "Keep running",
            "Cancel",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_vial_unlock_blocks_close_and_tray_actions() {
        assert!(should_keep_vial_unlock_visible(true, false));
        assert!(should_keep_vial_unlock_visible(false, true));
        assert!(should_keep_vial_unlock_visible(true, true));
        assert!(!should_keep_vial_unlock_visible(false, false));
    }
}
