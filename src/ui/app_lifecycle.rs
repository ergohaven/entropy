use super::*;

fn should_poll_device_scan(main_window_hidden_to_tray: bool) -> bool {
    !main_window_hidden_to_tray
}

fn theme_application_required(
    last_applied_theme: Option<(bool, AppAccentColor)>,
    dark_mode: bool,
    accent_color: AppAccentColor,
) -> bool {
    last_applied_theme != Some((dark_mode, accent_color))
}

fn app_visuals(dark_mode: bool) -> egui::Visuals {
    let mut visuals = if dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = app_panel_fill(dark_mode);
    visuals.window_fill = app_window_fill(dark_mode);
    visuals.faint_bg_color = if dark_mode {
        app_window_fill(true)
    } else {
        app_panel_fill(false)
    };
    visuals.extreme_bg_color = if dark_mode {
        Color32::from_rgb(24, 24, 24)
    } else {
        Color32::from_rgb(235, 235, 235)
    };
    visuals.widgets.noninteractive.bg_fill = if dark_mode {
        app_window_fill(true)
    } else {
        app_panel_fill(false)
    };
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, app_border_color(dark_mode));
    visuals.widgets.inactive.bg_fill = app_surface_fill(dark_mode);
    visuals.widgets.inactive.weak_bg_fill = app_surface_fill(dark_mode);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, app_border_color(dark_mode));
    visuals.widgets.hovered.bg_fill = app_hover_fill(dark_mode);
    visuals.widgets.hovered.weak_bg_fill = app_hover_fill(dark_mode);
    visuals.widgets.hovered.bg_stroke = Stroke::new(
        1.0_f32,
        if dark_mode {
            app_accent()
        } else {
            Color32::from_rgb(230, 230, 233)
        },
    );
    visuals.widgets.active.bg_fill = app_accent();
    visuals.widgets.active.weak_bg_fill = app_accent();
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, app_accent());
    visuals.selection.bg_fill =
        Color32::from_rgba_unmultiplied(82, 82, 86, if dark_mode { 140 } else { 72 });
    visuals.selection.stroke = Stroke::new(
        1.0_f32,
        if dark_mode {
            Color32::from_rgb(245, 245, 245)
        } else {
            Color32::from_rgb(38, 38, 40)
        },
    );
    visuals.hyperlink_color = app_accent();
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals
}

fn hid_lifecycle_writes_available(hid_write_task_active: bool) -> bool {
    !hid_write_task_active
}

