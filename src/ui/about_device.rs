use super::*;

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
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

fn device_about_text(info: &DeviceAboutInfo) -> String {
    let firmware_version = info
        .firmware_version
        .as_deref()
        .map(text_or_unknown)
        .unwrap_or("not reported");
    let macro_memory = info
        .macro_memory_bytes
        .map(|bytes| format!("{bytes} bytes"))
        .unwrap_or_else(|| "not reported".to_owned());

    format!(
        "Manufacturer: {manufacturer}\n\
Product: {product}\n\
Firmware version: {firmware_version}\n\
VID: {vendor_id:04X}\n\
PID: {product_id:04X}\n\
Device: {path}\n\
\n\
VIA protocol: {via_protocol}\n\
Vial protocol: {vial_protocol}\n\
Vial keyboard ID: {keyboard_id:016X}\n\
\n\
Macro entries: {macro_entries}\n\
Macro memory: {macro_memory}\n\
Macro delays: {macro_delays}\n\
Complex (2-byte) macro keycodes: {macro_ext}\n\
\n\
Tap Dance entries: {tap_dance_entries}\n\
Combo entries: {combo_entries}\n\
Key Override entries: {key_override_entries}\n\
Alt Repeat Key entries: {alt_repeat_entries}\n\
Caps Word: {caps_word}\n\
Layer Lock: {layer_lock}\n\
\n\
QMK Settings: {qmk_settings}",
        manufacturer = text_or_unknown(&info.manufacturer),
        product = text_or_unknown(&info.product),
        vendor_id = info.vendor_id,
        product_id = info.product_id,
        path = text_or_unknown(&info.path),
        via_protocol = info.via_protocol,
        vial_protocol = info.vial_protocol,
        keyboard_id = info.keyboard_id,
        macro_entries = info.macro_entries,
        macro_delays = yes_no(info.supports_macro_delays),
        macro_ext = yes_no(info.supports_macro_ext_keycodes),
        tap_dance_entries = info.tap_dance_entries,
        combo_entries = info.combo_entries,
        key_override_entries = info.key_override_entries,
        alt_repeat_entries = info.alt_repeat_entries,
        caps_word = yes_no(info.caps_word),
        layer_lock = yes_no(info.layer_lock),
        qmk_settings = yes_no(info.qmk_settings),
    )
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
        let body = device_about_text(&info);

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(RichText::new(title).size(metrics.value(18.0)).strong());
                ui.add_space(metrics.value(18.0));

                let panel_width = metrics
                    .value(560.0)
                    .min((ui.available_width() - metrics.value(20.0)).max(metrics.value(320.0)));
                let line_count = body.lines().count().max(1) as f32;
                let desired_height = metrics.value(24.0) + line_count * metrics.value(18.0);
                let panel_height = desired_height
                    .min((ui.available_height() - metrics.value(8.0)).max(metrics.value(180.0)));

                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(panel_width, panel_height),
                    egui::Sense::hover(),
                );
                let fill = if dark {
                    Color32::from_rgb(29, 29, 30)
                } else {
                    Color32::from_rgb(247, 247, 248)
                };
                ui.painter().rect_filled(rect, 2.0, fill);
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    crate::ui_style::modal_outline_stroke(dark),
                    egui::StrokeKind::Outside,
                );

                let inner_rect = rect.shrink(metrics.value(12.0));
                crate::ui_style::allocate_ui_at_rect(ui, inner_rect, |ui| {
                    ui.set_clip_rect(inner_rect);
                    ui.add(
                        egui::Label::new(
                            RichText::new(body)
                                .monospace()
                                .size(metrics.value(13.0))
                                .color(ui.visuals().text_color()),
                        )
                        .wrap(),
                    );
                });
            });
        });
    }
}
