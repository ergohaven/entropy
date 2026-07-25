use super::*;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum AppAccentColor {
    Rose,
    Violet,
    Blue,
    Amber,
    Copper,
    Slate,
}

impl AppAccentColor {
    pub(crate) const ALL: [Self; 6] = [
        Self::Rose,
        Self::Violet,
        Self::Blue,
        Self::Amber,
        Self::Copper,
        Self::Slate,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Rose => "Rose",
            Self::Violet => "Violet",
            Self::Blue => "Blue",
            Self::Amber => "Amber",
            Self::Copper => "Copper",
            Self::Slate => "Teal",
        }
    }

    pub(crate) fn color(self) -> Color32 {
        match self {
            Self::Rose => Color32::from_rgb(196, 132, 144),
            Self::Violet => Color32::from_rgb(146, 128, 184),
            Self::Blue => Color32::from_rgb(116, 154, 212),
            Self::Amber => Color32::from_rgb(210, 156, 92),
            Self::Copper => Color32::from_rgb(192, 116, 88),
            Self::Slate => Color32::from_rgb(88, 158, 148),
        }
    }
}

pub(crate) fn app_accent() -> Color32 {
    crate::ui_style::accent()
}
pub(crate) fn app_panel_fill(dark: bool) -> Color32 {
    crate::ui_style::panel_fill(dark)
}
pub(crate) fn app_window_fill(dark: bool) -> Color32 {
    crate::ui_style::window_fill(dark)
}
pub(crate) fn app_surface_fill(dark: bool) -> Color32 {
    crate::ui_style::surface_fill(dark)
}
pub(crate) fn app_hover_fill(dark: bool) -> Color32 {
    crate::ui_style::hover_fill(dark)
}
pub(crate) fn app_border_color(dark: bool) -> Color32 {
    crate::ui_style::border_color(dark)
}
pub(crate) fn app_muted_text(dark: bool) -> Color32 {
    crate::ui_style::muted_text(dark)
}

pub(crate) fn app_top_bar_quiet_text(dark: bool) -> Color32 {
    if dark {
        Color32::from_gray(58)
    } else {
        Color32::from_gray(178)
    }
}

pub(crate) fn app_inactive_entry_text(dark: bool) -> Color32 {
    if dark {
        Color32::from_gray(105)
    } else {
        Color32::from_gray(165)
    }
}

pub(crate) fn draw_theme_selector_labels(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    dark_mode: &mut bool,
    dark_first: bool,
) {
    ui.horizontal(|ui| {
        let active = app_accent();
        let inactive = app_muted_text(*dark_mode);
        let mut theme_label = |ui: &mut egui::Ui, is_dark: bool| {
            let key = if is_dark {
                "app_chrome.dark_dark"
            } else {
                "app_chrome.light_light"
            };
            let selected = *dark_mode == is_dark;
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(crate::i18n::tr_catalog(lang, key))
                        .size(12.0)
                        .color(if selected { active } else { inactive }),
                )
                .selectable(false)
                .sense(egui::Sense::click()),
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                *dark_mode = is_dark;
            }
        };

        let (first, second) = if dark_first {
            (true, false)
        } else {
            (false, true)
        };

        theme_label(ui, first);
        ui.add(egui::Label::new(RichText::new("|").size(12.0).color(inactive)).selectable(false));
        theme_label(ui, second);
    });
}
