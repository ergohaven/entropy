//! Catalog of the top menu icons.
//!
//! A menu row names its icon with [`TopMenuIcon`]. The catalog maps the icon
//! to a glyph and to its semantic group, and maps groups to tints.
//! The renderer in `top_dropdown.rs` uses the first icon group in each
//! divider-delimited visual block as the tint for that entire block. A
//! single-row semantic group can join its neighbor without changing hue.

use super::*;

/// Semantic group of related menu rows. The actual painted block may
/// contain multiple groups when the divider policy coalesces short groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TopMenuGroup {
    Devices,
    Layers,
    Files,
    Lighting,
    Input,
    KeyBehavior,
    Service,
    HostTools,
    /// Rows about the app or the device itself.
    Meta,
}

impl TopMenuGroup {
    #[cfg(test)]
    pub(super) const ALL: [TopMenuGroup; 9] = [
        TopMenuGroup::Devices,
        TopMenuGroup::Layers,
        TopMenuGroup::Files,
        TopMenuGroup::Lighting,
        TopMenuGroup::Input,
        TopMenuGroup::KeyBehavior,
        TopMenuGroup::Service,
        TopMenuGroup::HostTools,
        TopMenuGroup::Meta,
    ];

    /// Icon tint of the group for the given theme.
    ///
    /// Tints are pastel: one hue per group, saturation kept low so the
    /// column reads as a quiet marker, not a signal color. Every tint keeps
    /// at least 4.5:1 contrast against the menu surface and against the row
    /// hover fill of every accent color (see the tests below). Color is a
    /// redundant cue on top of glyph and divider, never the only difference
    /// between rows.
    pub(super) fn tint(self, dark: bool) -> Color32 {
        let (dark_tint, light_tint) = match self {
            TopMenuGroup::Devices | TopMenuGroup::Input => ((175, 195, 217), (70, 97, 129)),
            TopMenuGroup::Layers | TopMenuGroup::KeyBehavior => ((196, 179, 219), (100, 77, 132)),
            TopMenuGroup::Files => ((152, 205, 198), (56, 113, 105)),
            TopMenuGroup::Lighting => ((214, 196, 154), (118, 98, 50)),
            TopMenuGroup::Service => ((221, 185, 172), (137, 86, 67)),
            TopMenuGroup::HostTools => ((161, 206, 176), (59, 109, 76)),
            TopMenuGroup::Meta => ((185, 185, 193), (101, 101, 114)),
        };
        let (r, g, b) = if dark { dark_tint } else { light_tint };
        Color32::from_rgb(r, g, b)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TopMenuIcon {
    Device,
    KeyLegendOrder,
    LayerOperations,
    ShowHideKeys,
    ImportLayout,
    ExportLayout,
    ExportImage,
    LayoutIndicator,
    AboutDevice,
    TextExpander,
    TypingTrainer,
    Macros,
    TapDance,
    Combo,
    AutoShift,
    KeyOverrides,
    Rgb,
    LayerLeds,
    Display,
    DisplayPresets,
    Encoders,
    Touchpad,
    Modules,
    Bluetooth,
    LiveFeatures,
    TapHold,
    Magic,
    MatrixTester,
    Lock,
    Unlock,
    AppSettings,
    ApplicationLayouts,
    AboutEntropy,
}

impl TopMenuIcon {
    #[cfg(test)]
    pub(super) const ALL: [TopMenuIcon; 33] = [
        TopMenuIcon::Device,
        TopMenuIcon::KeyLegendOrder,
        TopMenuIcon::LayerOperations,
        TopMenuIcon::ShowHideKeys,
        TopMenuIcon::ImportLayout,
        TopMenuIcon::ExportLayout,
        TopMenuIcon::ExportImage,
        TopMenuIcon::LayoutIndicator,
        TopMenuIcon::AboutDevice,
        TopMenuIcon::TextExpander,
        TopMenuIcon::TypingTrainer,
        TopMenuIcon::Macros,
        TopMenuIcon::TapDance,
        TopMenuIcon::Combo,
        TopMenuIcon::AutoShift,
        TopMenuIcon::KeyOverrides,
        TopMenuIcon::Rgb,
        TopMenuIcon::LayerLeds,
        TopMenuIcon::Display,
        TopMenuIcon::DisplayPresets,
        TopMenuIcon::Encoders,
        TopMenuIcon::Touchpad,
        TopMenuIcon::Modules,
        TopMenuIcon::Bluetooth,
        TopMenuIcon::LiveFeatures,
        TopMenuIcon::TapHold,
        TopMenuIcon::Magic,
        TopMenuIcon::MatrixTester,
        TopMenuIcon::Lock,
        TopMenuIcon::Unlock,
        TopMenuIcon::AppSettings,
        TopMenuIcon::ApplicationLayouts,
        TopMenuIcon::AboutEntropy,
    ];

    /// Catalog line of the icon: its glyph and its group.
    ///
    /// Glyphs must exist in the embedded fonts (Noto Emoji, Noto Sans
    /// Symbols 2, DejaVu) and render as line art there. Keycap-style emoji
    /// such as "📶" or "⏺" render as solid boxes and are avoided.
    fn entry(self) -> (&'static str, TopMenuGroup) {
        use TopMenuGroup::*;
        match self {
            TopMenuIcon::Device => ("⌨", Devices),
            TopMenuIcon::KeyLegendOrder => ("🌐", Layers),
            TopMenuIcon::LayerOperations => ("☰", Layers),
            TopMenuIcon::ShowHideKeys => ("▧", Layers),
            TopMenuIcon::ImportLayout => ("📥", Files),
            TopMenuIcon::ExportLayout => ("📤", Files),
            TopMenuIcon::ExportImage => ("🖼", Files),
            TopMenuIcon::LayoutIndicator => ("📌", Meta),
            TopMenuIcon::AboutDevice => ("🛈", Meta),
            TopMenuIcon::TextExpander => ("📝", HostTools),
            TopMenuIcon::TypingTrainer => ("🎯", HostTools),
            TopMenuIcon::Macros => ("📜", KeyBehavior),
            TopMenuIcon::TapDance => ("👆", KeyBehavior),
            TopMenuIcon::Combo => ("🔗", KeyBehavior),
            TopMenuIcon::AutoShift => ("⇧", KeyBehavior),
            TopMenuIcon::KeyOverrides => ("⇄", KeyBehavior),
            TopMenuIcon::Rgb => ("💡", Lighting),
            TopMenuIcon::LayerLeds => ("🚦", Lighting),
            TopMenuIcon::Display => ("🖥", Lighting),
            TopMenuIcon::DisplayPresets => ("🎞", Lighting),
            TopMenuIcon::Encoders => ("🎛", Input),
            TopMenuIcon::Touchpad => ("🖱", Input),
            TopMenuIcon::Modules => ("🧩", Input),
            TopMenuIcon::Bluetooth => ("📡", Input),
            TopMenuIcon::LiveFeatures => ("⚡", Input),
            TopMenuIcon::TapHold => ("⏱", KeyBehavior),
            TopMenuIcon::Magic => ("🪄", KeyBehavior),
            TopMenuIcon::MatrixTester => ("▦", Service),
            TopMenuIcon::Lock => ("🔒", Service),
            TopMenuIcon::Unlock => ("🔓", Service),
            TopMenuIcon::AppSettings => ("⚙", Meta),
            TopMenuIcon::ApplicationLayouts => ("▤", Meta),
            TopMenuIcon::AboutEntropy => ("🛈", Meta),
        }
    }

    pub(super) fn glyph(self) -> &'static str {
        self.entry().0
    }

    pub(super) fn group(self) -> TopMenuGroup {
        self.entry().1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT_MENU_ICONS: [TopMenuIcon; 9] = [
        TopMenuIcon::Device,
        TopMenuIcon::KeyLegendOrder,
        TopMenuIcon::LayerOperations,
        TopMenuIcon::ShowHideKeys,
        TopMenuIcon::ImportLayout,
        TopMenuIcon::ExportLayout,
        TopMenuIcon::ExportImage,
        TopMenuIcon::LayoutIndicator,
        TopMenuIcon::AboutDevice,
    ];
    const ADVANCED_MENU_ICONS: [TopMenuIcon; 7] = [
        TopMenuIcon::TextExpander,
        TopMenuIcon::TypingTrainer,
        TopMenuIcon::Macros,
        TopMenuIcon::TapDance,
        TopMenuIcon::Combo,
        TopMenuIcon::AutoShift,
        TopMenuIcon::KeyOverrides,
    ];
    const CONFIG_MENU_ICONS: [TopMenuIcon; 17] = [
        TopMenuIcon::Rgb,
        TopMenuIcon::LayerLeds,
        TopMenuIcon::Display,
        TopMenuIcon::DisplayPresets,
        TopMenuIcon::Encoders,
        TopMenuIcon::Touchpad,
        TopMenuIcon::Modules,
        TopMenuIcon::Bluetooth,
        TopMenuIcon::LiveFeatures,
        TopMenuIcon::TapHold,
        TopMenuIcon::Magic,
        TopMenuIcon::MatrixTester,
        TopMenuIcon::Lock,
        TopMenuIcon::Unlock,
        TopMenuIcon::AppSettings,
        TopMenuIcon::ApplicationLayouts,
        TopMenuIcon::AboutEntropy,
    ];

    #[test]
    fn every_icon_belongs_to_exactly_one_menu() {
        let mut listed: Vec<TopMenuIcon> = LAYOUT_MENU_ICONS
            .iter()
            .chain(&ADVANCED_MENU_ICONS)
            .chain(&CONFIG_MENU_ICONS)
            .copied()
            .collect();
        assert_eq!(listed.len(), TopMenuIcon::ALL.len());
        for icon in TopMenuIcon::ALL {
            let position = listed
                .iter()
                .position(|listed| *listed == icon)
                .unwrap_or_else(|| panic!("{icon:?} is not listed in any menu"));
            listed.swap_remove(position);
        }
        assert!(listed.is_empty(), "icons listed twice: {listed:?}");
    }

    #[test]
    fn menu_icons_are_unique_within_each_menu() {
        for menu in [
            LAYOUT_MENU_ICONS.as_slice(),
            ADVANCED_MENU_ICONS.as_slice(),
            CONFIG_MENU_ICONS.as_slice(),
        ] {
            let mut seen = std::collections::HashSet::new();
            for icon in menu {
                assert!(seen.insert(icon.glyph()), "duplicate glyph for {icon:?}");
            }
        }
    }

    #[test]
    fn menu_icon_glyphs_exist_in_the_icon_font_family() {
        use ab_glyph::Font as _;

        // Same fonts and order as the "emoji_preview" family.
        let fonts = [
            include_bytes!("../../assets/NotoEmoji-Regular.ttf").as_slice(),
            include_bytes!("../../assets/NotoSansSymbols2-Regular.ttf").as_slice(),
            include_bytes!("../../assets/DejaVuSans.ttf").as_slice(),
        ]
        .map(|bytes| ab_glyph::FontRef::try_from_slice(bytes).expect("embedded font parses"));

        for icon in TopMenuIcon::ALL {
            for ch in icon.glyph().chars() {
                assert!(
                    fonts.iter().any(|font| font.glyph_id(ch).0 != 0),
                    "{icon:?} glyph U+{:04X} is missing from the embedded fonts",
                    ch as u32
                );
            }
        }
    }

    /// WCAG 2.x relative luminance of an sRGB color.
    fn relative_luminance(color: Color32) -> f32 {
        let channel = |value: u8| {
            let value = value as f32 / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    /// WCAG 2.x contrast ratio, 1.0 (none) to 21.0 (black on white).
    fn contrast_ratio(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (relative_luminance(a), relative_luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn group_tints_keep_contrast_on_every_menu_background() {
        // WCAG AA for text. Glyphs are thin line art, so the text threshold
        // applies rather than the 3:1 one for large graphics.
        const MIN_CONTRAST: f32 = 4.5;

        for dark in [false, true] {
            let mut backgrounds = vec![("surface", crate::ui_style::surface_fill(dark))];
            for accent in AppAccentColor::ALL {
                backgrounds.push((
                    accent.name(),
                    crate::ui_style::hover_fill_for_accent(dark, accent.color()),
                ));
            }
            for group in TopMenuGroup::ALL {
                let tint = group.tint(dark);
                for (background_name, background) in &backgrounds {
                    let contrast = contrast_ratio(tint, *background);
                    assert!(
                        contrast >= MIN_CONTRAST,
                        "{group:?} tint {tint:?} has {contrast:.2}:1 contrast on the {background_name} \
                         background in {} theme, need {MIN_CONTRAST}:1",
                        if dark { "dark" } else { "light" }
                    );
                }
            }
        }
    }
}
