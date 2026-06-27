use super::*;

fn yes_no(lang: crate::i18n::Language, value: bool) -> &'static str {
    match (lang, value) {
        (crate::i18n::Language::Russian, true) => "да",
        (crate::i18n::Language::Russian, false) => "нет",
        (crate::i18n::Language::English, true) => "yes",
        (crate::i18n::Language::English, false) => "no",
    }
}

fn text_or_unknown(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Unknown"
    } else {
        trimmed
    }
}

fn about_title(lang: crate::i18n::Language, product: &str) -> String {
    let product = text_or_unknown(product);
    match lang {
        crate::i18n::Language::Russian => format!("Об устройстве {product}"),
        crate::i18n::Language::English => format!("About {product}"),
    }
}

fn about_empty_title(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Нет данных об устройстве",
        crate::i18n::Language::English => "No device information",
    }
}

fn about_empty_detail(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Подключите клавиатуру, чтобы открыть About Device",
        crate::i18n::Language::English => "Connect a keyboard to open About Device",
    }
}

fn about_entropy_title(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Об Entropy",
        crate::i18n::Language::English => "About Entropy",
    }
}

fn about_entropy_description(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Версия приложения и сведения о сборке",
        crate::i18n::Language::English => "Application version and build information",
    }
}

fn device_about_description(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Информация от выбранной подключенной клавиатуры",
        crate::i18n::Language::English => "Information reported by the selected connected keyboard",
    }
}

struct AboutRow {
    label: &'static str,
    tooltip: &'static str,
    value: String,
    monospace: bool,
}

fn localized_row(
    lang: crate::i18n::Language,
    ru_label: &'static str,
    en_label: &'static str,
    ru_tooltip: &'static str,
    en_tooltip: &'static str,
    value: impl Into<String>,
) -> AboutRow {
    match lang {
        crate::i18n::Language::Russian => AboutRow {
            label: ru_label,
            tooltip: ru_tooltip,
            value: value.into(),
            monospace: false,
        },
        crate::i18n::Language::English => AboutRow {
            label: en_label,
            tooltip: en_tooltip,
            value: value.into(),
            monospace: false,
        },
    }
}

fn localized_monospace_row(
    lang: crate::i18n::Language,
    ru_label: &'static str,
    en_label: &'static str,
    ru_tooltip: &'static str,
    en_tooltip: &'static str,
    value: impl Into<String>,
) -> AboutRow {
    let mut row = localized_row(lang, ru_label, en_label, ru_tooltip, en_tooltip, value);
    row.monospace = true;
    row
}

fn not_reported(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "не сообщается",
        crate::i18n::Language::English => "not reported",
    }
}

fn bytes_label(lang: crate::i18n::Language, bytes: u16) -> String {
    match lang {
        crate::i18n::Language::Russian => format!("{bytes} байт"),
        crate::i18n::Language::English => format!("{bytes} bytes"),
    }
}

fn device_about_rows(lang: crate::i18n::Language, info: &DeviceAboutInfo) -> Vec<AboutRow> {
    let firmware_version = info
        .firmware_version
        .as_deref()
        .map(text_or_unknown)
        .unwrap_or_else(|| not_reported(lang));
    let macro_memory = info
        .macro_memory_bytes
        .map(|bytes| bytes_label(lang, bytes))
        .unwrap_or_else(|| not_reported(lang).to_owned());

    vec![
        localized_row(
            lang,
            "Производитель",
            "Manufacturer",
            "USB manufacturer string",
            "USB manufacturer string",
            text_or_unknown(&info.manufacturer),
        ),
        localized_row(
            lang,
            "Устройство",
            "Product",
            "USB product string",
            "USB product string",
            text_or_unknown(&info.product),
        ),
        localized_row(
            lang,
            "Версия прошивки",
            "Firmware version",
            "Версия из Vial JSON metadata",
            "Version from Vial JSON metadata",
            firmware_version,
        ),
        localized_monospace_row(
            lang,
            "VID",
            "VID",
            "USB vendor ID",
            "USB vendor ID",
            format!("{:04X}", info.vendor_id),
        ),
        localized_monospace_row(
            lang,
            "PID",
            "PID",
            "USB product ID",
            "USB product ID",
            format!("{:04X}", info.product_id),
        ),
        localized_monospace_row(
            lang,
            "Путь устройства",
            "Device path",
            "Текущий HID path выбранной клавиатуры",
            "Current HID path of the selected keyboard",
            text_or_unknown(&info.path),
        ),
        localized_row(
            lang,
            "VIA protocol",
            "VIA protocol",
            "Версия VIA protocol",
            "VIA protocol version",
            info.via_protocol.to_string(),
        ),
        localized_row(
            lang,
            "Vial protocol",
            "Vial protocol",
            "Версия Vial protocol",
            "Vial protocol version",
            info.vial_protocol.to_string(),
        ),
        localized_monospace_row(
            lang,
            "Vial keyboard ID",
            "Vial keyboard ID",
            "Идентификатор клавиатуры Vial",
            "Vial keyboard identifier",
            format!("{:016X}", info.keyboard_id),
        ),
        localized_row(
            lang,
            "Macro entries",
            "Macro entries",
            "Количество macro слотов",
            "Number of macro slots",
            info.macro_entries.to_string(),
        ),
        localized_row(
            lang,
            "Macro memory",
            "Macro memory",
            "Память, доступная для macro",
            "Memory available for macros",
            macro_memory,
        ),
        localized_row(
            lang,
            "Macro delays",
            "Macro delays",
            "Поддержка задержек в macro",
            "Macro delay support",
            yes_no(lang, info.supports_macro_delays),
        ),
        localized_row(
            lang,
            "2-byte macro keycodes",
            "2-byte macro keycodes",
            "Поддержка complex macro keycodes",
            "Complex macro keycode support",
            yes_no(lang, info.supports_macro_ext_keycodes),
        ),
        localized_row(
            lang,
            "Tap Dance",
            "Tap Dance",
            "Количество Tap Dance слотов",
            "Number of Tap Dance slots",
            info.tap_dance_entries.to_string(),
        ),
        localized_row(
            lang,
            "Combos",
            "Combos",
            "Количество Combo слотов",
            "Number of Combo slots",
            info.combo_entries.to_string(),
        ),
        localized_row(
            lang,
            "Key Overrides",
            "Key Overrides",
            "Количество Key Override слотов",
            "Number of Key Override slots",
            info.key_override_entries.to_string(),
        ),
        localized_row(
            lang,
            "Alt Repeat",
            "Alt Repeat",
            "Количество Alt Repeat слотов",
            "Number of Alt Repeat slots",
            info.alt_repeat_entries.to_string(),
        ),
        localized_row(
            lang,
            "Caps Word",
            "Caps Word",
            "Поддержка Caps Word",
            "Caps Word support",
            yes_no(lang, info.caps_word),
        ),
        localized_row(
            lang,
            "Layer Lock",
            "Layer Lock",
            "Поддержка Layer Lock",
            "Layer Lock support",
            yes_no(lang, info.layer_lock),
        ),
        localized_row(
            lang,
            "QMK Settings",
            "QMK Settings",
            "Поддержка Vial QMK Settings",
            "Vial QMK Settings support",
            yes_no(lang, info.qmk_settings),
        ),
    ]
}

