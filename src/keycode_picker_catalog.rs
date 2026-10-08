use super::KeycodeTab;
use egui::Color32;

/// Visual identity of a picker tab: the glyph drawn in front of the label
/// and the category tint per theme. The tab bar, the search result headings
/// and the tests all read it from here; no other module keeps its own table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TabStyle {
    /// Monochrome glyph present in the embedded fonts (Roboto, DejaVu,
    /// Noto Sans Symbols 2, Noto Emoji). It takes whatever color the caller
    /// paints it with.
    pub glyph: &'static str,
    dark_tint: Color32,
    light_tint: Color32,
}

impl TabStyle {
    const fn new(glyph: &'static str, dark: [u8; 3], light: [u8; 3]) -> Self {
        Self {
            glyph,
            dark_tint: Color32::from_rgb(dark[0], dark[1], dark[2]),
            light_tint: Color32::from_rgb(light[0], light[1], light[2]),
        }
    }

    /// Category tint for the given theme. A redundant cue next to the glyph
    /// and the label, never the only thing that tells tabs apart.
    pub fn tint(self, dark: bool) -> Color32 {
        if dark {
            self.dark_tint
        } else {
            self.light_tint
        }
    }
}

// Restrained category palette. The dark theme uses pastel tints (light, low
// saturation); the light theme uses muted deep tones of the same hues. Every
// tint keeps at least 4.5:1 contrast on the picker surfaces it is drawn on;
// the tests below verify that.
pub(super) const fn tab_style(tab: KeycodeTab) -> TabStyle {
    match tab {
        KeycodeTab::Basic => TabStyle::new("⌨", [150, 196, 235], [44, 104, 150]),
        KeycodeTab::Symbols => TabStyle::new("@", [232, 194, 130], [146, 96, 24]),
        KeycodeTab::UniversalSymbols => TabStyle::new("🌐", [140, 208, 178], [26, 118, 84]),
        KeycodeTab::Modifiers => TabStyle::new("⇧", [198, 178, 232], [108, 78, 164]),
        KeycodeTab::Layers => TabStyle::new("☰", [178, 170, 226], [92, 92, 158]),
        KeycodeTab::Media => TabStyle::new("🎵", [132, 202, 196], [20, 108, 104]),
        KeycodeTab::Special => TabStyle::new("✦", [240, 176, 140], [170, 84, 44]),
        KeycodeTab::Rgb => TabStyle::new("💡", [226, 208, 134], [132, 102, 24]),
        KeycodeTab::Macro => TabStyle::new("⏺", [236, 166, 166], [172, 56, 62]),
        KeycodeTab::TapDance => TabStyle::new("👆", [150, 212, 222], [24, 112, 128]),
        KeycodeTab::Bluetooth => TabStyle::new("📶", [164, 180, 240], [62, 90, 184]),
        KeycodeTab::Custom => TabStyle::new("🧩", [186, 186, 192], [104, 104, 114]),
    }
}

impl KeycodeTab {
    pub(super) fn style(self) -> TabStyle {
        tab_style(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// WCAG 2.x relative luminance of an opaque sRGB color.
    fn relative_luminance(color: Color32) -> f64 {
        let channel = |value: u8| {
            let v = f64::from(value) / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    /// WCAG 2.x contrast ratio between two opaque colors (1.0 to 21.0).
    fn contrast_ratio(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (relative_luminance(a), relative_luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// HSV saturation and value, both in 0.0..=1.0.
    fn saturation_and_value(color: Color32) -> (f64, f64) {
        let channels = [color.r(), color.g(), color.b()].map(|c| f64::from(c) / 255.0);
        let max = channels.iter().copied().fold(0.0, f64::max);
        let min = channels.iter().copied().fold(1.0, f64::min);
        let saturation = if max == 0.0 { 0.0 } else { (max - min) / max };
        (saturation, max)
    }

    #[test]
    fn every_tab_has_a_unique_glyph() {
        let mut seen = HashSet::new();
        for tab in KeycodeTab::ALL {
            let glyph = tab.style().glyph;
            assert!(!glyph.is_empty(), "tab {tab:?} has no glyph");
            assert!(seen.insert(glyph), "duplicate glyph for {tab:?}");
        }
    }

    #[test]
    fn tints_are_unique_within_each_theme() {
        for dark in [true, false] {
            let mut seen = HashSet::new();
            for tab in KeycodeTab::ALL {
                assert!(
                    seen.insert(tab.style().tint(dark).to_array()),
                    "duplicate tint for {tab:?} (dark: {dark})"
                );
            }
        }
    }

    #[test]
    fn tints_meet_text_contrast_on_picker_surfaces() {
        // Tab chips sit on the surface fill, result headings on the window fill.
        for dark in [true, false] {
            let surfaces = [
                crate::ui_style::surface_fill(dark),
                crate::ui_style::window_fill(dark),
            ];
            for tab in KeycodeTab::ALL {
                for surface in surfaces {
                    let ratio = contrast_ratio(tab.style().tint(dark), surface);
                    assert!(
                        ratio >= 4.5,
                        "{tab:?} tint has {ratio:.2}:1 contrast on {surface:?} (dark: {dark})"
                    );
                }
            }
        }
    }

    #[test]
    fn dark_theme_tints_are_pastel() {
        for tab in KeycodeTab::ALL {
            let (saturation, value) = saturation_and_value(tab.style().tint(true));
            assert!(
                saturation <= 0.5 && value >= 0.7,
                "{tab:?} dark tint is not pastel: saturation {saturation:.2}, value {value:.2}"
            );
        }
    }
}
