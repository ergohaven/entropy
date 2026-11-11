// Standard compact settings trigger, capped only by actual available space.
// Do not derive its width from the label/preview column's fractional width.
fn sm62_dropdown_width(available: f32) -> f32 { 196.0_f32.min(available.max(1.0)) }

// Shared editor only: the firmware retains its existing per-half fields.
// Rendering mixed stored values must not normalize or queue writes.
fn sm62_shared_toggle(
    ui: &mut egui::Ui,
    label: &str,
    id: &'static str,
    pair: Option<[u8; 2]>,
    russian: bool,
) -> Option<u8> {
    let width = ui.available_width();
    let mixed = pair.is_some_and(|p| p[0] != p[1]);
    let mut enabled = pair.is_some_and(|p| p.iter().all(|v| *v != 0));
    let mut picked = None;
    display_settings_row(ui, width, 54.0, label, true, 196.0, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = ui.add_enabled_ui(pair.is_some(), |ui| {
                crate::ui_style::settings_switch_sized_stable(ui, id, &mut enabled, Vec2::new(46.0,24.0))
            }).inner;
            if response.clicked() { picked = Some(u8::from(enabled)); }
            let response = response.on_disabled_hover_text(if russian { "Для этой настройки обновите прошивку SM62" } else { "Update SM62 firmware to enable this setting" });
            if id == "sm62_shared_stars" {
                response.on_hover_text(if russian { "Только часы и плеер. Анимация корабля не изменяется." } else { "Clock and player only. Spaceship animation is unchanged." });
            } else if id == "sm62_shared_screens_off" {
                response.on_hover_text(if russian { "Гасит оба дисплея и их подсветку. Режимы и сохранённая яркость остаются прежними." } else { "Turns off both displays and their backlights. Modes and saved brightness are retained." });
            }
            if mixed { ui.label(if russian { "Различаются" } else { "Mixed" }); }
        });
    });
    picked
}

impl EntropyApp {
    fn draw_sm62_timeout_control(&mut self, ui: &mut egui::Ui) {
        let lang = self.app_settings.language;
        let keys = [
            "display_settings.never",
            "display_settings.minute_1",
            "display_settings.minutes_2",
            "display_settings.minutes_5",
            "display_settings.minutes_10",
            "display_settings.minutes_15",
            "display_settings.minutes_30",
            "display_settings.minutes_60",
        ];
        let labels: Vec<String> = keys
            .iter()
            .map(|key| crate::i18n::tr_catalog(lang, key).to_owned())
            .collect();
        let mut picked = None;
        let width = ui.available_width();
        let supported = self.display_settings.display_timeout_supported;
        display_settings_row(ui, width, 54.0,
            crate::i18n::tr_catalog(lang, "display_settings.display_timeout"), true, 222.0, |ui| {
                // Keep the field on the common right edge; reserve status space
                // to its left rather than shifting the dropdown itself.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(supported, |ui| {
                        let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                            ui, ui.make_persistent_id("sm62_shared_display_timeout"), &labels,
                            self.display_settings.display_timeout as usize, sm62_dropdown_width(width), 34.0, 12.0);
                        picked = choice;
                    })
                    .response
                    .on_disabled_hover_text(
                        if lang == crate::i18n::Language::Russian {
                            "Для этой настройки обновите прошивку SM62"
                        } else {
                            "Update SM62 firmware to enable this setting"
                        },
                    );
                    // This shared control needs progress/errors, not a persistent
                    // success check. Keep the confirmed write state unchanged.
                    if self
                        .pending_settings_write_value(DISPLAY_TIMEOUT_QSID)
                        .is_some()
                        || !matches!(
                            self.settings_write_status(DISPLAY_TIMEOUT_QSID),
                            Some(super::settings_write_queue::SettingsWriteStatus::Saved)
                        )
                    {
                        self.draw_settings_write_status(
                            ui,
                            DISPLAY_TIMEOUT_QSID,
                            crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx()),
                            false,
                        );
                    }
                });
            },
        );
        // Only explicit edits enqueue a write. A failed/unconfirmed request must
        // not be retried by painting or be reported as saved without GET.
        if let Some(index) = picked {
            let old = self.display_settings.confirmed_display_timeout;
            self.display_settings.display_timeout = index as u8;
            if self.explicit_display_setting_needs_write(
                DISPLAY_TIMEOUT_QSID,
                old as u16,
                index as u16,
            ) {
                self.queue_display_setting_write(
                    crate::i18n::tr_catalog(lang, "display_settings.display_timeout").to_owned(),
                    DISPLAY_TIMEOUT_QSID,
                    old as u16,
                    index as u16,
                );
            }
        }
    }
}

