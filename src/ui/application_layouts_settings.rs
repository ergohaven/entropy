use super::application_layout_runtime::app_layout_text;
use super::*;

#[derive(Debug, Clone, Copy)]
struct ApplicationPickerGeometry {
    window_size: egui::Vec2,
    content_width: f32,
    list_height: f32,
}

#[derive(Debug, Clone, Copy)]
struct ControlPairGeometry {
    leading: egui::Rect,
    trailing: egui::Rect,
}

const APPLICATION_LAYOUT_CONTROL_GAP: f32 = 8.0;
const APPLICATION_LAYOUT_APPLICATION_CONTROL_WIDTH: f32 = 320.0;
fn application_layout_name_is_invalid(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    target_id: Option<&str>,
    value: &str,
) -> bool {
    let value = value.trim();
    value.is_empty() || settings.layout_name_exists(value, target_id)
}

fn window_detector_status(
    status: &crate::app_discovery::ForegroundStatus,
    language: crate::i18n::Language,
) -> (&'static str, String, bool) {
    match &status.state {
        crate::app_discovery::ForegroundState::BackendUnavailable(error) => (
            app_layout_text(language, "Недоступен", "Unavailable"),
            format!("{} — {error}", status.backend),
            false,
        ),
        crate::app_discovery::ForegroundState::Focused(_)
        | crate::app_discovery::ForegroundState::UnidentifiedWindow(_)
        | crate::app_discovery::ForegroundState::NoFocusedWindow => (
            app_layout_text(language, "Работает", "Running"),
            status.backend.clone(),
            true,
        ),
    }
}

fn control_pair_geometry(
    control_rect: egui::Rect,
    trailing_width: f32,
    gap: f32,
    height: f32,
) -> ControlPairGeometry {
    let height = height.min(control_rect.height()).max(0.0);
    let top = control_rect.center().y - height / 2.0;
    let trailing_width = trailing_width.min(control_rect.width()).max(0.0);
    let trailing = egui::Rect::from_min_size(
        egui::pos2(control_rect.right() - trailing_width, top),
        egui::vec2(trailing_width, height),
    );
    let leading_right = (trailing.left() - gap).max(control_rect.left());
    let leading = egui::Rect::from_min_max(
        egui::pos2(control_rect.left(), top),
        egui::pos2(leading_right, top + height),
    );

    ControlPairGeometry { leading, trailing }
}

fn centered_button_pair_geometry(
    container_rect: egui::Rect,
    button_size: egui::Vec2,
    gap: f32,
) -> ControlPairGeometry {
    let total_width = button_size.x * 2.0 + gap;
    let left = container_rect.center().x - total_width / 2.0;
    let top = container_rect.center().y - button_size.y / 2.0;
    let leading = egui::Rect::from_min_size(egui::pos2(left, top), button_size);
    let trailing = egui::Rect::from_min_size(egui::pos2(leading.right() + gap, top), button_size);

    ControlPairGeometry { leading, trailing }
}

fn automatic_application_layout_name(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    application: &crate::application_layouts::DetectedApplication,
    excluding_id: Option<&str>,
) -> String {
    let base = if application.display_name.trim().is_empty() {
        application.executable.trim()
    } else {
        application.display_name.trim()
    };
    let base = if base.is_empty() { "Application" } else { base };
    if !settings.layout_name_exists(base, excluding_id) {
        return base.to_owned();
    }
    (2..)
        .map(|suffix| format!("{base} ({suffix})"))
        .find(|name| !settings.layout_name_exists(name, excluding_id))
        .unwrap_or_else(|| base.to_owned())
}

fn application_picker_geometry(viewport: egui::Vec2, scale: f32) -> ApplicationPickerGeometry {
    let horizontal_margin = 24.0 * scale;
    let vertical_margin = 24.0 * scale;
    let available_width = (viewport.x - horizontal_margin * 2.0).max(1.0);
    let available_height = (viewport.y - vertical_margin * 2.0).max(1.0);
    let window_width = (680.0 * scale).min(available_width);
    let window_height = (460.0 * scale).min(available_height);
    let content_width = (window_width - 60.0 * scale)
        .max(220.0 * scale)
        .min((window_width - 20.0 * scale).max(1.0));
    let list_height = (window_height - 170.0 * scale).clamp(116.0 * scale, 280.0 * scale);

    ApplicationPickerGeometry {
        window_size: egui::vec2(window_width, window_height),
        content_width,
        list_height,
    }
}

fn centered_application_picker_content_rect(
    available: egui::Rect,
    requested_width: f32,
) -> egui::Rect {
    let width = requested_width.min(available.width()).max(1.0);
    egui::Rect::from_min_max(
        egui::pos2(available.center().x - width / 2.0, available.top()),
        egui::pos2(available.center().x + width / 2.0, available.bottom()),
    )
}

fn application_picker_content_ui<R>(
    ui: &mut egui::Ui,
    window_center_x: f32,
    requested_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui, f32) -> R,
) -> egui::InnerResponse<R> {
    let available = ui.available_rect_before_wrap();
    let width = requested_width.min(available.width()).max(1.0);
    let left = (window_center_x - width / 2.0).clamp(
        available.left(),
        (available.right() - width).max(available.left()),
    );
    let content_rect = egui::Rect::from_min_max(
        egui::pos2(left, available.top()),
        egui::pos2(left + width, available.bottom()),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.set_min_width(content_rect.width());
            ui.set_max_width(content_rect.width());
            add_contents(ui, content_rect.width())
        },
    )
}

impl EntropyApp {
    fn close_application_picker_dialog(&mut self) {
        self.application_picker_open = false;
        self.application_picker_selected = None;
        self.application_picker_target_layout_id = None;
        self.application_picker_name.clear();
        self.application_picker_search.clear();
        self.application_picker_category_changed = false;
        self.application_picker_custom_category_id = None;
    }

    fn close_application_categories_dialog(&mut self) {
        self.application_categories_open = false;
        self.application_categories_selected_id = None;
        self.application_categories_name.clear();
    }

    pub(super) fn dismiss_application_layouts_dialogs_if_page_inactive(&mut self) {
        if self.main_menu_tab == MainMenuTab::Advanced
            && self.settings_tab == SettingsTab::ApplicationLayouts
        {
            return;
        }
        if self.application_picker_open {
            self.close_application_picker_dialog();
        }
        if self.application_categories_open {
            self.close_application_categories_dialog();
        }
    }

    pub(super) fn draw_application_layouts_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        #[cfg(target_os = "linux")]
        self.poll_gnome_integration_install(ui.ctx());

        let language = self.app_settings.language;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let content_width = metrics.settings_content_width();
        let title_y = content_rect.top() + metrics.value(30.0);
        let description_y = title_y + metrics.value(28.0);
        let body_top = description_y + metrics.value(26.0);
        let center_x = content_rect.center().x;
        let body_rect = egui::Rect::from_min_max(
            egui::pos2(center_x - content_width / 2.0, body_top),
            egui::pos2(center_x + content_width / 2.0, content_rect.bottom()),
        );
        let dark = ui.visuals().dark_mode;

        ui.painter().text(
            egui::pos2(center_x, title_y),
            egui::Align2::CENTER_CENTER,
            app_layout_text(language, "Автослой", "Autolayer"),
            egui::FontId::proportional(metrics.value(18.0)),
            ui.visuals().text_color(),
        );
        ui.painter().text(
            egui::pos2(center_x, description_y),
            egui::Align2::CENTER_CENTER,
            app_layout_text(
                language,
                "Настройте автоматическое переключение раскладок для приложений",
                "Configure automatic layout switching for applications",
            ),
            egui::FontId::proportional(metrics.value(13.0)),
            app_muted_text(dark),
        );

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            if !self.application_layouts_supported() {
                ui.vertical_centered(|ui| {
                    ui.add_space(metrics.value(150.0));
                    let macropad_connected = self
                        .selected_device
                        .and_then(|index| self.device_manager.devices().get(index))
                        .is_some_and(super::application_layout_runtime::device_supports_application_layouts);
                    ui.label(if macropad_connected {
                        app_layout_text(
                            language,
                            "Установленная прошивка Macropad не поддерживает раскладки приложений. Обновите прошивку и переподключите устройство.",
                            "The installed Macropad firmware does not support application layouts. Update the firmware and reconnect the device.",
                        )
                    } else {
                        app_layout_text(
                            language,
                            "Подключите Macropad, чтобы настроить раскладки приложений.",
                            "Connect Macropad to configure application layouts.",
                        )
                    });
                });
                return;
            }