impl EntropyApp {
    fn main_window_hidden_to_tray(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.windows_window_hidden_to_tray
        }
        #[cfg(target_os = "macos")]
        {
            self.macos_window_hidden_to_menu_bar
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn update_native_background(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        main_window_hidden_to_tray: bool,
        selected_device_is_bluetooth: bool,
    ) {
        if should_poll_device_scan(main_window_hidden_to_tray) {
            if hid_lifecycle_writes_available(self.hid_write_task_active()) {
                self.handle_pending_imports(ctx, now);
            }
            if !self.hid_write_task_active() {
                self.poll_device_scan(ctx);
            }

            let is_connecting = matches!(self.connect_state, ConnectState::Loading { .. });
            let hid_write_active = self.hid_write_task_active();
            #[cfg(target_os = "macos")]
            let hid_session_active = self.hid_device.is_some();
            #[cfg(not(target_os = "macos"))]
            let hid_session_active = false;
            if !selected_device_is_bluetooth
                && (self.last_device_scan_at == 0.0 || now - self.last_device_scan_at >= 1.0)
                && !self.vial_unlock_polling
                && !is_connecting
                && !hid_write_active
                && !hid_session_active
            {
                self.scan_frame = self.scan_frame.wrapping_add(1);
                self.last_device_scan_at = now;
                self.start_device_scan();
            }
        }

        self.poll_layer_write(ctx);
        self.poll_combo_write(ctx);
        self.maybe_start_combo_write(ctx);
        self.finish_deferred_exit_after_hid_write(ctx);
        self.poll_text_expander_deferred_save(now);
        self.auto_reload_text_expander_rules_file(now);
        self.poll_single_instance_signal(ctx);
        self.poll_connect(ctx);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_pending(&self) -> bool {
        self.pending_entlayout_import_path.is_some()
            || self.pending_entsettings_import_path.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_pending_imports(&mut self, ctx: &egui::Context, now: f64) {
        if !self.import_pending() {
            self.import_progress_started_at = None;
            return;
        }
        let Some(started_at) = self.import_progress_started_at else {
            self.import_progress_started_at = Some(now);
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
            return;
        };
        if now - started_at < 0.05 {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
            return;
        }

        if let Some(path) = self.pending_entlayout_import_path.take() {
            match self.import_entlayout_from_path(&path) {
                Ok(report) => {
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.imported_entlayout",
                    )
                    .into();
                    self.import_report_title = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.layout_import_report_title",
                    )
                    .into();
                    self.import_report_body = report;
                    self.import_report_open = true;
                }
                Err(e) => {
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "status_messages.import_failed",
                        &[("error", &e.to_string())],
                    )
                }
            }
        }
        if let Some(path) = self.pending_entsettings_import_path.take() {
            match self.import_entsettings_from_path(ctx, &path) {
                Ok(report) => {
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.imported_app_settings",
                    )
                    .into();
                    self.import_report_title = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.app_settings_import_report_title",
                    )
                    .into();
                    self.import_report_body = report;
                    self.import_report_open = true;
                }
                Err(e) => {
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "status_messages.import_app_settings_failed",
                        &[("error", &e.to_string())],
                    )
                }
            }
        }
        self.import_progress_started_at = None;
    }

    fn draw_import_report_text(ui: &mut egui::Ui, body: &str) {
        let dark = ui.visuals().dark_mode;
        let text = ui.visuals().text_color();
        let muted = app_muted_text(dark);
        let warning = if dark {
            Color32::from_rgb(214, 160, 112)
        } else {
            Color32::from_rgb(154, 93, 48)
        };
        let success = if dark {
            Color32::from_rgb(150, 190, 165)
        } else {
            Color32::from_rgb(78, 122, 92)
        };

        let mut intro_lines = Vec::new();
        let mut sections: Vec<(String, Vec<String>)> = Vec::new();
        let mut current_section: Option<(String, Vec<String>)> = None;

        for raw_line in body.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if line.ends_with(':') {
                if let Some(section) = current_section.take() {
                    sections.push(section);
                }
                current_section = Some((line.trim_end_matches(':').to_owned(), Vec::new()));
            } else if let Some((_, lines)) = current_section.as_mut() {
                lines.push(line.to_owned());
            } else {
                intro_lines.push(line.to_owned());
            }
        }
        if let Some(section) = current_section.take() {
            sections.push(section);
        }

        ui.set_width(ui.available_width());

        for line in intro_lines {
            if line.ends_with("complete") {
                ui.label(RichText::new(line).size(16.0).strong().color(success));
            } else if let Some(value) = line.strip_prefix("Mode: ") {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    ui.label(RichText::new("Mode:").size(12.0).color(muted));
                    ui.label(RichText::new(value).size(12.0).strong().color(text));
                });
            } else {
                ui.label(RichText::new(line).size(12.0).color(text));
            }
        }

        ui.add_space(10.0);

        for (title, lines) in sections {
            let card_width = ui.available_width();
            egui::Frame::new()
                .fill(app_panel_fill(dark))
                .stroke(crate::ui_style::modal_outline_stroke(dark))
                .corner_radius(12.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(card_width - 28.0);
                    ui.label(RichText::new(title).size(12.5).strong().color(text));
                    ui.add_space(6.0);

                    for line in lines {
                        let line = line.strip_prefix("• ").unwrap_or(&line);
                        let is_none = line == "none";
                        let is_path = line.contains('\\')
                            || line.starts_with('/')
                            || line.starts_with("~/")
                            || line.contains(":/");
                        let is_warning = !is_none
                            && (line.contains("failed")
                                || line.contains("skipped")
                                || line.contains("not available")
                                || line.contains("safety mode")
                                || line.contains("missing")
                                || line.contains("unsupported"));
                        let color = if is_none || is_path {
                            muted
                        } else if is_warning {
                            warning
                        } else {
                            text
                        };

                        ui.horizontal_wrapped(|ui| {
                            if !is_path && !is_none {
                                ui.label(RichText::new("•").size(12.0).color(if is_warning {
                                    warning
                                } else {
                                    muted
                                }));
                            }
                            let mut rich = RichText::new(line)
                                .size(if is_path { 11.0 } else { 12.0 })
                                .color(color);
                            if is_path {
                                rich = rich.monospace();
                            }
                            ui.add(egui::Label::new(rich).wrap());
                        });
                    }
                });
            ui.add_space(8.0);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_import_progress_overlay(&mut self, ctx: &egui::Context) {
        if !self.import_pending() {
            return;
        }
        let screen_rect = ctx.content_rect();
        egui::Area::new("import_progress_backdrop".into())
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
                ui.interact(
                    rect,
                    egui::Id::new("import_progress_backdrop_blocker"),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    Color32::from_black_alpha(crate::ui_style::modal_backdrop_alpha(
                        ctx.global_style().visuals.dark_mode,
                    )),
                );
            });

        let mut open = true;
        crate::ui_style::centered_modal_window(
            ctx,
            &self.import_progress_title,
            egui::Id::new("import_progress_window"),
            &mut open,
            Vec2::new(420.0, 150.0),
        )
        .show(ctx, |ui| {
            crate::ui_style::modal_content(
                ui,
                crate::ui_style::ModalLayout::new(360.0).with_top_padding(14.0),
                |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add(egui::Spinner::new().size(20.0));
                        ui.add_space(10.0);
                        ui.label(RichText::new(&self.import_progress_body).size(12.0));
                    });
                },
            );
        });
    }
}

fn should_write_dynamic_entries(
    entries_dirty: bool,
    keycode_picker_open: bool,
    _active_hid_is_bluetooth: bool,
) -> bool {
    entries_dirty && !keycode_picker_open
}

pub(super) fn combo_write_lifecycle_plan(
    combo_dirty: bool,
    keycode_picker_open: bool,
    entries: &[ComboEntry],
    synced_entries: &[ComboEntry],
) -> Option<super::combo_write::ComboWritePlan> {
    (combo_dirty && !keycode_picker_open)
        .then(|| super::combo_write::next_combo_write(entries, synced_entries))
}

