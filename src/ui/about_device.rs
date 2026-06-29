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
        crate::i18n::Language::Russian => "Версия приложения и обновления для вашей ОС",
        crate::i18n::Language::English => "Application version and updates for your OS",
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

fn macro_ext_keycodes_status(lang: crate::i18n::Language, info: &DeviceAboutInfo) -> String {
    if info.supports_macro_ext_keycodes {
        return yes_no(lang, true).to_owned();
    }

    match info.macro_ext_keycodes_disabled_reason {
        Some(MacroExtKeycodesDisabledReason::RmkVialMacroExtUnsupported) => match lang {
            crate::i18n::Language::Russian => {
                "нет, отключено для RMK macro compatibility".to_owned()
            }
            crate::i18n::Language::English => "no, disabled for RMK macro compatibility".to_owned(),
        },
        None => yes_no(lang, false).to_owned(),
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
            macro_ext_keycodes_status(lang, info),
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

fn about_entropy_update_rows(
    lang: crate::i18n::Language,
    update_check: &UpdateCheckState,
) -> Vec<AboutRow> {
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
        localized_row(
            lang,
            "Платформа",
            "Platform",
            "ОС и архитектура текущей сборки",
            "OS and architecture of this build",
            update_platform_text(update_check),
        ),
        localized_monospace_row(
            lang,
            "Последний релиз",
            "Latest release",
            "Последняя версия на GitHub Releases",
            "Latest version on GitHub Releases",
            latest_release_text(lang, update_check),
        ),
        localized_row(
            lang,
            "Статус обновления",
            "Update status",
            "Результат проверки обновлений",
            "Update check result",
            update_status_text(lang, update_check),
        ),
        localized_row(
            lang,
            "Файл обновления",
            "Update file",
            "Подходящий файл релиза для этой ОС",
            "Release asset selected for this OS",
            update_asset_text(lang, update_check),
        ),
    ]
}

fn update_platform_text(update_check: &UpdateCheckState) -> String {
    match update_check {
        UpdateCheckState::Ready(result) => result.platform_label.clone(),
        _ => crate::app::current_platform_label(),
    }
}

fn latest_release_text(lang: crate::i18n::Language, update_check: &UpdateCheckState) -> String {
    match update_check {
        UpdateCheckState::Ready(result) => result.latest_version.clone(),
        UpdateCheckState::Checking { .. } => match lang {
            crate::i18n::Language::Russian => "проверяется".to_owned(),
            crate::i18n::Language::English => "checking".to_owned(),
        },
        _ => not_reported(lang).to_owned(),
    }
}

fn update_status_text(lang: crate::i18n::Language, update_check: &UpdateCheckState) -> String {
    match update_check {
        UpdateCheckState::Idle => match lang {
            crate::i18n::Language::Russian => "не проверялось".to_owned(),
            crate::i18n::Language::English => "not checked".to_owned(),
        },
        UpdateCheckState::Checking { .. } => match lang {
            crate::i18n::Language::Russian => "проверяем GitHub Releases".to_owned(),
            crate::i18n::Language::English => "checking GitHub Releases".to_owned(),
        },
        UpdateCheckState::Ready(result) => match result.relation {
            VersionRelation::UpdateAvailable => match lang {
                crate::i18n::Language::Russian => "доступно обновление".to_owned(),
                crate::i18n::Language::English => "update available".to_owned(),
            },
            VersionRelation::UpToDate => match lang {
                crate::i18n::Language::Russian => "актуальная версия".to_owned(),
                crate::i18n::Language::English => "up to date".to_owned(),
            },
            VersionRelation::DevelopmentBuild => match lang {
                crate::i18n::Language::Russian => "локальная сборка новее релиза".to_owned(),
                crate::i18n::Language::English => "local build is newer than latest".to_owned(),
            },
        },
        UpdateCheckState::Failed(error) => match lang {
            crate::i18n::Language::Russian => format!("ошибка: {error}"),
            crate::i18n::Language::English => format!("error: {error}"),
        },
    }
}

fn update_asset_text(lang: crate::i18n::Language, update_check: &UpdateCheckState) -> String {
    match update_check {
        UpdateCheckState::Ready(result) => result
            .asset
            .as_ref()
            .map(|asset| asset.name.clone())
            .unwrap_or_else(|| match lang {
                crate::i18n::Language::Russian => "нет файла для этой ОС".to_owned(),
                crate::i18n::Language::English => "no file for this OS".to_owned(),
            }),
        _ => not_reported(lang).to_owned(),
    }
}

fn check_updates_label(lang: crate::i18n::Language, checking: bool) -> &'static str {
    match (lang, checking) {
        (crate::i18n::Language::Russian, true) => "Проверяем...",
        (crate::i18n::Language::Russian, false) => "Проверить",
        (crate::i18n::Language::English, true) => "Checking...",
        (crate::i18n::Language::English, false) => "Check",
    }
}

