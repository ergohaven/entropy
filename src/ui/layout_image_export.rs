use super::*;

#[cfg(not(target_arch = "wasm32"))]
use ab_glyph::{point, Font, FontArc, ScaleFont};
#[cfg(not(target_arch = "wasm32"))]
use image::{Rgba, RgbaImage};
#[cfg(not(target_arch = "wasm32"))]
use std::fmt::Write as _;

const EXPORT_LAYOUT_UNIT: f32 = 74.0;
const EXPORT_KEY_PADDING: f32 = 3.5;
const EXPORT_MARGIN: f32 = 48.0;
const EXPORT_LAYER_HEADER_H: f32 = 44.0;
const EXPORT_LAYER_HEADER_H_HIDDEN: f32 = 16.0;
const EXPORT_LAYER_GAP: f32 = 42.0;
const EXPORT_MAX_SIDE: f32 = 12_000.0;

fn export_text(lang: crate::i18n::Language, key: &str) -> &'static str {
    match (lang, key) {
        (crate::i18n::Language::Russian, "title") => "Экспорт картинки раскладки",
        (crate::i18n::Language::English, "title") => "Export layout image",
        (crate::i18n::Language::Russian, "description") => {
            "PNG, SVG или PDF с выбранными слоями, темой и легендами клавиш"
        }
        (crate::i18n::Language::English, "description") => {
            "PNG, SVG, or PDF with selected layers, theme, and key legends"
        }
        (crate::i18n::Language::Russian, "pdf_pages_hint") => {
            "PDF для печати: каждый выбранный слой на отдельной странице A4"
        }
        (crate::i18n::Language::English, "pdf_pages_hint") => {
            "Printable PDF: each selected layer on its own A4 page"
        }
        (crate::i18n::Language::Russian, "format") => "Формат",
        (crate::i18n::Language::English, "format") => "Format",
        (crate::i18n::Language::Russian, "format_tooltip") => {
            "Выбрать формат экспортируемого файла"
        }
        (crate::i18n::Language::English, "format_tooltip") => "Choose exported file format",
        (crate::i18n::Language::Russian, "theme") => "Тема",
        (crate::i18n::Language::English, "theme") => "Theme",
        (crate::i18n::Language::Russian, "theme_tooltip") => "Выбрать тему экспортируемой картинки",
        (crate::i18n::Language::English, "theme_tooltip") => "Choose exported image theme",
        (crate::i18n::Language::Russian, "legends") => "Легенды клавиш",
        (crate::i18n::Language::English, "legends") => "Key legends",
        (crate::i18n::Language::Russian, "legends_tooltip") => {
            "Выбрать английские или русские подписи клавиш"
        }
        (crate::i18n::Language::English, "legends_tooltip") => {
            "Choose English or Russian key labels"
        }
        (crate::i18n::Language::Russian, "layer_names") => "Названия слоёв",
        (crate::i18n::Language::English, "layer_names") => "Layer names",
        (crate::i18n::Language::Russian, "layer_names_tooltip") => {
            "Показывать заголовок над каждым экспортируемым слоем"
        }
        (crate::i18n::Language::English, "layer_names_tooltip") => {
            "Show a title above each exported layer"
        }
        (crate::i18n::Language::Russian, "layer_tooltip") => {
            "Добавить слой в экспортируемую картинку"
        }
        (crate::i18n::Language::English, "layer_tooltip") => {
            "Include this layer in the exported image"
        }
        (crate::i18n::Language::Russian, "export") => "Экспорт",
        (crate::i18n::Language::English, "export") => "Export",
        (crate::i18n::Language::Russian, "select_layer") => {
            "Выберите хотя бы один слой для экспорта"
        }
        (crate::i18n::Language::English, "select_layer") => "Select at least one layer to export",
        (crate::i18n::Language::Russian, "saved") => "Картинка раскладки экспортирована",
        (crate::i18n::Language::English, "saved") => "Layout image exported",
        (crate::i18n::Language::Russian, "failed") => "Не удалось экспортировать картинку",
        (crate::i18n::Language::English, "failed") => "Failed to export layout image",
        _ => "",
    }
}

fn export_format_label(
    _lang: crate::i18n::Language,
    format: LayoutImageExportFormat,
) -> &'static str {
    match format {
        LayoutImageExportFormat::Png => "PNG",
        LayoutImageExportFormat::Svg => "SVG",
        LayoutImageExportFormat::Pdf => "PDF",
    }
}

fn export_button_label(lang: crate::i18n::Language, format: LayoutImageExportFormat) -> String {
    format!(
        "{} {}",
        export_text(lang, "export"),
        export_format_label(lang, format)
    )
}

fn export_theme_label(lang: crate::i18n::Language, theme: LayoutImageExportTheme) -> &'static str {
    match (lang, theme) {
        (crate::i18n::Language::Russian, LayoutImageExportTheme::Current) => "Текущая",
        (crate::i18n::Language::English, LayoutImageExportTheme::Current) => "Current",
        (crate::i18n::Language::Russian, LayoutImageExportTheme::Light) => "Светлая",
        (crate::i18n::Language::English, LayoutImageExportTheme::Light) => "Light",
        (crate::i18n::Language::Russian, LayoutImageExportTheme::Dark) => "Тёмная",
        (crate::i18n::Language::English, LayoutImageExportTheme::Dark) => "Dark",
    }
}

fn export_key_legend_label(
    lang: crate::i18n::Language,
    key_legend_layout: KeyLegendLayout,
) -> &'static str {
    match (lang, key_legend_layout) {
        (crate::i18n::Language::Russian, KeyLegendLayout::English) => "English",
        (crate::i18n::Language::English, KeyLegendLayout::English) => "English",
        (crate::i18n::Language::Russian, KeyLegendLayout::Russian) => "Русская: EN сверху",
        (crate::i18n::Language::English, KeyLegendLayout::Russian) => "Russian: EN first",
        (crate::i18n::Language::Russian, KeyLegendLayout::RussianPrimary) => "Русская: RU сверху",
        (crate::i18n::Language::English, KeyLegendLayout::RussianPrimary) => "Russian: RU first",
    }
}

fn layer_export_label(
    lang: crate::i18n::Language,
    layer_names: &[String],
    layer_idx: usize,
) -> String {
    let raw = layer_names
        .get(layer_idx)
        .map(|name| name.trim())
        .unwrap_or("");
    if !raw.is_empty() && raw != layer_idx.to_string() {
        return format!("{layer_idx}. {raw}");
    }

    match lang {
        crate::i18n::Language::Russian => format!("Слой {layer_idx}"),
        crate::i18n::Language::English => format!("Layer {layer_idx}"),
    }
}

fn export_safe_key_label(label: String) -> String {
    match label.as_str() {
        "⏻\nPower" => "Power".to_string(),
        "🌙\nSleep" => "Sleep".to_string(),
        "☀\nWake" => "Wake".to_string(),
        "🔇\nMute" => "Mute".to_string(),
        "🔊\nVol+" => "Vol\n+".to_string(),
        "🔉\nVol-" => "Vol\n-".to_string(),
        "⏭\nNext" => "Next\nTrack".to_string(),
        "⏮\nPrev" => "Prev\nTrack".to_string(),
        "⏹\nStop" => "Stop".to_string(),
        "⏯\nPlay" => "Play\nPause".to_string(),
        "🎵\nMedia" => "Media".to_string(),
        "⏏\nEject" => "Eject".to_string(),
        "✉\nMail" => "Mail".to_string(),
        "🖩\nCalc" => "Calc".to_string(),
        "💻\nFiles" => "Files".to_string(),
        "🔍\nSearch" => "Search".to_string(),
        "🏠\nHome" => "Home".to_string(),
        _ => label,
    }
}

