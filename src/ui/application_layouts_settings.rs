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
const APPLICATION_LAYOUT_DROPDOWN_ARROW_WIDTH: f32 = 32.0;

fn application_layout_name_area_contains(
    rect: egui::Rect,
    pointer: egui::Pos2,
    arrow_width: f32,
) -> bool {
    rect.contains(pointer) && pointer.x < rect.right() - arrow_width
}

fn application_layout_name_is_invalid(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    target_id: &str,
    value: &str,
) -> bool {
    let value = value.trim();
    value.is_empty() || settings.layout_name_exists(value, Some(target_id))
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
    pub(super) fn draw_application_layouts_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        #[cfg(target_os = "linux")]
        self.poll_gnome_integration_install(ui.ctx());

        let language = self.app_settings.language;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let content_width = metrics.value(620.0);
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
            app_layout_text(language, "Раскладки приложений", "Application layouts"),
            egui::FontId::proportional(metrics.value(18.0)),
            ui.visuals().text_color(),
        );
        ui.painter().text(
            egui::pos2(center_x, description_y),
            egui::Align2::CENTER_CENTER,
            app_layout_text(
                language,
                "Автоматически меняйте клавиши и энкодер для приложения в фокусе",
                "Automatically switch keys and encoder for the focused application",
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
            crate::ui_style::allocate_ui_at_rect(ui, body_rect, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("application_layouts_settings")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        crate::ui_style::modal_content(
                            ui,
                            crate::ui_style::ModalLayout::new(content_width)
                                .with_top_padding(metrics.value(4.0)),
                            |ui| self.draw_application_layouts_editor(ui, metrics),
                        );
                    });
            });
        });
        self.draw_application_picker_v2(ui.ctx());
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

    fn draw_application_layouts_editor(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
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
        let mut layouts = snapshot
            .layouts
            .values()
            .map(|layout| (layout.id.clone(), layout.name.clone()))
            .collect::<Vec<_>>();
        layouts.sort_by(|left, right| {
            let left_default = left.0 == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
            let right_default =
                right.0 == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
            right_default
                .cmp(&left_default)
                .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
        });
        let selected_name = snapshot
            .editor_layout()
            .map(|layout| layout.name.clone())
            .unwrap_or_else(|| "Default".to_owned());
        let mut automatically_return_to_default = snapshot.automatically_return_to_default;
        let mut automatic_switching_enabled = snapshot.automatic_switching_enabled;

        let row_width = metrics.value(602.0);
        let row_height = metrics.settings_row_height();
        let control_width = metrics.value(260.0);
        let control_height = metrics.settings_control_height();
        let control_font = metrics.settings_control_font_size();
        ui.spacing_mut().item_spacing.y = 0.0;

        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(language, "Автопереключение", "Automatic switching"),
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
        let mut changed = false;
        let mut deleted_layout = false;
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

        self.draw_application_layouts_global_status(
            ui,
            &device_key,
            language,
            row_width,
            row_height,
            control_width,
            control_height,
            control_font,
        );

        if self
            .application_layout_rename_target_id
            .as_deref()
            .is_some_and(|target_id| target_id != selected_id)
        {
            self.commit_pending_application_layout_rename();
        }
        let editing_selected_name =
            self.application_layout_rename_target_id.as_deref() == Some(selected_id.as_str());
        let rename_invalid = editing_selected_name
            && self
                .app_settings
                .application_layouts
                .get(&device_key)
                .is_some_and(|settings| {
                    application_layout_name_is_invalid(
                        settings,
                        &selected_id,
                        &self.application_layout_rename_value,
                    )
                });
        let mut submit_rename = false;
        let mut cancel_rename = false;

        let layout_selector_tooltip = app_layout_text(
            language,
            "Нажмите стрелку справа, чтобы выбрать раскладку. Нажмите на имя пользовательской раскладки, чтобы переименовать её",
            "Click the arrow on the right to select a layout. Click a custom layout name to rename it",
        );
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(language, "Раскладка", "Layout"),
            true,
            Some(layout_selector_tooltip),
            control_width,
            |ui| {
                if editing_selected_name {
                    let edit_id = ui.make_persistent_id((
                        "application_layout_inline_rename",
                        selected_id.as_str(),
                    ));
                    let response = crate::ui_style::allocate_ui_at_rect(
                        ui,
                        egui::Rect::from_center_size(
                            ui.max_rect().center(),
                            egui::vec2(control_width, control_height),
                        ),
                        |ui| {
                            ui.scope(|ui| {
                                if rename_invalid {
                                    ui.visuals_mut().override_text_color =
                                        Some(egui::Color32::from_rgb(220, 92, 76));
                                }
                                crate::ui_style::modern_text_field_sized(
                                    ui,
                                    edit_id,
                                    &mut self.application_layout_rename_value,
                                    control_width,
                                    control_height,
                                    app_layout_text(language, "Раскладка", "Layout"),
                                    80,
                                    egui::Align::Min,
                                )
                            })
                            .inner
                        },
                    )
                    .inner;
                    if self.application_layout_rename_focus_requested {
                        response.request_focus();
                        self.application_layout_rename_focus_requested = false;
                    }
                    if response.has_focus() {
                        submit_rename = ui.input(|input| input.key_pressed(egui::Key::Enter));
                        cancel_rename = ui.input(|input| input.key_pressed(egui::Key::Escape));
                        if cancel_rename {
                            ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            });
                        }
                    }
                    submit_rename |= response.lost_focus();
                    submit_rename |= ui.input(|input| input.viewport().focused == Some(false));
                    response.on_hover_text(app_layout_text(
                        language,
                        "Введите уникальное имя. Enter или потеря фокуса сохраняет, Esc отменяет. Пустое или повторяющееся имя подсвечивается красным",
                        "Enter a unique name. Enter or losing focus saves; Esc cancels. An empty or duplicate name is highlighted in red",
                    ));
                    return;
                }

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
                crate::ui_style::popup_below_widget(
                    ui,
                    dropdown_id,
                    &dropdown,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(control_width);
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                        for (id, name) in &layouts {
                            if ui.selectable_label(selected_id == *id, name).clicked() {
                                selected_id = id.clone();
                                egui::Popup::close_id(ui.ctx(), dropdown_id);
                            }
                        }
                    },
                );
                let can_rename =
                    selected_id != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
                let clicked_name = dropdown.clicked()
                    && can_rename
                    && dropdown.interact_pointer_pos().is_some_and(|pointer| {
                        application_layout_name_area_contains(
                            dropdown.rect,
                            pointer,
                            metrics.value(APPLICATION_LAYOUT_DROPDOWN_ARROW_WIDTH),
                        )
                    });
                if clicked_name {
                    egui::Popup::close_id(ui.ctx(), dropdown_id);
                    self.open_application_layout_rename(&selected_id, &selected_name);
                }
                dropdown.on_hover_text(layout_selector_tooltip);
            },
        );

        if cancel_rename {
            self.close_application_layout_rename();
        } else if submit_rename {
            self.commit_pending_application_layout_rename();
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
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                row_width,
                row_height,
                app_layout_text(language, "Приложение", "Application"),
                !is_default,
                Some(application_tooltip),
                control_width,
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

            ui.add_space(metrics.value(20.0));
            let action_size = metrics.size(126.0, 34.0);
            let action_gap = metrics.value(10.0);
            let (action_bar_rect, _) =
                ui.allocate_exact_size(egui::vec2(row_width, action_size.y), egui::Sense::hover());
            let actions = centered_button_pair_geometry(action_bar_rect, action_size, action_gap);
            let add_response = crate::ui_style::allocate_ui_at_rect(ui, actions.leading, |ui| {
                crate::ui_style::modern_button(
                    ui,
                    app_layout_text(language, "Добавить…", "Add…"),
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
            let delete_response =
                crate::ui_style::allocate_ui_at_rect(ui, actions.trailing, |ui| {
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
            if delete_response.clicked() {
                if let Some(settings) = self.app_settings.application_layouts.get_mut(&device_key) {
                    deleted_layout = settings.remove(&selected.id);
                    changed |= deleted_layout;
                }
            }
        }

        if deleted_layout {
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
        }

        if changed {
            save_app_settings(&self.app_settings);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_application_layouts_global_status(
        &mut self,
        ui: &mut egui::Ui,
        device_key: &str,
        language: crate::i18n::Language,
        row_width: f32,
        row_height: f32,
        control_width: f32,
        control_height: f32,
        control_font: f32,
    ) {
        let detector = self.application_discovery.foreground_status.clone();
        let (detector_text, detector_ok) = match &detector.state {
            crate::app_discovery::ForegroundState::BackendUnavailable(error) => {
                (format!("{} — {error}", detector.backend), false)
            }
            crate::app_discovery::ForegroundState::Focused(_)
            | crate::app_discovery::ForegroundState::UnidentifiedWindow(_)
            | crate::app_discovery::ForegroundState::NoFocusedWindow => (
                format!(
                    "{} — {}",
                    detector.backend,
                    app_layout_text(language, "работает", "running")
                ),
                true,
            ),
        };
        let detector_tooltip = app_layout_text(
            language,
            "Показывает механизм, через который Entropy определяет активное окно. Наведите на значение, чтобы увидеть полный статус",
            "Shows the mechanism Entropy uses to detect the active window. Hover the value to see the full status",
        );
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(language, "Детектор окон", "Window detector"),
            true,
            Some(detector_tooltip),
            control_width,
            |ui| {
                ui.add_sized(
                    [control_width, control_height],
                    egui::Label::new(RichText::new(&detector_text).size(control_font).color(
                        if detector_ok {
                            app_muted_text(ui.visuals().dark_mode)
                        } else {
                            egui::Color32::from_rgb(220, 92, 76)
                        },
                    ))
                    .truncate(),
                );
            },
        );

        #[cfg(target_os = "linux")]
        let show_gnome_integration = crate::app_discovery::gnome_shell_integration_needed()
            && (matches!(
                detector.state,
                crate::app_discovery::ForegroundState::BackendUnavailable(_)
            ) || self.gnome_integration_install_task.is_some()
                || self.gnome_integration_install_result.is_some());
        #[cfg(not(target_os = "linux"))]
        let show_gnome_integration = false;

        if show_gnome_integration {
            let integration_tooltip = app_layout_text(
                language,
                "Устанавливает включённую в Entropy интеграцию GNOME для определения активного окна в Wayland",
                "Installs the GNOME integration bundled with Entropy to detect the active window on Wayland",
            );
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
                        app_layout_text(language, "Установить и включить", "Install and enable"),
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

            #[cfg(target_os = "linux")]
            if self.gnome_integration_install_task.is_some()
                || self.gnome_integration_install_result.is_some()
            {
                let (message, color) =
                    self.gnome_integration_install_feedback(language, ui.visuals().dark_mode);
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
        }

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
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(language, "Приложение в фокусе", "Focused application"),
            true,
            Some(foreground_tooltip),
            control_width,
            |ui| {
                ui.add_sized(
                    [control_width, control_height],
                    egui::Label::new(
                        RichText::new(&foreground)
                            .size(control_font)
                            .color(app_muted_text(ui.visuals().dark_mode)),
                    )
                    .truncate(),
                );
            },
        );

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
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row_width,
            row_height,
            app_layout_text(language, "Активная раскладка", "Active layout"),
            true,
            Some(active_layout_tooltip),
            control_width,
            |ui| {
                ui.add_sized(
                    [control_width, control_height],
                    egui::Label::new(
                        RichText::new(&active_layout)
                            .size(control_font)
                            .color(app_muted_text(ui.visuals().dark_mode)),
                    )
                    .truncate(),
                );
            },
        );
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
                    "Установлено. Выйдите из Ubuntu и войдите снова",
                    "Installed. Sign out of Ubuntu and sign back in",
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

    fn open_application_layout_rename(&mut self, id: &str, current_name: &str) {
        if id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID {
            return;
        }
        self.application_layout_rename_target_id = Some(id.to_owned());
        self.application_layout_rename_value = current_name.to_owned();
        self.application_layout_rename_focus_requested = true;
    }

    pub(super) fn close_application_layout_rename(&mut self) {
        self.application_layout_rename_focus_requested = false;
        self.application_layout_rename_target_id = None;
        self.application_layout_rename_value.clear();
    }

    pub(super) fn commit_pending_application_layout_rename(&mut self) -> bool {
        let Some(target_id) = self.application_layout_rename_target_id.clone() else {
            return true;
        };
        let Some(device_key) = self.application_layout_device_key() else {
            return false;
        };
        let name = self.application_layout_rename_value.trim().to_owned();
        let valid = self
            .app_settings
            .application_layouts
            .get(&device_key)
            .is_some_and(|settings| {
                settings.layouts.contains_key(&target_id)
                    && !application_layout_name_is_invalid(settings, &target_id, &name)
            });
        if !valid {
            self.application_layout_rename_focus_requested = true;
            return false;
        }

        let changed = self
            .app_settings
            .application_layouts
            .get_mut(&device_key)
            .is_some_and(|settings| settings.rename_layout(&target_id, &name));
        if changed {
            save_app_settings(&self.app_settings);
        }
        self.close_application_layout_rename();
        true
    }

    fn open_application_picker(&mut self, assign_existing: bool) {
        self.application_picker_assign_existing = assign_existing;
        self.application_picker_target_layout_id = None;
        self.application_picker_open = true;
        self.application_picker_search.clear();
        self.application_picker_selected = None;

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
        let geometry = application_picker_geometry(ctx.content_rect().size(), metrics.scale);
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
                                            self.application_picker_selected =
                                                Some((*application).clone());
                                            ui.ctx().request_repaint();
                                        }
                                    }
                                });
                        });

                    ui.add_space(metrics.value(12.0));
                    let duplicate_rule = self.application_picker_has_duplicate_rule();
                    if duplicate_rule {
                        ui.label(
                            RichText::new(app_layout_text(
                                language,
                                "Для этого приложения уже создана раскладка",
                                "A layout already exists for this application",
                            ))
                            .size(metrics.value(11.0))
                            .color(egui::Color32::from_rgb(220, 92, 76)),
                        );
                        ui.add_space(metrics.value(6.0));
                    }
                    let can_confirm = self.application_picker_selected.is_some() && !duplicate_rule;
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
                                "Сохранить новое приложение для выбранной раскладки",
                                "Save the new application for the selected layout",
                            )
                        } else {
                            app_layout_text(
                                language,
                                "Создать раскладку для выбранного приложения",
                                "Create a layout for the selected application",
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
                self.apply_picker_selection(application);
            }
            open = false;
        }
        if cancel {
            open = false;
        }
        if !open {
            self.application_picker_selected = None;
            self.application_picker_target_layout_id = None;
        }
        self.application_picker_open = open;
    }

    fn apply_picker_selection(
        &mut self,
        application: crate::application_layouts::DetectedApplication,
    ) {
        let Some(device_key) = self.application_layout_device_key() else {
            return;
        };
        let settings = self
            .app_settings
            .application_layouts
            .entry(device_key)
            .or_default();
        let name = automatic_application_layout_name(
            settings,
            &application,
            self.application_picker_target_layout_id.as_deref(),
        );
        if let Some(target_id) = self.application_picker_target_layout_id.as_deref() {
            settings.update_application_rule(target_id, &application, &name, "");
        } else {
            settings.create_for_application_named(&application, Some(&name), "");
        }
        save_app_settings(&self.app_settings);
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

    #[test]
    fn inline_rename_marks_empty_and_duplicate_names_invalid() {
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
            &settings, &telegram, "  "
        ));
        assert!(application_layout_name_is_invalid(
            &settings,
            &telegram,
            " blender "
        ));
        assert!(!application_layout_name_is_invalid(
            &settings, &telegram, "Telegram"
        ));
        assert!(!application_layout_name_is_invalid(
            &settings,
            &telegram,
            "Telegram — работа"
        ));
    }

    #[test]
    fn layout_name_click_excludes_the_dropdown_arrow() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(260.0, 32.0));

        assert!(application_layout_name_area_contains(
            rect,
            egui::pos2(120.0, 36.0),
            APPLICATION_LAYOUT_DROPDOWN_ARROW_WIDTH,
        ));
        assert!(!application_layout_name_area_contains(
            rect,
            egui::pos2(255.0, 36.0),
            APPLICATION_LAYOUT_DROPDOWN_ARROW_WIDTH,
        ));
    }

    #[test]
    fn automatic_layout_names_are_unique_without_a_manual_name_field() {
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