            self.ensure_application_layout_settings();
            let action_size = metrics.size(126.0, 34.0);
            let action_gap = metrics.value(10.0);
            let row_count = self.application_layouts_editor_row_count();
            let list_rect = body_rect.translate(egui::vec2(0.0, metrics.value(4.0)));
            crate::ui_style::allocate_ui_at_rect(ui, list_rect, |ui| {
                let list = allocate_adaptive_settings_list_viewport_capped(
                    ui,
                    "application_layouts_settings",
                    metrics,
                    row_count,
                    metrics.value(26.0) + action_size.y,
                    metrics.settings_row_height(),
                    6,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    self.draw_application_layouts_editor(
                        ui,
                        metrics,
                        list.first_visible_row..list.last_visible_row,
                    );
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
                let action_rect = fixed_settings_action_bar_rect(
                    list.viewport,
                    metrics,
                    action_size,
                    2,
                    action_gap,
                );
                self.draw_application_layouts_actions(ui, action_rect, metrics);
            });
        });
        self.draw_application_picker_v2(ui.ctx());
        self.draw_application_categories_modal(ui.ctx());
    }

    fn draw_application_categories_modal(&mut self, ctx: &egui::Context) {
        if !self.application_categories_open {
            return;
        }
        let language = self.app_settings.language;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ctx);
        let choices = self.application_layout_category_choices(language);
        let ids = choices.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        let mut labels = choices
            .iter()
            .map(|(_, label)| label.clone())
            .collect::<Vec<_>>();
        labels.push(app_layout_text(language, "Новая категория", "New category").to_owned());
        let selected = ids
            .iter()
            .position(|id| Some(id) == self.application_categories_selected_id.as_ref())
            .unwrap_or(ids.len());
        let selected_id = ids.get(selected).cloned();
        let mut open = true;
        let mut add = false;
        let mut begin_add = false;
        let mut save_name = false;
        let mut delete = false;
        let mut selection_changed = false;
        crate::ui_style::centered_modal_window(
            ctx,
            app_layout_text(language, "Категории", "Categories"),
            egui::Id::new("autolayer_categories_modal"),
            &mut open,
            metrics.size(380.0, 208.0),
        )
        .movable(false)
        .show(ctx, |ui| {
            let width = metrics.value(340.0).min(ui.available_width());
            ui.set_width(width);
            ui.label(app_layout_text(language, "Категория", "Category"));
            ui.add_space(metrics.value(6.0));
            if let (_, Some(picked)) = crate::ui_style::modern_dropdown_select_sized(
                ui,
                ui.make_persistent_id("autolayer_manage_category"),
                &labels,
                selected,
                width,
                metrics.settings_control_height(),
                metrics.settings_control_font_size(),
            ) {
                selection_changed = true;
                self.application_categories_selected_id = ids.get(picked).cloned();
                self.application_categories_name = ids
                    .get(picked)
                    .map(|_| labels[picked].clone())
                    .unwrap_or_default();
            }
            ui.add_space(metrics.value(8.0));
            ui.label(app_layout_text(language, "Название", "Name"));
            ui.add_space(metrics.value(6.0));
            let name_response = crate::ui_style::modern_text_field_sized(
                ui,
                ui.make_persistent_id("autolayer_manage_category_name"),
                &mut self.application_categories_name,
                width,
                metrics.settings_control_height(),
                app_layout_text(language, "Название категории", "Category name"),
                48,
                egui::Align::Min,
            )
            .on_hover_text(app_layout_text(
                language,
                "Нажмите Enter или выйдите из поля, чтобы сохранить название",
                "Press Enter or leave the field to save the name",
            ));
            let name = self.application_categories_name.trim();
            let unique_name = !name.is_empty()
                && name.chars().count() <= 48
                && !choices.iter().any(|(id, label)| {
                    Some(id) != self.application_categories_selected_id.as_ref()
                        && label.eq_ignore_ascii_case(name)
                });
            save_name = self.application_categories_selected_id.is_some()
                && !selection_changed
                && unique_name
                && (name_response.lost_focus()
                    || (name_response.has_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))));
            ui.add_space(metrics.value(12.0));
            let button_size = metrics.size(112.0, 32.0);
            let (action_rect, _) =
                ui.allocate_exact_size(egui::vec2(width, button_size.y), egui::Sense::hover());
            let actions =
                centered_button_pair_geometry(action_rect, button_size, metrics.value(8.0));
            let first = crate::ui_style::allocate_ui_at_rect(ui, actions.leading, |ui| {
                crate::ui_style::modern_button(
                    ui,
                    if self.application_categories_selected_id.is_some() {
                        app_layout_text(language, "Добавить", "Add")
                    } else {
                        app_layout_text(language, "Создать", "Create")
                    },
                    actions.leading.size(),
                    self.application_categories_selected_id.is_some() || unique_name,
                )
            })
            .inner;
            if self.application_categories_selected_id.is_some() {
                begin_add = first.clicked();
            } else {
                add = first.clicked();
            }
            delete = crate::ui_style::allocate_ui_at_rect(ui, actions.trailing, |ui| {
                crate::ui_style::modern_button(
                    ui,
                    app_layout_text(language, "Удалить", "Delete"),
                    actions.trailing.size(),
                    self.application_categories_selected_id
                        .as_deref()
                        .is_some_and(|id| id != "other"),
                )
            })
            .inner
            .on_hover_text(app_layout_text(
                language,
                "Приложения из удалённой категории перейдут в «Другие»",
                "Applications in a deleted category move to Other",
            ))
            .clicked();
        });
        if begin_add {
            self.application_categories_selected_id = None;
            self.application_categories_name.clear();
        }
        let name = self.application_categories_name.trim().to_owned();
        if let Some(settings) = self.application_layout_settings_mut() {
            let new_id = if add {
                settings.create_category(&name)
            } else {
                None
            };
            let changed = new_id.is_some()
                || (save_name
                    && !begin_add
                    && !delete
                    && selected_id
                        .as_deref()
                        .is_some_and(|id| settings.rename_category(id, &name)))
                || (delete
                    && selected_id
                        .as_deref()
                        .is_some_and(|id| settings.remove_category(id)));
            if changed {
                if delete {
                    self.application_categories_selected_id = Some("other".to_owned());
                    self.application_categories_name = choices
                        .iter()
                        .find(|(id, _)| id == "other")
                        .map(|(_, label)| label.clone())
                        .unwrap_or_default();
                }
                if let Some(id) = new_id {
                    self.application_categories_selected_id = Some(id);
                }
                save_app_settings(&self.app_settings);
            }
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            || !open
        {
            self.close_application_categories_dialog();
        }
    }

    fn ensure_application_layout_settings(&mut self) {
        let Some(key) = self.application_layout_device_key() else {
            return;
        };
        let installed_presets = crate::app_discovery::installed_builtin_presets();
        let settings = self
            .app_settings
            .application_layouts
            .entry(key)
            .or_default();
        let mut changed = settings.normalize();
        changed |= settings.provision_builtin_presets(installed_presets.as_ref().ok());
        if changed {
            save_app_settings(&self.app_settings);
        }
    }

    fn application_layouts_show_gnome_integration(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            crate::app_discovery::gnome_shell_integration_needed()
                && (matches!(
                    self.application_discovery.foreground_status.state,
                    crate::app_discovery::ForegroundState::BackendUnavailable(_)
                ) || self.gnome_integration_install_task.is_some()
                    || self.gnome_integration_install_result.is_some())
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    fn application_layouts_install_feedback_visible(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.gnome_integration_install_task.is_some()
                || self.gnome_integration_install_result.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    fn application_layouts_editor_row_count(&self) -> usize {
        let show_gnome = self.application_layouts_show_gnome_integration();
        8 + usize::from(show_gnome)
            + usize::from(show_gnome && self.application_layouts_install_feedback_visible())
    }

    fn draw_application_layouts_editor(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        visible_rows: std::ops::Range<usize>,
    ) {
        let language = self.app_settings.language;
        let Some(device_key) = self.application_layout_device_key() else {
            return;
        };
        let Some(snapshot) = self
            .app_settings
            .application_layouts
            .get(&device_key)
            .cloned()
        else {
            return;
        };
        let mut selected_id = snapshot.editor_layout_id.clone();
        let layouts = self.application_layout_editor_options();
        let selected_name = snapshot
            .editor_layout()
            .map(|layout| layout.name.clone())
            .unwrap_or_else(|| "Default".to_owned());
        let mut automatically_return_to_default = snapshot.automatically_return_to_default;
        let mut automatic_switching_enabled = snapshot.automatic_switching_enabled;

        let row_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let control_width = metrics.value(260.0);
        let control_height = metrics.settings_control_height();
        let control_font = metrics.settings_control_font_size();
        ui.spacing_mut().item_spacing.y = 0.0;

        if visible_rows.contains(&0) {
            crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(language, "Включить", "Enable"),
            true,
            Some(app_layout_text(language,
                "OFF: фокус окна не меняет ручной выбор. ON: действуют правила приложений",
                "OFF: window focus does not change the manual selection. ON: application rules apply")),
            metrics.value(46.0),
            |ui| {
                crate::ui_style::settings_switch_sized_stable(
                    ui,
                    "application_layout_master_automatic_switching",
                    &mut automatic_switching_enabled,
                    metrics.size(46.0, 24.0),
                );
            },
        );
        }
        if visible_rows.contains(&1) {
            crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(
                language,
                "Автоматически возвращаться к Default",
                "Automatically return to Default",
            ),
            true,
            Some(app_layout_text(
                language,
                "Общая настройка для всех раскладок. Приложения с включённым автопереключением активируют свои раскладки. Приложения с выключенным автопереключением сохраняют ручной выбор. Если включено, любое другое окно возвращает Default.",
                "Global setting for all layouts. Applications with automatic switching enabled activate their layouts. Applications with it disabled preserve the manual selection. When enabled, any other window returns to Default.",
            )),
            metrics.value(46.0),
            |ui| {
                crate::ui_style::settings_switch_sized_stable(
                    ui,
                    "application_layout_automatic_return_default",
                    &mut automatically_return_to_default,
                    metrics.size(46.0, 24.0),
                )
                .on_hover_text(app_layout_text(
                    language,
                    "Если активному приложению не назначена включённая раскладка, Macropad автоматически возвращается к Default",
                    "When the active application has no enabled assigned layout, Macropad automatically returns to Default",
                ));
            },
        );
        }
        let mut changed = false;
        if let Some(settings) = self.app_settings.application_layouts.get_mut(&device_key) {
            if settings.automatic_switching_enabled != automatic_switching_enabled {
                settings.automatic_switching_enabled = automatic_switching_enabled;
                changed = true;
            }
            if settings.automatically_return_to_default != automatically_return_to_default {
                settings.automatically_return_to_default = automatically_return_to_default;
                changed = true;
            }
        }

        let next_row = self.draw_application_layouts_detector_status(
            ui,
            language,
            row_width,
            row_height,
            control_width,
            control_height,
            control_font,
            &visible_rows,
        );

        let layout_selector_tooltip = app_layout_text(
            language,
            "Выберите раскладку для редактирования",
            "Select a layout to edit",
        );
        if visible_rows.contains(&next_row) {
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_width,
                row_height,
                app_layout_text(language, "Раскладка", "Layout"),
                true,
                Some(layout_selector_tooltip),
                control_width,
                |ui| {
                    let dropdown_id = ui.make_persistent_id("application_layout_selector");
                    let dropdown = crate::ui_style::modern_dropdown_button_sized(
                        ui,
                        dropdown_id,
                        &selected_name,
                        ui.visuals().text_color(),
                        control_width,
                        control_height,
                        control_font,
                    );
                    let groups = if egui::Popup::is_id_open(ui.ctx(), dropdown_id) {
                        self.application_layout_editor_labeled_groups(&layouts, language)
                    } else {
                        Vec::new()
                    };
                    let picked = crate::ui_style::modern_dropdown_grouped_options(
                        ui,
                        dropdown_id,
                        &dropdown,
                        &layouts[0],
                        &groups,
                        &selected_id,
                        control_width,
                        control_font,
                    );
                    if let Some(id) = picked {
                        selected_id = id;
                    }
                    dropdown.on_hover_text(layout_selector_tooltip);
                },
            );
        }

        if snapshot.editor_layout_id != selected_id {
            self.select_application_layout_for_editing(&selected_id);
        }

        let selected = self
            .app_settings
            .application_layouts
            .get(&device_key)
            .and_then(|settings| settings.editor_layout())
            .cloned();
        if let Some(selected) = selected {
            let is_default =
                selected.id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
            let executable = selected.executable.clone();

            let mut application_display = if is_default {
                app_layout_text(
                    language,
                    "Для остальных приложений",
                    "All other applications",
                )
                .to_owned()
            } else {
                executable.clone()
            };
            let application_tooltip = if is_default {
                app_layout_text(
                    language,
                    "Default используется для приложений, которым не назначена отдельная раскладка",
                    "Default is used for applications that do not have their own assigned layout",
                )
            } else {
                app_layout_text(
                    language,
                    "Приложение, связанное с выбранной раскладкой. Нажмите «Изменить», чтобы выбрать другое запущенное приложение",
                    "The application linked to the selected layout. Click Edit to choose another running application",
                )
            };
            if visible_rows.contains(&(next_row + 1)) {
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_width,
                    row_height,
                    app_layout_text(language, "Приложение", "Application"),
                    !is_default,
                    Some(application_tooltip),
                    metrics.value(APPLICATION_LAYOUT_APPLICATION_CONTROL_WIDTH),
                    |ui| {
                        let gap = metrics.value(APPLICATION_LAYOUT_CONTROL_GAP);
                        let choose_width = metrics.value(82.0);
                        let pair =
                            control_pair_geometry(ui.max_rect(), choose_width, gap, control_height);
                        crate::ui_style::allocate_ui_at_rect(ui, pair.leading, |ui| {
                            crate::ui_style::modern_text_field_interactive(
                                ui,
                                ui.make_persistent_id("application_layout_executable"),
                                &mut application_display,
                                pair.leading.width(),
                                app_layout_text(language, "Исполняемый файл", "Executable"),
                                120,
                                egui::Align::Min,
                                false,
                            )
                            .on_hover_text(application_tooltip)
                        });
                        let edit_response =
                            crate::ui_style::allocate_ui_at_rect(ui, pair.trailing, |ui| {
                                crate::ui_style::modern_button(
                                    ui,
                                    app_layout_text(language, "Изменить", "Edit"),
                                    pair.trailing.size(),
                                    !is_default,
                                )
                            })
                            .inner
                            .on_hover_text(application_tooltip);
                        if edit_response.clicked() {
                            self.open_application_picker(true);
                        }
                    },
                );
            }
        }

        if visible_rows.contains(&(next_row + 2)) {
            let tooltip = app_layout_text(
                language,
                "Создать, переименовать или удалить категории приложений",
                "Create, rename, or delete application categories",
            );
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_width,
                row_height,
                app_layout_text(language, "Категории", "Categories"),
                true,
                Some(tooltip),
                metrics.value(82.0),
                |ui| {
                    if crate::ui_style::modern_button(
                        ui,
                        app_layout_text(language, "Изменить", "Edit"),
                        egui::vec2(metrics.value(82.0), control_height),
                        true,
                    )
                    .clicked()
                    {
                        self.application_categories_open = true;
                        self.application_categories_selected_id = Some("other".to_owned());
                        self.application_categories_name = self
                            .application_layout_category_choices(language)
                            .into_iter()
                            .find(|(id, _)| id == "other")
                            .map(|(_, label)| label)
                            .unwrap_or_default();
                    }
                },
            );
        }

        self.draw_application_layouts_runtime_status(
            ui,
            &device_key,
            language,
            row_width,
            row_height,
            control_width,
            control_font,
            next_row + 3,
            &visible_rows,
        );

        if changed {
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_application_layouts_actions(
        &mut self,
        ui: &mut egui::Ui,
        action_rect: egui::Rect,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        let Some(device_key) = self.application_layout_device_key() else {
            return;
        };
        let Some(selected) = self
            .app_settings
            .application_layouts
            .get(&device_key)
            .and_then(|settings| settings.editor_layout())
        else {
            return;
        };
        let selected_id = selected.id.clone();
        let is_default = selected_id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
        let language = self.app_settings.language;
        let action_size = metrics.size(126.0, 34.0);
        let action_gap = metrics.value(10.0);
        let actions = centered_button_pair_geometry(action_rect, action_size, action_gap);
        let add_response = crate::ui_style::allocate_ui_at_rect(ui, actions.leading, |ui| {
            crate::ui_style::modern_button(
                ui,
                app_layout_text(language, "Добавить", "Add"),
                actions.leading.size(),
                true,
            )
        })
        .inner
        .on_hover_text(app_layout_text(
            language,
            "Создать новую раскладку для запущенного приложения",
            "Create a new layout for a running application",
        ));
        if add_response.clicked() {
            self.open_application_picker(false);
        }
        let delete_response = crate::ui_style::allocate_ui_at_rect(ui, actions.trailing, |ui| {
            crate::ui_style::modern_button(
                ui,
                app_layout_text(language, "Удалить", "Delete"),
                actions.trailing.size(),
                !is_default,
            )
        })
        .inner
        .on_hover_text(if is_default {
            app_layout_text(
                language,
                "Default нельзя удалить",
                "Default cannot be deleted",
            )
        } else {
            app_layout_text(
                language,
                "Удалить выбранную раскладку приложения",
                "Delete the selected application layout",
            )
        });
        if delete_response.clicked()
            && self
                .app_settings
                .application_layouts
                .get_mut(&device_key)
                .is_some_and(|settings| settings.remove(&selected_id))
        {
            // Deletion can change both the active and editor profiles without a
            // foreground-window event. Invalidate the cached source so the next
            // runtime tick resolves the currently focused app from stable IDs.
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.application_layout_foreground = None;
            }
            self.selected_layer = 0;
            self.selected_key = None;
            self.selected_encoder = None;
            self.reset_matrix_tester_state();
            save_app_settings(&self.app_settings);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_application_layouts_detector_status(
        &mut self,
        ui: &mut egui::Ui,
        language: crate::i18n::Language,
        row_width: f32,
        row_height: f32,
        control_width: f32,
        control_height: f32,
        control_font: f32,
        visible_rows: &std::ops::Range<usize>,
    ) -> usize {
        let mut row = 2;
        let detector = self.application_discovery.foreground_status.clone();
        let (detector_text, detector_details, detector_ok) =
            window_detector_status(&detector, language);
        let detector_tooltip = app_layout_text(
            language,
            "Показывает механизм, через который Entropy определяет активное окно. Наведите на значение, чтобы увидеть полный статус",
            "Shows the mechanism Entropy uses to detect the active window. Hover the value to see the full status",
        );
        if visible_rows.contains(&row) {
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_width,
                row_height,
                app_layout_text(language, "Детектор окон", "Window detector"),
                true,
                Some(detector_tooltip),
                control_width,
                |ui| {
                    crate::ui_style::settings_value_label(
                        ui,
                        RichText::new(detector_text)
                            .size(control_font)
                            .color(if detector_ok {
                                app_muted_text(ui.visuals().dark_mode)
                            } else {
                                egui::Color32::from_rgb(220, 92, 76)
                            }),
                    )
                    .on_hover_text(&detector_details);
                },
            );
        }
        row += 1;
        if self.application_layouts_show_gnome_integration() {
            let integration_tooltip = app_layout_text(
                language,
                "Устанавливает включённую в Entropy интеграцию GNOME для определения активного окна в Wayland",
                "Installs the GNOME integration bundled with Entropy to detect the active window on Wayland",
            );
            if visible_rows.contains(&row) {
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_width,
                    row_height,
                    app_layout_text(language, "Интеграция GNOME", "GNOME integration"),
                    true,
                    Some(integration_tooltip),
                    control_width,
                    |ui| {
                        #[cfg(target_os = "linux")]
                        if self.gnome_integration_install_task.is_some() {
                            ui.allocate_ui_with_layout(
                                egui::vec2(control_width, control_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.spinner();
                                    ui.label(app_layout_text(
                                        language,
                                        "Установка и проверка…",
                                        "Installing and verifying…",
                                    ));
                                },
                            );
                        } else if crate::ui_style::modern_button(
                            ui,
                            app_layout_text(
                                language,
                                "Установить и включить",
                                "Install and enable",
                            ),
                            egui::vec2(control_width, control_height),
                            true,
                        )
                        .on_hover_text(integration_tooltip)
                        .clicked()
                        {
                            #[cfg(target_os = "linux")]
                            self.start_gnome_integration_install();
                        }
                    },
                );
            }
            row += 1;
            #[cfg(target_os = "linux")]
            if self.application_layouts_install_feedback_visible() {
                let (message, color) =
                    self.gnome_integration_install_feedback(language, ui.visuals().dark_mode);
                if visible_rows.contains(&row) {
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        row_width,
                        row_height,
                        app_layout_text(language, "Статус установки", "Installation status"),
                        true,
                        Some(&message),
                        control_width,
                        |ui| {
                            ui.add_sized(
                                [control_width, control_height],
                                egui::Label::new(
                                    RichText::new(&message).size(control_font).color(color),
                                )
                                .truncate(),
                            );
                        },
                    );
                }
                row += 1;
            }
        }

        row
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_application_layouts_runtime_status(
        &mut self,
        ui: &mut egui::Ui,
        device_key: &str,
        language: crate::i18n::Language,
        row_width: f32,
        row_height: f32,
        control_width: f32,
        control_font: f32,
        first_row: usize,
        visible_rows: &std::ops::Range<usize>,
    ) {
        let mut row = first_row;
        let detector = &self.application_discovery.foreground_status;
        let foreground = match &detector.state {
            crate::app_discovery::ForegroundState::Focused(application) => application.label(),
            crate::app_discovery::ForegroundState::UnidentifiedWindow(title) => {
                if title.trim().is_empty() {
                    app_layout_text(language, "неизвестное окно", "unidentified window").to_owned()
                } else {
                    format!(
                        "{} — {}",
                        app_layout_text(language, "неизвестное окно", "unidentified window"),
                        title.trim()
                    )
                }
            }
            crate::app_discovery::ForegroundState::NoFocusedWindow => {
                app_layout_text(language, "нет активного окна", "no focused window").to_owned()
            }
            crate::app_discovery::ForegroundState::BackendUnavailable(_) => {
                app_layout_text(language, "детектор недоступен", "detector unavailable").to_owned()
            }
        };
        let foreground_tooltip = app_layout_text(
            language,
            "Приложение, окно которого сейчас находится в фокусе и используется для автопереключения",
            "The application whose window is currently focused and used for automatic switching",
        );
        if visible_rows.contains(&row) {
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_width,
                row_height,
                app_layout_text(language, "Приложение в фокусе", "Focused application"),
                true,
                Some(foreground_tooltip),
                control_width,
                |ui| {
                    crate::ui_style::settings_value_label(
                        ui,
                        RichText::new(&foreground)
                            .size(control_font)
                            .color(app_muted_text(ui.visuals().dark_mode)),
                    );
                },
            );
        }
        row += 1;
        let active_layout = self
            .app_settings
            .application_layouts
            .get(device_key)
            .and_then(|settings| settings.active_layout())
            .map(|layout| layout.name.clone())
            .unwrap_or_else(|| "Default".to_owned());
        let active_layout_tooltip = app_layout_text(
            language,
            "Раскладка, которая сейчас активна на Macropad",
            "The layout that is currently active on Macropad",
        );
        if visible_rows.contains(&row) {
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_width,
                row_height,
                app_layout_text(language, "Активная раскладка", "Active layout"),
                true,
                Some(active_layout_tooltip),
                control_width,
                |ui| {
                    crate::ui_style::settings_value_label(
                        ui,
                        RichText::new(&active_layout)
                            .size(control_font)
                            .color(app_muted_text(ui.visuals().dark_mode)),
                    );
                },
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn start_gnome_integration_install(&mut self) {
        self.gnome_integration_install_result = None;
        match crate::app_discovery::start_gnome_shell_integration_install() {
            Ok(task) => self.gnome_integration_install_task = Some(task),
            Err(error) => self.gnome_integration_install_result = Some(Err(error)),
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_gnome_integration_install(&mut self, ctx: &egui::Context) {
        let outcome = self
            .gnome_integration_install_task
            .as_ref()
            .map(crate::app_discovery::GnomeIntegrationInstallTask::try_recv);
        match outcome {
            Some(Ok(result)) => {
                self.gnome_integration_install_task = None;
                self.status_msg = match &result {
                    Ok(report) => report.message.clone(),
                    Err(error) => format!("GNOME integration: {error}"),
                };
                self.gnome_integration_install_result = Some(result);
                crate::app_discovery::refresh_application_discovery();
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.gnome_integration_install_task = None;
                let error = "GNOME integration installer stopped without a result".to_owned();
                self.status_msg = error.clone();
                self.gnome_integration_install_result = Some(Err(error));
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            None => {}
        }
    }

    #[cfg(target_os = "linux")]
    fn gnome_integration_install_feedback(
        &self,
        language: crate::i18n::Language,
        dark: bool,
    ) -> (String, egui::Color32) {
        if self.gnome_integration_install_task.is_some() {
            return (
                app_layout_text(
                    language,
                    "Устанавливаю файлы и проверяю GNOME…",
                    "Installing files and checking GNOME…",
                )
                .to_owned(),
                app_muted_text(dark),
            );
        }
        match self.gnome_integration_install_result.as_ref() {
            Some(Ok(report)) if report.active => (
                app_layout_text(language, "Установлено и запущено", "Installed and running")
                    .to_owned(),
                egui::Color32::from_rgb(74, 170, 108),
            ),
            Some(Ok(report)) if report.restart_required => (
                app_layout_text(
                    language,
                    "Установлено. Выйдите из сеанса и войдите снова",
                    "Installed. Sign out and sign back in",
                )
                .to_owned(),
                egui::Color32::from_rgb(218, 164, 70),
            ),
            Some(Ok(report)) if report.enabled => (
                report.message.clone(),
                egui::Color32::from_rgb(218, 164, 70),
            ),
            Some(Ok(report)) => (report.message.clone(), app_muted_text(dark)),
            Some(Err(error)) => (
                format!(
                    "{}: {error}",
                    app_layout_text(language, "Ошибка установки", "Installation failed")
                ),
                egui::Color32::from_rgb(220, 92, 76),
            ),
            None => (String::new(), app_muted_text(dark)),
        }
    }

    fn open_application_picker(&mut self, assign_existing: bool) {
        self.application_picker_assign_existing = assign_existing;
        self.application_picker_target_layout_id = None;
        self.application_picker_open = true;
        self.application_picker_search.clear();
        self.application_picker_selected = None;
        self.application_picker_name.clear();
        self.application_picker_category_changed = false;
        self.application_picker_custom_category_id = None;
        self.application_picker_category =
            crate::application_layouts::ApplicationLayoutCategory::Other;

        if assign_existing {
            let selected = self
                .application_layout_device_key()
                .and_then(|key| self.app_settings.application_layouts.get(&key))
                .and_then(|settings| settings.editor_layout())
                .filter(|layout| {
                    layout.id != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
                })
                .cloned();
            if let Some(layout) = selected {
                let category_id = self
                    .application_layout_settings()
                    .map(|settings| settings.category_id_for_layout(&layout))
                    .unwrap_or_else(|| "other".to_owned());
                self.application_picker_custom_category_id = category_id
                    .starts_with("custom:")
                    .then(|| category_id.clone());
                self.application_picker_category =
                    crate::application_layouts::ApplicationLayoutCategory::ALL
                        .into_iter()
                        .find(|category| category.id() == category_id)
                        .unwrap_or(crate::application_layouts::ApplicationLayoutCategory::Other);
                self.application_picker_name = layout.name.clone();
                self.application_picker_target_layout_id = Some(layout.id.clone());
                self.application_picker_selected =
                    Some(crate::application_layouts::DetectedApplication {
                        executable: layout.executable,
                        identities: layout.application_identities,
                        display_name: layout.name,
                        window_title: String::new(),
                    });
            }
        }
        crate::app_discovery::refresh_application_discovery();
    }

    fn select_application_picker_choice(
        &mut self,
        application: crate::application_layouts::DetectedApplication,
    ) {
        if let Some(target_id) = self.application_picker_target_layout_id.as_deref() {
            let suggested_name = self.application_layout_settings().and_then(|settings| {
                settings
                    .layouts
                    .get(target_id)
                    .filter(|layout| layout.name == self.application_picker_name)
                    .map(|_| {
                        automatic_application_layout_name(settings, &application, Some(target_id))
                    })
            });
            if let Some(name) = suggested_name {
                self.application_picker_name = name;
            }
        } else {
            let name_follows_selection = self.application_picker_name.trim().is_empty()
                || self
                    .application_picker_selected
                    .as_ref()
                    .is_some_and(|previous| {
                        self.application_layout_settings().is_some_and(|settings| {
                            self.application_picker_name
                                == automatic_application_layout_name(settings, previous, None)
                        })
                    });
            if name_follows_selection {
                if let Some(settings) = self.application_layout_settings() {
                    self.application_picker_name =
                        automatic_application_layout_name(settings, &application, None);
                }
            }
        }
        if !self.application_picker_category_changed
            && self
                .application_picker_target_layout_id
                .as_deref()
                .is_none_or(|id| {
                    self.application_layout_settings()
                        .and_then(|settings| settings.layouts.get(id))
                        .is_some_and(|layout| {
                            layout.category.is_none() && layout.custom_category_id.is_none()
                        })
                })
        {
            self.application_picker_custom_category_id = None;
            self.application_picker_category =
                super::application_layout_runtime::application_layout_category_for_executable(
                    &application.executable,
                );
            if self.application_layout_settings().is_some_and(|settings| {
                !settings.category_exists(self.application_picker_category.id())
            }) {
                self.application_picker_category =
                    crate::application_layouts::ApplicationLayoutCategory::Other;
            }
        }
        self.application_picker_selected = Some(application);
    }

    fn application_picker_has_duplicate_rule(&self) -> bool {
        let Some(device_key) = self.application_layout_device_key() else {
            return false;
        };
        let Some(settings) = self.app_settings.application_layouts.get(&device_key) else {
            return false;
        };
        let excluding_id = self.application_picker_target_layout_id.as_deref();
        self.application_picker_selected
            .as_ref()
            .is_some_and(|application| {
                settings.application_rule_exists(application, "", excluding_id)
            })
    }

    fn draw_application_picker_v2(&mut self, ctx: &egui::Context) {
        if !self.application_picker_open {
            return;
        }
        let language = self.app_settings.language;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ctx);
        let mut geometry = application_picker_geometry(ctx.content_rect().size(), metrics.scale);
        let validation_height = metrics.value(38.0);
        geometry.list_height = (geometry.list_height - metrics.value(96.0) - validation_height)
            .max(metrics.value(80.0));
        let mut open = self.application_picker_open;
        let mut confirm = false;
        let mut cancel = false;
        let applications = crate::app_discovery::running_application_choices(
            &self.application_discovery.available,
        );
        let search = self.application_picker_search.trim().to_ascii_lowercase();
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        let picker_window_id = egui::Id::new("application_picker_v2");
        let window_center_x = ctx.content_rect().center().x;
        crate::ui_style::centered_modal_window(
            ctx,
            if self.application_picker_assign_existing {
                app_layout_text(language, "Изменить приложение", "Edit application")
            } else {
                app_layout_text(language, "Добавить приложение", "Add application")
            },
            picker_window_id,
            &mut open,
            geometry.window_size,
        )
        .frame(
            crate::ui_style::modal_window_frame(
                ctx.global_style().as_ref(),
                ctx.global_style().visuals.dark_mode,
            )
            .inner_margin(egui::Margin::symmetric(30, 10)),
        )
        .movable(false)
        .show(ctx, |ui| {
            application_picker_content_ui(
                ui,
                window_center_x,
                geometry.content_width,
                |ui, content_width| {
                    ui.add_space(metrics.value(4.0));
                    ui.label(
                        RichText::new(app_layout_text(
                            language,
                            "Показаны открытые пользовательские приложения. Список обновляется автоматически.",
                            "Open user applications are shown. The list updates automatically.",
                        ))
                        .size(metrics.value(12.0))
                        .color(app_muted_text(ui.visuals().dark_mode)),
                    );
                    {
                        use crate::application_layouts::ApplicationLayoutCategory as Category;
                        ui.add_space(metrics.value(8.0));
                        ui.label(RichText::new(app_layout_text(language, "Название раскладки", "Layout name"))
                            .size(metrics.value(12.0)).color(app_muted_text(ui.visuals().dark_mode)));
                        crate::ui_style::modern_text_field_sized(
                            ui,
                            ui.make_persistent_id("application_picker_layout_name"),
                            &mut self.application_picker_name,
                            content_width,
                            metrics.settings_control_height(),
                            app_layout_text(language, "Название раскладки", "Layout name"),
                            80,
                            egui::Align::Min,
                        ).on_hover_text(app_layout_text(language,
                            "Уникальное имя раскладки для этого приложения",
                            "A unique layout name for this application"));
                        ui.add_space(metrics.value(8.0));
                        ui.label(RichText::new(app_layout_text(language, "Категория", "Category"))
                            .size(metrics.value(12.0)).color(app_muted_text(ui.visuals().dark_mode)));
                        let categories = self.application_layout_category_choices(language);
                        let labels = categories.iter().map(|(_, label)| label.clone()).collect::<Vec<_>>();
                        let selected_id = self.application_picker_custom_category_id.as_deref()
                            .unwrap_or(self.application_picker_category.id());
                        let selected = categories.iter().position(|(id, _)| id == selected_id).unwrap_or(0);
                        let (_, picked) = crate::ui_style::modern_dropdown_select_sized(
                            ui,
                            ui.make_persistent_id("application_picker_category"),
                            &labels,
                            selected,
                            content_width,
                            metrics.settings_control_height(),
                            metrics.settings_control_font_size(),
                        );
                        if let Some(index) = picked {
                            self.application_picker_category_changed = true;
                            let id = &categories[index].0;
                            self.application_picker_custom_category_id = id.starts_with("custom:").then(|| id.clone());
                            if let Some(category) = Category::ALL.iter().find(|category| category.id() == id) {
                                self.application_picker_category = *category;
                            }
                        }
                    }
                    ui.add_space(metrics.value(10.0));
                    crate::ui_style::modern_text_field_sized(
                        ui,
                        ui.make_persistent_id("application_picker_search_v2"),
                        &mut self.application_picker_search,
                        content_width,
                        metrics.settings_control_height(),
                        app_layout_text(language, "Поиск приложения", "Search applications"),
                        120,
                        egui::Align::Min,
                    )
                    .on_hover_text(app_layout_text(
                        language,
                        "Фильтрует список запущенных приложений по названию и исполняемому файлу",
                        "Filters running applications by name and executable",
                    ));
                    ui.add_space(metrics.value(10.0));

                    let filtered = applications
                        .iter()
                        .filter(|application| {
                            search.is_empty()
                                || application.label().to_ascii_lowercase().contains(&search)
                                || application
                                    .executable
                                    .to_ascii_lowercase()
                                    .contains(&search)
                        })
                        .collect::<Vec<_>>();
                    egui::Frame::new()
                        .fill(app_surface_fill(ui.visuals().dark_mode))
                        .stroke(crate::ui_style::modal_outline_stroke(
                            ui.visuals().dark_mode,
                        ))
                        .corner_radius(metrics.value(10.0))
                        .inner_margin(metrics.value(8.0))
                        .show(ui, |ui| {
                            ui.set_width(content_width - metrics.value(16.0));
                            ui.set_height(geometry.list_height);
                            egui::ScrollArea::vertical()
                                .id_salt("application_picker_list_v2")
                                .max_height(geometry.list_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if filtered.is_empty() {
                                        crate::ui_style::modal_empty_state(
                                            ui,
                                            app_layout_text(
                                                language,
                                                "Открытые приложения не найдены. Запустите нужное приложение — оно появится автоматически.",
                                                "No open applications found. Launch an application and it will appear automatically.",
                                            ),
                                            None,
                                        );
                                    }
                                    for application in filtered {
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), metrics.value(44.0)),
                                            egui::Sense::click(),
                                        );
                                        let response = response.on_hover_text(format!(
                                            "{}\n{}: {}",
                                            app_layout_text(
                                                language,
                                                "Выбрать это приложение",
                                                "Select this application",
                                            ),
                                            app_layout_text(
                                                language,
                                                "Исполняемый файл",
                                                "Executable",
                                            ),
                                            application.executable,
                                        ));
                                        if response.hovered() {
                                            ui.painter().rect_filled(
                                                rect,
                                                metrics.value(8.0),
                                                app_hover_fill(ui.visuals().dark_mode),
                                            );
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        let selected = self
                                            .application_picker_selected
                                            .as_ref()
                                            .is_some_and(|selected| {
                                                selected.executable == application.executable
                                                    && selected.identities == application.identities
                                            });
                                        if selected {
                                            ui.painter().rect_stroke(
                                                rect.shrink(metrics.value(1.0)),
                                                metrics.value(8.0),
                                                egui::Stroke::new(
                                                    metrics.value(1.5),
                                                    egui::Color32::from_rgb(218, 164, 70),
                                                ),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        let left = rect.left() + metrics.value(12.0);
                                        ui.painter().text(
                                            egui::pos2(left, rect.center().y),
                                            egui::Align2::LEFT_CENTER,
                                            application.label(),
                                            egui::FontId::proportional(metrics.value(13.0)),
                                            ui.visuals().text_color(),
                                        );
                                        if response.clicked() {
                                            self.select_application_picker_choice((*application).clone());
                                            ui.ctx().request_repaint();
                                        }
                                    }
                                });
                        });

                    ui.add_space(metrics.value(12.0));
                    let duplicate_rule = self.application_picker_has_duplicate_rule();
                    let invalid_name = self.application_picker_selected.is_some()
                        && self.application_layout_settings().is_some_and(|settings| {
                            application_layout_name_is_invalid(
                                settings,
                                self.application_picker_target_layout_id.as_deref(),
                                &self.application_picker_name,
                            )
                        });
                    // Always allocate both validation rows so errors cannot move
                    // the modal or its action buttons.
                    for (show, ru, en) in [
                        (
                            duplicate_rule,
                            "Для этого приложения уже создана раскладка",
                            "A layout already exists for this application",
                        ),
                        (
                            invalid_name,
                            "Введите уникальное непустое имя раскладки",
                            "Enter a unique, non-empty layout name",
                        ),
                    ] {
                        ui.add_sized(
                            egui::vec2(content_width, metrics.value(16.0)),
                            egui::Label::new(
                                RichText::new(if show { app_layout_text(language, ru, en) } else { "" })
                                    .size(metrics.value(11.0))
                                    .color(egui::Color32::from_rgb(220, 92, 76)),
                            ),
                        );
                    }
                    let can_confirm = self.application_picker_selected.is_some()
                        && !duplicate_rule
                        && !invalid_name;
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        confirm = crate::ui_style::modern_button(
                            ui,
                            if self.application_picker_assign_existing {
                                app_layout_text(language, "Сохранить", "Save")
                            } else {
                                app_layout_text(language, "Добавить", "Add")
                            },
                            metrics.size(120.0, 32.0),
                            can_confirm,
                        )
                        .on_hover_text(if self.application_picker_assign_existing {
                            app_layout_text(
                                language,
                                "Сохранить приложение, название и категорию раскладки",
                                "Save the application, layout name and category",
                            )
                        } else {
                            app_layout_text(
                                language,
                                "Создать раскладку с указанным названием и категорией для выбранного приложения",
                                "Create a layout with the chosen name and category for the selected application",
                            )
                        })
                        .clicked();
                        ui.add_space(metrics.value(APPLICATION_LAYOUT_CONTROL_GAP));
                        cancel = crate::ui_style::modern_button(
                            ui,
                            app_layout_text(language, "Отмена", "Cancel"),
                            metrics.size(104.0, 32.0),
                            true,
                        )
                        .on_hover_text(app_layout_text(
                            language,
                            "Закрыть окно без изменений",
                            "Close the window without changes",
                        ))
                        .clicked();
                    });
                },
            );
        });

        if confirm {
            if let Some(application) = self.application_picker_selected.clone() {
                open = !self.apply_picker_selection(application);
            }
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            cancel = true;
        }
        if cancel {
            open = false;
        }
        if !open {
            self.close_application_picker_dialog();
        } else {
            self.application_picker_open = true;
        }
    }

    fn apply_picker_selection(
        &mut self,
        application: crate::application_layouts::DetectedApplication,
    ) -> bool {
        let Some(device_key) = self.application_layout_device_key() else {
            return false;
        };
        if self.application_picker_has_duplicate_rule() {
            return false;
        }
        let settings = self
            .app_settings
            .application_layouts
            .entry(device_key)
            .or_default();
        if let Some(target_id) = self.application_picker_target_layout_id.as_deref() {
            if application_layout_name_is_invalid(
                settings,
                Some(target_id),
                &self.application_picker_name,
            ) || !settings.layouts.contains_key(target_id)
            {
                return false;
            }
            let changed = settings.update_application_rule(
                target_id,
                &application,
                &self.application_picker_name,
                "",
            );
            let category_changed = self.application_picker_category_changed
                && if let Some(id) = self.application_picker_custom_category_id.as_deref() {
                    settings.set_layout_category_id(target_id, id)
                } else {
                    settings.set_layout_category(target_id, self.application_picker_category)
                };
            if changed || category_changed {
                save_app_settings(&self.app_settings);
            }
        } else {
            if application_layout_name_is_invalid(settings, None, &self.application_picker_name) {
                return false;
            }
            let id = settings.create_for_application_named(
                &application,
                Some(&self.application_picker_name),
                "",
            );
            if self.application_picker_category_changed {
                if let Some(category_id) = self.application_picker_custom_category_id.as_deref() {
                    settings.set_layout_category_id(&id, category_id);
                } else {
                    settings.set_layout_category(&id, self.application_picker_category);
                }
            }
            save_app_settings(&self.app_settings);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected_application(
        executable: &str,
        display_name: &str,
    ) -> crate::application_layouts::DetectedApplication {
        crate::application_layouts::DetectedApplication {
            executable: executable.to_owned(),
            identities: vec![executable.to_owned()],
            display_name: display_name.to_owned(),
            window_title: String::new(),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_relogin_feedback_does_not_name_a_distribution() {
        let mut app = EntropyApp::new_inert_for_test();
        app.gnome_integration_install_result =
            Some(Ok(crate::app_discovery::GnomeIntegrationInstallReport {
                message: "GNOME integration installed and enabled".to_owned(),
                enabled: true,
                active: false,
                restart_required: true,
            }));
        let english = app
            .gnome_integration_install_feedback(crate::i18n::Language::English, true)
            .0;
        let russian = app
            .gnome_integration_install_feedback(crate::i18n::Language::Russian, true)
            .0;
        assert_eq!(english, "Installed. Sign out and sign back in");
        assert_eq!(russian, "Установлено. Выйдите из сеанса и войдите снова");
    }

    #[test]
    fn window_detector_shows_short_status_and_keeps_backend_details() {
        use crate::app_discovery::{ForegroundState, ForegroundStatus};
        use crate::i18n::Language;

        let status = ForegroundStatus {
            backend: "GNOME Wayland / Entropy Shell (events)".to_owned(),
            state: ForegroundState::NoFocusedWindow,
        };
        let (text, details, ok) = window_detector_status(&status, Language::English);
        assert_eq!(text, "Running");
        assert_eq!(details, status.backend);
        assert!(ok);
        assert_eq!(
            window_detector_status(&status, Language::Russian).0,
            "Работает"
        );

        let unavailable = ForegroundStatus {
            backend: status.backend.clone(),
            state: ForegroundState::BackendUnavailable("Shell integration missing".to_owned()),
        };
        let (text, details, ok) = window_detector_status(&unavailable, Language::English);
        assert_eq!(text, "Unavailable");
        assert_eq!(
            details,
            "GNOME Wayland / Entropy Shell (events) — Shell integration missing"
        );
        assert!(!ok);
        assert_eq!(
            window_detector_status(&unavailable, Language::Russian).0,
            "Недоступен"
        );
    }

    #[test]
    fn status_value_text_reaches_settings_row_right_edge() {
        let ctx = egui::Context::default();
        let edge = std::cell::Cell::new(0.0);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    452.0,
                    54.0,
                    "Window detector",
                    true,
                    Some("Current detector status"),
                    260.0,
                    |ui| {
                        edge.set(ui.max_rect().right());
                        crate::ui_style::settings_value_label(ui, egui::RichText::new("Running"));
                    },
                );
            },
        );
        let text_edge = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape) if shape.galley.text() == "Running" => {
                    Some(shape.visual_bounding_rect().right())
                }
                _ => None,
            })
            .expect("status value must be painted");
        assert!(
            (edge.get() - text_edge).abs() <= 4.0,
            "value right={text_edge}, row right={}",
            edge.get()
        );
    }

    #[test]
    fn standard_page_width_fits_status_labels_and_description() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let page_width = metrics.settings_content_width();
                let row_width = metrics.settings_row_content_width();
                assert_eq!(page_width, 470.0);
                assert_eq!(row_width, 452.0);
                assert_eq!(metrics.settings_control_font_size(), 12.5);
                let font = egui::FontId::proportional(13.0);
                for description in [
                    "Настройте автоматическое переключение раскладок для приложений",
                    "Configure automatic layout switching for applications",
                ] {
                    let width = ui
                        .painter()
                        .layout_no_wrap(description.to_owned(), font.clone(), egui::Color32::WHITE)
                        .size()
                        .x;
                    assert!(width <= page_width, "description width={width}");
                }
                for label in [
                    "Приложение в фокусе",
                    "Активная раскладка",
                    "Focused application",
                ] {
                    let width = ui
                        .painter()
                        .layout_no_wrap(label.to_owned(), font.clone(), egui::Color32::WHITE)
                        .size()
                        .x;
                    assert!(width + 4.0 <= row_width - 260.0, "{label}: width={width}");
                }
            },
        );
    }

    #[test]
    fn page_layout_dropdown_shows_shared_categories_and_preserves_active_layout() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        for (executable, name) in [("firefox", "Firefox"), ("my-tool", "My Tool")] {
            settings.create_for_application_named(
                &detected_application(executable, name),
                Some(name),
                "",
            );
        }
        settings.editor_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned();
        let device_key = "offline-macropad-category-test".to_owned();
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(device_key);
        let options = app.application_layout_editor_options();
        assert_eq!(options[0].1, "Default");
        let russian_groups =
            app.application_layout_editor_labeled_groups(&options, crate::i18n::Language::Russian);
        assert_eq!(russian_groups[0].0, "Браузеры");
        assert_eq!(russian_groups.last().unwrap().0, "Другие");

        let ctx = egui::Context::default();
        let frame = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_application_layouts_settings_page(ui, ui.max_rect()),
            )
        };
        let first = frame(&mut app, vec![]);
        let selector = first
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Default" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .expect("page dropdown should display Default");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(selector),
                    egui::Event::PointerButton {
                        pos: selector,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let popup = frame(&mut app, vec![]);
        let painted = popup
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if shape.clip_rect.intersects(text.visual_bounding_rect()) =>
                {
                    Some(text.galley.text())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(painted.contains(&"Browsers"), "visible labels: {painted:?}");
        assert!(painted.contains(&"Other"), "visible labels: {painted:?}");
        assert!(
            !painted.contains(&"Firefox"),
            "submenu stays hidden until hover"
        );
        let browser_category = popup
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Browsers" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("browser category must be painted");
        frame(&mut app, vec![egui::Event::PointerMoved(browser_category)]);
        let submenu = frame(&mut app, vec![]);
        assert!(submenu.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Firefox"
                && shape.clip_rect.intersects(text.visual_bounding_rect())
        )));
        let firefox = submenu
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Firefox" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("browser submenu must contain Firefox");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(firefox),
                    egui::Event::PointerButton {
                        pos: firefox,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(
            app.application_layout_settings()
                .unwrap()
                .editor_layout()
                .unwrap()
                .name,
            "Firefox"
        );
        assert_eq!(
            app.application_layout_settings().unwrap().active_layout_id,
            "default"
        );
    }

    #[test]
    fn page_actions_stay_fixed_while_settings_rows_scroll() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let device_key = "offline-macropad-fixed-actions-test".to_owned();
        app.app_settings.application_layouts.insert(
            device_key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(device_key);
        let ctx = egui::Context::default();
        let frame = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_application_layouts_settings_page(ui, ui.max_rect()),
            )
        };
        let position = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == label
                            && clipped.clip_rect.intersects(text.visual_bounding_rect()) =>
                    {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing visible {label}"))
        };
        let before = frame(&mut app, vec![]);
        let scrollbar_shapes = before
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape)
                    if (shape.rect.right() - 685.0).abs() < 2.0
                        && (shape.rect.width() - 6.0).abs() < 1.0 =>
                {
                    Some(shape.rect)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            scrollbar_shapes.len(),
            1,
            "only the floating scrollbar handle is painted"
        );
        assert!(
            scrollbar_shapes[0].height() < 324.0,
            "a full-height track must not be painted"
        );
        let add = position(&before, "Add");
        let delete = position(&before, "Delete");
        let row = position(&before, "Window detector");
        let mut after = frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(450.0, 280.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -210.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                },
            ],
        );
        for _ in 0..6 {
            after = frame(&mut app, vec![]);
        }
        assert!(position(&after, "Window detector").y < row.y - 10.0);
        assert!(position(&after, "Application").y < position(&after, "Focused application").y);
        assert!(position(&after, "Focused application").y < position(&after, "Active layout").y);
        assert!((position(&after, "Add").y - add.y).abs() < 1.0);
        assert!((position(&after, "Delete").y - delete.y).abs() < 1.0);
    }

    #[test]
    fn clicking_custom_layout_name_opens_selector_instead_of_rename() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let device_key = "offline-macropad-layout-name-test".to_owned();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let firefox = settings.create_for_application(&detected_application("firefox", "Firefox"));
        settings.editor_layout_id = firefox;
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(device_key);
        let ctx = egui::Context::default();
        let frame = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_application_layouts_settings_page(ui, ui.max_rect()),
            )
        };
        let output = frame(&mut app, vec![]);
        let name = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text() == "Firefox"
                        && shape.clip_rect.intersects(text.visual_bounding_rect()) =>
                {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("selected name is painted in dropdown");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(name),
                    egui::Event::PointerButton {
                        pos: name,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let popup = frame(&mut app, vec![]);
        assert!(popup.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Browsers"
                && shape.clip_rect.intersects(text.visual_bounding_rect()))));
    }

    #[test]
    fn edit_application_dialog_renders_name_and_category_controls() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let device_key = "offline-macropad-edit-dialog-test".to_owned();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let telegram =
            settings.create_for_application(&detected_application("telegram", "Telegram"));
        settings.editor_layout_id = telegram;
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(device_key);
        app.open_application_picker(true);

        let ctx = egui::Context::default();
        let mut painted = Vec::new();
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_application_picker_v2(ui.ctx()),
            );
            painted = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if shape.clip_rect.intersects(text.visual_bounding_rect()) =>
                    {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
        }
        assert!(
            painted.iter().any(|text| text == "Layout name"),
            "{painted:?}"
        );
        assert!(painted.iter().any(|text| text == "Category"), "{painted:?}");
        assert!(painted.iter().any(|text| text == "Other"), "{painted:?}");
        assert!(painted.iter().any(|text| text == "Save"), "{painted:?}");
        let window = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("application_picker_v2")))
            .expect("Edit application window must be placed");
        assert!(window.bottom() <= 700.0, "window {window:?}");
        let mut small_output = None;
        for _ in 0..3 {
            small_output = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_application_picker_v2(ui.ctx()),
            ));
        }
        let small_output = small_output.unwrap();
        let save = small_output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text() == "Save"
                        && shape.clip_rect.intersects(text.visual_bounding_rect()) =>
                {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("Save remains visible at 480×480");
        assert!(save.bottom() < 480.0, "Save {save:?}");
    }

    #[test]
    fn application_picker_validation_does_not_resize_modal() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let key = "offline-macropad-modal-error-height-test".to_owned();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        settings.create_for_application(&detected_application("firefox", "Firefox"));
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(key);
        app.open_application_picker(false);
        let ctx = egui::Context::default();
        let frame = |app: &mut EntropyApp| {
            let mut output = None;
            for _ in 0..3 {
                output = Some(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.draw_application_picker_v2(ui.ctx()),
                ));
            }
            let rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::new("application_picker_v2")))
                .expect("application picker remains open");
            let output = output.unwrap();
            let footer_y = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Cancel" => {
                        Some(text.visual_bounding_rect().center().y)
                    }
                    _ => None,
                })
                .expect("footer remains visible");
            (rect, footer_y)
        };
        let clean = frame(&mut app);
        app.select_application_picker_choice(detected_application("firefox", "Firefox"));
        let duplicate = frame(&mut app);
        app.select_application_picker_choice(detected_application("chrome", "Chrome"));
        app.application_picker_name = "   ".to_owned();
        let invalid_name = frame(&mut app);
        app.select_application_picker_choice(detected_application("firefox", "Firefox"));
        app.application_picker_name = "   ".to_owned();
        let both = frame(&mut app);
        for (label, (rect, footer_y)) in [
            ("duplicate", duplicate),
            ("invalid name", invalid_name),
            ("both", both),
        ] {
            assert!(
                (rect.top() - clean.0.top()).abs() <= 1.0,
                "{label} moved modal top: {rect:?} vs {:?}",
                clean.0
            );
            assert!(
                (rect.bottom() - clean.0.bottom()).abs() <= 1.0,
                "{label} resized modal: {rect:?} vs {:?}",
                clean.0
            );
            assert!(
                (footer_y - clean.1).abs() <= 1.0,
                "{label} moved footer: {footer_y} vs {}",
                clean.1
            );
        }

        app.open_application_picker(true);
        let edit_clean = frame(&mut app);
        app.application_picker_name = "Default".to_owned();
        let edit_invalid = frame(&mut app);
        assert!((edit_clean.0.top() - edit_invalid.0.top()).abs() <= 1.0);
        assert!((edit_clean.0.bottom() - edit_invalid.0.bottom()).abs() <= 1.0);
        assert!((edit_clean.1 - edit_invalid.1).abs() <= 1.0);
    }

    #[test]
    fn escape_cancels_application_picker_without_saving_drafts() {
        for edit in [false, true] {
            let mut app = EntropyApp::new_inert_for_test();
            app.app_settings.language = crate::i18n::Language::English;
            let key = "offline-macropad-escape-picker-test".to_owned();
            let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
            settings.create_for_application(&detected_application("firefox", "Firefox"));
            app.app_settings
                .application_layouts
                .insert(key.clone(), settings);
            app.app_settings.last_application_layout_device_key = Some(key.clone());
            app.open_application_picker(edit);
            app.application_picker_name = "Unsaved name".to_owned();
            let ctx = egui::Context::default();
            for _ in 0..3 {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    app.draw_application_picker_v2(ui.ctx());
                });
            }
            let _ = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| {
                    app.draw_application_picker_v2(ui.ctx());
                    assert!(!ui.input(|input| input.key_pressed(egui::Key::Escape)));
                },
            );
            assert!(!app.application_picker_open);
            assert!(app.application_picker_selected.is_none());
            assert!(app.application_picker_name.is_empty());
            let saved = &app.app_settings.application_layouts[&key];
            assert!(!saved
                .layouts
                .values()
                .any(|layout| layout.name == "Unsaved name"));
        }
    }

    #[test]
    fn category_manager_row_follows_application_and_precedes_runtime_status() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let key = "offline-macropad-category-row-test".to_owned();
        app.app_settings.application_layouts.insert(
            key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(key);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.draw_application_layouts_editor(
                    ui,
                    crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx()),
                    0..app.application_layouts_editor_row_count(),
                );
            },
        );
        let label_y = |label| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.visual_bounding_rect().center().y)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing row: {label}"))
        };
        assert!(label_y("Application") < label_y("Categories"));
        assert!(label_y("Categories") < label_y("Focused application"));
    }

    #[test]
    fn category_manager_shows_crud_controls_and_escape_closes_it() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let key = "offline-macropad-category-manager".to_owned();
        app.app_settings.application_layouts.insert(
            key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(key);
        app.application_categories_open = true;
        app.application_categories_selected_id = Some("browsers".to_owned());
        app.application_categories_name = "Browsers".to_owned();
        let ctx = egui::Context::default();
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_application_categories_modal(ui.ctx()),
            ));
        }
        let output = output.unwrap();
        for label in ["Categories", "Category", "Add", "Delete"] {
            assert!(
                output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == label
                    && text.visual_bounding_rect().bottom() < 480.0)),
                "missing {label}"
            );
        }
        for redundant in ["Rename", "Close", "Create"] {
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == redundant)));
        }
        let window = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("autolayer_categories_modal")))
            .expect("category manager window");
        assert!(
            window.left() >= 0.0 && window.right() <= 480.0,
            "{window:?}"
        );
        assert!(
            window.top() >= 0.0 && window.bottom() <= 480.0,
            "{window:?}"
        );
        assert!(
            window.width() <= 400.0 && window.height() <= 240.0,
            "{window:?}"
        );
        let button_label_center = |shapes: &[egui::epaint::ClippedShape], label: &str| {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing button {label}"))
        };
        let add_center = button_label_center(&output.shapes, "Add");
        let delete_center = button_label_center(&output.shapes, "Delete");
        assert!(
            ((add_center.x + delete_center.x) / 2.0 - window.center().x).abs() < 2.0,
            "Add {add_center:?}, Delete {delete_center:?}, window {window:?}"
        );
        let add_position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Add" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("Add button text");
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 480.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(add_position),
                        egui::Event::PointerButton {
                            pos: add_position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.draw_application_categories_modal(ui.ctx()),
            );
        }
        assert_eq!(app.application_categories_selected_id, None);
        assert!(app.application_categories_name.is_empty());
        app.application_categories_name = "Projects".to_owned();
        let create_output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw_application_categories_modal(ui.ctx());
        });
        assert!(create_output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Create")));
        let create_center = button_label_center(&create_output.shapes, "Create");
        let delete_center = button_label_center(&create_output.shapes, "Delete");
        assert!(
            ((create_center.x + delete_center.x) / 2.0 - window.center().x).abs() < 2.0,
            "Create {create_center:?}, Delete {delete_center:?}, window {window:?}"
        );
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.draw_application_categories_modal(ui.ctx()),
        );
        assert!(!app.application_categories_open);
    }

    #[test]
    fn add_application_dialog_shows_name_category_and_add_on_small_screen() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        let device_key = "offline-macropad-add-dialog-test".to_owned();
        app.app_settings.application_layouts.insert(
            device_key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(device_key);
        app.open_application_picker(false);

        let ctx = egui::Context::default();
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_application_picker_v2(ui.ctx()),
            ));
        }
        let output = output.unwrap();
        let painted = |label: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text() == label
                        && shape.clip_rect.intersects(text.visual_bounding_rect()) =>
                {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
        };
        assert!(painted("Layout name").is_some());
        assert!(painted("Category").is_some());
        assert!(painted("Other").is_some());
        let add = painted("Add").expect("Add must remain visible at 480×480");
        assert!(add.bottom() < 480.0, "Add {add:?}");
        let baseline_rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("application_picker_v2")))
            .unwrap();

        app.select_application_picker_choice(detected_application("firefox", "Firefox"));
        app.application_picker_name = "Default".to_owned();
        let mut invalid = None;
        for _ in 0..3 {
            invalid = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_application_picker_v2(ui.ctx()),
            ));
        }
        let invalid = invalid.unwrap();
        let invalid_rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("application_picker_v2")))
            .unwrap();
        assert!((invalid_rect.top() - baseline_rect.top()).abs() <= 1.0);
        assert!((invalid_rect.bottom() - baseline_rect.bottom()).abs() <= 1.0);
        assert!(invalid.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Enter a unique, non-empty layout name"
                && shape.clip_rect.intersects(text.visual_bounding_rect()))));
        assert!(invalid.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Add"
                && shape.clip_rect.intersects(text.visual_bounding_rect()))));
    }

    #[test]
    fn add_application_suggests_and_saves_name_and_category() {
        use crate::application_layouts::{
            ApplicationLayoutCategory as Category, DeviceApplicationLayouts,
        };
        let mut app = EntropyApp::new_inert_for_test();
        let key = "offline-macropad-add-category-test".to_owned();
        let mut settings = DeviceApplicationLayouts::default();
        settings.create_for_application_named(
            &detected_application("settings-tool", "Settings"),
            Some("Settings"),
            "",
        );
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        app.open_application_picker(false);
        assert!(app.application_picker_name.is_empty());
        let first = detected_application("first-tool", "Settings");
        app.select_application_picker_choice(first);
        assert_eq!(app.application_picker_name, "Settings (2)");
        app.select_application_picker_choice(detected_application("firefox", "Firefox"));
        assert_eq!(app.application_picker_name, "Firefox");
        assert_eq!(app.application_picker_category, Category::Browsers);
        assert!(!app.application_picker_category_changed);
        app.application_picker_name = "Research".to_owned();
        app.select_application_picker_choice(detected_application("chrome", "Chrome"));
        assert_eq!(
            app.application_picker_name, "Research",
            "manual draft survives selection"
        );
        app.application_picker_name = " Settings ".to_owned();
        assert!(!app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
        assert_eq!(app.app_settings.application_layouts[&key].layouts.len(), 2);
        app.application_picker_name = "  ".to_owned();
        assert!(!app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
        app.application_picker_name = "  Research  ".to_owned();
        app.application_picker_category = Category::Development;
        app.application_picker_category_changed = true;
        assert!(app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
        let settings = &app.app_settings.application_layouts[&key];
        let new = settings.editor_layout().unwrap();
        assert_eq!(new.name, "Research");
        assert_eq!(new.executable, "chrome");
        assert_eq!(new.category, Some(Category::Development));
        let groups =
            app.application_layout_editor_grouped_options(&app.application_layout_editor_options());
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Development.id()
                && entries
                    .iter()
                    .any(|(id, name)| id == &new.id && name == "Research")));
    }

    #[test]
    fn add_application_auto_category_does_not_persist_override() {
        use crate::application_layouts::{
            ApplicationLayoutCategory as Category, DeviceApplicationLayouts,
        };
        let mut app = EntropyApp::new_inert_for_test();
        let key = "offline-macropad-add-auto-category-test".to_owned();
        app.app_settings
            .application_layouts
            .insert(key.clone(), DeviceApplicationLayouts::default());
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        app.open_application_picker(false);
        app.select_application_picker_choice(detected_application("firefox", "Firefox"));
        assert_eq!(app.application_picker_category, Category::Browsers);
        assert!(!app.application_picker_category_changed);
        assert!(app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
        let layout = app.app_settings.application_layouts[&key]
            .editor_layout()
            .unwrap();
        assert_eq!(layout.name, "Firefox");
        assert_eq!(layout.category, None);
    }

    #[test]
    fn choosing_new_application_suggests_name_and_auto_category() {
        use crate::application_layouts::ApplicationLayoutCategory as Category;
        let mut app = EntropyApp::new_inert_for_test();
        let key = "offline-macropad-rebind-test".to_owned();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let id = settings.create_for_application(&detected_application("telegram", "Telegram"));
        settings.editor_layout_id = id.clone();
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        app.open_application_picker(true);
        app.select_application_picker_choice(detected_application("firefox", "Firefox"));
        assert_eq!(app.application_picker_name, "Firefox");
        assert_eq!(app.application_picker_category, Category::Browsers);
        assert!(!app.application_picker_category_changed);
        let selected = app.application_picker_selected.clone().unwrap();
        assert!(app.apply_picker_selection(selected));
        let saved = &app.app_settings.application_layouts[&key].layouts[&id];
        assert_eq!(saved.name, "Firefox");
        assert_eq!(saved.executable, "firefox");
        assert_eq!(
            saved.category, None,
            "automatic preset grouping is not overridden"
        );
    }

    #[test]
    fn edit_application_saves_rename_and_category_without_changing_identity() {
        use crate::application_layouts::{
            ApplicationLayoutCategory as Category, DeviceApplicationLayouts,
        };
        let mut app = EntropyApp::new_inert_for_test();
        let device_key = "offline-macropad-edit-test".to_owned();
        let mut settings = DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application_named(
            &detected_application("telegram-desktop", "Telegram"),
            Some("Telegram"),
            "",
        );
        settings.create_for_application_named(
            &detected_application("org.blender.Blender", "Blender"),
            Some("Blender"),
            "",
        );
        settings.editor_layout_id = telegram.clone();
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(device_key.clone());

        app.open_application_picker(true);
        assert_eq!(app.application_picker_name, "Telegram");
        assert_eq!(app.application_picker_category, Category::Other);
        app.application_picker_name = "Blender".to_owned();
        let original = app.application_picker_selected.clone().unwrap();
        app.application_picker_category = Category::Browsers;
        app.application_picker_category_changed = true;
        assert!(
            !app.apply_picker_selection(original.clone()),
            "duplicate name must not save"
        );
        assert_eq!(
            app.app_settings.application_layouts[&device_key].layouts[&telegram].name,
            "Telegram"
        );

        app.application_picker_name = "Work chat".to_owned();
        assert!(app.apply_picker_selection(original));
        let settings = &app.app_settings.application_layouts[&device_key];
        let saved = &settings.layouts[&telegram];
        assert_eq!(saved.name, "Work chat");
        assert_eq!(saved.executable, "telegram-desktop");
        assert_eq!(saved.category, Some(Category::Browsers));
        assert_eq!(settings.active_layout_id, "default");
        let groups =
            app.application_layout_editor_grouped_options(&app.application_layout_editor_options());
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Browsers.id()
                && entries
                    .iter()
                    .any(|(id, name)| id == &telegram && name == "Work chat")));
    }

    #[test]
    fn edit_application_rejects_empty_and_duplicate_names() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application_named(
            &detected_application("telegram-desktop", "Telegram"),
            Some("Telegram"),
            "",
        );
        settings.create_for_application_named(
            &detected_application("org.blender.Blender", "Blender"),
            Some("Blender"),
            "",
        );

        assert!(application_layout_name_is_invalid(
            &settings,
            Some(&telegram),
            "  "
        ));
        assert!(application_layout_name_is_invalid(
            &settings,
            Some(&telegram),
            " blender "
        ));
        assert!(!application_layout_name_is_invalid(
            &settings,
            Some(&telegram),
            "Telegram"
        ));
        assert!(!application_layout_name_is_invalid(
            &settings,
            Some(&telegram),
            "Telegram — работа"
        ));
    }

    #[test]
    fn automatic_layout_names_are_unique_for_new_applications() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let first = crate::application_layouts::DetectedApplication {
            executable: "first-settings".to_owned(),
            identities: vec!["first-settings".to_owned()],
            display_name: "Settings".to_owned(),
            window_title: String::new(),
        };
        settings.create_for_application_named(&first, Some("Settings"), "");
        let second = crate::application_layouts::DetectedApplication {
            executable: "second-settings".to_owned(),
            identities: vec!["second-settings".to_owned()],
            display_name: "Settings".to_owned(),
            window_title: String::new(),
        };

        assert_eq!(
            automatic_application_layout_name(&settings, &second, None),
            "Settings (2)"
        );
    }

    #[test]
    fn bottom_layout_actions_are_centered_with_the_standard_gap() {
        let scale = 1.12;
        let container = egui::Rect::from_min_size(
            egui::pos2(20.0, 10.0),
            egui::vec2(602.0 * scale, 34.0 * scale),
        );
        let button_size = egui::vec2(126.0 * scale, 34.0 * scale);
        let gap = 10.0 * scale;
        let actions = centered_button_pair_geometry(container, button_size, gap);

        assert!((actions.trailing.left() - actions.leading.right() - gap).abs() <= 0.01);
        assert!((actions.leading.width() - button_size.x).abs() <= 0.01);
        assert!((actions.trailing.width() - button_size.x).abs() <= 0.01);
        assert!((actions.leading.center().y - container.center().y).abs() <= 0.01);
        assert!((actions.trailing.center().y - container.center().y).abs() <= 0.01);
        assert!(
            (actions.leading.left()
                - container.left()
                - (container.right() - actions.trailing.right()))
            .abs()
                <= 0.01
        );
    }

    #[test]
    fn default_application_text_fits_beside_edit_in_russian() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                ..Default::default()
            },
            |ui| {
                for scale in [1.0, 1.12] {
                    let row_width = 452.0 * scale;
                    let control_width = APPLICATION_LAYOUT_APPLICATION_CONTROL_WIDTH * scale;
                    let pair = control_pair_geometry(
                        egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(control_width, 32.0 * scale),
                        ),
                        82.0 * scale,
                        APPLICATION_LAYOUT_CONTROL_GAP * scale,
                        32.0 * scale,
                    );
                    let text_width = ui
                        .painter()
                        .layout_no_wrap(
                            "Для остальных приложений".to_owned(),
                            egui::FontId::proportional(12.5),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let label_width = ui
                        .painter()
                        .layout_no_wrap(
                            "Приложение".to_owned(),
                            egui::FontId::proportional(13.0 * scale),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    assert!(
                        text_width + 20.0 <= pair.leading.width(),
                        "scale={scale}: text={text_width}, input={}",
                        pair.leading.width()
                    );
                    assert!(
                        label_width + 12.0 <= row_width - control_width,
                        "scale={scale}: label={label_width}, available={}",
                        row_width - control_width
                    );
                    assert!((pair.trailing.right() - control_width).abs() < 0.01);
                }
            },
        );
    }

    #[test]
    fn application_control_rows_use_exact_gap_and_right_edge_at_scaled_ui() {
        let scale = 1.12;
        let content = egui::Rect::from_min_size(
            egui::pos2(20.0, 40.0),
            egui::vec2(620.0 * scale, 32.0 * scale),
        );
        let gap = APPLICATION_LAYOUT_CONTROL_GAP * scale;
        let trailing_width = 104.0 * scale;

        let application = control_pair_geometry(content, trailing_width, gap, content.height());

        for pair in [application] {
            assert!((pair.trailing.left() - pair.leading.right() - gap).abs() <= 0.01);
            assert!((pair.leading.left() - content.left()).abs() <= 0.01);
            assert!((pair.trailing.right() - content.right()).abs() <= 0.01);
        }
    }

    #[test]
    fn application_picker_fits_reference_viewport_without_touching_edges() {
        let geometry = application_picker_geometry(egui::vec2(700.0, 720.0), 1.0);

        assert_eq!(geometry.window_size, egui::vec2(652.0, 460.0));
        assert_eq!(geometry.content_width, 592.0);
        assert_eq!(geometry.list_height, 280.0);
    }

    #[test]
    fn application_picker_shrinks_list_before_clipping_window() {
        let geometry = application_picker_geometry(egui::vec2(480.0, 480.0), 1.0);

        assert_eq!(geometry.window_size, egui::vec2(432.0, 432.0));
        assert_eq!(geometry.content_width, 372.0);
        assert_eq!(geometry.list_height, 262.0);
    }

    #[test]
    fn application_picker_caps_large_desktop_size() {
        let geometry = application_picker_geometry(egui::vec2(1_920.0, 1_080.0), 1.0);

        assert_eq!(geometry.window_size, egui::vec2(680.0, 460.0));
        assert_eq!(geometry.content_width, 620.0);
        assert_eq!(geometry.list_height, 280.0);
    }

    #[test]
    fn application_picker_content_has_equal_side_margins() {
        let available = egui::Rect::from_min_max(egui::pos2(10.0, 30.0), egui::pos2(662.0, 510.0));
        let content = centered_application_picker_content_rect(available, 592.0);

        assert_eq!(content.width(), 592.0);
        assert_eq!(content.left() - available.left(), 30.0);
        assert_eq!(available.right() - content.right(), 30.0);
    }

    #[test]
    fn application_picker_content_clamps_symmetrically_on_narrow_width() {
        let available = egui::Rect::from_min_max(egui::pos2(10.0, 30.0), egui::pos2(310.0, 510.0));
        let content = centered_application_picker_content_rect(available, 592.0);

        assert_eq!(content, available);
    }

    #[test]
    fn application_picker_real_window_places_widgets_symmetrically() {
        let ctx = egui::Context::default();
        let measured = std::cell::Cell::new(None);
        let mut open = true;

        for _ in 0..3 {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(726.0, 620.0),
            ));
            let _ = ctx.run_ui(input, |_ui| {
                let id = egui::Id::new("application_picker_real_window_test");
                let response = crate::ui_style::centered_modal_window(
                    &ctx,
                    "Add application",
                    id,
                    &mut open,
                    egui::vec2(652.0, 460.0),
                )
                .frame(
                    crate::ui_style::modal_window_frame(
                        ctx.global_style().as_ref(),
                        ctx.global_style().visuals.dark_mode,
                    )
                    .inner_margin(egui::Margin::symmetric(30, 10)),
                )
                .movable(false)
                .show(&ctx, |ui| {
                    application_picker_content_ui(
                        ui,
                        ctx.content_rect().center().x,
                        592.0,
                        |ui, width| {
                            let (field, _) = ui
                                .allocate_exact_size(egui::vec2(width, 32.0), egui::Sense::hover());
                            measured.set(Some(field));
                        },
                    );
                });
                let _ = response;
            });
        }

        let window = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("application_picker_real_window_test")))
            .expect("modal window should render");
        let field = measured.get().expect("modal content should render");
        let left = field.left() - window.left();
        let right = window.right() - field.right();
        assert!(
            (left - right).abs() <= 0.5,
            "left={left}, right={right}, window={window:?}"
        );
        assert!((left - 30.0).abs() <= 0.5, "left={left}");
    }
}