fn about_entropy_rows(lang: crate::i18n::Language) -> Vec<AboutRow> {
    vec![
        localized_row(
            lang,
            "Приложение",
            "Application",
            "Название программы",
            "Application name",
            "Entropy",
        ),
        localized_monospace_row(
            lang,
            "Версия",
            "Version",
            "Версия из Cargo package metadata",
            "Version from Cargo package metadata",
            env!("CARGO_PKG_VERSION"),
        ),
    ]
}

fn draw_about_rows(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    rows: &[AboutRow],
) {
    let list = allocate_adaptive_settings_list_viewport(
        ui,
        id_salt,
        metrics,
        rows.len(),
        metrics.value(4.0),
    );

    crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
        ui.set_clip_rect(list.viewport);
        ui.set_min_size(list.content_rect.size());
        ui.spacing_mut().item_spacing.y = 0.0;
        let tooltip_enabled = !list.suppress_tooltips;
        let control_width = metrics.value(248.0);
        for row in &rows[list.first_visible_row..list.last_visible_row] {
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                list.row_content_width,
                list.row_height,
                row.label,
                true,
                tooltip_enabled.then_some(row.tooltip),
                control_width,
                |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut text = RichText::new(row.value.as_str())
                            .size(metrics.value(12.0))
                            .color(ui.visuals().text_color());
                        if row.monospace {
                            text = text.monospace();
                        }
                        let resp = ui.add(egui::Label::new(text).truncate());
                        if row.value.chars().count() > 20 {
                            resp.on_hover_text(row.value.as_str());
                        }
                    });
                },
            );
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
}

impl EntropyApp {
    pub(super) fn draw_about_device_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let Some(info) = self.device_about_info.clone() else {
            crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
                crate::ui_style::modal_empty_state(
                    ui,
                    about_empty_title(lang),
                    Some(about_empty_detail(lang)),
                );
            });
            return;
        };

        let title = about_title(lang, &info.product);
        let rows = device_about_rows(lang, &info);

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(RichText::new(title).size(metrics.value(18.0)).strong());
                ui.add_space(metrics.value(6.0));
                ui.label(
                    RichText::new(device_about_description(lang))
                        .size(metrics.value(13.0))
                        .color(app_muted_text(dark)),
                );
                ui.add_space(metrics.value(24.0));
                draw_about_rows(ui, "about_device", metrics, &rows);
            });
        });
    }

    pub(super) fn draw_about_entropy_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let rows = about_entropy_rows(lang);

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(about_entropy_title(lang))
                        .size(metrics.value(18.0))
                        .strong(),
                );
                ui.add_space(metrics.value(6.0));
                ui.label(
                    RichText::new(about_entropy_description(lang))
                        .size(metrics.value(13.0))
                        .color(app_muted_text(dark)),
                );
                ui.add_space(metrics.value(24.0));
                draw_about_rows(ui, "about_entropy", metrics, &rows);
            });
        });
    }
}
