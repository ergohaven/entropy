use super::layer_operations::LAYER_OPERATIONS_SUBMENU_HEIGHT;
use super::*;

const LAYER_OPERATIONS_SUBMENU_GAP: f32 = 4.0;

fn layer_operations_submenu_rect(
    row_rect: egui::Rect,
    submenu_size: egui::Vec2,
    content_rect: egui::Rect,
) -> egui::Rect {
    let right_x = row_rect.right() + 8.0 + LAYER_OPERATIONS_SUBMENU_GAP;
    let left_x = row_rect.left() - 8.0 - LAYER_OPERATIONS_SUBMENU_GAP - submenu_size.x;
    let x = if right_x + submenu_size.x <= content_rect.right() - 4.0 {
        right_x
    } else {
        left_x.max(content_rect.left() + 4.0)
    };
    let preferred_y = row_rect.top() - 6.0;
    let max_y = (content_rect.bottom() - submenu_size.y - 4.0).max(content_rect.top() + 4.0);
    let y = preferred_y.clamp(content_rect.top() + 4.0, max_y);
    egui::Rect::from_min_size(egui::pos2(x, y), submenu_size)
}

fn pointer_over_layer_operations_bridge(
    pointer: Option<egui::Pos2>,
    row_rect: Option<egui::Rect>,
    submenu_rect: Option<egui::Rect>,
) -> bool {
    let (Some(pointer), Some(row_rect), Some(submenu_rect)) = (pointer, row_rect, submenu_rect)
    else {
        return false;
    };
    let connector = if submenu_rect.left() >= row_rect.right() {
        egui::Rect::from_min_max(
            egui::pos2(row_rect.right() - 1.0, row_rect.top() - 3.0),
            egui::pos2(submenu_rect.left() + 1.0, row_rect.bottom() + 3.0),
        )
    } else {
        egui::Rect::from_min_max(
            egui::pos2(submenu_rect.right() - 1.0, row_rect.top() - 3.0),
            egui::pos2(row_rect.left() + 1.0, row_rect.bottom() + 3.0),
        )
    };
    row_rect.expand(3.0).contains(pointer)
        || submenu_rect.expand(3.0).contains(pointer)
        || connector.contains(pointer)
}

fn entlayout_import_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Импорт раскладки",
        crate::i18n::Language::English => "Import layout",
    }
}

fn entlayout_export_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Экспорт раскладки",
        crate::i18n::Language::English => "Export layout",
    }
}

fn layout_image_export_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Экспорт картинки",
        crate::i18n::Language::English => "Export image",
    }
}

fn about_device_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Об устройстве",
        crate::i18n::Language::English => "About device",
    }
}