fn download_update_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Скачать",
        crate::i18n::Language::English => "Download",
    }
}

fn changelog_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Changelog",
        crate::i18n::Language::English => "Changelog",
    }
}

fn changelog_tooltip(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Открывает changelog релиза на GitHub",
        crate::i18n::Language::English => "Opens the GitHub release changelog",
    }
}

fn browser_open_failed(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Не удалось открыть ссылку в браузере",
        crate::i18n::Language::English => "Failed to open link in the browser",
    }
}

fn draw_about_rows(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    rows: &[AboutRow],
) -> egui::Rect {
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

    list.viewport
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
        crate::app::poll_update_check(&mut self.update_check);
        if matches!(self.update_check, UpdateCheckState::Checking { .. }) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }

        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let rows = about_entropy_update_rows(lang, &self.update_check);

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
                let list_viewport = draw_about_rows(ui, "about_entropy", metrics, &rows);

                let checking = matches!(self.update_check, UpdateCheckState::Checking { .. });
                let ready = match &self.update_check {
                    UpdateCheckState::Ready(result) => Some(result.clone()),
                    _ => None,
                };
                let has_asset = ready.as_ref().is_some_and(|result| result.asset.is_some());
                let button_size = egui::vec2(metrics.value(132.0), metrics.value(32.0));
                let button_gap = metrics.value(8.0);
                let button_count = 1 + usize::from(has_asset) + usize::from(ready.is_some());
                let actions_width = button_size.x * button_count as f32
                    + button_gap * button_count.saturating_sub(1) as f32;
                let actions_rect = egui::Rect::from_center_size(
                    egui::pos2(
                        list_viewport.center().x,
                        list_viewport.bottom() + metrics.value(34.0),
                    ),
                    egui::vec2(actions_width, button_size.y),
                );
                crate::ui_style::allocate_ui_at_rect(ui, actions_rect, |ui| {
                    ui.set_min_size(actions_rect.size());
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = button_gap;
                        if crate::ui_style::modern_button(
                            ui,
                            check_updates_label(lang, checking),
                            button_size,
                            !checking,
                        )
                        .clicked()
                        {
                            self.update_check = crate::app::start_update_check();
                        }

                        if let Some(result) = ready {
                            if let Some(asset) = result.asset {
                                if crate::ui_style::modern_button(
                                    ui,
                                    download_update_label(lang),
                                    button_size,
                                    true,
                                )
                                .clicked()
                                    && !crate::app::open_url_in_browser(&asset.url)
                                {
                                    self.status_msg = browser_open_failed(lang).to_owned();
                                }
                            }

                            if crate::ui_style::modern_button(
                                ui,
                                changelog_label(lang),
                                button_size,
                                true,
                            )
                            .on_hover_text(changelog_tooltip(lang))
                            .clicked()
                                && !crate::app::open_url_in_browser(&result.release_url)
                            {
                                self.status_msg = browser_open_failed(lang).to_owned();
                            }
                        }
                    });
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_ext_status_explains_rmk_guard() {
        let info = DeviceAboutInfo {
            supports_macro_ext_keycodes: false,
            macro_ext_keycodes_disabled_reason: Some(
                MacroExtKeycodesDisabledReason::RmkVialMacroExtUnsupported,
            ),
            ..Default::default()
        };

        assert!(macro_ext_keycodes_status(crate::i18n::Language::English, &info).contains("RMK"));
    }
}
