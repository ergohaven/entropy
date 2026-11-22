use std::collections::BTreeMap;

use crate::i18n::{tr_catalog, Language};
use crate::key_legends::{self, Layout, STANDARD_RUSSIAN};
use crate::keycode::KeyLegendLayout;
use crate::ui_style::{
    modern_dropdown_button_sized, modern_dropdown_grouped_options, ResponsiveMetrics,
};

const ENGLISH: &str = "";
const RUSSIAN_FIRST: &str = "legacy:russian_primary";

pub(super) fn legend_selector_width(metrics: ResponsiveMetrics) -> f32 {
    metrics.value(218.0)
}

fn language_name(lang: Language, language: &str) -> &str {
    match language {
        "Russian" => tr_catalog(lang, "ui.key_legends_language_russian"),
        "French" => tr_catalog(lang, "ui.key_legends_language_french"),
        _ => language,
    }
}

fn variant_name(lang: Language, layout: &Layout) -> &str {
    match layout.id.as_str() {
        STANDARD_RUSSIAN => tr_catalog(lang, "ui.key_legends_variant_standard"),
        "ru-typewriter" => tr_catalog(lang, "ui.key_legends_variant_typewriter"),
        "fr-azerty-legacy" => tr_catalog(lang, "ui.key_legends_variant_azerty_legacy"),
        _ => &layout.name,
    }
}

fn selection_id(layout: &KeyLegendLayout) -> &str {
    match layout {
        KeyLegendLayout::English => ENGLISH,
        KeyLegendLayout::Russian => STANDARD_RUSSIAN,
        KeyLegendLayout::RussianPrimary => RUSSIAN_FIRST,
        KeyLegendLayout::Catalog(id) => id,
    }
}

fn selection_label(lang: Language, selected: &KeyLegendLayout) -> String {
    if let Some(layout) = selected.catalog_layout() {
        let variant = if matches!(selected, KeyLegendLayout::RussianPrimary) {
            tr_catalog(lang, "ui.key_legends_variant_standard_ru_first")
        } else {
            variant_name(lang, layout)
        };
        return format!("{} · {}", language_name(lang, &layout.language), variant);
    }
    let key = if matches!(selected, KeyLegendLayout::English) {
        "ui.key_legends_english"
    } else {
        "ui.key_legends_unavailable"
    };
    tr_catalog(lang, key).to_owned()
}

pub(super) fn draw_key_legend_selector(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: ResponsiveMetrics,
    lang: Language,
    selected: &mut KeyLegendLayout,
) {
    let id = ui.make_persistent_id(id_salt);
    let caption = selection_label(lang, selected);
    let width = legend_selector_width(metrics);
    let font_size = metrics.settings_control_font_size();
    let response = modern_dropdown_button_sized(
        ui,
        id,
        &caption,
        ui.visuals().text_color(),
        width,
        metrics.settings_control_height(),
        font_size,
    );
    let tooltip = match selected {
        KeyLegendLayout::Catalog(id) if key_legends::layout(id).is_none() => {
            format!("{caption} ({id})")
        }
        _ => caption,
    };
    let response = response.on_hover_text(tooltip);
    let mut languages: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    for layout in key_legends::catalog() {
        let variants = languages.entry(&layout.language).or_default();
        variants.push((layout.id.clone(), variant_name(lang, layout).to_owned()));
        if layout.id == STANDARD_RUSSIAN {
            variants.push((
                RUSSIAN_FIRST.to_owned(),
                tr_catalog(lang, "ui.key_legends_variant_standard_ru_first").to_owned(),
            ));
        }
    }
    let mut groups = languages
        .into_iter()
        .map(|(language, variants)| (language_name(lang, language).to_owned(), variants))
        .collect::<Vec<_>>();
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    let english = (
        ENGLISH.to_owned(),
        tr_catalog(lang, "ui.key_legends_english").to_owned(),
    );
    if let Some(id) = modern_dropdown_grouped_options(
        ui,
        id,
        &response,
        &english,
        &groups,
        selection_id(selected),
        width,
        font_size,
    ) {
        *selected = match id.as_str() {
            ENGLISH => KeyLegendLayout::English,
            STANDARD_RUSSIAN => KeyLegendLayout::Russian,
            RUSSIAN_FIRST => KeyLegendLayout::RussianPrimary,
            _ => KeyLegendLayout::Catalog(id),
        };
    }
}