impl EntropyApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_layout_device_dropdown(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        lang: crate::i18n::Language,
        device_tab_rect: Option<egui::Rect>,
        device_tab_hovered: bool,
        advanced_tab_hovered: bool,
        settings_tab_hovered: bool,
    ) {
        use crate::i18n::Key as TrKey;

        if let Some(device_rect) = device_tab_rect {
            let dropdown_id = device_dropdown_open_id();
            let was_open = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(dropdown_id))
                .unwrap_or(false);
            let device_count = self.device_manager.devices().len();
            let device_rows = device_count.max(1) as f32;
            let devices_h = 12.0 + device_rows * 30.0;
            let sticky_layout_h = 36.0;
            let layer_operations_h = 36.0;
            #[cfg(not(target_arch = "wasm32"))]
            let import_export_h = 102.0;
            #[cfg(target_arch = "wasm32")]
            let import_export_h = 0.0;
            let about_device_h = 36.0;
            let show_key_legend_switcher = self.app_settings.key_legend_layout.is_multilingual();
            let key_legend_switcher_h = if show_key_legend_switcher { 36.0 } else { 0.0 };
            let mut device_menu_labels: Vec<String> = if self.device_manager.devices().is_empty() {
                vec![crate::i18n::tr(lang, TrKey::NoDevicesFound).to_owned()]
            } else {
                self.device_manager
                    .devices()
                    .iter()
                    .map(|dev| {
                        let display_name = self
                            .device_display_names
                            .get(&dev.display_name_cache_key())
                            .map(String::as_str)
                            .unwrap_or(dev.name.as_str());
                        dev.display_name_with_transport(display_name)
                    })
                    .collect()
            };
            if show_key_legend_switcher {
                if let Some(order_key) = self.app_settings.key_legend_layout.order_i18n_key() {
                    device_menu_labels.push(crate::i18n::tr_catalog(lang, order_key).to_owned());
                }
            }
            device_menu_labels.push(crate::i18n::tr_catalog(lang, "layer_actions.menu").to_owned());
            #[cfg(not(target_arch = "wasm32"))]
            {
                device_menu_labels.push(entlayout_import_label(lang).to_owned());
                device_menu_labels.push(entlayout_export_label(lang).to_owned());
                device_menu_labels.push(layout_image_export_label(lang).to_owned());
            }
            device_menu_labels
                .push(crate::i18n::tr_catalog(lang, "ui.sticky_layout_window_label").to_owned());
            device_menu_labels.push(about_device_label(lang).to_owned());
            let dropdown_size = Vec2::new(
                adaptive_top_dropdown_width(
                    ui,
                    device_menu_labels.iter().map(String::as_str),
                    152.0,
                ),
                devices_h
                    + key_legend_switcher_h
                    + layer_operations_h
                    + import_export_h
                    + sticky_layout_h
                    + about_device_h
                    + 12.0,
            );
            let dropdown_rect = egui::Rect::from_min_size(
                egui::pos2(
                    device_rect.center().x - dropdown_size.x / 2.0,
                    device_rect.bottom() + 6.0,
                ),
                dropdown_size,
            );
            let layer_operations_available = self
                .layout
                .as_ref()
                .and_then(|layout| layout.layers.get(self.selected_layer))
                .is_some();
            let layer_operations_submenu_width = self.layer_operations_submenu_width(ui);
            let layer_operations_submenu_size = egui::vec2(
                layer_operations_submenu_width,
                LAYER_OPERATIONS_SUBMENU_HEIGHT,
            );
            let layer_operations_submenu_id = device_layer_operations_submenu_open_id();
            let layer_operations_row_rect_id =
                ui.make_persistent_id("device_layer_operations_row_rect");
            let layer_operations_submenu_rect_id =
                ui.make_persistent_id("device_layer_operations_submenu_rect");
            let submenu_was_open = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(layer_operations_submenu_id))
                .unwrap_or(false);
            let stored_layer_operations_row_rect = ui
                .ctx()
                .data(|d| d.get_temp::<egui::Rect>(layer_operations_row_rect_id));
            let stored_layer_operations_submenu_rect = ui
                .ctx()
                .data(|d| d.get_temp::<egui::Rect>(layer_operations_submenu_rect_id));
            let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos());
            let pointer_over_stored_layer_operations_bridge = pointer_over_layer_operations_bridge(
                pointer_pos,
                stored_layer_operations_row_rect,
                stored_layer_operations_submenu_rect,
            );
            let hover_bridge_rect = device_rect.union(dropdown_rect).expand(3.0);
            let pointer_over_bridge = ui
                .ctx()
                .input(|i| i.pointer.hover_pos())
                .map(|pos| hover_bridge_rect.contains(pos))
                .unwrap_or(false);
            let show_dropdown = !advanced_tab_hovered
                && !settings_tab_hovered
                && (device_tab_hovered
                    || (was_open
                        && (pointer_over_bridge
                            || (submenu_was_open && pointer_over_stored_layer_operations_bridge))));

            if show_dropdown {
                let area_id = ui.make_persistent_id("device_dropdown_area");
                let mut device_clicked = false;
                let mut layer_operations_row_rect = None;
                let mut layer_operations_hovered = false;
                egui::Area::new(area_id)
                        .order(egui::Order::Foreground)
                        .fixed_pos(dropdown_rect.min)
                        .show(ctx, |ui| {
                            let dark = ui.visuals().dark_mode;
                            top_dropdown_frame(dark).show(ui, |ui| {
                                ui.set_min_width(dropdown_size.x - 16.0);

                                let prev_selected = self.selected_device;
                                if self.device_manager.devices().is_empty() {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(dropdown_size.x - 16.0, 30.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(10.0);
                                            ui.label(
                                                RichText::new(crate::i18n::tr(
                                                    lang,
                                                    TrKey::NoDevicesFound,
                                                ))
                                                .size(13.0)
                                                .color(app_muted_text(ui.visuals().dark_mode)),
                                            );
                                        },
                                    );
                                } else {
                                    for (i, dev) in self.device_manager.devices().iter().enumerate()
                                    {
                                        let is_selected = self.selected_device == Some(i);
                                        #[cfg(not(target_arch = "wasm32"))]
                                        let switch_enabled = !self.hid_user_action_busy();
                                        #[cfg(target_arch = "wasm32")]
                                        let switch_enabled = true;
                                        let cached_display_name = self
                                            .device_display_names
                                            .get(&dev.display_name_cache_key())
                                            .map(String::as_str);
                                        let display_name = dev.display_name_with_transport(
                                            cached_display_name.unwrap_or(dev.name.as_str()),
                                        );
                                        let resp = top_dropdown_item(
                                            ui,
                                            dropdown_size.x - 16.0,
                                            &display_name,
                                            switch_enabled,
                                            is_selected,
                                        );
                                        if switch_enabled && resp.clicked() {
                                            self.selected_device = Some(i);
                                            self.main_menu_tab = MainMenuTab::Keyboard;
                                            device_clicked = true;
                                        }
                                    }
                                }

                                #[cfg(not(target_arch = "wasm32"))]
                                if self.selected_device != prev_selected {
                                    if let Some(idx) = self.selected_device {
                                        self.start_connect(idx);
                                    }
                                }

                                if show_key_legend_switcher {
                                    if let Some(order_key) =
                                        self.app_settings.key_legend_layout.order_i18n_key()
                                    {
                                        ui.add_space(6.0);
                                        let order_label = crate::i18n::tr_catalog(lang, order_key);
                                        if top_dropdown_item(
                                            ui,
                                            dropdown_size.x - 16.0,
                                            order_label,
                                            true,
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.app_settings.key_legend_layout =
                                                self.app_settings.key_legend_layout.toggled_order();
                                            save_app_settings(&self.app_settings);
                                            ctx.request_repaint();
                                        }
                                    }
                                }

                                ui.add_space(6.0);
                                let layer_operations_response = top_dropdown_submenu_item(
                                    ui,
                                    dropdown_size.x - 16.0,
                                    crate::i18n::tr_catalog(lang, "layer_actions.menu"),
                                    layer_operations_available,
                                    submenu_was_open
                                        && pointer_over_stored_layer_operations_bridge,
                                );
                                layer_operations_row_rect =
                                    Some(layer_operations_response.rect);
                                layer_operations_hovered =
                                    layer_operations_response.hovered()
                                        && layer_operations_available;

                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    ui.add_space(6.0);
                                    if top_dropdown_item(
                                        ui,
                                        dropdown_size.x - 16.0,
                                        entlayout_import_label(lang),
                                        self.layout.is_some(),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.close_top_dropdowns(ctx);
                                        self.request_entlayout_import_after_full_load();
                                        ctx.request_repaint();
                                    }
                                    if top_dropdown_item(
                                        ui,
                                        dropdown_size.x - 16.0,
                                        entlayout_export_label(lang),
                                        self.layout.is_some(),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.close_top_dropdowns(ctx);
                                        self.request_entlayout_export_after_full_load();
                                        ctx.request_repaint();
                                    }
                                    if top_dropdown_item(
                                        ui,
                                        dropdown_size.x - 16.0,
                                        layout_image_export_label(lang),
                                        self.layout.is_some(),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.close_top_dropdowns(ctx);
                                        self.request_image_export_after_full_load();
                                        ctx.request_repaint();
                                    }
                                }

                                ui.add_space(6.0);
                                if top_dropdown_item(
                                    ui,
                                    dropdown_size.x - 16.0,
                                    crate::i18n::tr_catalog(lang, "ui.sticky_layout_window_label"),
                                    true,
                                    self.app_settings.sticky_layout_window,
                                )
                                .clicked()
                                {
                                    if self.app_settings.sticky_layout_window {
                                        self.app_settings.sticky_layout_window = false;
                                        self.pending_layout_indicator_open_after_unlock = false;
                                        self.sticky_layout_last_size = None;
                                        save_app_settings(&self.app_settings);
                                    } else if self.is_vial_locked() {
                                        self.pending_layout_indicator_open_after_unlock = true;
                                        self.unlock_open = true;
                                        self.status_msg = crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "matrix_tester.keyboard_is_locked_unlock_it_to_use_matrix_tester",
                                        )
                                        .into();
                                    } else {
                                        self.app_settings.sticky_layout_window = true;
                                        self.sticky_layout_last_size = None;
                                        save_app_settings(&self.app_settings);
                                    }
                                    ctx.request_repaint();
                                    device_clicked = true;
                                }

                                if top_dropdown_item(
                                    ui,
                                    dropdown_size.x - 16.0,
                                    about_device_label(lang),
                                    self.layout.is_some(),
                                    self.main_menu_tab == MainMenuTab::Settings
                                        && self.settings_tab == SettingsTab::AboutDevice,
                                )
                                .clicked()
                                {
                                    self.close_top_dropdowns(ctx);
                                    self.open_about_device_page();
                                    ctx.request_repaint();
                                    device_clicked = true;
                                }
                            });
                        });

                let mut submenu_open = false;
                let mut submenu_rect_for_state = None;
                if let Some(row_rect) = layer_operations_row_rect {
                    let desired_submenu_rect = layer_operations_submenu_rect(
                        row_rect,
                        layer_operations_submenu_size,
                        ctx.content_rect(),
                    );
                    let pointer_over_current_layer_operations_bridge =
                        pointer_over_layer_operations_bridge(
                            pointer_pos,
                            Some(row_rect),
                            Some(desired_submenu_rect),
                        );
                    submenu_open = layer_operations_available
                        && (layer_operations_hovered
                            || (submenu_was_open && pointer_over_current_layer_operations_bridge));

                    if submenu_open {
                        let submenu_area =
                            egui::Area::new(egui::Id::new("device_layer_operations_submenu_area"))
                                .order(egui::Order::Foreground)
                                .fixed_pos(desired_submenu_rect.min)
                                .show(ctx, |ui| {
                                    top_dropdown_frame(ui.visuals().dark_mode)
                                        .show(ui, |ui| {
                                            self.draw_layer_operations_submenu(
                                                ui,
                                                layer_operations_submenu_width - 16.0,
                                            )
                                        })
                                        .inner
                                });
                        let action_clicked = submenu_area.inner;
                        submenu_rect_for_state = Some(submenu_area.response.rect);
                        if action_clicked {
                            self.close_top_dropdowns(ctx);
                            ctx.request_repaint();
                            device_clicked = true;
                            submenu_open = false;
                        }
                    }
                }

                ui.ctx().data_mut(|d| {
                    if let Some(row_rect) = layer_operations_row_rect {
                        d.insert_temp(layer_operations_row_rect_id, row_rect);
                    }
                    if let Some(submenu_rect) = submenu_rect_for_state {
                        d.insert_temp(layer_operations_submenu_rect_id, submenu_rect);
                    }
                    d.insert_temp(layer_operations_submenu_id, submenu_open && !device_clicked);
                    d.insert_temp(
                        dropdown_id,
                        !device_clicked
                            && (device_tab_hovered || pointer_over_bridge || submenu_open),
                    );
                });
            } else {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(dropdown_id, false);
                    d.insert_temp(layer_operations_submenu_id, false);
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{layer_operations_submenu_rect, pointer_over_layer_operations_bridge};

    #[test]
    fn layer_operations_submenu_opens_to_the_right_when_space_allows() {
        let row = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(150.0, 30.0));
        let content = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let submenu = layer_operations_submenu_rect(row, egui::vec2(210.0, 174.0), content);

        assert_eq!(submenu.min, egui::pos2(262.0, 94.0));
    }

    #[test]
    fn layer_operations_submenu_flips_left_near_the_window_edge() {
        let row = egui::Rect::from_min_size(egui::pos2(650.0, 100.0), egui::vec2(140.0, 30.0));
        let content = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let submenu = layer_operations_submenu_rect(row, egui::vec2(210.0, 174.0), content);

        assert_eq!(submenu.min, egui::pos2(428.0, 94.0));
    }

    #[test]
    fn layer_operations_hover_bridge_does_not_cover_unrelated_parent_rows() {
        let row = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(150.0, 30.0));
        let submenu = egui::Rect::from_min_size(egui::pos2(262.0, 94.0), egui::vec2(210.0, 174.0));

        assert!(pointer_over_layer_operations_bridge(
            Some(egui::pos2(256.0, 115.0)),
            Some(row),
            Some(submenu),
        ));
        assert!(pointer_over_layer_operations_bridge(
            Some(egui::pos2(300.0, 200.0)),
            Some(row),
            Some(submenu),
        ));
        assert!(!pointer_over_layer_operations_bridge(
            Some(egui::pos2(150.0, 200.0)),
            Some(row),
            Some(submenu),
        ));
    }
}