fn export_keycode_label_with_macro_names(
    value: u16,
    custom: &[crate::keyboard::CustomKeycode],
    layer_names: &[String],
    macro_names: &[String],
    tap_dance_names: &[String],
    key_legend_layout: KeyLegendLayout,
) -> String {
    export_safe_key_label(keycode_label_with_macro_names(
        value,
        custom,
        layer_names,
        macro_names,
        tap_dance_names,
        key_legend_layout,
    ))
}

fn export_key_binding_label_with_macro_names(
    binding: crate::keyboard::KeyBinding,
    custom: &[crate::keyboard::CustomKeycode],
    layer_names: &[String],
    macro_names: &[String],
    tap_dance_names: &[String],
    key_legend_layout: KeyLegendLayout,
) -> String {
    export_safe_key_label(key_binding_label_with_macro_names(
        binding,
        custom,
        layer_names,
        macro_names,
        tap_dance_names,
        key_legend_layout,
    ))
}

fn draw_format_dropdown(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    dark: bool,
    lang: crate::i18n::Language,
    selected_format: &mut LayoutImageExportFormat,
) {
    let dropdown_id = ui.make_persistent_id("layout_image_export_format_dropdown");
    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
        ui,
        dropdown_id,
        export_format_label(lang, *selected_format),
        ui.visuals().text_color(),
        metrics.settings_control_width(),
        metrics.settings_control_height(),
        metrics.settings_control_font_size(),
    );
    crate::ui_style::popup_below_widget(
        ui,
        dropdown_id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(metrics.settings_control_width());
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            for format in [
                LayoutImageExportFormat::Png,
                LayoutImageExportFormat::Svg,
                LayoutImageExportFormat::Pdf,
            ] {
                let selected = format == *selected_format;
                let (option_rect, option_resp) =
                    ui.allocate_exact_size(metrics.size(168.0, 28.0), Sense::click());
                if option_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let option_fill = if selected {
                    if dark {
                        Color32::from_rgb(58, 58, 61)
                    } else {
                        Color32::from_rgb(236, 236, 238)
                    }
                } else if option_resp.hovered() {
                    crate::ui_style::hover_fill(dark)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(option_rect, 7.0, option_fill);
                ui.painter().text(
                    egui::pos2(
                        option_rect.left() + metrics.value(10.0),
                        option_rect.center().y,
                    ),
                    egui::Align2::LEFT_CENTER,
                    export_format_label(lang, format),
                    FontId::proportional(metrics.value(12.0)),
                    if selected {
                        ui.visuals().text_color()
                    } else {
                        app_muted_text(dark)
                    },
                );
                if option_resp.clicked() {
                    *selected_format = format;
                    egui::Popup::close_all(ui.ctx());
                }
            }
        },
    );
}

fn draw_theme_dropdown(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    dark: bool,
    lang: crate::i18n::Language,
    selected_theme: &mut LayoutImageExportTheme,
) {
    let dropdown_id = ui.make_persistent_id("layout_image_export_theme_dropdown");
    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
        ui,
        dropdown_id,
        export_theme_label(lang, *selected_theme),
        ui.visuals().text_color(),
        metrics.settings_control_width(),
        metrics.settings_control_height(),
        metrics.settings_control_font_size(),
    );
    crate::ui_style::popup_below_widget(
        ui,
        dropdown_id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(metrics.settings_control_width());
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            for theme in [
                LayoutImageExportTheme::Current,
                LayoutImageExportTheme::Light,
                LayoutImageExportTheme::Dark,
            ] {
                let selected = theme == *selected_theme;
                let (option_rect, option_resp) =
                    ui.allocate_exact_size(metrics.size(168.0, 28.0), Sense::click());
                if option_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let option_fill = if selected {
                    if dark {
                        Color32::from_rgb(58, 58, 61)
                    } else {
                        Color32::from_rgb(236, 236, 238)
                    }
                } else if option_resp.hovered() {
                    crate::ui_style::hover_fill(dark)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(option_rect, 7.0, option_fill);
                ui.painter().text(
                    egui::pos2(
                        option_rect.left() + metrics.value(10.0),
                        option_rect.center().y,
                    ),
                    egui::Align2::LEFT_CENTER,
                    export_theme_label(lang, theme),
                    FontId::proportional(metrics.value(12.0)),
                    if selected {
                        ui.visuals().text_color()
                    } else {
                        app_muted_text(dark)
                    },
                );
                if option_resp.clicked() {
                    *selected_theme = theme;
                    egui::Popup::close_all(ui.ctx());
                }
            }
        },
    );
}

fn draw_key_legend_dropdown(
    ui: &mut egui::Ui,
    metrics: crate::ui_style::ResponsiveMetrics,
    dark: bool,
    lang: crate::i18n::Language,
    selected_layout: &mut KeyLegendLayout,
) {
    let dropdown_id = ui.make_persistent_id("layout_image_export_legends_dropdown");
    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
        ui,
        dropdown_id,
        export_key_legend_label(lang, *selected_layout),
        ui.visuals().text_color(),
        metrics.settings_control_width(),
        metrics.settings_control_height(),
        metrics.settings_control_font_size(),
    );
    crate::ui_style::popup_below_widget(
        ui,
        dropdown_id,
        &dropdown_resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(metrics.settings_control_width());
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            for layout in [
                KeyLegendLayout::English,
                KeyLegendLayout::Russian,
                KeyLegendLayout::RussianPrimary,
            ] {
                let selected = layout == *selected_layout;
                let (option_rect, option_resp) =
                    ui.allocate_exact_size(metrics.size(168.0, 28.0), Sense::click());
                if option_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let option_fill = if selected {
                    if dark {
                        Color32::from_rgb(58, 58, 61)
                    } else {
                        Color32::from_rgb(236, 236, 238)
                    }
                } else if option_resp.hovered() {
                    crate::ui_style::hover_fill(dark)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(option_rect, 7.0, option_fill);
                ui.painter().text(
                    egui::pos2(
                        option_rect.left() + metrics.value(10.0),
                        option_rect.center().y,
                    ),
                    egui::Align2::LEFT_CENTER,
                    export_key_legend_label(lang, layout),
                    FontId::proportional(metrics.value(12.0)),
                    if selected {
                        ui.visuals().text_color()
                    } else {
                        app_muted_text(dark)
                    },
                );
                if option_resp.clicked() {
                    *selected_layout = layout;
                    egui::Popup::close_all(ui.ctx());
                }
            }
        },
    );
}

/// File extension and picker filter label for a layout-image export format.
/// Shared by the picker (spawn) and the writer so every format — PNG, SVG, and
/// PDF — goes through the same async worker dialog and cannot silently diverge.
#[cfg(not(target_arch = "wasm32"))]
fn layout_image_export_descriptor(format: LayoutImageExportFormat) -> (&'static str, &'static str) {
    match format {
        LayoutImageExportFormat::Png => ("png", "PNG image"),
        LayoutImageExportFormat::Svg => ("svg", "SVG image"),
        LayoutImageExportFormat::Pdf => ("pdf", "PDF document"),
    }
}