fn tap_dance_entries_to_write(
    entries: &[crate::keycode_picker::TapDanceEntry],
    synced_entries: &[crate::keycode_picker::TapDanceEntry],
) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (synced_entries.get(index) != Some(entry)).then_some(index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::combo_write::ComboWritePlan;
    use super::*;
    use crate::keyboard::{KeyboardLayout, LayoutOption};

    #[test]
    fn dirty_dynamic_entries_write_over_bluetooth() {
        assert!(should_write_dynamic_entries(true, false, true));
    }

    #[test]
    fn dynamic_entries_wait_while_key_picker_is_open() {
        assert!(!should_write_dynamic_entries(true, true, true));
        assert!(!should_write_dynamic_entries(true, true, false));
    }

    #[test]
    fn clean_dynamic_entries_do_not_write() {
        assert!(!should_write_dynamic_entries(false, false, true));
        assert!(!should_write_dynamic_entries(false, false, false));
    }

    fn combo(keys: [u16; 4], output: u16) -> ComboEntry {
        ComboEntry { keys, output }
    }

    #[test]
    fn combo_lifecycle_keeps_incomplete_draft_off_hid_path() {
        let entries = [combo([0x0004, 0, 0, 0], 0x0005)];
        let synced = [ComboEntry::default()];

        assert_eq!(
            combo_write_lifecycle_plan(true, false, &entries, &synced),
            Some(ComboWritePlan::Incomplete { index: 0 })
        );
    }

    #[test]
    fn combo_lifecycle_schedules_only_changed_valid_slot() {
        let unchanged = combo([0x0004, 0x0005, 0, 0], 0x0006);
        let changed = combo([0x0007, 0x0008, 0, 0], 0x0009);
        let previous = combo([0x0007, 0x0008, 0, 0], 0x000a);

        assert_eq!(
            combo_write_lifecycle_plan(
                true,
                false,
                &[unchanged.clone(), changed.clone(), unchanged.clone()],
                &[unchanged.clone(), previous, unchanged]
            ),
            Some(ComboWritePlan::Write {
                index: 1,
                entry: changed,
            })
        );
    }

    #[test]
    fn tap_dance_writeback_targets_only_changed_entries() {
        let unchanged = crate::keycode_picker::TapDanceEntry {
            on_tap: 0x002c,
            tapping_term: 150,
            ..Default::default()
        };
        let changed = crate::keycode_picker::TapDanceEntry {
            on_hold: 0x0202,
            ..unchanged.clone()
        };

        assert_eq!(
            tap_dance_entries_to_write(
                &[unchanged.clone(), changed],
                &[unchanged.clone(), unchanged]
            ),
            vec![1]
        );
    }

    #[test]
    fn combo_task_defers_settings_changes_refresh_and_exit_writes_until_handle_returns() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        app.combo_entries = vec![combo([0x0004, 0x0005, 0, 0], 0x0006)];
        app.combo_synced_entries = vec![ComboEntry::default()];
        app.combo_dirty = true;
        app.combo_edit_revision = 1;
        app.maybe_start_combo_write(&ctx);
        assert!(app.hid_write_task_active());

        app.key_override_entries = vec![KeyOverrideEntry::default()];
        app.key_override_pick_target = Some(KeyOverridePickField::Trigger);
        app.keycode_picker.result = Some(0x0004);
        app.pending_tap_hold_numeric_writes.insert(7, 175);
        app.layout = Some(KeyboardLayout {
            name: "Test".into(),
            rows: 0,
            cols: 0,
            keys: vec![],
            encoders: vec![],
            layers: vec![],
            encoder_layers: vec![],
            layer_names: vec![],
            custom_keycodes: vec![],
            layout_options: vec![LayoutOption {
                label: "Display preset".into(),
                choices: vec!["Disabled".into(), "Clock".into()],
            }],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        });
        app.layout_options_value = Some(1);

        app.apply_picker_results();
        assert_eq!(app.keycode_picker.result, Some(0x0004));
        assert_eq!(app.key_override_entries[0].trigger, 0);

        app.refresh_current_device_data();
        assert_eq!(
            app.status_msg,
            crate::i18n::tr_catalog(
                app.app_settings.language,
                "status_messages.refresh_device_data_pending_write"
            )
        );
        app.app_settings.minimize_to_tray_on_close = false;
        app.app_settings.close_to_tray_behavior = CloseToTrayBehavior::Close;
        let mut close_input = egui::RawInput::default();
        close_input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport exists")
            .events
            .push(egui::ViewportEvent::Close);
        let close_output = ctx.run_ui(close_input, |_ui| {
            app.handle_close_to_tray(&ctx);
        });
        assert!(close_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output exists")
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(app.exit_after_hid_write);
        eframe::App::on_exit(&mut app, None);
        assert_eq!(app.pending_tap_hold_numeric_writes.get(&7), Some(&175));

        let active_requests = recorder.requests();
        assert_eq!(
            active_requests
                .iter()
                .filter(|request| request[..3] == [0xfe, 0x0d, 0x06])
                .count(),
            0
        );
        assert_eq!(
            active_requests
                .iter()
                .filter(|request| request[..2] == [0xfe, 0x0b])
                .count(),
            0
        );
        assert_eq!(
            active_requests
                .iter()
                .filter(|request| request[..6] == [0x03, 0x02, 0, 0, 0, 0])
                .count(),
            0
        );

        for _ in 0..100 {
            app.poll_combo_write(&ctx);
            if !app.hid_write_task_active() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!app.hid_write_task_active());
        assert!(app.hid_device.is_some());

        app.apply_picker_results();
        assert_eq!(app.key_override_entries[0].trigger, 0x0004);
        assert!(app.keycode_picker.result.is_none());
        app.apply_picker_results();
        assert_eq!(app.key_override_entries[0].trigger, 0x0004);

        let final_close_output = ctx.run_ui(egui::RawInput::default(), |_ui| {
            app.finish_deferred_exit_after_hid_write(&ctx);
        });
        assert!(!app.exit_after_hid_write);
        assert!(app.pending_tap_hold_numeric_writes.is_empty());
        assert_eq!(app.layout_options_value, Some(0));
        assert!(final_close_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output exists")
            .commands
            .contains(&egui::ViewportCommand::Close));

        let completed_requests = recorder.requests();
        assert_eq!(
            completed_requests
                .iter()
                .filter(|request| request[..3] == [0xfe, 0x0d, 0x04])
                .count(),
            1
        );
        assert_eq!(
            completed_requests
                .iter()
                .filter(|request| request[..3] == [0xfe, 0x0d, 0x06])
                .count(),
            1
        );
        assert_eq!(
            completed_requests
                .iter()
                .filter(|request| request[..2] == [0xfe, 0x0b])
                .count(),
            1
        );
        assert_eq!(
            completed_requests
                .iter()
                .filter(|request| request[..6] == [0x03, 0x02, 0, 0, 0, 0])
                .count(),
            1
        );
    }

    #[test]
    fn background_lifecycle_chains_combo_writes_before_deferred_exit() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid_device, _recorder) = crate::hid::HidDevice::test_device();
        let first = combo([0x0004, 0x0005, 0, 0], 0x0006);
        let second = combo([0x0007, 0x0008, 0, 0], 0x0009);
        app.hid_device = Some(hid_device);
        app.combo_entries = vec![first.clone(), second];
        app.combo_synced_entries = vec![ComboEntry::default(), ComboEntry::default()];
        app.combo_dirty = true;
        app.combo_edit_revision = 1;
        app.exit_after_hid_write = true;
        app.maybe_start_combo_write(&ctx);

        for _ in 0..100 {
            app.update_native_background(&ctx, 0.0, true, true);
            if app.combo_synced_entries.first() == Some(&first) && app.combo_write_task.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert_eq!(app.combo_synced_entries.first(), Some(&first));
        assert!(app.combo_write_task.is_some());
        assert!(app.exit_after_hid_write);
    }

    #[test]
    fn hidden_to_tray_skips_device_scan_polling() {
        assert!(!should_poll_device_scan(true));
        // Minimized and occluded windows are not tray-hidden, so App::logic
        // keeps their background device polling alive while App::ui is skipped.
        assert!(should_poll_device_scan(false));
    }

    #[test]
    fn theme_is_applied_only_initially_and_after_changes() {
        assert!(theme_application_required(
            None,
            false,
            AppAccentColor::Rose
        ));
        assert!(!theme_application_required(
            Some((false, AppAccentColor::Rose)),
            false,
            AppAccentColor::Rose
        ));
        assert!(theme_application_required(
            Some((false, AppAccentColor::Rose)),
            true,
            AppAccentColor::Rose
        ));
        assert!(theme_application_required(
            Some((false, AppAccentColor::Rose)),
            false,
            AppAccentColor::Blue
        ));
    }
}

