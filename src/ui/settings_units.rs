#[derive(Clone, Copy)]
pub(crate) enum SettingsFieldUnit {
    Milliseconds,
    Minutes,
    Seconds,
    CursorSteps,
    SpeedSteps,
}

impl SettingsFieldUnit {
    pub(crate) fn tooltip_key(self) -> &'static str {
        match self {
            SettingsFieldUnit::Milliseconds => "field_units.milliseconds",
            SettingsFieldUnit::Minutes => "field_units.minutes",
            SettingsFieldUnit::Seconds => "field_units.seconds",
            SettingsFieldUnit::CursorSteps => "field_units.cursor_steps",
            SettingsFieldUnit::SpeedSteps => "field_units.speed_steps",
        }
    }
}

pub(crate) fn settings_field_unit_tooltip(
    response: egui::Response,
    language: crate::i18n::Language,
    suppress_tooltip: bool,
    unit: SettingsFieldUnit,
) -> egui::Response {
    if suppress_tooltip {
        response
    } else {
        response.on_hover_text(crate::i18n::tr_catalog(language, unit.tooltip_key()))
    }
}
