use super::*;

pub(super) struct LayoutTopTabsState {
    pub(super) lang: crate::i18n::Language,
    pub(super) device_tab_rect: Option<egui::Rect>,
    pub(super) device_tab_hovered: bool,
    pub(super) advanced_tab_rect: Option<egui::Rect>,
    pub(super) advanced_tab_hovered: bool,
    pub(super) settings_tab_rect: Option<egui::Rect>,
    pub(super) settings_tab_hovered: bool,
}

struct LayoutTopTabMetrics {
    font_size: f32,
    gap: f32,
    widths: [f32; 3],
    total_width: f32,
}

fn layout_top_tab_metrics(
    ui: &egui::Ui,
    labels: [&str; 3],
    available_width: f32,
) -> LayoutTopTabMetrics {
    const ATTEMPTS: [(f32, f32, f32, f32); 5] = [
        (15.0, 16.0, 34.0, 96.0),
        (15.0, 10.0, 24.0, 0.0),
        (14.0, 8.0, 18.0, 0.0),
        (13.0, 6.0, 14.0, 0.0),
        (12.0, 6.0, 10.0, 0.0),
    ];

    let mut selected = None;
    for (font_size, gap, padding, min_width) in ATTEMPTS {
        let widths = labels
            .map(|label| (top_menu_text_width(ui, label, font_size) + padding).max(min_width));
        let total_width = widths.iter().sum::<f32>() + gap * (labels.len() - 1) as f32;
        selected = Some(LayoutTopTabMetrics {
            font_size,
            gap,
            widths,
            total_width,
        });
        if available_width <= 0.0 || total_width <= available_width {
            break;
        }
    }

    selected.expect("top tab metrics attempts are non-empty")
}

