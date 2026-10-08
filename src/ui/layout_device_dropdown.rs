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
            let show_key_legend_switcher = self.app_settings.key_legend_layout.is_multilingual();
            // Rows in drawing order: the device list (or its placeholder) |
            // key legend order, layer operations, show/hide keys | import, export, image
            // (native builds only) | layout indicator, about device.
            #[cfg(not(target_arch = "wasm32"))]
            let (file_rows, file_dividers) = (3, 1);
            #[cfg(target_arch = "wasm32")]
            let (file_rows, file_dividers) = (0, 0);
            let row_count =
                device_count.max(1) + show_key_legend_switcher as usize + 2 + file_rows + 2;
            let divider_count = 2 + file_dividers;
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
            device_menu_labels
                .push(crate::i18n::tr_catalog(lang, "main_menu.show_hide_keys").to_owned());
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
                adaptive_top_icon_dropdown_width(
                    ui,
                    device_menu_labels.iter().map(String::as_str),
                    152.0,
                ),
                top_dropdown_height(row_count, divider_count),
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
                show_top_dropdown(ctx, area_id, dropdown_rect.min, |ui| {
                    ui.set_min_width(dropdown_size.x - 16.0);

                    let mut requested_device = None;
                    if self.device_manager.devices().is_empty() {
                        ui.allocate_ui_with_layout(
                            egui::vec2(dropdown_size.x - 16.0, TOP_DROPDOWN_ITEM_HEIGHT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add_space(TOP_DROPDOWN_ICON_TEXT_LEFT);
                                ui.label(
                                    RichText::new(crate::i18n::tr(lang, TrKey::NoDevicesFound))
                                        .size(13.0)
                                        .color(app_muted_text(ui.visuals().dark_mode)),
                                );
                            },
                        );
                    } else {
                        for (i, dev) in self.device_manager.devices().iter().enumerate() {
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
                            let resp = top_dropdown_icon_item(
                                ui,
                                dropdown_size.x - 16.0,
                                TopMenuIcon::Device,
                                &display_name,
                                switch_enabled,
                                is_selected,
                            );
                            if switch_enabled && resp.clicked() {
                                requested_device = Some(i);
                                self.main_menu_tab = MainMenuTab::Keyboard;
                                device_clicked = true;
                            }
                        }
                    }

                    if let Some(idx) = requested_device {
                        #[cfg(not(target_arch = "wasm32"))]
                        if self.selected_device != Some(idx)
                            || self.pending_device_connect.is_some()
                        {
                            self.start_connect(idx);
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            self.selected_device = Some(idx);
                        }
                    }

                    top_dropdown_divider(ui, dropdown_size.x - 16.0);
                    if show_key_legend_switcher {
                        if let Some(order_key) =
                            self.app_settings.key_legend_layout.order_i18n_key()
                        {
                            let order_label = crate::i18n::tr_catalog(lang, order_key);
                            if top_dropdown_icon_item(
                                ui,
                                dropdown_size.x - 16.0,
                                TopMenuIcon::KeyLegendOrder,
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

                    let layer_operations_response = top_dropdown_icon_submenu_item(
                        ui,
                        dropdown_size.x - 16.0,
                        TopMenuIcon::LayerOperations,
                        crate::i18n::tr_catalog(lang, "layer_actions.menu"),
                        layer_operations_available,
                        submenu_was_open && pointer_over_stored_layer_operations_bridge,
                    );
                    layer_operations_row_rect = Some(layer_operations_response.rect);
                    layer_operations_hovered =
                        layer_operations_response.hovered() && layer_operations_available;

                    if top_dropdown_icon_item(
                        ui,
                        dropdown_size.x - 16.0,
                        TopMenuIcon::ShowHideKeys,
                        crate::i18n::tr_catalog(lang, "main_menu.show_hide_keys"),
                        self.layout.is_some() && !self.current_encoder_visibility_id.is_empty(),
                        self.editing_layout_visibility,
                    )
                    .clicked()
                    {
                        self.close_top_dropdowns(ctx);
                        self.start_layout_visibility_edit();
                        ctx.request_repaint();
                        device_clicked = true;
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        top_dropdown_divider(ui, dropdown_size.x - 16.0);
                        if top_dropdown_icon_item(
                            ui,
                            dropdown_size.x - 16.0,
                            TopMenuIcon::ImportLayout,
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
                        if top_dropdown_icon_item(
                            ui,
                            dropdown_size.x - 16.0,
                            TopMenuIcon::ExportLayout,
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
                        if top_dropdown_icon_item(
                            ui,
                            dropdown_size.x - 16.0,
                            TopMenuIcon::ExportImage,
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

                    top_dropdown_divider(ui, dropdown_size.x - 16.0);
                    if top_dropdown_icon_item(
                        ui,
                        dropdown_size.x - 16.0,
                        TopMenuIcon::LayoutIndicator,
                        crate::i18n::tr_catalog(lang, "ui.sticky_layout_window_label"),
                        true,
                        self.app_settings.sticky_layout_window,
                    )
                    .clicked()
                    {
                        self.toggle_sticky_layout_window();
                        ctx.request_repaint();
                        device_clicked = true;
                    }

                    if top_dropdown_icon_item_with_indicator(
                        ui,
                        dropdown_size.x - 16.0,
                        TopMenuIcon::AboutDevice,
                        about_device_label(lang),
                        self.layout.is_some(),
                        self.main_menu_tab == MainMenuTab::Settings
                            && self.settings_tab == SettingsTab::AboutDevice,
                        crate::app::firmware_update_available(&self.firmware_update_check)
                            && self
                                .device_about_info
                                .as_ref()
                                .and_then(|info| info.firmware_update_target.as_ref())
                                == crate::app::firmware_update_target(&self.firmware_update_check),
                    )
                    .clicked()
                    {
                        self.close_top_dropdowns(ctx);
                        self.open_about_device_page();
                        ctx.request_repaint();
                        device_clicked = true;
                    }
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
