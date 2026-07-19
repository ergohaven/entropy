use super::*;

fn module_setting_catalog_keys(title: &str) -> Option<(&'static str, &'static str)> {
    match title.to_ascii_lowercase().as_str() {
        "module" => Some(("modules_settings.module", "modules_settings.module_tooltip")),
        "mode" => Some(("modules_settings.mode", "modules_settings.mode_tooltip")),
        "ball axis" => Some((
            "modules_settings.ball_axis",
            "modules_settings.ball_axis_tooltip",
        )),
        "touch axis" => Some((
            "modules_settings.touch_axis",
            "modules_settings.touch_axis_tooltip",
        )),
        "ball dpi" => Some((
            "modules_settings.ball_dpi",
            "modules_settings.ball_dpi_tooltip",
        )),
        "touch dpi" => Some((
            "modules_settings.touch_dpi",
            "modules_settings.touch_dpi_tooltip",
        )),
        "encoder interval" => Some((
            "modules_settings.encoder_interval",
            "modules_settings.encoder_interval_tooltip",
        )),
        "scroll sens" => Some((
            "modules_settings.scroll_sens",
            "modules_settings.scroll_sens_tooltip",
        )),
        "sniper sens" => Some((
            "modules_settings.sniper_sens",
            "modules_settings.sniper_sens_tooltip",
        )),
        "text sens" => Some((
            "modules_settings.text_sens",
            "modules_settings.text_sens_tooltip",
        )),
        "touch gestures" => Some((
            "modules_settings.touch_gestures",
            "modules_settings.touch_gestures_tooltip",
        )),
        "invert scroll" => Some((
            "modules_settings.invert_scroll",
            "modules_settings.invert_scroll_tooltip",
        )),
        "invert scroll vertical" => Some((
            "modules_settings.invert_scroll_vertical",
            "modules_settings.invert_scroll_vertical_tooltip",
        )),
        "invert scroll horizontal" => Some((
            "modules_settings.invert_scroll_horizontal",
            "modules_settings.invert_scroll_horizontal_tooltip",
        )),
        "invert text" => Some((
            "modules_settings.invert_text",
            "modules_settings.invert_text_tooltip",
        )),
        "invert text vertical" => Some((
            "modules_settings.invert_text_vertical",
            "modules_settings.invert_text_vertical_tooltip",
        )),
        "invert text horizontal" => Some((
            "modules_settings.invert_text_horizontal",
            "modules_settings.invert_text_horizontal_tooltip",
        )),
        "acceleration" => Some((
            "modules_settings.acceleration",
            "modules_settings.acceleration_tooltip",
        )),
        "sticky mode" => Some((
            "modules_settings.sticky_mode",
            "modules_settings.sticky_mode_tooltip",
        )),
        "led blinks" => Some((
            "modules_settings.led_blinks",
            "modules_settings.led_blinks_tooltip",
        )),
        "auto layer in normal" => Some((
            "modules_settings.auto_layer_in_normal",
            "modules_settings.auto_layer_normal_tooltip",
        )),
        "auto layer" => Some((
            "modules_settings.auto_layer",
            "modules_settings.auto_layer_tooltip",
        )),
        "auto layer in sniper" => Some((
            "modules_settings.auto_layer_in_sniper",
            "modules_settings.auto_layer_sniper_tooltip",
        )),
        "auto layer in scroll" => Some((
            "modules_settings.auto_layer_in_scroll",
            "modules_settings.auto_layer_scroll_tooltip",
        )),
        "auto layer in text" => Some((
            "modules_settings.auto_layer_in_text",
            "modules_settings.auto_layer_text_tooltip",
        )),
        "auto layer timeout" => Some((
            "modules_settings.auto_layer_timeout",
            "modules_settings.auto_layer_timeout_tooltip",
        )),
        _ => None,
    }
}