impl EntropyApp {
    pub(super) fn draw_layout_top_tabs(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        viewport: egui::Rect,
        top_base_y: f32,
        suppress_tooltips: bool,
    ) -> LayoutTopTabsState {
        use crate::i18n::Key as TrKey;

        let lang = self.app_settings.language;
        let center_x = viewport.center().x;
        let tabs_y = top_base_y;
        let tab_height = 28.0;
        let tabs = [
            (
                MainMenuTab::Keyboard,
                crate::i18n::tr(lang, TrKey::MainTabLayout),
                "main_menu.layout_tooltip",
            ),
            (
                MainMenuTab::Advanced,
                crate::i18n::tr(lang, TrKey::MainTabAdvanced),
                "main_menu.advanced_tooltip",
            ),
            (
                MainMenuTab::Settings,
                crate::i18n::tr(lang, TrKey::MainTabConfig),
                "main_menu.settings_tooltip",
            ),
        ];
        let tab_labels = tabs.map(|(_, label, _)| label);
        let undo_label = crate::i18n::tr_catalog(lang, "alt_repeat_editor.undo_curved");
        let undo_font = FontId::proportional(14.0);
        let undo_text_w = ui.fonts(|f| {
            f.layout_no_wrap(
                undo_label.to_owned(),
                undo_font.clone(),
                ui.visuals().widgets.inactive.fg_stroke.color,
            )
            .size()
            .x
        });
        let undo_rect = egui::Rect::from_min_size(
            egui::pos2(viewport.left() + 24.0, tabs_y),
            Vec2::new(undo_text_w + 12.0, tab_height),
        );
        let zoom_width = 108.0;
        let zoom_left_top = egui::pos2(viewport.right() - 18.0 - zoom_width, tabs_y);
        let side_gap = 12.0;
        let safe_left = undo_rect.right() + side_gap;
        let safe_right = zoom_left_top.x - side_gap;
        let safe_width = (safe_right - safe_left).max(0.0);
        let metrics = layout_top_tab_metrics(ui, tab_labels, safe_width);
        let tab_font_size = metrics.font_size;
        let tab_gap = metrics.gap;
        let tab_widths = metrics.widths;
        let total_w = metrics.total_width;
        let preferred_start_x = center_x - total_w / 2.0;
        let start_x = if safe_width >= total_w {
            preferred_start_x.clamp(safe_left, safe_right - total_w)
        } else if safe_width > 0.0 {
            safe_left
        } else {
            preferred_start_x
        };
        let mut device_tab_rect = None;
        let mut device_tab_hovered = false;
        let mut advanced_tab_rect = None;
        let mut advanced_tab_hovered = false;
        let mut settings_tab_rect = None;
        let mut settings_tab_hovered = false;

        let mut tab_x = start_x;
        for (idx, (tab, label, tooltip)) in tabs.iter().enumerate() {
            let slot_rect = egui::Rect::from_min_size(
                egui::pos2(tab_x, tabs_y),
                Vec2::new(tab_widths[idx], tab_height),
            );
            tab_x += tab_widths[idx] + tab_gap;
            let resp = ui.allocate_rect(slot_rect, Sense::CLICK);
            if !suppress_tooltips {
                resp.clone()
                    .on_hover_text(crate::i18n::tr_catalog(lang, tooltip));
            }
            if matches!(tab, MainMenuTab::Keyboard) {
                device_tab_rect = Some(slot_rect);
                device_tab_hovered = resp.hovered();
            }
            if matches!(tab, MainMenuTab::Advanced) {
                advanced_tab_rect = Some(slot_rect);
                advanced_tab_hovered = resp.hovered();
            }
            if matches!(tab, MainMenuTab::Settings) {
                settings_tab_rect = Some(slot_rect);
                settings_tab_hovered = resp.hovered();
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                match tab {
                    MainMenuTab::Keyboard => {
                        self.main_menu_tab = MainMenuTab::Keyboard;
                    }
                    MainMenuTab::Advanced => {}
                    MainMenuTab::Settings => {
                        if self.main_menu_tab != MainMenuTab::Settings {
                            self.reset_matrix_tester_state();
                        }
                        self.matrix_tester_unlock_prompted = false;
                        self.matrix_tester_lock_checked = false;
                        self.main_menu_tab = MainMenuTab::Settings;
                    }
                }
            }

            let is_active = self.main_menu_tab == *tab;
            let text_color = if is_active {
                ui.visuals().widgets.inactive.fg_stroke.color
            } else if resp.hovered() {
                if ui.visuals().dark_mode {
                    Color32::from_gray(135)
                } else {
                    Color32::from_gray(120)
                }
            } else if ui.visuals().dark_mode {
                Color32::from_gray(90)
            } else {
                Color32::from_gray(150)
            };

            ui.painter().text(
                slot_rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                FontId::proportional(tab_font_size),
                text_color,
            );
        }

        self.register_tour_target(
            TourTarget::MainNavigation,
            egui::Rect::from_min_size(egui::pos2(start_x, tabs_y), Vec2::new(total_w, tab_height)),
        );
        if let Some(device_rect) = device_tab_rect {
            self.register_tour_target(TourTarget::DeviceSelector, device_rect);
        }
        if let Some(settings_rect) = settings_tab_rect {
            self.register_tour_target(TourTarget::SettingsMenu, settings_rect);
        }

        self.draw_ui_scale_controls(ui, zoom_left_top);

        #[cfg(not(target_arch = "wasm32"))]
        let undo_enabled = !self.undo_stack.is_empty() && self.layer_write_task.is_none();
        #[cfg(target_arch = "wasm32")]
        let undo_enabled = !self.undo_stack.is_empty();
        let undo_resp = ui.allocate_rect(undo_rect, Sense::CLICK);
        if undo_enabled && !suppress_tooltips {
            undo_resp.clone().on_hover_text(crate::i18n::tr_catalog(
                lang,
                "key_picker_text.undo_last_change",
            ));
        }
        if undo_resp.hovered() && undo_enabled {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if undo_resp.clicked() && undo_enabled {
            self.undo();
            ctx.request_repaint();
        }
        let undo_color = if !undo_enabled {
            if ui.visuals().dark_mode {
                Color32::from_gray(58)
            } else {
                Color32::from_gray(178)
            }
        } else if undo_resp.hovered() {
            app_accent()
        } else {
            ui.visuals().widgets.inactive.fg_stroke.color
        };
        let undo_text_pos = egui::pos2(undo_rect.left() + 6.0, undo_rect.center().y);
        ui.painter().text(
            undo_text_pos,
            egui::Align2::LEFT_CENTER,
            undo_label,
            undo_font,
            undo_color,
        );

        let divider_color = if ui.visuals().dark_mode {
            Color32::from_gray(105)
        } else {
            Color32::from_gray(170)
        };
        let divider_top = tabs_y + 4.0;
        let divider_bottom = tabs_y + tab_height - 4.0;
        let mut divider_x = start_x;
        for width in tab_widths.iter().take(tabs.len() - 1) {
            divider_x += *width;
            let x = divider_x + tab_gap / 2.0;
            ui.painter().line_segment(
                [egui::pos2(x, divider_top), egui::pos2(x, divider_bottom)],
                egui::Stroke::new(1.5, divider_color),
            );
            divider_x += tab_gap;
        }

        LayoutTopTabsState {
            lang,
            device_tab_rect,
            device_tab_hovered,
            advanced_tab_rect,
            advanced_tab_hovered,
            settings_tab_rect,
            settings_tab_hovered,
        }
    }
}