impl eframe::App for EntropyApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        app_panel_fill(visuals.dark_mode).to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.hid_write_task_active() {
            self.flush_pending_tap_hold_numeric_writes();
            self.fallback_entropy_display_presets_before_exit();
        }
        #[cfg(target_arch = "wasm32")]
        self.flush_pending_tap_hold_numeric_writes();
        self.flush_pending_text_expander_settings();
        self.app_settings.dark_mode = self.dark_mode;
        save_app_settings(&self.app_settings);
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(target_os = "windows")]
            self.cache_windows_hwnd(frame);
            #[cfg(target_os = "macos")]
            self.cache_macos_ns_window(frame);
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            self.handle_tray_quit_request(ctx);
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            self.handle_tray_restore_request(ctx);
            self.handle_close_to_tray(ctx);
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            self.poll_tray_events(ctx);
            #[cfg(target_os = "macos")]
            self.handle_macos_dock_reopen(ctx);

            crate::app::poll_update_check(&mut self.update_check);
            let main_window_hidden_to_tray = self.main_window_hidden_to_tray();
            let selected_device_is_bluetooth = self
                .selected_device
                .and_then(|idx| self.device_manager.devices().get(idx))
                .map(|device| device.is_bluetooth_transport())
                .unwrap_or(false);
            let high_frequency_bluetooth_repaint =
                should_use_high_frequency_bluetooth_repaint(selected_device_is_bluetooth);
            let connect_pending = matches!(self.connect_state, ConnectState::Loading { .. });
            let update_check_pending =
                matches!(self.update_check, UpdateCheckState::Checking { .. });
            ctx.request_repaint_after(native_repaint_interval(
                main_window_hidden_to_tray,
                high_frequency_bluetooth_repaint,
                connect_pending,
                update_check_pending,
            ));
            self.update_native_background(
                ctx,
                ctx.input(|i| i.time),
                main_window_hidden_to_tray,
                selected_device_is_bluetooth,
            );
        }

        #[cfg(target_arch = "wasm32")]
        {
            crate::app::poll_update_check(&mut self.update_check);
            let now = ctx.input(|i| i.time);
            self.poll_text_expander_deferred_save(now);
            self.auto_reload_text_expander_rules_file(now);
        }
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx_owned = root_ui.ctx().clone();
        let ctx = &ctx_owned;
        self.apply_ui_scale(ctx);
        self.handle_ui_scale_shortcuts(ctx);
        self.remember_main_window_size(ctx);

        self.tour_target_rects.clear();

        let keyboard_input_wanted_at_frame_start = ctx.egui_wants_keyboard_input();
        #[cfg(not(target_arch = "wasm32"))]
        let import_pending_at_frame_start = self.import_pending();
        #[cfg(target_arch = "wasm32")]
        let import_pending_at_frame_start = false;
        let modal_or_popup_open_at_frame_start = self.keycode_picker.open
            || self.unlock_open
            || self.vial_unlock_polling
            || self.close_to_tray_prompt_open
            || self.import_report_open
            || self.typing_trainer_history_open
            || import_pending_at_frame_start
            || self.top_dropdown_open(ctx)
            || egui::Popup::is_any_open(ctx);

        // Auto-scan for device connect/disconnect changes.
        self.secondary_click_handled = false;

        if let Some((layer, ki, kc)) = self.pending_handed_swap {
            if !ctx.input(|i| i.modifiers.ctrl) {
                #[cfg(not(target_arch = "wasm32"))]
                if !self.hid_write_task_active() {
                    self.assign_keycode(layer, ki, kc);
                    self.pending_handed_swap = None;
                }
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(layout) = &mut self.layout {
                        layout.set_keycode(layer, ki, kc);
                    }
                    self.pending_handed_swap = None;
                }
            }
        }
        let accent_color = self.app_settings.accent_color;
        if theme_application_required(self.last_applied_theme, self.dark_mode, accent_color) {
            ctx.set_visuals(app_visuals(self.dark_mode));
            self.last_applied_theme = Some((self.dark_mode, accent_color));
        }

        self.apply_picker_results();

        // Deselect key when picker is closed without choosing
        if !self.keycode_picker.open
            && (self.selected_key.is_some() || self.selected_encoder.is_some())
            && self.keycode_picker.result.is_none()
        {
            self.selected_key = None;
            self.selected_encoder = None;
        }

        if !self.keycode_picker.open || self.keycode_picker.selected_tab != KeycodeTab::Macro {
            self.macro_auto_unlock_cancelled = false;
        }

        if self.firmware == FirmwareProtocol::Vial
            && self.keycode_picker.open
            && self.keycode_picker.selected_tab == KeycodeTab::Macro
            && !self.unlock_open
            && !self.vial_unlock_polling
            && !self.macro_auto_unlock_cancelled
            && self.is_vial_locked()
        {
            self.unlock_open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.keyboard_locked_edit_macros",
            )
            .into();
        }

        // Arrow keys Left/Right switch layers (when picker is closed and no text field is focused)
        if !self.tour_state.active && !self.keycode_picker.open && !ctx.egui_wants_keyboard_input()
        {
            let layer_count = self.layer_count;
            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowLeft) && self.selected_layer > 0 {
                    self.selected_layer -= 1;
                    self.jump_back_stack.clear();
                }
                if i.key_pressed(egui::Key::ArrowRight) && self.selected_layer + 1 < layer_count {
                    self.selected_layer += 1;
                    self.jump_back_stack.clear();
                }
            });
        }

        // Check if loading
        #[cfg(not(target_arch = "wasm32"))]
        let is_loading = matches!(self.connect_state, ConnectState::Loading { .. })
            || self.hid_write_task_active();
        #[cfg(target_arch = "wasm32")]
        let is_loading = false;

        // Main canvas
        egui::CentralPanel::default().show_inside(root_ui, |ui| {
            if self.selected_device.is_none() {
                let rect = ui.max_rect();
                #[cfg(target_os = "linux")]
                if !super::app_settings_ui::linux_vial_udev_rules_installed() {
                    let empty_rect = egui::Rect::from_center_size(
                        rect.center(),
                        egui::vec2(rect.width().min(520.0), 210.0),
                    );
                    crate::ui_style::allocate_ui_at_rect(ui, empty_rect, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("✦").size(28.0).color(app_accent()));
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "connection.linux_vial_udev_required_title",
                                ))
                                .size(20.0)
                                .strong()
                                .color(if self.dark_mode {
                                    Color32::from_rgb(235, 235, 235)
                                } else {
                                    Color32::from_rgb(42, 42, 44)
                                }),
                            );
                            ui.add_space(7.0);
                            ui.add_sized(
                                egui::vec2(empty_rect.width().min(440.0), 42.0),
                                egui::Label::new(
                                    RichText::new(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "connection.linux_vial_udev_required_body",
                                    ))
                                    .size(13.0)
                                    .color(app_muted_text(self.dark_mode)),
                                )
                                .wrap()
                                .halign(egui::Align::Center),
                            );
                            ui.add_space(14.0);
                            if crate::ui_style::modern_button(
                                ui,
                                crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "ui.install_vial_udev_rules",
                                ),
                                egui::vec2(168.0, 34.0),
                                true,
                            )
                            .clicked()
                            {
                                self.run_linux_vial_udev_rules_install();
                                self.start_device_scan();
                            }
                        });
                    });
                    return;
                }

                if !self.device_manager.devices().is_empty() {
                    self.draw_device_selection_empty_state(ui, rect);
                    return;
                }

                let empty_rect = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(rect.width().min(520.0), 150.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, empty_rect, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("✦").size(28.0).color(app_accent()));
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "connection.waiting_for_keyboard",
                            ))
                            .size(20.0)
                            .strong()
                            .color(if self.dark_mode {
                                Color32::from_rgb(235, 235, 235)
                            } else {
                                Color32::from_rgb(42, 42, 44)
                            }),
                        );
                        ui.add_space(7.0);
                        ui.label(
                            RichText::new(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "connection.connect_vial_device",
                            ))
                            .size(13.0)
                            .color(app_muted_text(self.dark_mode)),
                        );
                    });
                });
                return;
            }

            if is_loading {
                let rect = ui.max_rect();
                let text = if self.status_msg.is_empty() {
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "connection.loading_keyboard",
                    )
                    .to_owned()
                } else {
                    crate::i18n::tr_text(self.app_settings.language, &self.status_msg)
                };
                let font_id = FontId::proportional(16.0);
                let text_width = ui.fonts_mut(|f| {
                    f.layout_no_wrap(text.to_owned(), font_id.clone(), Color32::GRAY)
                        .size()
                        .x
                });
                let spinner_size = 18.0;
                let gap = 8.0;
                let row_width = spinner_size + gap + text_width;
                let row_left = rect.center().x - row_width * 0.5;
                let spinner_rect = egui::Rect::from_center_size(
                    egui::pos2(row_left + spinner_size * 0.5, rect.center().y),
                    egui::vec2(spinner_size, spinner_size),
                );
                egui::Spinner::new()
                    .size(spinner_size)
                    .color(Color32::GRAY)
                    .paint_at(ui, spinner_rect);
                ui.painter().text(
                    egui::pos2(row_left + spinner_size + gap, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &text,
                    font_id,
                    Color32::GRAY,
                );
                return;
            }

            if let Some(layout) = self.layout.clone() {
                self.draw_layout(ui, &layout, ctx);
            } else if !self.status_msg.is_empty() {
                let rect = ui.max_rect();
                let status_text =
                    crate::i18n::tr_text(self.app_settings.language, &self.status_msg);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    status_text,
                    FontId::proportional(16.0),
                    Color32::GRAY,
                );
            } else {
                self.draw_placeholder(ui);
            }
        });

        self.draw_sticky_layout_window(ctx);

        let chrome_opacity = self.typing_trainer_chrome_opacity(ctx);
        if chrome_opacity > 0.0 && chrome_opacity < 1.0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        if self.app_settings.show_made_by_signature && chrome_opacity > 0.01 {
            egui::Area::new(egui::Id::new("made_by_signature"))
                .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_opacity(chrome_opacity);
                    if chrome_opacity <= 0.96 {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        let muted = app_muted_text(self.dark_mode);
                        ui.spacing_mut().item_spacing.x = 3.0;
                        ui.label(
                            RichText::new("tools of the future by")
                                .size(12.0)
                                .color(muted),
                        );
                        let (site_label, site_url) =
                            if matches!(self.app_settings.language, crate::i18n::Language::Russian)
                            {
                                ("eh.works", "https://eh.works")
                            } else {
                                ("eh.industries", "https://eh.industries")
                            };
                        ui.add(egui::Hyperlink::from_label_and_url(
                            RichText::new(site_label).size(12.0),
                            site_url,
                        ));
                    });
                });
        }

        if chrome_opacity > 0.01 {
            egui::Area::new(egui::Id::new("theme_selector"))
                .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_opacity(chrome_opacity);
                    if chrome_opacity <= 0.96 {
                        ui.disable();
                    }
                    let previous_dark_mode = self.dark_mode;
                    draw_theme_selector_labels(
                        ui,
                        self.app_settings.language,
                        &mut self.dark_mode,
                        false,
                    );
                    if self.dark_mode != previous_dark_mode {
                        self.app_settings.dark_mode = self.dark_mode;
                        save_app_settings(&self.app_settings);
                    }
                });
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.draw_import_progress_overlay(ctx);

        if self.import_report_open {
            let screen_rect = ctx.content_rect();
            egui::Area::new("import_report_backdrop".into())
                .order(egui::Order::Foreground)
                .fixed_pos(screen_rect.min)
                .show(ctx, |ui| {
                    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
                    ui.interact(
                        rect,
                        egui::Id::new("import_report_backdrop_blocker"),
                        egui::Sense::click_and_drag(),
                    );
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        Color32::from_black_alpha(crate::ui_style::modal_backdrop_alpha(
                            ctx.global_style().visuals.dark_mode,
                        )),
                    );
                });

            let mut open = self.import_report_open;
            let mut close_clicked = false;
            crate::ui_style::centered_modal_window(
                ctx,
                &self.import_report_title,
                egui::Id::new("import_report_window"),
                &mut open,
                Vec2::new(680.0, 620.0),
            )
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(660.0, 560.0));
                let rect = ui.max_rect();
                let content_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 34.0, rect.top() + 18.0),
                    egui::pos2(rect.right() - 34.0, rect.bottom() - 74.0),
                );
                let button_size = crate::ui_style::modal_action_button_size();
                let button_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.center().x, rect.bottom() - 34.0),
                    button_size,
                );

                crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(content_rect.height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(content_rect.width() - 18.0);
                            Self::draw_import_report_text(ui, &self.import_report_body);
                        });
                });

                crate::ui_style::allocate_ui_at_rect(ui, button_rect, |ui| {
                    if crate::ui_style::modern_button(ui, "OK", button_size, true).clicked() {
                        close_clicked = true;
                    }
                });
            });
            self.import_report_open = open && !close_clicked;
        }

        self.draw_close_to_tray_prompt(ctx);

        // Keycode picker modal
        self.draw_vial_unlock_overlay(ctx);

        if self.keycode_picker.open {
            let screen_rect = ctx.content_rect();
            egui::Area::new("window_backdrop".into())
                .order(egui::Order::Middle)
                .fixed_pos(screen_rect.min)
                .show(ctx, |ui| {
                    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
                    let response =
                        ui.interact(rect, ui.id().with("backdrop_click"), egui::Sense::click());
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        Color32::from_black_alpha(crate::ui_style::modal_backdrop_alpha(
                            ctx.global_style().visuals.dark_mode,
                        )),
                    );
                    if response.clicked() {
                        self.keycode_picker.close_from_backdrop();
                        if let Some(id) = ctx.memory(|m| m.focused()) {
                            ctx.memory_mut(|m| m.surrender_focus(id));
                        }
                    }
                });
        }

        if !self.unlock_open && !self.vial_unlock_polling {
            self.keycode_picker.language = self.app_settings.language;
            self.keycode_picker.key_legend_layout = self.app_settings.key_legend_layout;
            self.keycode_picker.show_shifted_number_symbols =
                self.app_settings.show_shifted_number_symbols;
            self.keycode_picker.show(ctx);
            self.apply_picker_results();
        }

        if self.combo_pick_target.is_some()
            && !self.keycode_picker.open
            && self.keycode_picker.result.is_none()
        {
            self.combo_pick_target = None;
        }
        if self.key_override_pick_target.is_some()
            && !self.keycode_picker.open
            && self.keycode_picker.result.is_none()
        {
            self.key_override_pick_target = None;
        }
        if self.alt_repeat_pick_target.is_some()
            && !self.keycode_picker.open
            && self.keycode_picker.result.is_none()
        {
            self.alt_repeat_pick_target = None;
        }

        // Write macros to device if changed
        self.maybe_start_onboarding_tour(ctx);
        self.draw_onboarding_tour(ctx);

        let active_hid_is_bluetooth = self
            .hid_device
            .as_ref()
            .map(|hid| hid.is_bluetooth_transport())
            .unwrap_or(false);
        #[cfg(not(target_arch = "wasm32"))]
        let hid_lifecycle_writes_available =
            hid_lifecycle_writes_available(self.hid_write_task_active());
        #[cfg(target_arch = "wasm32")]
        let hid_lifecycle_writes_available = true;

        if hid_lifecycle_writes_available
            && self.keycode_picker.macros_dirty
            && !self.keycode_picker.open
        {
            if self.unlock_open || self.vial_unlock_polling {
                // Defer macro write until unlock flow fully finishes.
            } else if self.is_vial_locked() {
                self.unlock_open = true;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "connection.keyboard_locked_edit_macros",
                )
                .into();
            } else {
                if let Some(hid) = &self.hid_device {
                    match hid.get_macro_buffer_size() {
                        Ok(size) => {
                            let buf = crate::hid::HidDevice::encode_macros(
                                &self.keycode_picker.macro_texts,
                                size,
                            );
                            match hid.set_macro_buffer(&buf) {
                                Ok(()) => {
                                    self.keycode_picker.macros_dirty = false;
                                    self.status_msg = crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "status_messages.macros_saved",
                                    )
                                    .into()
                                }
                                Err(e) => {
                                    self.keycode_picker.macros_dirty = false;
                                    self.status_msg = crate::i18n::tr_catalog_format(
                                        self.app_settings.language,
                                        "status_messages.macro_write_error",
                                        &[("error", &e.to_string())],
                                    )
                                }
                            }
                        }
                        Err(e) => {
                            self.keycode_picker.macros_dirty = false;
                            self.status_msg = crate::i18n::tr_catalog_format(
                                self.app_settings.language,
                                "status_messages.macro_write_error",
                                &[("error", &e.to_string())],
                            )
                        }
                    }
                } else {
                    self.keycode_picker.macros_dirty = false;
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "status_messages.macro_write_error",
                        &[("error", "device handle is not available")],
                    )
                }
            }
        }

        if hid_lifecycle_writes_available
            && self.combo_term_dirty
            && !self.keycode_picker.open
            && !active_hid_is_bluetooth
        {
            let mut term_save_ok = true;
            if let (Some(hid), Some(value)) = (&self.hid_device, self.combo_term) {
                if let Err(e) = hid.set_qmk_setting_u16(2, value) {
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "status_messages.combo_timeout_write_error",
                        &[("error", &e.to_string())],
                    );
                    term_save_ok = false;
                }
            }
            if term_save_ok {
                self.combo_term_dirty = false;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.combo_timeout_saved",
                )
                .into();
            }
        }

        if self.combo_names_dirty {
            save_combo_names(&self.combo_names, &self.current_device_name);
            self.combo_names_dirty = false;
        }

        if self.keycode_picker.macro_metadata_dirty && !self.current_device_name.is_empty() {
            save_macro_metadata(
                &self.keycode_picker.macro_names,
                &self.keycode_picker.macro_descriptions,
                &self.current_device_name,
            );
            self.keycode_picker.macro_metadata_dirty = false;
        }

        if self.combo_colors_dirty {
            save_combo_colors(&self.combo_colors, &self.current_device_name);
            self.combo_colors_dirty = false;
        }

        if hid_lifecycle_writes_available {
            self.flush_due_tap_hold_numeric_writes();
        }

        // Write tap dance to device if changed
        if hid_lifecycle_writes_available
            && should_write_dynamic_entries(
                self.keycode_picker.tap_dance_dirty,
                self.keycode_picker.open,
                active_hid_is_bluetooth,
            )
        {
            let entries_to_write = tap_dance_entries_to_write(
                &self.keycode_picker.tap_dance_entries,
                &self.keycode_picker.tap_dance_synced_entries,
            );
            let mut td_save_ok = true;
            if entries_to_write.is_empty() {
                // Names are local metadata; no device round-trip is needed.
            } else if let Some(hid) = &self.hid_device {
                for i in entries_to_write {
                    let Some(td) = self.keycode_picker.tap_dance_entries.get(i).cloned() else {
                        continue;
                    };
                    match hid.set_tap_dance(
                        i as u8,
                        td.on_tap,
                        td.on_hold,
                        td.on_double_tap,
                        td.on_tap_hold,
                        td.tapping_term,
                    ) {
                        Ok(()) => {
                            if self.keycode_picker.tap_dance_synced_entries.len() <= i {
                                self.keycode_picker
                                    .tap_dance_synced_entries
                                    .resize(i + 1, Default::default());
                            }
                            self.keycode_picker.tap_dance_synced_entries[i] = td;
                        }
                        Err(e) => {
                            self.status_msg = crate::i18n::tr_catalog_format(
                                self.app_settings.language,
                                "status_messages.tap_dance_write_error",
                                &[("error", &e.to_string())],
                            );
                            td_save_ok = false;
                            break;
                        }
                    }
                }
            } else {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.tap_dance_write_error",
                    &[(
                        "error",
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "status_messages.device_unavailable",
                        ),
                    )],
                );
                td_save_ok = false;
            }
            save_tap_dance_names(
                &self.keycode_picker.tap_dance_names,
                &self.current_device_name,
            );
            // Consume this attempt. Failed device entries remain different from the
            // synced snapshot and retry after the next edit or picker close.
            self.keycode_picker.tap_dance_dirty = false;
            if td_save_ok {
                if self.status_msg.is_empty() || self.status_msg.starts_with("✓") {
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.tap_dance_saved",
                    )
                    .into();
                }
            }
        }

        // Start background combo I/O after synchronous dynamic-entry writes have finished
        // using the shared HID handle for this frame.
        #[cfg(not(target_arch = "wasm32"))]
        self.maybe_start_combo_write(ctx);

        let mut settings_page_navigation_handled = false;
        if self.can_return_from_settings_page(
            ctx,
            modal_or_popup_open_at_frame_start,
            keyboard_input_wanted_at_frame_start,
        ) {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let rclick = ctx.input(|i| i.pointer.secondary_clicked());
            if esc_pressed || rclick {
                self.close_top_dropdowns(ctx);
                self.main_menu_tab = MainMenuTab::Keyboard;
                settings_page_navigation_handled = true;
            }
        }

        // Right-click anywhere = pop back one step (only if NOT hovering a layer key and not handled by key)
        if !settings_page_navigation_handled
            && !self.jump_back_stack.is_empty()
            && !self.keycode_picker.open
            && !self.secondary_click_handled
        {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let rclick = self.hover_layer.is_none() && ctx.input(|i| i.pointer.secondary_clicked());
            if rclick || esc_pressed {
                if let Some(back_layer) = self.jump_back_stack.pop() {
                    self.selected_layer = back_layer;
                }
            }
        }

        self.pause_typing_trainer_if_inactive(std::time::Instant::now());
    }
}