fn module_setting_variant_label(language: crate::i18n::Language, variant: &str) -> String {
    let key = match variant.to_ascii_lowercase().as_str() {
        "none" => Some("modules_settings.none"),
        "normal" => Some("modules_settings.normal"),
        "sniper" => Some("modules_settings.sniper"),
        "scroll" => Some("modules_settings.scroll"),
        "text" => Some("modules_settings.text"),
        "trackball" => Some("modules_settings.trackball"),
        "touchpad" => Some("modules_settings.touchpad"),
        "ball" => Some("modules_settings.ball"),
        "touch" => Some("modules_settings.touch"),
        "encoder" => Some("modules_settings.encoder"),
        _ => None,
    };
    key.map(|key| crate::i18n::tr_catalog(language, key).to_owned())
        .unwrap_or_else(|| crate::i18n::tr_text(language, variant))
}

#[derive(Clone, Copy)]
enum ModuleSettingsRow {
    Refresh,
    SideSelector,
    Section(usize),
    Field { group_idx: usize, field_idx: usize },
}

const MODULE_SETTING_WRITEBACK_DELAYS: [std::time::Duration; MODULE_SETTING_READBACK_ATTEMPTS] = [
    std::time::Duration::from_millis(20),
    std::time::Duration::from_millis(80),
    std::time::Duration::from_millis(200),
];

fn module_setting_diagnostic_labels(groups: &[ModuleSettingsGroup], qsid: u16) -> String {
    let mut labels = groups
        .iter()
        .flat_map(|group| {
            group
                .fields
                .iter()
                .filter(move |field| field.qsid == qsid)
                .map(move |field| format!("{} / {}", group.title, field.title))
        })
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    labels.join(" | ")
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_module_settings_refresh_job<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> std::sync::mpsc::Receiver<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(job());
    });
    receiver
}

#[cfg(not(target_arch = "wasm32"))]
fn module_settings_refresh_start_allowed(
    hid_write_task_active: bool,
    hid_available: bool,
    identity_available: bool,
) -> bool {
    !hid_write_task_active && hid_available && identity_available
}