impl EntropyApp {
    pub(super) fn open_layout_image_export_page(&mut self) {
        let layer_count = self.layer_count.max(1);
        self.ensure_layout_image_export_layers(layer_count);
        self.settings_tab = SettingsTab::LayoutImageExport;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn draw_layout_image_export_page(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let layer_count = self.layer_count.max(layout.layers.len()).max(1);
        self.ensure_layout_image_export_layers(layer_count);

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(export_text(lang, "title"))
                        .size(metrics.value(18.0))
                        .strong(),
                );
                ui.add_space(metrics.value(6.0));
                ui.label(
                    RichText::new(export_text(lang, "description"))
                        .size(metrics.value(13.0))
                        .color(app_muted_text(dark)),
                );
                if self.app_settings.layout_image_export.format == LayoutImageExportFormat::Pdf {
                    ui.add_space(metrics.value(4.0));
                    ui.label(
                        RichText::new(export_text(lang, "pdf_pages_hint"))
                            .size(metrics.value(13.0))
                            .color(app_muted_text(dark)),
                    );
                }
                ui.add_space(metrics.value(24.0));

                let total_rows = 4 + layer_count;
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "layout_image_export",
                    metrics,
                    total_rows,
                    metrics.value(44.0),
                );

                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_layout_image_export_rows(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
                        metrics,
                        dark,
                        list.suppress_tooltips,
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

                let selected_layers = self
                    .app_settings
                    .layout_image_export
                    .selected_layers
                    .iter()
                    .take(layer_count)
                    .filter(|selected| **selected)
                    .count();
                let export_enabled = selected_layers > 0;
                let button_size = metrics.size(168.0, 32.0);
                let button_rect = egui::Rect::from_center_size(
                    egui::pos2(
                        list.viewport.center().x,
                        list.viewport.bottom() + metrics.value(34.0),
                    ),
                    button_size,
                );
                crate::ui_style::allocate_ui_at_rect(ui, button_rect, |ui| {
                    ui.set_min_size(button_rect.size());
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui_style::modern_button(
                        ui,
                        &export_button_label(lang, self.app_settings.layout_image_export.format),
                        button_size,
                        export_enabled,
                    )
                    .clicked()
                    {
                        self.export_layout_image_dialog(layout);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = crate::ui_style::modern_button(
                            ui,
                            &export_button_label(
                                lang,
                                self.app_settings.layout_image_export.format,
                            ),
                            button_size,
                            false,
                        );
                    }
                });
            });
        });
    }

    fn ensure_layout_image_export_layers(&mut self, layer_count: usize) {
        let selected_layers = &mut self.app_settings.layout_image_export.selected_layers;
        let previous_len = selected_layers.len();
        if previous_len < layer_count {
            selected_layers.extend((previous_len..layer_count).map(|layer_idx| layer_idx == 0));
            save_app_settings(&self.app_settings);
        }
    }

    fn draw_layout_image_export_rows(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        dark: bool,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let tooltip = |text: &'static str| (!suppress_tooltips).then_some(text);
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);

        for row_idx in row_range {
            match row_idx {
                0 => {
                    let before = self.app_settings.layout_image_export.format;
                    let mut format = before;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        crate::ui_style::SettingsListRowLayout::new(
                            content_width,
                            row_height,
                            metrics.settings_control_width(),
                        ),
                        export_text(lang, "format"),
                        true,
                        tooltip(export_text(lang, "format_tooltip")),
                        |ui| draw_format_dropdown(ui, metrics, dark, lang, &mut format),
                    );
                    if format != before {
                        self.app_settings.layout_image_export.format = format;
                        save_app_settings(&self.app_settings);
                    }
                }
                1 => {
                    let before = self.app_settings.layout_image_export.theme;
                    let mut theme = before;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        crate::ui_style::SettingsListRowLayout::new(
                            content_width,
                            row_height,
                            metrics.settings_control_width(),
                        ),
                        export_text(lang, "theme"),
                        true,
                        tooltip(export_text(lang, "theme_tooltip")),
                        |ui| draw_theme_dropdown(ui, metrics, dark, lang, &mut theme),
                    );
                    if theme != before {
                        self.app_settings.layout_image_export.theme = theme;
                        save_app_settings(&self.app_settings);
                    }
                }
                2 => {
                    let before = self.app_settings.layout_image_export.key_legend_layout;
                    let mut key_legend_layout = before;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        crate::ui_style::SettingsListRowLayout::new(
                            content_width,
                            row_height,
                            metrics.settings_control_width(),
                        ),
                        export_text(lang, "legends"),
                        true,
                        tooltip(export_text(lang, "legends_tooltip")),
                        |ui| {
                            draw_key_legend_dropdown(
                                ui,
                                metrics,
                                dark,
                                lang,
                                &mut key_legend_layout,
                            )
                        },
                    );
                    if key_legend_layout != before {
                        self.app_settings.layout_image_export.key_legend_layout = key_legend_layout;
                        save_app_settings(&self.app_settings);
                    }
                }
                3 => {
                    let before = self.app_settings.layout_image_export.show_layer_names;
                    let mut show_layer_names = before;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        crate::ui_style::SettingsListRowLayout::new(
                            content_width,
                            row_height,
                            switch_width,
                        ),
                        export_text(lang, "layer_names"),
                        true,
                        tooltip(export_text(lang, "layer_names_tooltip")),
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "layout_image_export_layer_names",
                                &mut show_layer_names,
                                switch_size,
                            );
                        },
                    );
                    if show_layer_names != before {
                        self.app_settings.layout_image_export.show_layer_names = show_layer_names;
                        save_app_settings(&self.app_settings);
                    }
                }
                layer_row => {
                    let layer_idx = layer_row - 4;
                    let Some(selected) = self
                        .app_settings
                        .layout_image_export
                        .selected_layers
                        .get(layer_idx)
                        .copied()
                    else {
                        continue;
                    };
                    let mut selected = selected;
                    let label = layer_export_label(lang, &self.layer_names, layer_idx);
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        crate::ui_style::SettingsListRowLayout::new(
                            content_width,
                            row_height,
                            switch_width,
                        ),
                        label.as_str(),
                        true,
                        tooltip(export_text(lang, "layer_tooltip")),
                        |ui| {
                            let _ = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                ("layout_image_export_layer", layer_idx),
                                &mut selected,
                                switch_size,
                            );
                        },
                    );
                    if let Some(slot) = self
                        .app_settings
                        .layout_image_export
                        .selected_layers
                        .get_mut(layer_idx)
                    {
                        if selected != *slot {
                            *slot = selected;
                            save_app_settings(&self.app_settings);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_layout_image_dialog(&mut self, layout: &KeyboardLayout) {
        let lang = self.app_settings.language;
        let layer_count = self.layer_count.max(layout.layers.len()).max(1);
        let any_selected = self
            .app_settings
            .layout_image_export
            .selected_layers
            .iter()
            .take(layer_count)
            .any(|selected| *selected);
        if !any_selected {
            self.status_msg = export_text(lang, "select_layer").into();
            return;
        }

        let format = self.app_settings.layout_image_export.format;
        let (extension, filter_label) = layout_image_export_descriptor(format);
        let file_name = format!("{}-layout.{extension}", device_id_slug(&layout.name));
        // The picker runs on a worker thread; rendering + writing happen in
        // write_layout_image_export once a path comes back. All three formats
        // (PNG, SVG, PDF) go through this same async parented-dialog path.
        self.spawn_file_dialog(
            crate::app::file_dialog::FileDialogAction::ExportLayoutImage,
            rfd::FileDialog::new()
                .add_filter(filter_label, &[extension])
                .set_file_name(&file_name),
            true,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_layout_image_export(&mut self, mut path: std::path::PathBuf) {
        let lang = self.app_settings.language;
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let layer_count = self.layer_count.max(layout.layers.len()).max(1);
        let selected_layers: Vec<usize> = self
            .app_settings
            .layout_image_export
            .selected_layers
            .iter()
            .take(layer_count)
            .enumerate()
            .filter_map(|(idx, selected)| selected.then_some(idx))
            .collect();
        if selected_layers.is_empty() {
            self.status_msg = export_text(lang, "select_layer").into();
            return;
        }

        let format = self.app_settings.layout_image_export.format;
        let (extension, _) = layout_image_export_descriptor(format);
        // The picker already ran on the worker thread; `path` is the chosen file.
        // Only the extension normalization, render, and write happen here.
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| !ext.eq_ignore_ascii_case(extension))
            .unwrap_or(true)
        {
            path.set_extension(extension);
        }

        let result = match format {
            LayoutImageExportFormat::Png => self
                .render_layout_image(&layout, &selected_layers)
                .and_then(|image| {
                    image
                        .save(&path)
                        .map_err(|e| anyhow::anyhow!("{e}"))
                        .map(|_| ())
                }),
            LayoutImageExportFormat::Svg => self
                .render_layout_svg(&layout, &selected_layers)
                .and_then(|svg| std::fs::write(&path, svg).map_err(|e| anyhow::anyhow!("{e}"))),
            LayoutImageExportFormat::Pdf => self
                .render_layout_pdf(&layout, &selected_layers)
                .and_then(|pdf| std::fs::write(&path, pdf).map_err(|e| anyhow::anyhow!("{e}"))),
        };

        match result {
            Ok(()) => {
                self.status_msg = format!("{}: {}", export_text(lang, "saved"), path.display());
            }
            Err(e) => {
                self.status_msg = format!("{}: {e}", export_text(lang, "failed"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_layout_geometry(
        &self,
        layout: &KeyboardLayout,
        selected_layers: &[usize],
    ) -> anyhow::Result<ExportGeometry> {
        let bounds = export_layout_bounds(
            layout,
            self.layout_options_value,
            &self.encoder_visibility,
            &self.module_settings,
        )
        .ok_or_else(|| anyhow::anyhow!("layout has no visible keys"))?;
        let span_x = (bounds.right() - bounds.left()).max(1.0);
        let span_y = (bounds.bottom() - bounds.top()).max(1.0);
        let header_h = if self.app_settings.layout_image_export.show_layer_names {
            EXPORT_LAYER_HEADER_H
        } else {
            EXPORT_LAYER_HEADER_H_HIDDEN
        };
        let layout_w = span_x * EXPORT_LAYOUT_UNIT;
        let layout_h = span_y * EXPORT_LAYOUT_UNIT;
        let width = (layout_w + EXPORT_MARGIN * 2.0)
            .ceil()
            .clamp(1.0, EXPORT_MAX_SIDE);
        let height = (EXPORT_MARGIN * 2.0
            + selected_layers.len() as f32 * (header_h + layout_h)
            + selected_layers.len().saturating_sub(1) as f32 * EXPORT_LAYER_GAP)
            .ceil()
            .clamp(1.0, EXPORT_MAX_SIDE);
        if width >= EXPORT_MAX_SIDE || height >= EXPORT_MAX_SIDE {
            anyhow::bail!("export image would be too large");
        }

        let dark = match self.app_settings.layout_image_export.theme {
            LayoutImageExportTheme::Current => self.dark_mode,
            LayoutImageExportTheme::Light => false,
            LayoutImageExportTheme::Dark => true,
        };
        Ok(ExportGeometry {
            bounds,
            width,
            height,
            layout_h,
            header_h,
            palette: ExportPalette::new(dark),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render_layout_svg(
        &self,
        layout: &KeyboardLayout,
        selected_layers: &[usize],
    ) -> anyhow::Result<String> {
        let font = FontArc::try_from_slice(include_bytes!("../../assets/DejaVuSans.ttf"))
            .map_err(|_| anyhow::anyhow!("failed to load export font"))?;
        let geometry = self.export_layout_geometry(layout, selected_layers)?;
        let palette = geometry.palette;

        let mut svg = String::new();
        writeln!(svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="0 0 {:.0} {:.0}">"#,
            geometry.width, geometry.height, geometry.width, geometry.height
        )?;
        writeln!(
            svg,
            r#"<rect width="100%" height="100%" fill="{}"/>"#,
            svg_color(palette.background)
        )?;

        for (section_idx, layer_idx) in selected_layers.iter().copied().enumerate() {
            let section_y = EXPORT_MARGIN
                + section_idx as f32 * (geometry.header_h + geometry.layout_h + EXPORT_LAYER_GAP);
            if self.app_settings.layout_image_export.show_layer_names {
                let title =
                    layer_export_label(self.app_settings.language, &self.layer_names, layer_idx);
                svg_text_centered_rotated(
                    &mut svg,
                    &title,
                    geometry.width * 0.5,
                    section_y + 18.0,
                    18.0,
                    palette.title_text,
                    0.0,
                )?;
            }
            let layout_y = section_y + geometry.header_h;
            self.write_export_layer_svg(
                &mut svg,
                &font,
                layout,
                geometry.bounds,
                layer_idx,
                layout_y,
                palette,
            )?;
        }

        writeln!(svg, "</svg>")?;
        Ok(svg)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_export_layer_svg(
        &self,
        svg: &mut String,
        font: &FontArc,
        layout: &KeyboardLayout,
        bounds: egui::Rect,
        layer_idx: usize,
        layout_y: f32,
        palette: ExportPalette,
    ) -> anyhow::Result<()> {
        let encoder_groups = export_encoder_groups(
            layout,
            bounds,
            layout_y,
            self.layout_options_value,
            &self.encoder_visibility,
            &self.module_settings,
        );

        for (key_idx, key) in layout.keys.iter().enumerate() {
            if !Self::layout_condition_visible(
                layout,
                key.layout_condition,
                self.layout_options_value,
            ) {
                continue;
            }
            let rect = export_item_rect(
                key.x,
                key.y,
                key.w,
                key.h,
                key.rotation,
                key.rotation_x,
                key.rotation_y,
                bounds,
                layout_y,
            );
            write_rotated_rect_svg(svg, rect, key.rotation, palette)?;

            let binding = layout.get_key_binding(layer_idx, key_idx);
            let (label, dimmed) = if binding.is_no() {
                (String::new(), false)
            } else if binding.is_transparent() {
                let fallback = (0..layer_idx)
                    .rev()
                    .map(|fallback_layer| layout.get_key_binding(fallback_layer, key_idx))
                    .find(|fallback| !fallback.is_no() && !fallback.is_transparent());
                match fallback {
                    Some(fallback_binding) => (
                        export_key_binding_label_with_macro_names(
                            fallback_binding,
                            &layout.custom_keycodes,
                            &self.layer_names,
                            &self.keycode_picker.macro_names,
                            &self.keycode_picker.tap_dance_names,
                            self.app_settings.layout_image_export.key_legend_layout,
                        ),
                        true,
                    ),
                    None => (String::new(), false),
                }
            } else {
                (
                    export_key_binding_label_with_macro_names(
                        binding,
                        &layout.custom_keycodes,
                        &self.layer_names,
                        &self.keycode_picker.macro_names,
                        &self.keycode_picker.tap_dance_names,
                        self.app_settings.layout_image_export.key_legend_layout,
                    ),
                    false,
                )
            };
            if !label.is_empty() {
                let label = number_row_shifted_label(
                    label,
                    self.app_settings.show_shifted_number_symbols,
                    self.app_settings.layout_image_export.key_legend_layout,
                );
                write_key_label_svg(
                    svg,
                    font,
                    rect,
                    key.rotation.to_radians(),
                    &label,
                    palette,
                    dimmed,
                )?;
            }
        }

        for group in encoder_groups {
            write_encoder_svg(svg, font, layout, layer_idx, &group, self, palette)?;
        }

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render_layout_image(
        &self,
        layout: &KeyboardLayout,
        selected_layers: &[usize],
    ) -> anyhow::Result<RgbaImage> {
        let font = FontArc::try_from_slice(include_bytes!("../../assets/DejaVuSans.ttf"))
            .map_err(|_| anyhow::anyhow!("failed to load export font"))?;
        let geometry = self.export_layout_geometry(layout, selected_layers)?;
        let palette = geometry.palette;
        let mut image = RgbaImage::from_pixel(
            geometry.width as u32,
            geometry.height as u32,
            palette.background,
        );

        for (section_idx, layer_idx) in selected_layers.iter().copied().enumerate() {
            let section_y = EXPORT_MARGIN
                + section_idx as f32 * (geometry.header_h + geometry.layout_h + EXPORT_LAYER_GAP);
            if self.app_settings.layout_image_export.show_layer_names {
                let title =
                    layer_export_label(self.app_settings.language, &self.layer_names, layer_idx);
                draw_text_centered_rotated(
                    &mut image,
                    &font,
                    &title,
                    geometry.width * 0.5,
                    section_y + 18.0,
                    18.0,
                    palette.title_text,
                    0.0,
                );
            }
            let layout_y = section_y + geometry.header_h;
            self.draw_export_layer(
                &mut image,
                &font,
                layout,
                geometry.bounds,
                layer_idx,
                layout_y,
                palette,
            );
        }

        Ok(image)
    }

    /// One A4 page per selected layer, each page embedding that layer's
    /// raster render, so the file is ready for printing as-is.
    #[cfg(not(target_arch = "wasm32"))]
    fn render_layout_pdf(
        &self,
        layout: &KeyboardLayout,
        selected_layers: &[usize],
    ) -> anyhow::Result<Vec<u8>> {
        // Every page is a single layer of the same layout, so all pages share
        // one geometry. Compute it once up front so the PDF builder can enforce
        // the pixel budget from the declared size *before* rendering allocates a
        // page. Render one layer at a time so only one page's pixels are live.
        let geometry =
            self.export_layout_geometry(layout, &[*selected_layers.first().unwrap_or(&0)])?;
        let page_size = (geometry.width as u32, geometry.height as u32);
        crate::pdf::build_layer_pdf(
            selected_layers.len(),
            |_page| page_size,
            |page| self.render_layout_image(layout, &[selected_layers[page]]),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_export_layer(
        &self,
        image: &mut RgbaImage,
        font: &FontArc,
        layout: &KeyboardLayout,
        bounds: egui::Rect,
        layer_idx: usize,
        layout_y: f32,
        palette: ExportPalette,
    ) {
        let encoder_groups = export_encoder_groups(
            layout,
            bounds,
            layout_y,
            self.layout_options_value,
            &self.encoder_visibility,
            &self.module_settings,
        );

        for (key_idx, key) in layout.keys.iter().enumerate() {
            if !Self::layout_condition_visible(
                layout,
                key.layout_condition,
                self.layout_options_value,
            ) {
                continue;
            }
            let rect = export_item_rect(
                key.x,
                key.y,
                key.w,
                key.h,
                key.rotation,
                key.rotation_x,
                key.rotation_y,
                bounds,
                layout_y,
            );
            draw_rotated_rounded_rect(
                image,
                rect.center(),
                rect.size(),
                key.rotation.to_radians(),
                7.0,
                palette.key_fill,
                palette.key_stroke,
                1.4,
            );

            let binding = layout.get_key_binding(layer_idx, key_idx);
            let (label, dimmed) = if binding.is_no() {
                (String::new(), false)
            } else if binding.is_transparent() {
                let fallback = (0..layer_idx)
                    .rev()
                    .map(|fallback_layer| layout.get_key_binding(fallback_layer, key_idx))
                    .find(|fallback| !fallback.is_no() && !fallback.is_transparent());
                match fallback {
                    Some(fallback_binding) => (
                        export_key_binding_label_with_macro_names(
                            fallback_binding,
                            &layout.custom_keycodes,
                            &self.layer_names,
                            &self.keycode_picker.macro_names,
                            &self.keycode_picker.tap_dance_names,
                            self.app_settings.layout_image_export.key_legend_layout,
                        ),
                        true,
                    ),
                    None => (String::new(), false),
                }
            } else {
                (
                    export_key_binding_label_with_macro_names(
                        binding,
                        &layout.custom_keycodes,
                        &self.layer_names,
                        &self.keycode_picker.macro_names,
                        &self.keycode_picker.tap_dance_names,
                        self.app_settings.layout_image_export.key_legend_layout,
                    ),
                    false,
                )
            };
            if !label.is_empty() {
                let label = number_row_shifted_label(
                    label,
                    self.app_settings.show_shifted_number_symbols,
                    self.app_settings.layout_image_export.key_legend_layout,
                );
                draw_key_label_export(
                    image,
                    font,
                    rect,
                    key.rotation.to_radians(),
                    &label,
                    palette,
                    dimmed,
                );
            }
        }

        for group in encoder_groups {
            draw_encoder_export(image, font, layout, layer_idx, &group, self, palette);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
struct ExportPalette {
    background: Rgba<u8>,
    key_fill: Rgba<u8>,
    key_stroke: Rgba<u8>,
    text: Rgba<u8>,
    top_text: Rgba<u8>,
    dim_text: Rgba<u8>,
    title_text: Rgba<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ExportPalette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: rgba(26, 26, 29),
                key_fill: rgba(48, 48, 52),
                key_stroke: rgba(68, 68, 74),
                text: rgba(239, 233, 232),
                top_text: rgba(142, 142, 158),
                dim_text: rgba(86, 82, 88),
                title_text: rgba(239, 233, 232),
            }
        } else {
            Self {
                background: rgba(246, 245, 244),
                key_fill: rgba(255, 255, 255),
                key_stroke: rgba(222, 222, 226),
                text: rgba(28, 28, 32),
                top_text: rgba(118, 118, 136),
                dim_text: rgba(188, 188, 196),
                title_text: rgba(32, 32, 36),
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
struct ExportGeometry {
    bounds: egui::Rect,
    width: f32,
    height: f32,
    layout_h: f32,
    header_h: f32,
    palette: ExportPalette,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct ExportEncoderGroup {
    rect: egui::Rect,
    ccw: Option<usize>,
    cw: Option<usize>,
}

#[cfg(not(target_arch = "wasm32"))]
fn rgba(r: u8, g: u8, b: u8) -> Rgba<u8> {
    Rgba([r, g, b, 255])
}

#[cfg(not(target_arch = "wasm32"))]
fn export_layout_bounds(
    layout: &KeyboardLayout,
    layout_options_value: Option<u32>,
    encoder_visibility: &[bool],
    module_settings: &ModuleSettingsState,
) -> Option<egui::Rect> {
    let mut rect: Option<egui::Rect> = None;
    for key in &layout.keys {
        if !EntropyApp::layout_condition_visible(layout, key.layout_condition, layout_options_value)
        {
            continue;
        }
        let item_rect = layout_aabb_rect(
            key.x,
            key.y,
            key.w,
            key.h,
            key.rotation,
            key.rotation_x,
            key.rotation_y,
        );
        rect = Some(rect.map(|rect| rect.union(item_rect)).unwrap_or(item_rect));
    }
    for encoder in &layout.encoders {
        if !EntropyApp::layout_condition_visible(
            layout,
            encoder.layout_condition,
            layout_options_value,
        ) || !EntropyApp::module_settings_encoder_visible(
            module_settings,
            layout,
            encoder.encoder_idx,
        ) || !encoder_visibility
            .get(encoder.encoder_idx as usize)
            .copied()
            .unwrap_or(true)
        {
            continue;
        }
        let item_rect = layout_aabb_rect(
            encoder.x,
            encoder.y,
            encoder.w,
            encoder.h,
            encoder.rotation,
            encoder.rotation_x,
            encoder.rotation_y,
        );
        rect = Some(rect.map(|rect| rect.union(item_rect)).unwrap_or(item_rect));
    }
    rect
}

#[cfg(not(target_arch = "wasm32"))]
fn layout_aabb_rect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
) -> egui::Rect {
    let (x1, y1, x2, y2) = rotated_item_aabb(x, y, w, h, rotation, rotation_x, rotation_y);
    egui::Rect::from_min_max(egui::pos2(x1, y1), egui::pos2(x2, y2))
}

#[cfg(not(target_arch = "wasm32"))]
fn export_item_rect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
    bounds: egui::Rect,
    layout_y: f32,
) -> egui::Rect {
    let (center_x, center_y) =
        rotate_layout_point(x + w * 0.5, y + h * 0.5, rotation_x, rotation_y, rotation);
    let size = egui::vec2(
        (w * EXPORT_LAYOUT_UNIT - EXPORT_KEY_PADDING * 2.0).max(1.0),
        (h * EXPORT_LAYOUT_UNIT - EXPORT_KEY_PADDING * 2.0).max(1.0),
    );
    egui::Rect::from_center_size(
        egui::pos2(
            EXPORT_MARGIN + (center_x - bounds.left()) * EXPORT_LAYOUT_UNIT,
            layout_y + (center_y - bounds.top()) * EXPORT_LAYOUT_UNIT,
        ),
        size,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn export_encoder_groups(
    layout: &KeyboardLayout,
    bounds: egui::Rect,
    layout_y: f32,
    layout_options_value: Option<u32>,
    encoder_visibility: &[bool],
    module_settings: &ModuleSettingsState,
) -> Vec<ExportEncoderGroup> {
    let mut groups: Vec<(u8, ExportEncoderGroup)> = Vec::new();
    for (encoder_idx, encoder) in layout.encoders.iter().enumerate() {
        if !EntropyApp::layout_condition_visible(
            layout,
            encoder.layout_condition,
            layout_options_value,
        ) || !EntropyApp::module_settings_encoder_visible(
            module_settings,
            layout,
            encoder.encoder_idx,
        ) || !encoder_visibility
            .get(encoder.encoder_idx as usize)
            .copied()
            .unwrap_or(true)
        {
            continue;
        }
        let layout_rect = layout_aabb_rect(
            encoder.x,
            encoder.y,
            encoder.w,
            encoder.h,
            encoder.rotation,
            encoder.rotation_x,
            encoder.rotation_y,
        );
        let rect = egui::Rect::from_min_max(
            egui::pos2(
                EXPORT_MARGIN + (layout_rect.left() - bounds.left()) * EXPORT_LAYOUT_UNIT,
                layout_y + (layout_rect.top() - bounds.top()) * EXPORT_LAYOUT_UNIT,
            ),
            egui::pos2(
                EXPORT_MARGIN + (layout_rect.right() - bounds.left()) * EXPORT_LAYOUT_UNIT,
                layout_y + (layout_rect.bottom() - bounds.top()) * EXPORT_LAYOUT_UNIT,
            ),
        );
        if let Some((_, group)) = groups
            .iter_mut()
            .find(|(idx, _)| *idx == encoder.encoder_idx)
        {
            group.rect = group.rect.union(rect);
            if encoder.direction == 0 {
                group.ccw = Some(encoder_idx);
            } else {
                group.cw = Some(encoder_idx);
            }
        } else {
            groups.push((
                encoder.encoder_idx,
                ExportEncoderGroup {
                    rect,
                    ccw: if encoder.direction == 0 {
                        Some(encoder_idx)
                    } else {
                        None
                    },
                    cw: if encoder.direction == 0 {
                        None
                    } else {
                        Some(encoder_idx)
                    },
                },
            ));
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_encoder_export(
    image: &mut RgbaImage,
    font: &FontArc,
    layout: &KeyboardLayout,
    layer_idx: usize,
    group: &ExportEncoderGroup,
    app: &EntropyApp,
    palette: ExportPalette,
) {
    let center = group.rect.center();
    let radius = group.rect.width().min(group.rect.height()) * LAYOUT_ENCODER_RADIUS_FACTOR;
    draw_circle(
        image,
        center.x,
        center.y,
        radius,
        palette.key_fill,
        palette.key_stroke,
        1.4,
    );
    draw_line_segment(
        image,
        center.x - radius * 0.58,
        center.y,
        center.x + radius * 0.58,
        center.y,
        1.2,
        palette.key_stroke,
    );

    let encoder_value_label = |kc: u16| -> String {
        export_keycode_label_with_macro_names(
            kc,
            &layout.custom_keycodes,
            &app.layer_names,
            &app.keycode_picker.macro_names,
            &app.keycode_picker.tap_dance_names,
            app.app_settings.layout_image_export.key_legend_layout,
        )
        .replace('\n', " ")
    };
    let encoder_label = |visual_idx: usize, kc: u16| -> (String, bool) {
        match kc {
            0x0000 => (String::new(), false),
            0x0001 => {
                let fallback = (0..layer_idx)
                    .rev()
                    .map(|fallback_layer| layout.get_encoder_keycode(fallback_layer, visual_idx))
                    .find(|fallback| !matches!(*fallback, 0x0000 | 0x0001));
                match fallback {
                    Some(fallback_kc) => (encoder_value_label(fallback_kc), true),
                    None => ("▽".to_string(), false),
                }
            }
            value => (encoder_value_label(value), false),
        }
    };

    if let Some(visual_idx) = group.cw {
        let (label, dimmed) = encoder_label(
            visual_idx,
            layout.get_encoder_keycode(layer_idx, visual_idx),
        );
        draw_text_centered_rotated(
            image,
            font,
            &label,
            center.x,
            center.y - radius * 0.34,
            fit_text_size(font, &label, 10.5, radius * 1.35, 6.5),
            if dimmed {
                palette.dim_text
            } else {
                palette.text
            },
            0.0,
        );
    }
    if let Some(visual_idx) = group.ccw {
        let (label, dimmed) = encoder_label(
            visual_idx,
            layout.get_encoder_keycode(layer_idx, visual_idx),
        );
        draw_text_centered_rotated(
            image,
            font,
            &label,
            center.x,
            center.y + radius * 0.38,
            fit_text_size(font, &label, 10.5, radius * 1.35, 6.5),
            if dimmed {
                palette.dim_text
            } else {
                palette.text
            },
            0.0,
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_rotated_rect_svg(
    svg: &mut String,
    rect: egui::Rect,
    rotation_deg: f32,
    palette: ExportPalette,
) -> anyhow::Result<()> {
    let center = rect.center();
    let transform = if rotation_deg.abs() > 0.001 {
        format!(
            r#" transform="rotate({:.3} {:.2} {:.2})""#,
            rotation_deg, center.x, center.y
        )
    } else {
        String::new()
    };
    writeln!(
        svg,
        r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="7" ry="7" fill="{}" stroke="{}" stroke-width="1.4"{} />"#,
        rect.left(),
        rect.top(),
        rect.width(),
        rect.height(),
        svg_color(palette.key_fill),
        svg_color(palette.key_stroke),
        transform
    )?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_encoder_svg(
    svg: &mut String,
    font: &FontArc,
    layout: &KeyboardLayout,
    layer_idx: usize,
    group: &ExportEncoderGroup,
    app: &EntropyApp,
    palette: ExportPalette,
) -> anyhow::Result<()> {
    let center = group.rect.center();
    let radius = group.rect.width().min(group.rect.height()) * LAYOUT_ENCODER_RADIUS_FACTOR;
    writeln!(
        svg,
        r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{}" stroke="{}" stroke-width="1.4"/>"#,
        center.x,
        center.y,
        radius,
        svg_color(palette.key_fill),
        svg_color(palette.key_stroke)
    )?;
    writeln!(
        svg,
        r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="1.2" stroke-linecap="round"/>"#,
        center.x - radius * 0.58,
        center.y,
        center.x + radius * 0.58,
        center.y,
        svg_color(palette.key_stroke)
    )?;

    let encoder_value_label = |kc: u16| -> String {
        export_keycode_label_with_macro_names(
            kc,
            &layout.custom_keycodes,
            &app.layer_names,
            &app.keycode_picker.macro_names,
            &app.keycode_picker.tap_dance_names,
            app.app_settings.layout_image_export.key_legend_layout,
        )
        .replace('\n', " ")
    };
    let encoder_label = |visual_idx: usize, kc: u16| -> (String, bool) {
        match kc {
            0x0000 => (String::new(), false),
            0x0001 => {
                let fallback = (0..layer_idx)
                    .rev()
                    .map(|fallback_layer| layout.get_encoder_keycode(fallback_layer, visual_idx))
                    .find(|fallback| !matches!(*fallback, 0x0000 | 0x0001));
                match fallback {
                    Some(fallback_kc) => (encoder_value_label(fallback_kc), true),
                    None => ("▽".to_string(), false),
                }
            }
            value => (encoder_value_label(value), false),
        }
    };

    if let Some(visual_idx) = group.cw {
        let (label, dimmed) = encoder_label(
            visual_idx,
            layout.get_encoder_keycode(layer_idx, visual_idx),
        );
        svg_text_centered_rotated(
            svg,
            &label,
            center.x,
            center.y - radius * 0.34,
            fit_text_size(font, &label, 10.5, radius * 1.35, 6.5),
            if dimmed {
                palette.dim_text
            } else {
                palette.text
            },
            0.0,
        )?;
    }
    if let Some(visual_idx) = group.ccw {
        let (label, dimmed) = encoder_label(
            visual_idx,
            layout.get_encoder_keycode(layer_idx, visual_idx),
        );
        svg_text_centered_rotated(
            svg,
            &label,
            center.x,
            center.y + radius * 0.38,
            fit_text_size(font, &label, 10.5, radius * 1.35, 6.5),
            if dimmed {
                palette.dim_text
            } else {
                palette.text
            },
            0.0,
        )?;
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_key_label_svg(
    svg: &mut String,
    font: &FontArc,
    rect: egui::Rect,
    rotation: f32,
    label: &str,
    palette: ExportPalette,
    dimmed: bool,
) -> anyhow::Result<()> {
    let lines: Vec<&str> = label
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Ok(());
    }
    let center = rect.center();
    let scale = (rect.width().min(rect.height()) / 54.0).clamp(0.82, 1.35);
    let available_width = (rect.width() - 11.0 * scale).max(1.0);
    let main = if dimmed {
        palette.dim_text
    } else {
        palette.text
    };
    let top = if dimmed {
        palette.dim_text
    } else {
        palette.top_text
    };

    match lines.as_slice() {
        [only] => {
            let base = if *only == "↵" { 20.0 } else { 12.4 } * scale;
            let size = fit_text_size(font, only, base, available_width, 7.0 * scale);
            svg_text_centered_rotated(svg, only, center.x, center.y, size, main, rotation)?;
        }
        [upper, lower] => {
            let upper_size = fit_text_size(font, upper, 9.0 * scale, available_width, 5.8 * scale);
            let lower_size = fit_text_size(font, lower, 12.0 * scale, available_width, 6.8 * scale);
            let upper_offset = rotated_offset(0.0, -7.0 * scale, rotation);
            let lower_offset = rotated_offset(0.0, 6.0 * scale, rotation);
            svg_text_centered_rotated(
                svg,
                upper,
                center.x + upper_offset.x,
                center.y + upper_offset.y,
                upper_size,
                top,
                rotation,
            )?;
            svg_text_centered_rotated(
                svg,
                lower,
                center.x + lower_offset.x,
                center.y + lower_offset.y,
                lower_size,
                main,
                rotation,
            )?;
        }
        _ => {
            let line_count = lines.len().min(3);
            for (idx, line) in lines.into_iter().take(3).enumerate() {
                let y_offset = (idx as f32 - (line_count as f32 - 1.0) * 0.5) * 11.0 * scale;
                let offset = rotated_offset(0.0, y_offset, rotation);
                let base = if idx == 0 { 8.3 } else { 9.5 } * scale;
                let size = fit_text_size(font, line, base, available_width, 5.5 * scale);
                svg_text_centered_rotated(
                    svg,
                    line,
                    center.x + offset.x,
                    center.y + offset.y,
                    size,
                    if idx == 0 { top } else { main },
                    rotation,
                )?;
            }
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn svg_text_centered_rotated(
    svg: &mut String,
    text: &str,
    center_x: f32,
    center_y: f32,
    size: f32,
    color: Rgba<u8>,
    rotation: f32,
) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let transform = if rotation.abs() > 0.001 {
        format!(
            r#" transform="rotate({:.3} {:.2} {:.2})""#,
            rotation.to_degrees(),
            center_x,
            center_y
        )
    } else {
        String::new()
    };
    writeln!(
        svg,
        r#"<text x="{:.2}" y="{:.2}" text-anchor="middle" dominant-baseline="central" font-family="Inter, Arial, sans-serif" font-size="{:.2}" fill="{}"{}>{}</text>"#,
        center_x,
        center_y,
        size,
        svg_color(color),
        transform,
        escape_xml_text(text)
    )?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn svg_color(color: Rgba<u8>) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

#[cfg(not(target_arch = "wasm32"))]
fn escape_xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_key_label_export(
    image: &mut RgbaImage,
    font: &FontArc,
    rect: egui::Rect,
    rotation: f32,
    label: &str,
    palette: ExportPalette,
    dimmed: bool,
) {
    let lines: Vec<&str> = label
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return;
    }
    let center = rect.center();
    let scale = (rect.width().min(rect.height()) / 54.0).clamp(0.82, 1.35);
    let available_width = (rect.width() - 11.0 * scale).max(1.0);
    let main = if dimmed {
        palette.dim_text
    } else {
        palette.text
    };
    let top = if dimmed {
        palette.dim_text
    } else {
        palette.top_text
    };

    match lines.as_slice() {
        [only] => {
            let base = if *only == "↵" { 20.0 } else { 12.4 } * scale;
            let size = fit_text_size(font, only, base, available_width, 7.0 * scale);
            draw_text_centered_rotated(image, font, only, center.x, center.y, size, main, rotation);
        }
        [upper, lower] => {
            let upper_size = fit_text_size(font, upper, 9.0 * scale, available_width, 5.8 * scale);
            let lower_size = fit_text_size(font, lower, 12.0 * scale, available_width, 6.8 * scale);
            let upper_offset = rotated_offset(0.0, -7.0 * scale, rotation);
            let lower_offset = rotated_offset(0.0, 6.0 * scale, rotation);
            draw_text_centered_rotated(
                image,
                font,
                upper,
                center.x + upper_offset.x,
                center.y + upper_offset.y,
                upper_size,
                top,
                rotation,
            );
            draw_text_centered_rotated(
                image,
                font,
                lower,
                center.x + lower_offset.x,
                center.y + lower_offset.y,
                lower_size,
                main,
                rotation,
            );
        }
        _ => {
            let line_count = lines.len().min(3);
            for (idx, line) in lines.into_iter().take(3).enumerate() {
                let y_offset = (idx as f32 - (line_count as f32 - 1.0) * 0.5) * 11.0 * scale;
                let offset = rotated_offset(0.0, y_offset, rotation);
                let base = if idx == 0 { 8.3 } else { 9.5 } * scale;
                let size = fit_text_size(font, line, base, available_width, 5.5 * scale);
                draw_text_centered_rotated(
                    image,
                    font,
                    line,
                    center.x + offset.x,
                    center.y + offset.y,
                    size,
                    if idx == 0 { top } else { main },
                    rotation,
                );
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn fit_text_size(
    font: &FontArc,
    text: &str,
    base_size: f32,
    available_width: f32,
    min_size: f32,
) -> f32 {
    let width = measure_text(font, text, base_size).max(1.0);
    if width <= available_width {
        base_size
    } else {
        (base_size * available_width.max(1.0) / width).clamp(min_size, base_size)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn measure_text(font: &FontArc, text: &str, size: f32) -> f32 {
    let scaled = font.as_scaled(size);
    text.chars()
        .map(|ch| scaled.h_advance(font.glyph_id(ch)))
        .sum()
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_text_centered_rotated(
    image: &mut RgbaImage,
    font: &FontArc,
    text: &str,
    center_x: f32,
    center_y: f32,
    size: f32,
    color: Rgba<u8>,
    rotation: f32,
) {
    if text.trim().is_empty() {
        return;
    }
    let scaled = font.as_scaled(size);
    let width = measure_text(font, text, size);
    let mut cursor_x = center_x - width * 0.5;
    let baseline_y = center_y + size * 0.36;
    let cos = rotation.cos();
    let sin = rotation.sin();
    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        let advance = scaled.h_advance(glyph_id);
        let glyph = glyph_id.with_scale_and_position(size, point(cursor_x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|x, y, coverage| {
                let src_x = bounds.min.x + x as f32;
                let src_y = bounds.min.y + y as f32;
                let dx = src_x - center_x;
                let dy = src_y - center_y;
                let dst_x = center_x + dx * cos - dy * sin;
                let dst_y = center_y + dx * sin + dy * cos;
                blend_pixel(
                    image,
                    dst_x.round() as i32,
                    dst_y.round() as i32,
                    color,
                    coverage,
                );
            });
        }
        cursor_x += advance;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_rotated_rounded_rect(
    image: &mut RgbaImage,
    center: egui::Pos2,
    size: egui::Vec2,
    rotation: f32,
    radius: f32,
    fill: Rgba<u8>,
    stroke: Rgba<u8>,
    stroke_width: f32,
) {
    let half_w = size.x * 0.5;
    let half_h = size.y * 0.5;
    let extent = (half_w * half_w + half_h * half_h).sqrt() + stroke_width + 2.0;
    let min_x = (center.x - extent).floor() as i32;
    let max_x = (center.x + extent).ceil() as i32;
    let min_y = (center.y - extent).floor() as i32;
    let max_y = (center.y + extent).ceil() as i32;
    let cos = rotation.cos();
    let sin = rotation.sin();
    let radius = radius.min(half_w).min(half_h).max(0.0);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let dx = px - center.x;
            let dy = py - center.y;
            let local_x = dx * cos + dy * sin;
            let local_y = -dx * sin + dy * cos;
            let dist = rounded_rect_sdf(local_x, local_y, half_w, half_h, radius);
            if dist <= 0.75 {
                let coverage = (0.75 - dist).clamp(0.0, 1.0);
                let color = if dist > -stroke_width { stroke } else { fill };
                blend_pixel(image, x, y, color, coverage);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn rounded_rect_sdf(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = x.abs() - (half_w - radius);
    let qy = y.abs() - (half_h - radius);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - radius
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_circle(
    image: &mut RgbaImage,
    center_x: f32,
    center_y: f32,
    radius: f32,
    fill: Rgba<u8>,
    stroke: Rgba<u8>,
    stroke_width: f32,
) {
    let extent = radius + stroke_width + 2.0;
    let min_x = (center_x - extent).floor() as i32;
    let max_x = (center_x + extent).ceil() as i32;
    let min_y = (center_y - extent).floor() as i32;
    let max_y = (center_y + extent).ceil() as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let dist = (dx * dx + dy * dy).sqrt() - radius;
            if dist <= 0.75 {
                let coverage = (0.75 - dist).clamp(0.0, 1.0);
                let color = if dist > -stroke_width { stroke } else { fill };
                blend_pixel(image, x, y, color, coverage);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_line_segment(
    image: &mut RgbaImage,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: f32,
    color: Rgba<u8>,
) {
    let min_x = x1.min(x2).floor() as i32 - width.ceil() as i32 - 1;
    let max_x = x1.max(x2).ceil() as i32 + width.ceil() as i32 + 1;
    let min_y = y1.min(y2).floor() as i32 - width.ceil() as i32 - 1;
    let max_y = y1.max(y2).ceil() as i32 + width.ceil() as i32 + 1;
    let vx = x2 - x1;
    let vy = y2 - y1;
    let len2 = (vx * vx + vy * vy).max(1.0);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let t = (((px - x1) * vx + (py - y1) * vy) / len2).clamp(0.0, 1.0);
            let cx = x1 + vx * t;
            let cy = y1 + vy * t;
            let dx = px - cx;
            let dy = py - cy;
            let dist = (dx * dx + dy * dy).sqrt() - width * 0.5;
            if dist <= 0.75 {
                blend_pixel(image, x, y, color, (0.75 - dist).clamp(0.0, 1.0));
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn blend_pixel(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>, coverage: f32) {
    if x < 0 || y < 0 {
        return;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= image.width() || y >= image.height() {
        return;
    }
    let alpha = (color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let dst = image.get_pixel_mut(x, y);
    let inv = 1.0 - alpha;
    dst[0] = (color[0] as f32 * alpha + dst[0] as f32 * inv).round() as u8;
    dst[1] = (color[1] as f32 * alpha + dst[1] as f32 * inv).round() as u8;
    dst[2] = (color[2] as f32 * alpha + dst[2] as f32 * inv).round() as u8;
    dst[3] = 255;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn every_export_format_has_a_distinct_extension_and_filter() {
        // Regression guard for the #78/#80 merge: PNG, SVG, and PDF must each
        // resolve to a picker filter + extension so no format is dropped when the
        // export routes through the shared async worker dialog.
        let png = layout_image_export_descriptor(LayoutImageExportFormat::Png);
        let svg = layout_image_export_descriptor(LayoutImageExportFormat::Svg);
        let pdf = layout_image_export_descriptor(LayoutImageExportFormat::Pdf);

        assert_eq!(png, ("png", "PNG image"));
        assert_eq!(svg, ("svg", "SVG image"));
        assert_eq!(pdf, ("pdf", "PDF document"));

        // Extensions are unique, so a normalized path can never collapse two
        // formats onto the same file type.
        let extensions = [png.0, svg.0, pdf.0];
        for (i, a) in extensions.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &extensions[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn picker_and_writer_agree_on_the_extension_for_every_format() {
        // The picker (spawn) advertises `extension` in its filter and the writer
        // normalizes the chosen path to the same `extension`. Both read it from
        // this one descriptor, so PDF can never get an SVG picker or a PNG file
        // name — the exact drift the #78/#80 merge could have introduced.
        for format in [
            LayoutImageExportFormat::Png,
            LayoutImageExportFormat::Svg,
            LayoutImageExportFormat::Pdf,
        ] {
            let (extension, filter) = layout_image_export_descriptor(format);
            assert!(!extension.is_empty(), "{format:?} has no extension");
            assert!(!filter.is_empty(), "{format:?} has no picker filter");
            // The picker builds "<name>.<extension>" and the writer forces the
            // same extension, so a round-trip is idempotent.
            let file_name = format!("board-layout.{extension}");
            assert!(file_name.ends_with(&format!(".{extension}")));
        }
    }
}