fn sm62_shared_controls(
    ui: &mut egui::Ui,
    values: &mut [u8; 10],
    changed: &mut Vec<usize>,
    russian: bool,
    date: Option<&mut [u8; 2]>,
    hold: Option<&mut [u16; 2]>,
    language: Option<&mut [u8; 2]>,
    stars: Option<&mut [u8; 2]>,
    screens_off: Option<&mut [u8; 2]>,
) {
    ui.heading(if russian {
        "Общие настройки экранов"
    } else {
        "Shared display settings"
    });
    ui.label(if russian {
        "Изменения применяются к обоим экранам."
    } else {
        "Changes apply to both displays."
    });
    ui.add_space(8.0);
    if let Some(value) = sm62_shared_toggle(
        ui,
        if russian {
            "Кириллические символы"
        } else {
            "Cyrillic symbols"
        },
        "sm62_shared_cyrillic",
        Some([values[4], values[9]]),
        russian,
    ) {
        for index in [4, 9] {
            if values[index] != value {
                values[index] = value;
                changed.push(index);
            }
        }
    }
    let supported = hold.is_some();
    let pair = hold.as_deref().copied().unwrap_or([350;2]);
    let mixed = pair[0] != pair[1];
    let options: Vec<u16> = (300..=1000).step_by(100).collect();
    let labels: Vec<String> = options.iter().map(|n|format!("{n} {}",if russian {"мс"} else {"ms"})).collect();
    let selected = if mixed {usize::MAX} else {options.iter().position(|value|*value==pair[0]).unwrap_or(usize::MAX)};
    // Legacy values remain visible and untouched; they are not additional
    // selectable options and painting never clamps either half.
    let text = if mixed {if russian {"Различаются".to_owned()} else {"Mixed".to_owned()}}
        else {format!("{} {}",pair[0],if russian {"мс"} else {"ms"})};
    let mut picked = None;
    let width = ui.available_width();
    let control_width = sm62_dropdown_width(width);
    display_settings_row(ui,width,54.0,
        if russian { "Показывать раскладку в течение" } else { "Show layout for" },true,control_width,|ui| {
            ui.add_enabled_ui(supported,|ui| {
                let id=ui.make_persistent_id("sm62_shared_hold");
                let response=crate::ui_style::modern_dropdown_button_sized(ui,id,&text,ui.visuals().text_color(),control_width,34.0,12.0);
                #[cfg(test)]
                ui.ctx().data_mut(|data|data.insert_temp(egui::Id::unique("sm62_hold_render_trigger"),response.rect));
                picked=crate::ui_style::modern_dropdown_popup_options(ui,id,&response,&labels,selected,control_width,12.0);
                if mixed {response.on_hover_text(if russian {
                    format!("Левый: {} мс; правый: {} мс. Новое значение применяется к обоим.",pair[0],pair[1])
                } else {format!("Left: {} ms; right: {} ms. Editing applies to both.",pair[0],pair[1])});}
            });
        });
    if let Some(index)=picked {if let Some(pair)=hold {pair.fill(options[index]);}}
    if let Some(value) = sm62_shared_toggle(
        ui,
        if russian {
            "Показывать текущую раскладку RU/EN"
        } else {
            "Show current layout RU/EN"
        },
        "sm62_shared_language",
        language.as_deref().copied(),
        russian,
    ) {
        if let Some(pair) = language {
            pair.fill(value);
        }
    }
    if let Some(value) = sm62_shared_toggle(
        ui,
        if russian {
            "Дата на часах"
        } else {
            "Date on clock"
        },
        "sm62_shared_date",
        date.as_deref().copied(),
        russian,
    ) {
        if let Some(pair) = date {
            pair.fill(value);
        }
    }
    if let Some(value) = sm62_shared_toggle(
        ui,
        if russian {
            "Анимация звёзд"
        } else {
            "Star animation"
        },
        "sm62_shared_stars",
        stars.as_deref().copied(),
        russian,
    ) {
        if let Some(pair) = stars {
            pair.fill(value);
        }
    }
    if let Some(value) = sm62_shared_toggle(
        ui,
        if russian {
            "Выключить экраны"
        } else {
            "Turn off displays"
        },
        "sm62_shared_screens_off",
        screens_off.as_deref().copied(),
        russian,
    ) {
        if let Some(pair) = screens_off {
            pair.fill(value);
        }
    }
}