#[cfg(not(target_arch = "wasm32"))]
fn module_settings_refresh_identity_matches(
    expected: &ModuleSettingsDeviceIdentity,
    current: Option<&ModuleSettingsDeviceIdentity>,
) -> bool {
    current.is_some_and(|current| expected.matches(current))
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    fn current_module_settings_device_identity(&self) -> Option<ModuleSettingsDeviceIdentity> {
        self.device_about_info
            .as_ref()
            .map(ModuleSettingsDeviceIdentity::from_about)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn refresh_module_settings_values(&mut self) {
        let lang = self.app_settings.language;
        if self.hid_write_task_active() {
            return;
        }
        let Some(identity) = self.current_module_settings_device_identity() else {
            self.status_msg =
                crate::i18n::tr_catalog(lang, "modules_settings.refresh_unavailable").to_owned();
            return;
        };
        let Some(hid_device) = self.hid_device.take() else {
            self.status_msg =
                crate::i18n::tr_catalog(lang, "modules_settings.refresh_unavailable").to_owned();
            return;
        };

        let device_name = self
            .device_about_info
            .as_ref()
            .map(|info| info.product.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.current_device_name.clone());
        let module_settings = self.module_settings.clone();
        let worker_identity = identity.clone();
        let receiver = spawn_module_settings_refresh_job(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = crate::hid::macos_hid_operation_lock();

            let mut disconnected = false;
            let report = module_settings.refresh_values(|qsid, width| {
                if width > 1 {
                    hid_device.get_qmk_setting_u16(qsid)
                } else {
                    hid_device.get_qmk_setting_u8(qsid).map(u16::from)
                }
                .map_err(|error| {
                    disconnected |= crate::hid::is_disconnect_error(&error);
                    error.to_string()
                })
            });
            ModuleSettingsRefreshTaskResult {
                hid_device: (!disconnected).then_some(hid_device),
                identity: worker_identity,
                device_name,
                report,
            }
        });
        self.module_settings_refresh_task = Some(ModuleSettingsRefreshTask { receiver, identity });
        self.status_msg = crate::i18n::tr_catalog(lang, "modules_settings.refreshing").to_owned();
    }

    #[cfg(target_arch = "wasm32")]
    fn refresh_module_settings_values(&mut self) {
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            "modules_settings.refresh_unavailable",
        )
        .to_owned();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_module_settings_refresh(&mut self, ctx: &egui::Context) {
        let result = match self.module_settings_refresh_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };

        match result {
            Ok(result) => {
                self.module_settings_refresh_task = None;
                self.finish_module_settings_refresh(result);
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .module_settings_refresh_task
                    .take()
                    .expect("module settings refresh task checked above");
                let current = self.current_module_settings_device_identity();
                if module_settings_refresh_identity_matches(&task.identity, current.as_ref()) {
                    self.preserve_deferred_hid_settings_for_disconnect();
                    self.clear_connected_keyboard_state(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "modules_settings.refresh_task_failed",
                    ));
                }
                log::warn!("module settings refresh worker stopped before returning a result");
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_module_settings_refresh(&mut self, result: ModuleSettingsRefreshTaskResult) {
        let current = self.current_module_settings_device_identity();
        if !module_settings_refresh_identity_matches(&result.identity, current.as_ref()) {
            log::info!(
                "discarded stale module settings refresh result: device={:?} keyboard_id={:016X}",
                result.identity.path,
                result.identity.keyboard_id,
            );
            return;
        }

        let Some(hid_device) = result.hid_device else {
            self.preserve_deferred_hid_settings_for_disconnect();
            self.clear_connected_keyboard_state(crate::i18n::tr_catalog(
                self.app_settings.language,
                "modules_settings.refresh_disconnected",
            ));
            return;
        };

        self.hid_device = Some(hid_device);
        self.module_settings.apply_refresh_report(&result.report);
        self.apply_module_settings_refresh_report(&result.device_name, &result.report);
    }

    fn apply_module_settings_refresh_report(
        &mut self,
        device: &str,
        report: &ModuleSettingsRefreshReport,
    ) {
        let lang = self.app_settings.language;

        log::info!(
            "module settings snapshot: source=manual-refresh device={device:?} persistent_qmk=true runtime_mode_observable=false entries={} changed={} failed={} skipped={}",
            report.entries.len(),
            report.changed_count(),
            report.failed_count(),
            report.skipped_count(),
        );
        for entry in &report.entries {
            let labels = module_setting_diagnostic_labels(&self.module_settings.groups, entry.qsid);
            match &entry.outcome {
                ModuleSettingRefreshOutcome::Success(value) => log::info!(
                    "module setting snapshot entry: labels={labels:?} qsid={} width={} before={} after={} changed={}",
                    entry.qsid,
                    entry.width,
                    entry.previous,
                    value,
                    entry.previous != *value,
                ),
                ModuleSettingRefreshOutcome::Failed(error) => log::warn!(
                    "module setting snapshot entry failed: labels={labels:?} qsid={} width={} before={} after=unavailable error={error}",
                    entry.qsid,
                    entry.width,
                    entry.previous,
                ),
                ModuleSettingRefreshOutcome::Skipped(error) => log::info!(
                    "module setting snapshot entry skipped: labels={labels:?} qsid={} width={} before={} after=unavailable previous_error={error}",
                    entry.qsid,
                    entry.width,
                    entry.previous,
                ),
            }
        }

        let successful = report.successful_count().to_string();
        let changed = report.changed_count().to_string();
        let failed = report.failed_count().to_string();
        let skipped = report.skipped_count().to_string();
        let key = if report.failed_count() == 0 {
            "modules_settings.refresh_status"
        } else {
            "modules_settings.refresh_partial_status"
        };
        self.status_msg = crate::i18n::tr_catalog_format(
            lang,
            key,
            &[
                ("count", successful.as_str()),
                ("changed", changed.as_str()),
                ("failed", failed.as_str()),
                ("skipped", skipped.as_str()),
            ],
        );
    }

    fn module_setting_display_title<'a>(
        &self,
        group_kind: ModuleSettingsGroupKind,
        title: &'a str,
    ) -> &'a str {
        if !matches!(
            group_kind,
            ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
        ) {
            return title;
        }
        let normalized = title.to_ascii_lowercase();
        if normalized.starts_with("left ") || normalized.starts_with("right ") {
            &title[5..]
        } else {
            title
        }
    }

    fn module_setting_label(&self, group_kind: ModuleSettingsGroupKind, title: &str) -> String {
        let lang = self.app_settings.language;
        let display_title = self.module_setting_display_title(group_kind, title);
        module_setting_catalog_keys(display_title)
            .map(|(label_key, _)| crate::i18n::tr_catalog(lang, label_key).to_owned())
            .unwrap_or_else(|| crate::i18n::tr_text(lang, display_title))
    }

    fn module_setting_tooltip(
        &self,
        group_kind: ModuleSettingsGroupKind,
        field: &ModuleSettingField,
    ) -> String {
        let lang = self.app_settings.language;
        let display_title = self.module_setting_display_title(group_kind, &field.title);
        let key = module_setting_catalog_keys(display_title)
            .map(|(_, tooltip_key)| tooltip_key)
            .unwrap_or("modules_settings.generic_tooltip");
        let field_label = self.module_setting_label(group_kind, &field.title);
        crate::i18n::tr_catalog_format(lang, key, &[("field", field_label.as_str())])
    }

    fn module_setting_transport_value(field: &ModuleSettingField, value: u16) -> u16 {
        if field.width > 1 {
            value
        } else {
            value.min(u8::MAX as u16)
        }
    }

    fn write_module_setting_value(
        &mut self,
        _ctx: &egui::Context,
        group_idx: usize,
        field: &ModuleSettingField,
        value: u16,
    ) {
        let group = self.module_settings.groups.get(group_idx);
        let group_title = group
            .map(|group| group.title.clone())
            .unwrap_or_else(|| "Modules".to_owned());
        let group_kind = group
            .map(|group| group.kind)
            .unwrap_or(ModuleSettingsGroupKind::Other);
        let field_title = field.title.clone();
        let display_label = self.module_setting_label(group_kind, &field_title);
        let old_value = self.module_settings.value(field.qsid);
        let requested = Self::module_setting_transport_value(field, value);

        self.queue_module_setting_write(
            group_title,
            field_title,
            display_label,
            field.qsid,
            field.width,
            old_value,
            requested,
        );
    }

    fn draw_module_settings_field_row(
        &mut self,
        ui: &mut egui::Ui,
        group_idx: usize,
        field_idx: usize,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return;
        };
        let group_kind = group.kind;
        let Some(field) = group.fields.get(field_idx).cloned() else {
            return;
        };
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let dark = ui.visuals().dark_mode;
        let label = self.module_setting_label(group_kind, &field.title);
        let tooltip = if suppress_tooltips {
            None
        } else {
            Some(self.module_setting_tooltip(group_kind, &field))
        };
        let raw_value = self
            .pending_settings_write_value(field.qsid)
            .unwrap_or_else(|| self.module_settings.value(field.qsid));
        match field.kind {
            ModuleSettingKind::Boolean => {
                let switch_width = metrics.value(46.0);
                let switch_size = metrics.size(46.0, 24.0);
                let control_width = self.settings_write_control_width(ui, metrics, switch_width);
                let mask = 1u16 << field.bit;
                let mut checked = raw_value & mask != 0;
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    control_width,
                    |ui| {
                        self.draw_settings_write_status(ui, field.qsid, metrics, suppress_tooltips);
                        let resp = crate::ui_style::settings_switch_sized_stable(
                            ui,
                            ("module_settings", group_idx, field.qsid, field.bit),
                            &mut checked,
                            switch_size,
                        );
                        if resp.changed() {
                            let new_value = if checked {
                                raw_value | mask
                            } else {
                                raw_value & !mask
                            };
                            self.write_module_setting_value(ui.ctx(), group_idx, &field, new_value);
                        }
                    },
                );
            }
            ModuleSettingKind::Integer => {
                let field_width = metrics.value(86.0);
                let control_width = self.settings_write_control_width(ui, metrics, field_width);
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    control_width,
                    |ui| {
                        self.draw_settings_write_status(ui, field.qsid, metrics, suppress_tooltips);
                        let edit_id = egui::Id::new(("module_setting_edit", group_idx, field.qsid));
                        let current = raw_value.clamp(field.min, field.max);
                        let mut text = ui.ctx().data_mut(|d| {
                            d.get_temp::<String>(edit_id)
                                .unwrap_or_else(|| current.to_string())
                        });
                        if text.parse::<u16>().ok() != Some(current)
                            && !ui.memory(|m| m.has_focus(edit_id))
                        {
                            text = current.to_string();
                        }
                        let resp = crate::ui_style::modern_text_field_sized(
                            ui,
                            edit_id,
                            &mut text,
                            field_width,
                            metrics.settings_control_height(),
                            "",
                            5,
                            egui::Align::Center,
                        );
                        let commit = resp.lost_focus()
                            || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if commit {
                            match text.trim().parse::<u16>() {
                                Ok(value) => {
                                    let value = value.clamp(field.min, field.max);
                                    if value != raw_value {
                                        self.write_module_setting_value(
                                            ui.ctx(),
                                            group_idx,
                                            &field,
                                            value,
                                        );
                                    }
                                    text = value.to_string();
                                }
                                Err(_) => text = current.to_string(),
                            }
                        }
                        ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                    },
                );
            }
            ModuleSettingKind::Select => {
                let dropdown_width = metrics.value(120.0);
                let control_width = self.settings_write_control_width(ui, metrics, dropdown_width);
                let selected_idx = (raw_value as usize).min(field.variants.len().saturating_sub(1));
                let variants = field
                    .variants
                    .iter()
                    .map(|variant| {
                        module_setting_variant_label(self.app_settings.language, variant)
                    })
                    .collect::<Vec<_>>();
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label.as_str(),
                    true,
                    tooltip.as_deref(),
                    control_width,
                    |ui| {
                        self.draw_settings_write_status(ui, field.qsid, metrics, suppress_tooltips);
                        let dropdown_id = ui.make_persistent_id((
                            "module_setting_dropdown",
                            group_idx,
                            field.qsid,
                        ));
                        let (_, picked) = Self::draw_touchpad_select_control(
                            ui,
                            dark,
                            dropdown_id,
                            selected_idx,
                            &variants,
                            dropdown_width,
                        );
                        if let Some(picked) = picked {
                            self.write_module_setting_value(
                                ui.ctx(),
                                group_idx,
                                &field,
                                picked as u16,
                            );
                        }
                    },
                );
            }
        }
    }

    fn module_settings_rows(&self) -> Vec<ModuleSettingsRow> {
        let mut rows = vec![ModuleSettingsRow::Refresh];
        let side_groups = self.module_settings_side_group_indices();
        let selected_side_group = self.module_settings.selected_module_group();
        if side_groups.len() > 1 {
            rows.push(ModuleSettingsRow::SideSelector);
        }
        if let Some(group_idx) = selected_side_group {
            rows.extend(self.module_settings_field_rows(group_idx));
        }
        for (group_idx, group) in self.module_settings.groups.iter().enumerate() {
            if matches!(
                group.kind,
                ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
            ) {
                continue;
            }
            rows.push(ModuleSettingsRow::Section(group_idx));
            rows.extend(self.module_settings_field_rows(group_idx));
        }
        rows
    }

    fn module_settings_field_rows(&self, group_idx: usize) -> Vec<ModuleSettingsRow> {
        self.module_settings
            .groups
            .get(group_idx)
            .into_iter()
            .flat_map(move |group| {
                (0..group.fields.len()).map(move |field_idx| ModuleSettingsRow::Field {
                    group_idx,
                    field_idx,
                })
            })
            .collect()
    }

    fn module_settings_side_group_indices(&self) -> Vec<usize> {
        self.module_settings
            .groups
            .iter()
            .enumerate()
            .filter_map(|(idx, group)| {
                matches!(
                    group.kind,
                    ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
                )
                .then_some(idx)
            })
            .collect()
    }

    fn module_settings_group_label(&self, group: &ModuleSettingsGroup) -> String {
        let lang = self.app_settings.language;
        match group.kind {
            ModuleSettingsGroupKind::Left => {
                crate::i18n::tr_catalog(lang, "modules_settings.left_half").to_owned()
            }
            ModuleSettingsGroupKind::Right => {
                crate::i18n::tr_catalog(lang, "modules_settings.right_half").to_owned()
            }
            ModuleSettingsGroupKind::AutoLayer => {
                crate::i18n::tr_catalog(lang, "modules_settings.auto_layer_section").to_owned()
            }
            ModuleSettingsGroupKind::Other => crate::i18n::tr_text(lang, &group.title),
        }
    }

    fn draw_module_settings_section(
        &self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        group_idx: usize,
    ) {
        let Some(group) = self.module_settings.groups.get(group_idx) else {
            return;
        };
        let dark = ui.visuals().dark_mode;
        let title = self.module_settings_group_label(group);
        let (row_rect, _) =
            ui.allocate_exact_size(egui::vec2(content_width, row_height), egui::Sense::hover());
        let separator =
            crate::ui_style::border_color(dark).gamma_multiply(if dark { 0.72 } else { 0.9 });
        ui.painter().line_segment(
            [row_rect.left_bottom(), row_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, separator),
        );
        ui.painter().text(
            row_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.5),
            app_muted_text(dark),
        );
    }

    fn draw_module_settings_side_selector(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let side_groups = self.module_settings_side_group_indices();
        if side_groups.len() <= 1 {
            return;
        }
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let dark = ui.visuals().dark_mode;
        let lang = self.app_settings.language;
        let selected_group = self
            .module_settings
            .selected_module_group()
            .unwrap_or(side_groups[0]);
        let selected_idx = side_groups
            .iter()
            .position(|group_idx| *group_idx == selected_group)
            .unwrap_or(0);
        let labels = side_groups
            .iter()
            .filter_map(|group_idx| self.module_settings.groups.get(*group_idx))
            .map(|group| self.module_settings_group_label(group))
            .collect::<Vec<_>>();
        let dropdown_width = metrics.value(156.0);
        let tooltip = (!suppress_tooltips).then(|| {
            crate::i18n::tr_catalog(lang, "modules_settings.module_side_tooltip").to_owned()
        });
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "modules_settings.module_side"),
            true,
            tooltip.as_deref(),
            dropdown_width,
            |ui| {
                let dropdown_id = ui.make_persistent_id("module_settings_side_dropdown");
                let (_, picked) = Self::draw_touchpad_select_control(
                    ui,
                    dark,
                    dropdown_id,
                    selected_idx,
                    &labels,
                    dropdown_width,
                );
                if let Some(picked) = picked.and_then(|idx| side_groups.get(idx).copied()) {
                    self.module_settings.set_selected_module_group(picked);
                }
            },
        );
    }

    fn draw_module_settings_refresh_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let lang = self.app_settings.language;
        let tooltip = (!suppress_tooltips).then(|| {
            crate::i18n::tr_catalog(lang, "modules_settings.persistent_values_tooltip").to_owned()
        });
        let button_width = metrics.value(112.0);
        #[cfg(not(target_arch = "wasm32"))]
        let refresh_active = self.module_settings_refresh_task.is_some();
        #[cfg(target_arch = "wasm32")]
        let refresh_active = false;
        #[cfg(not(target_arch = "wasm32"))]
        let refresh_enabled = module_settings_refresh_start_allowed(
            self.hid_write_task_active(),
            self.hid_device.is_some(),
            self.current_module_settings_device_identity().is_some(),
        );
        #[cfg(target_arch = "wasm32")]
        let refresh_enabled = false;
        let button_label = if refresh_active {
            crate::i18n::tr_catalog(lang, "modules_settings.refreshing")
        } else {
            crate::i18n::tr_catalog(lang, "modules_settings.refresh")
        };
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            content_width,
            row_height,
            crate::i18n::tr_catalog(lang, "modules_settings.persistent_values"),
            true,
            tooltip.as_deref(),
            button_width,
            |ui| {
                let mut response = crate::ui_style::modern_button(
                    ui,
                    button_label,
                    egui::vec2(button_width, metrics.settings_control_height()),
                    refresh_enabled,
                );
                if !suppress_tooltips {
                    response = response.on_hover_text(crate::i18n::tr_catalog(
                        lang,
                        "modules_settings.persistent_values_tooltip",
                    ));
                }
                if response.clicked() {
                    self.refresh_module_settings_values();
                }
            },
        );
    }

    fn draw_module_settings_row_entry(
        &mut self,
        ui: &mut egui::Ui,
        row: ModuleSettingsRow,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        match row {
            ModuleSettingsRow::Refresh => self.draw_module_settings_refresh_row(
                ui,
                content_width,
                row_height,
                suppress_tooltips,
            ),
            ModuleSettingsRow::SideSelector => self.draw_module_settings_side_selector(
                ui,
                content_width,
                row_height,
                suppress_tooltips,
            ),
            ModuleSettingsRow::Section(group_idx) => {
                self.draw_module_settings_section(ui, content_width, row_height, group_idx)
            }
            ModuleSettingsRow::Field {
                group_idx,
                field_idx,
            } => self.draw_module_settings_field_row(
                ui,
                group_idx,
                field_idx,
                content_width,
                row_height,
                suppress_tooltips,
            ),
        }
    }

    pub(super) fn draw_module_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(lang, "modules_settings.title"))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        "modules_settings.description",
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.module_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "modules_settings.unavailable"),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::QmkSettingsEnableHint,
                        )),
                    );
                    return;
                }

                let rows = self.module_settings_rows();
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "module_settings",
                    metrics,
                    rows.len(),
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row_idx in list.first_visible_row..list.last_visible_row {
                        if let Some(row) = rows.get(row_idx).copied() {
                            self.draw_module_settings_row_entry(
                                ui,
                                row,
                                list.row_content_width,
                                list.row_height,
                                list.suppress_tooltips,
                            );
                        }
                    }
                });

                if list.has_scrollbar {
                    crate::ui_style::paint_floating_scrollbar_handle(
                        ui,
                        list.track_rect,
                        list.handle_height,
                        list.scroll_ratio,
                        list.track_hovered,
                    );
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{module_setting_catalog_keys, module_setting_variant_label};

    #[test]
    fn all_firmware_module_fields_have_catalog_entries_case_insensitively() {
        for title in [
            "Module",
            "Encoder interval",
            "Sticky mode",
            "Invert scroll vertical",
            "invert scroll horizontal",
            "Invert text vertical",
            "invert text horizontal",
            "Auto layer in Normal",
            "auto layer in sniper",
            "Auto layer in Scroll",
            "auto layer in text",
            "Auto layer timeout",
        ] {
            assert!(
                module_setting_catalog_keys(title).is_some(),
                "missing module translation for {title}"
            );
        }
    }

    #[test]
    fn lowercase_module_variants_are_localized_and_capitalized() {
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "trackball"),
            "Трекбол"
        );
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "normal"),
            "Обычный"
        );
        assert_eq!(
            module_setting_variant_label(crate::i18n::Language::Russian, "none"),
            "Нет"
        );
    }

    use super::{
        module_setting_diagnostic_labels, ModuleSettingField, ModuleSettingKind,
        ModuleSettingsGroup, ModuleSettingsGroupKind,
    };

    fn diagnostic_field(qsid: u16, title: &str) -> ModuleSettingField {
        ModuleSettingField {
            title: title.to_owned(),
            qsid,
            kind: ModuleSettingKind::Select,
            bit: 0,
            width: 1,
            min: 0,
            max: 1,
            variants: vec!["Normal".to_owned(), "Scroll".to_owned()],
        }
    }

    #[test]
    fn module_setting_diagnostics_keep_left_and_right_firmware_labels() {
        let groups = vec![
            ModuleSettingsGroup {
                title: "Left Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Left,
                fields: vec![diagnostic_field(42, "Left Mode")],
            },
            ModuleSettingsGroup {
                title: "Right Modules".to_owned(),
                kind: ModuleSettingsGroupKind::Right,
                fields: vec![diagnostic_field(42, "Right Mode")],
            },
        ];

        assert_eq!(
            module_setting_diagnostic_labels(&groups, 42),
            "Left Modules / Left Mode | Right Modules / Right Mode"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_settings_refresh_job_runs_without_blocking_the_caller() {
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();

        let result_receiver = spawn_module_settings_refresh_job(move || {
            started_sender.send(()).unwrap();
            release_receiver.recv().unwrap();
            42
        });

        started_receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        assert!(matches!(
            result_receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        release_sender.send(()).unwrap();
        assert_eq!(
            result_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            42
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_settings_refresh_rejects_duplicate_or_incomplete_requests() {
        assert!(module_settings_refresh_start_allowed(false, true, true));
        assert!(!module_settings_refresh_start_allowed(true, true, true));
        assert!(!module_settings_refresh_start_allowed(false, false, true));
        assert!(!module_settings_refresh_start_allowed(false, true, false));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn module_settings_refresh_result_requires_the_same_connected_device() {
        let expected = ModuleSettingsDeviceIdentity {
            path: "usb:phenom".to_owned(),
            serial_number: None,
            vendor_id: 0,
            product_id: 0,
            keyboard_id: 42,
        };
        let changed_path = ModuleSettingsDeviceIdentity {
            path: "bluetooth:phenom".to_owned(),
            serial_number: None,
            vendor_id: 0,
            product_id: 0,
            keyboard_id: 42,
        };
        let changed_keyboard = ModuleSettingsDeviceIdentity {
            path: expected.path.clone(),
            serial_number: None,
            vendor_id: 0,
            product_id: 0,
            keyboard_id: 43,
        };
        let duplicate_serial_different_path = ModuleSettingsDeviceIdentity {
            path: "usb:another-phenom".to_owned(),
            serial_number: Some("duplicate".to_owned()),
            vendor_id: expected.vendor_id,
            product_id: expected.product_id,
            keyboard_id: expected.keyboard_id,
        };
        let expected_duplicate_serial = ModuleSettingsDeviceIdentity {
            path: expected.path.clone(),
            serial_number: Some("duplicate".to_owned()),
            vendor_id: expected.vendor_id,
            product_id: expected.product_id,
            keyboard_id: expected.keyboard_id,
        };

        assert!(module_settings_refresh_identity_matches(
            &expected,
            Some(&expected)
        ));
        assert!(!module_settings_refresh_identity_matches(&expected, None));
        assert!(!module_settings_refresh_identity_matches(
            &expected,
            Some(&changed_path)
        ));
        assert!(!module_settings_refresh_identity_matches(
            &expected,
            Some(&changed_keyboard)
        ));
        assert!(!module_settings_refresh_identity_matches(
            &expected_duplicate_serial,
            Some(&duplicate_serial_different_path)
        ));
    }
}
