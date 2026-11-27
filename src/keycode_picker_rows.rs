use super::*;
use std::sync::Arc;

/// What clicking a picker row does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PickerAction {
    /// Assign the binding and close the picker.
    Assign(crate::keyboard::KeyBinding),
    /// Open the layer chooser for a layer action (MO, TG, LT...).
    PickLayer(u16),
    /// Open the key chooser for a Mod+Key chord with this modifier base.
    PickModKey(u16),
    /// Open the tap-key chooser for a Mod-Tap with this modifier base.
    PickModTap(u16),
    /// Open the macro or Tap Dance slot chooser.
    ChooseSlot(AdvancedSlotKind),
}

/// One keycap on a picker tab. Tabs render from these rows and the search
/// index is built from the same rows, so every visible key is searchable by
/// the words the user sees on it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PickerRow {
    /// Keycap caption. "\n" splits a small top line from the main line.
    pub label: String,
    /// Search-only aliases: QMK names, slot ids, user-given names.
    pub aliases: String,
    /// Localized tooltip.
    pub tooltip: String,
    /// Home tab; groups search results.
    pub tab: KeycodeTab,
    /// Localized section heading inside the tab. Empty on single-section
    /// tabs, whose heading comes from `KeycodePicker::tab_heading`.
    pub section: &'static str,
    pub action: PickerAction,
    /// Right-click action, e.g. the right-hand modifier variant.
    pub secondary: Option<PickerAction>,
}

impl PickerRow {
    pub(super) fn new(
        tab: KeycodeTab,
        section: &'static str,
        label: impl Into<String>,
        tooltip: impl Into<String>,
        action: PickerAction,
    ) -> Self {
        Self {
            label: label.into(),
            aliases: String::new(),
            tooltip: tooltip.into(),
            tab,
            section,
            action,
            secondary: None,
        }
    }

    pub(super) fn with_aliases(mut self, aliases: impl AsRef<str>) -> Self {
        let aliases = aliases.as_ref().trim();
        if !aliases.is_empty() {
            if !self.aliases.is_empty() {
                self.aliases.push(' ');
            }
            self.aliases.push_str(aliases);
        }
        self
    }

    pub(super) fn with_secondary(mut self, action: PickerAction) -> Self {
        self.secondary = Some(action);
        self
    }

    /// Caption on one line: the accessible name of the keycap.
    pub(super) fn accessible_name(&self) -> String {
        self.label.replace('\n', " ")
    }

    /// Case-insensitive substring match over caption, aliases, tooltip and
    /// section heading. `needle_lower` must already be normalized.
    pub(super) fn matches(&self, needle_lower: &str, tab_heading: Option<&str>) -> bool {
        search_text_matches(
            needle_lower,
            &[
                &self.label,
                &self.aliases,
                &self.tooltip,
                self.section,
                tab_heading.unwrap_or(""),
            ],
        )
    }
}

/// Loading state of the deferred device data behind the Macro and Tap Dance
/// slot choosers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SlotDataStates {
    pub macro_state: DeferredPickerDataState,
    pub tap_dance_state: DeferredPickerDataState,
}

impl SlotDataStates {
    pub(super) const READY: Self = Self {
        macro_state: DeferredPickerDataState::Ready,
        tap_dance_state: DeferredPickerDataState::Ready,
    };

    fn for_kind(self, kind: AdvancedSlotKind) -> DeferredPickerDataState {
        match kind {
            AdvancedSlotKind::Macro => self.macro_state,
            AdvancedSlotKind::TapDance => self.tap_dance_state,
        }
    }
}

/// Rows per tab and the Keys grid, rebuilt when `KeycodePicker::row_fingerprint`
/// changes. Rendering and the search index share these rows.
#[derive(Default)]
pub(super) struct RowCache {
    fingerprint: Option<u64>,
    tabs: Vec<(KeycodeTab, Arc<Vec<PickerRow>>)>,
    basic_cells: Option<Arc<Vec<BasicGridCell>>>,
}

/// Section heading above a group of keycaps.
pub(super) fn picker_heading_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_gray(150)
    } else {
        Color32::from_gray(110)
    }
}

pub(super) fn show_section_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.0)
            .color(picker_heading_color(ui.visuals().dark_mode)),
    );
    ui.add_space(4.0);
}

/// Draw one keycap and report the action its click triggered, if any.
pub(super) fn row_button(ui: &mut egui::Ui, row: &PickerRow) -> Option<PickerAction> {
    let resp = picker_keycap_row_button(ui, KeycodePicker::picker_key_size(ui.ctx()), row);
    let triggered = if resp.clicked() {
        Some(row.action)
    } else if resp.secondary_clicked() {
        row.secondary
    } else {
        None
    };
    resp.on_hover_text(row.tooltip.clone());
    triggered
}

/// Draw rows as one wrapped keycap grid and report the triggered action.
pub(super) fn show_row_grid(ui: &mut egui::Ui, rows: &[PickerRow]) -> Option<PickerAction> {
    let mut triggered = None;
    ui.horizontal_wrapped(|ui| {
        for row in rows {
            if let Some(action) = row_button(ui, row) {
                triggered = Some(action);
            }
        }
    });
    triggered
}

impl KeycodePicker {
    /// Rows the tab renders, in display order. Only keys the user can pick
    /// are listed: firmware gating and the value policy are already applied.
    pub(super) fn tab_rows(&self, tab: KeycodeTab) -> Vec<PickerRow> {
        match tab {
            KeycodeTab::Basic => self
                .basic_grid_cells()
                .into_iter()
                .filter(|cell| cell.enabled)
                .map(|cell| cell.row)
                .collect(),
            KeycodeTab::Symbols | KeycodeTab::Media => self.keycode_table_rows(tab),
            KeycodeTab::UniversalSymbols => {
                let mut rows = if self.native_universal_symbols_available() {
                    universal_symbol_rows(self.language, self.supports_universal_numero)
                } else {
                    Vec::new()
                };
                rows.extend(self.device_keycode_rows(KeycodeTab::UniversalSymbols, ""));
                rows
            }
            KeycodeTab::Layers => self.layer_rows(KeycodeTab::Layers),
            KeycodeTab::Modifiers => self.modifier_rows(),
            KeycodeTab::Rgb => self.rgb_rows(),
            KeycodeTab::Special => self.special_rows(),
            KeycodeTab::Bluetooth => self.custom_keycode_rows(true),
            KeycodeTab::Custom => self.custom_keycode_rows(false),
            // Editors, not key grids.
            KeycodeTab::Macro | KeycodeTab::TapDance => Vec::new(),
        }
    }

    /// Heading drawn above a single-section tab. Multi-section tabs carry
    /// their headings on the rows instead.
    pub(super) fn tab_heading(&self, tab: KeycodeTab) -> Option<&'static str> {
        let key = match tab {
            KeycodeTab::Basic => "key_picker.section_basic",
            KeycodeTab::Symbols => "key_picker.section_layout_symbols",
            KeycodeTab::UniversalSymbols => "key_picker.section_universal_symbols",
            KeycodeTab::Custom => "key_picker.section_custom_keycodes",
            KeycodeTab::Bluetooth => "key_picker.section_bluetooth_keycodes",
            _ => return None,
        };
        Some(tr_picker(self.language, key))
    }

    /// Hash of every picker field the rows derive from. Rows and the search
    /// index are rebuilt when it changes, e.g. when deferred device data
    /// arrives or a macro is renamed while the picker is open.
    pub(super) fn row_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.language.hash(&mut hasher);
        self.key_legend_layout.hash(&mut hasher);
        self.show_shifted_number_symbols.hash(&mut hasher);
        self.basic_layout.hash(&mut hasher);
        self.value_policy.hash(&mut hasher);
        self.layer_names.hash(&mut hasher);
        self.custom_keycodes.hash(&mut hasher);
        self.macro_count.hash(&mut hasher);
        self.macro_names.hash(&mut hasher);
        self.tap_dance_entries.len().hash(&mut hasher);
        self.tap_dance_names.hash(&mut hasher);
        [
            self.supports_rgb,
            self.supports_macro,
            self.supports_tap_dance,
            self.supports_mouse_keys,
            self.supports_combo,
            self.supports_auto_shift,
            self.supports_caps_word,
            self.supports_repeat_key,
            self.supports_alt_repeat_key,
            self.supports_layer_lock,
            self.supports_persistent_default_layer,
            self.supports_rmk_native_key_actions,
            self.supports_universal_symbols,
            self.supports_universal_numero,
            self.supports_universal_russian_letters,
            self.supports_extended_russian_letters,
            self.rmk_native_key_actions_allowed_for_target,
        ]
        .hash(&mut hasher);
        self.magic_keycodes.hash(&mut hasher);
        hasher.finish()
    }

    /// Drop cached rows when the state they were built from changed.
    fn refresh_row_cache(&mut self) {
        let fingerprint = self.row_fingerprint();
        if self.row_cache.fingerprint != Some(fingerprint) {
            self.row_cache = RowCache {
                fingerprint: Some(fingerprint),
                ..RowCache::default()
            };
        }
    }

    /// Rows of the tab, built once per picker state.
    pub(super) fn cached_tab_rows(&mut self, tab: KeycodeTab) -> Arc<Vec<PickerRow>> {
        self.refresh_row_cache();
        if let Some((_, rows)) = self
            .row_cache
            .tabs
            .iter()
            .find(|(cached_tab, _)| *cached_tab == tab)
        {
            return Arc::clone(rows);
        }
        let rows = Arc::new(self.tab_rows(tab));
        self.row_cache.tabs.push((tab, Arc::clone(&rows)));
        rows
    }

    /// The Keys grid cells, built once per picker state.
    pub(super) fn cached_basic_grid_cells(&mut self) -> Arc<Vec<BasicGridCell>> {
        self.refresh_row_cache();
        if let Some(cells) = &self.row_cache.basic_cells {
            return Arc::clone(cells);
        }
        let cells = Arc::new(self.basic_grid_cells());
        self.row_cache.basic_cells = Some(Arc::clone(&cells));
        cells
    }

    /// Everything the search index covers: the rows of every visible tab plus
    /// the macro and Tap Dance slots behind the Special tab choosers.
    pub(super) fn search_rows(&mut self) -> Vec<PickerRow> {
        let mut rows = Vec::new();
        for tab in self.visible_vial_tabs() {
            rows.extend(self.cached_tab_rows(tab).iter().cloned());
        }
        rows.extend(self.slot_rows());
        rows
    }

    /// Row for a plain Vial keycode, searchable by its QMK name as well.
    pub(super) fn vial_row(
        &self,
        tab: KeycodeTab,
        section: &'static str,
        value: u16,
        label: impl Into<String>,
        tooltip: impl Into<String>,
    ) -> PickerRow {
        let row = PickerRow::new(
            tab,
            section,
            label,
            tooltip,
            PickerAction::Assign(crate::keyboard::KeyBinding::Vial(value)),
        );
        match crate::keycode::find_keycode(value) {
            Some(kc) => row.with_aliases(kc.name),
            None => row,
        }
    }

    /// Row for a table keycode with its standard caption and tooltip.
    pub(super) fn table_keycode_row(
        &self,
        tab: KeycodeTab,
        section: &'static str,
        value: u16,
    ) -> PickerRow {
        self.vial_row(
            tab,
            section,
            value,
            keycode_label_with_names_and_layout(
                value,
                &[],
                &self.layer_names,
                self.key_legend_layout,
            ),
            crate::i18n::tr_text(self.language, &self.picker_keycode_tooltip(value, &[])),
        )
    }

    /// Macro and Tap Dance slots by id and user-given name. They live behind
    /// the Special tab choosers, so the search lists them under Special.
    pub(super) fn slot_rows(&self) -> Vec<PickerRow> {
        let mut rows = Vec::new();
        if self.supports_macro {
            let caption = tr_picker(self.language, "macro_editor.picker_item");
            for idx in 0..self.macro_count {
                let value = 0x7700 + idx as u16;
                let user_name = self.macro_names.get(idx).map(String::as_str).unwrap_or("");
                rows.push(
                    self.table_keycode_row(KeycodeTab::Special, caption, value)
                        .with_aliases(format!("{caption} M{idx} {user_name}")),
                );
            }
        }
        if self.supports_tap_dance {
            let caption = tr_picker(self.language, "tap_dance_editor.picker_item");
            for idx in 0..self.tap_dance_entries.len() {
                let value = 0x5700 + idx as u16;
                let slot_name = self
                    .tap_dance_names
                    .get(idx)
                    .map(String::as_str)
                    .unwrap_or("");
                rows.push(
                    self.table_keycode_row(KeycodeTab::Special, caption, value)
                        .with_aliases(format!("{caption} TD{idx} {slot_name}")),
                );
            }
        }
        rows
    }

    pub(super) fn perform_picker_action(&mut self, action: PickerAction) {
        match action {
            PickerAction::Assign(crate::keyboard::KeyBinding::Vial(value)) => {
                self.assign_keycode_value(value);
            }
            PickerAction::Assign(binding) => {
                if crate::universal_symbols::binding_contains_extended_russian_letter(binding)
                    && !self.supports_extended_russian_letters
                {
                    return;
                }
                if crate::universal_symbols::binding_contains_numero(binding)
                    && !self.supports_universal_numero
                {
                    return;
                }
                self.result = Some(binding);
                self.open = false;
            }
            PickerAction::PickLayer(base) => self.vial_layer_pending = Some(base),
            PickerAction::PickModKey(base) => self.vial_quantum_pending_mod = Some(base),
            PickerAction::PickModTap(base) => self.vial_quantum_pending_mt = Some(base),
            PickerAction::ChooseSlot(kind) => self.advanced_slot_picker = Some(kind),
        }
    }

    /// Render a whole tab: its heading when it has one, then its rows.
    pub(super) fn show_tab_rows(&mut self, ui: &mut egui::Ui, tab: KeycodeTab) {
        if let Some(heading) = self.tab_heading(tab) {
            show_section_heading(ui, heading);
        }
        let rows = self.cached_tab_rows(tab);
        self.show_picker_rows(ui, &rows, SlotDataStates::READY);
    }

    /// Render rows grouped by section, in order. Each section gets its
    /// heading once, above a wrapped keycap grid.
    pub(super) fn show_picker_rows(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[PickerRow],
        states: SlotDataStates,
    ) {
        let mut start = 0;
        while start < rows.len() {
            let section = rows[start].section;
            let end = rows[start..]
                .iter()
                .position(|row| row.section != section)
                .map_or(rows.len(), |offset| start + offset);
            if start > 0 {
                ui.add_space(10.0);
            }
            if !section.is_empty() {
                show_section_heading(ui, section);
            }
            ui.horizontal_wrapped(|ui| {
                for row in &rows[start..end] {
                    self.show_picker_row(ui, row, states);
                }
            });
            start = end;
        }
    }

    /// Render one row and act on its click.
    pub(super) fn show_picker_row(
        &mut self,
        ui: &mut egui::Ui,
        row: &PickerRow,
        states: SlotDataStates,
    ) {
        if let PickerAction::ChooseSlot(kind) = row.action {
            self.show_special_action_kind_button(
                ui,
                kind,
                &row.label,
                &row.tooltip,
                states.for_kind(kind),
            );
            return;
        }
        if let Some(action) = row_button(ui, row) {
            self.perform_picker_action(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;

    #[test]
    fn picker_headings_meet_normal_text_contrast_in_both_themes() {
        let luminance = |color: Color32| {
            let channel = color.r() as f64 / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        for dark in [false, true] {
            let text = picker_heading_color(dark);
            let background = crate::ui_style::surface_fill(dark);
            assert_eq!(text.r(), text.g());
            assert_eq!(text.g(), text.b());
            let (a, b) = (luminance(text), luminance(background));
            let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(
                contrast >= 4.5,
                "{dark:?} theme heading contrast {contrast:.2}:1"
            );
        }
    }

    fn full_picker() -> KeycodePicker {
        KeycodePicker {
            supports_rmk_native_key_actions: true,
            supports_universal_numero: false,
            supports_universal_symbols: true,
            supports_universal_russian_letters: true,
            supports_extended_russian_letters: false,
            rmk_native_key_actions_allowed_for_target: true,
            custom_keycodes: vec![
                (
                    "BT0".into(),
                    "BT0".into(),
                    "Bluetooth Profile 0".into(),
                    0x7E00,
                ),
                (
                    "EH_SCR".into(),
                    "Scroll".into(),
                    "Scroll mode".into(),
                    0x7E01,
                ),
            ],
            tap_dance_entries: vec![TapDanceEntry::default(); 2],
            tap_dance_names: vec![String::new(), "Wave".into()],
            ..Default::default()
        }
    }

    #[test]
    fn every_visible_tab_renders_rows() {
        let picker = full_picker();
        for tab in picker.visible_vial_tabs() {
            assert!(!picker.tab_rows(tab).is_empty(), "{tab:?} has no rows");
        }
    }

    #[test]
    fn rows_only_list_keys_the_user_can_pick() {
        let mut picker = KeycodePicker::default();
        picker.open_key_override_trigger_picker(0);
        for tab in picker.visible_vial_tabs() {
            for row in picker.tab_rows(tab) {
                if let PickerAction::Assign(KeyBinding::Vial(value)) = row.action {
                    assert!(
                        picker.picker_value_supported(value),
                        "{tab:?} lists {value:#06X}, which the trigger picker rejects"
                    );
                }
            }
        }
    }

    #[test]
    fn rows_keep_the_home_tab_they_are_rendered_on() {
        let picker = full_picker();
        for tab in picker.visible_vial_tabs() {
            for row in picker.tab_rows(tab) {
                assert_eq!(row.tab, tab, "row {:?} rendered on {tab:?}", row.label);
            }
        }
    }

    #[test]
    fn actions_drive_the_same_state_as_the_tab_buttons() {
        let mut picker = KeycodePicker {
            open: true,
            ..Default::default()
        };
        picker.perform_picker_action(PickerAction::PickLayer(0x5220));
        assert_eq!(picker.vial_layer_pending, Some(0x5220));
        picker.perform_picker_action(PickerAction::PickModKey(0x0100));
        assert_eq!(picker.vial_quantum_pending_mod, Some(0x0100));
        picker.perform_picker_action(PickerAction::PickModTap(0x2100));
        assert_eq!(picker.vial_quantum_pending_mt, Some(0x2100));
        picker.perform_picker_action(PickerAction::ChooseSlot(AdvancedSlotKind::TapDance));
        assert_eq!(
            picker.advanced_slot_picker,
            Some(AdvancedSlotKind::TapDance)
        );
        assert!(picker.open, "pending picks keep the picker open");

        picker.perform_picker_action(PickerAction::Assign(KeyBinding::Vial(0x0004)));
        assert_eq!(picker.result, Some(KeyBinding::Vial(0x0004)));
        assert!(!picker.open);
    }

    /// Accessible name and description of a keycap: the caption users see
    /// and the tooltip behind it. Together they tell apart same-caption keys
    /// such as the three lighting "Toggle" rows.
    fn accessible_pair(row: &PickerRow) -> (String, Option<String>) {
        let description = (!row.tooltip.is_empty()).then(|| row.tooltip.clone());
        (row.accessible_name(), description)
    }

    fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
        items.sort();
        items
    }

    /// Renders every visible tab and checks the accessibility tree: the
    /// keycaps on screen are exactly the tab's rows, and each of them has a
    /// search entry with the same name and description.
    #[test]
    fn every_rendered_keycap_is_searchable_and_every_row_is_rendered() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::{NodeT, Queryable};
        use std::collections::HashSet;

        let configs: [fn() -> KeycodePicker; 2] = [KeycodePicker::default, full_picker];
        for make_picker in configs {
            for tab in make_picker().visible_vial_tabs() {
                let mut picker = make_picker();
                picker.open = true;
                picker.selected_tab = tab;
                picker.language = crate::i18n::Language::English;
                let mut harness = egui_kittest::Harness::builder()
                    .with_size(egui::vec2(1_400.0, 1_000.0))
                    .allow_missing_glyphs()
                    .build_ui_state(
                        |ui, picker: &mut KeycodePicker| {
                            picker.show(
                                ui.ctx(),
                                DeferredPickerDataState::Ready,
                                DeferredPickerDataState::Ready,
                            );
                        },
                        picker,
                    );
                harness.run();

                // Tabs carry a selected state and the window chrome has its
                // own buttons; everything else with a name is a keycap.
                let rendered: Vec<(String, Option<String>)> = harness
                    .get_all_by_role(Role::Button)
                    .filter(|node| node.accesskit_node().toggled().is_none())
                    .filter_map(|node| {
                        let node = node.accesskit_node();
                        node.label()
                            .filter(|label| label != "Close window")
                            .map(|label| (label, node.description()))
                    })
                    .collect();
                assert!(!rendered.is_empty(), "{tab:?} rendered no keycaps");

                let expected: Vec<(String, Option<String>)> = harness
                    .state()
                    .tab_rows(tab)
                    .iter()
                    .map(accessible_pair)
                    .collect();
                assert_eq!(
                    sorted(rendered.clone()),
                    sorted(expected),
                    "{tab:?}: keycaps on screen differ from the tab's rows"
                );

                let searchable: HashSet<(String, Option<String>)> = harness
                    .state_mut()
                    .search_rows()
                    .iter()
                    .map(accessible_pair)
                    .collect();
                let unsearchable: Vec<_> = rendered
                    .iter()
                    .filter(|pair| !searchable.contains(*pair))
                    .collect();
                assert!(
                    unsearchable.is_empty(),
                    "{tab:?}: rendered keycaps without a search entry: {unsearchable:?}"
                );
            }
        }
    }

    #[test]
    fn cached_rows_follow_picker_state() {
        let mut picker = KeycodePicker::default();
        assert!(picker.cached_tab_rows(KeycodeTab::Custom).is_empty());
        let q_cell = |cells: &[BasicGridCell]| {
            cells
                .iter()
                .find(|cell| cell.grid.label == "Q")
                .map(|cell| cell.row.label.clone())
                .expect("Q slot")
        };
        assert_eq!(q_cell(&picker.cached_basic_grid_cells()), "Q");

        picker.custom_keycodes.push((
            "EH_SCR".into(),
            "Scroll".into(),
            "Scroll mode".into(),
            0x7E01,
        ));
        picker.basic_layout = BasicPickerLayout::Dvorak;

        let custom = picker.cached_tab_rows(KeycodeTab::Custom);
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].label, "Scroll");
        // Dvorak puts the quote key in the Q slot; the caption shows both
        // the shifted and the plain symbol.
        assert_eq!(q_cell(&picker.cached_basic_grid_cells()), "\"\n'");
    }

    #[test]
    fn approved_clear_eeprom_catalog_picker_search_and_assigned_ru_en() {
        use crate::i18n::{tr_text, Language};
        for language in [Language::Russian, Language::English] {
            let mut picker = KeycodePicker::default();
            picker.language = language;
            let rows = picker.special_rows();
            let row = rows
                .iter()
                .find(|r| r.action == PickerAction::Assign(KeyBinding::Vial(0x7c03)))
                .expect("Clear EEPROM on Special tab");
            assert_eq!(row.label.replace('\n', " "), "Clear EEPROM");
            let group: Vec<_> = rows.iter().filter(|r| r.section == row.section).collect();
            assert_eq!(
                group[group.len() - 2].action,
                PickerAction::Assign(KeyBinding::Vial(0x7c7a))
            );
            assert_eq!(
                group.last().unwrap().action,
                PickerAction::Assign(KeyBinding::Vial(0x7c03))
            );
            for query in ["Clear EEPROM", "QK_CLEAR_EEPROM", "EE_CLR"] {
                assert!(row.matches(&query.to_lowercase(), None), "{query}");
            }
            assert_eq!(
                crate::keycode::find_keycode(0x7c03).unwrap().name,
                "QK_CLEAR_EEPROM"
            );
            let action = crate::rynk::keycode::binding_action(KeyBinding::Vial(0x7c03)).unwrap();
            assert!(matches!(
                action,
                rmk_types_rynk::action::KeyAction::Single(
                    rmk_types_rynk::action::Action::KeyboardControl(
                        rmk_types_rynk::action::KeyboardAction::ClearEeprom
                    )
                )
            ));
            assert_eq!(
                crate::rynk::keycode::key_binding(action),
                KeyBinding::Vial(0x7c03)
            );
            let serialized = serde_json::to_string(&KeyBinding::Vial(0x7c03)).unwrap();
            assert_eq!(
                serde_json::from_str::<KeyBinding>(&serialized).unwrap(),
                KeyBinding::Vial(0x7c03)
            );
            assert_eq!(
                crate::keycode::keycode_label_with_custom(0x7c03, &[]),
                "Clear\nEEPROM"
            );
            let tip = tr_text(language, &crate::keycode::keycode_tooltip(0x7c03, &[], &[]));
            assert_eq!(row.tooltip, tip);
            if language == Language::Russian {
                assert_eq!(tip,"Сброс сохранённых назначений клавиш и настроек к заводским с перезапуском клавиатуры");
                assert_eq!(row.section, "Специальные клавиши");
            } else {
                assert_eq!(tip,"Reset saved key assignments and settings to factory defaults and restart the keyboard");
            }
        }
    }

    #[test]
    fn k04_followon_normal_picker_and_actual_rynk_assignment_roundtrip() {
        use crate::rynk::session::testing::{board, link, Stored};
        use rmk_types_rynk::action::{Action, KeyAction};
        let definitions: std::collections::BTreeMap<String, serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fixtures/k04-followon-settings.json"))
                .unwrap();
        let old: std::collections::BTreeMap<String, serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fixtures/k04-upstream-9e2aef92.json"))
                .unwrap();
        for (name, definition) in definitions {
            assert_eq!(
                &definition["customKeycodes"].as_array().unwrap()[..60],
                old[&name]["customKeycodes"].as_array().unwrap()
            );
            let layout = crate::keyboard::KeyboardLayout::from_vial_json(&definition).unwrap();
            let mut picker = KeycodePicker::default();
            picker.custom_keycodes = layout
                .custom_keycodes
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    (
                        k.name.clone(),
                        k.label.clone(),
                        k.title.clone(),
                        0x7e00 + i as u16,
                    )
                })
                .collect();
            let (board, state) = board(Stored::new(1, 1, 1));
            let session =
                crate::rynk::session::RynkSession::new(link("k04-normal-assignment", &board));
            for language in [
                crate::i18n::Language::Russian,
                crate::i18n::Language::English,
            ] {
                picker.language = language;
                let rows = picker.custom_keycode_rows(false);
                let names = [
                    "EH_NORMAL",
                    "EH_SNP",
                    "EH_SCR",
                    "EH_TXT",
                    "EH_L_NORMAL",
                    "EH_L_SNP",
                    "EH_L_SCR",
                    "EH_L_TXT",
                    "EH_R_NORMAL",
                    "EH_USR1",
                    "EH_USR2",
                    "EH_USR3",
                ];
                let expected: Vec<_> = names
                    .iter()
                    .map(|name| {
                        PickerAction::Assign(KeyBinding::Vial(
                            picker
                                .custom_keycodes
                                .iter()
                                .find(|(n, _, _, _)| n == name)
                                .unwrap()
                                .3,
                        ))
                    })
                    .collect();
                assert_eq!(
                    rows[..12].iter().map(|r| r.action).collect::<Vec<_>>(),
                    expected
                );
                for (id, name, anchor) in [
                    (60, "EH_L_NORMAL", "EH_TXT"),
                    (61, "EH_NORMAL", ""),
                    (62, "EH_R_NORMAL", "EH_L_TXT"),
                ] {
                    let code = 0x7e00 + id;
                    let pos = rows
                        .iter()
                        .position(|r| r.action == PickerAction::Assign(KeyBinding::Vial(code)))
                        .unwrap();
                    if anchor.is_empty() {
                        assert_eq!(pos, 0);
                    } else {
                        let anchor = picker
                            .custom_keycodes
                            .iter()
                            .find(|(n, _, _, _)| n == anchor)
                            .unwrap()
                            .3;
                        assert_eq!(
                            rows[pos - 1].action,
                            PickerAction::Assign(KeyBinding::Vial(anchor))
                        );
                    }
                    assert!(rows[pos].matches(&name.to_lowercase(), None));
                    session
                        .set_key_binding(0, 0, 0, KeyBinding::Vial(code))
                        .unwrap();
                    let action = state.lock().unwrap().keymap[0];
                    assert_eq!(action, KeyAction::Single(Action::User(id as u8)));
                    assert_eq!(
                        crate::rynk::keycode::key_binding(action),
                        KeyBinding::Vial(code)
                    );
                }
            }
            let legacy = crate::keyboard::KeyboardLayout::from_vial_json(&old[&name]).unwrap();
            assert!(!legacy.custom_keycodes.iter().any(|k| k.name == "EH_NORMAL"));
        }
    }

    #[test]
    fn approved_normal_descriptor_only_custom_picker_decode_search_and_old_gate() {
        let definition: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/phenom-approved-timeout20-normal.json"
        ))
        .unwrap();
        for advertised in [false, true] {
            let mut json = definition.clone();
            if !advertised {
                json["customKeycodes"].as_array_mut().unwrap().truncate(73);
            }
            let layout = crate::keyboard::KeyboardLayout::from_vial_json(&json).unwrap();
            let mut picker = KeycodePicker::default();
            picker.custom_keycodes = layout
                .custom_keycodes
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    (
                        k.name.clone(),
                        k.label.clone(),
                        k.title.clone(),
                        0x7e00 + i as u16,
                    )
                })
                .collect();
            let rows = picker.custom_keycode_rows(false);
            for (code, label, name) in [
                (0x7e49, "Left\nNormal", "EH_L_NORMAL"),
                (0x7e4a, "Normal\nmode", "EH_NORMAL"),
                (0x7e4b, "Right\nNormal", "EH_R_NORMAL"),
            ] {
                let row = rows
                    .iter()
                    .find(|r| r.action == PickerAction::Assign(KeyBinding::Vial(code)));
                assert_eq!(row.is_some(), advertised);
                if let Some(row) = row {
                    assert_eq!(row.tab, KeycodeTab::Custom);
                    assert_eq!(row.label, label);
                    assert!(row.matches(&name.to_lowercase(), None));
                    assert!(row.matches("normal", None));
                    assert_eq!(
                        crate::keycode::keycode_label_with_custom(code, &layout.custom_keycodes),
                        label
                    );
                    assert_eq!(
                        crate::keycode::keycode_tooltip(code, &layout.custom_keycodes, &[]),
                        row.tooltip
                    );
                }
            }
        }
    }

    #[test]
    fn approved_normal_picker_order_moves_only_display_rows_and_keeps_wire_slots() {
        let definition: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/phenom-approved-timeout20-normal.json"
        ))
        .unwrap();
        let layout = crate::keyboard::KeyboardLayout::from_vial_json(&definition).unwrap();
        let mut picker = KeycodePicker::default();
        picker.custom_keycodes = layout
            .custom_keycodes
            .iter()
            .enumerate()
            .map(|(i, k)| {
                (
                    k.name.clone(),
                    k.label.clone(),
                    k.title.clone(),
                    0x7e00 + i as u16,
                )
            })
            .collect();
        let original = picker.custom_keycodes.clone();
        let original_values: Vec<_> = original
            .iter()
            .filter(|(n, l, t, _)| {
                !l.trim().is_empty() && picker.custom_keycode_tab(n, l, t) == KeycodeTab::Custom
            })
            .map(|(_, _, _, v)| PickerAction::Assign(KeyBinding::Vial(*v)))
            .collect();
        let names = [
            "EH_NORMAL",
            "EH_SNP",
            "EH_SCR",
            "EH_TXT",
            "EH_L_NORMAL",
            "EH_L_SNP",
            "EH_L_SCR",
            "EH_L_TXT",
            "EH_R_NORMAL",
            "EH_USR1",
            "EH_USR2",
            "EH_USR3",
        ];
        let mut expected: Vec<_> = names
            .iter()
            .map(|name| {
                PickerAction::Assign(KeyBinding::Vial(
                    original.iter().find(|(n, _, _, _)| n == name).unwrap().3,
                ))
            })
            .collect();
        let mode_actions = expected.clone();
        expected.extend(
            original_values
                .iter()
                .filter(|a| !mode_actions.contains(a))
                .copied(),
        );
        for language in [
            crate::i18n::Language::Russian,
            crate::i18n::Language::English,
        ] {
            picker.language = language;
            let rows = picker.custom_keycode_rows(false);
            assert_eq!(
                rows.iter().map(|r| r.action.clone()).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                picker.custom_keycodes, original,
                "descriptor slot ownership must not move"
            );
        }
        // Legacy descriptors do not acquire phantom entries or change order.
        picker.custom_keycodes.truncate(73);
        let legacy: Vec<_> = original_values
            .into_iter()
            .filter(|a| !matches!(a, PickerAction::Assign(KeyBinding::Vial(0x7e49..=0x7e4b))))
            .collect();
        assert_eq!(
            picker
                .custom_keycode_rows(false)
                .iter()
                .map(|r| r.action.clone())
                .collect::<Vec<_>>(),
            legacy
        );
        // Same visible labels from an unrelated descriptor are not sufficient.
        picker.custom_keycodes = original
            .iter()
            .map(|(n, l, t, v)| (format!("OTHER_{n}"), l.clone(), t.clone(), *v))
            .collect();
        let unrelated: Vec<_> = picker
            .custom_keycode_rows(false)
            .into_iter()
            .map(|r| r.action)
            .collect();
        let original_order: Vec<_> = picker
            .custom_keycodes
            .iter()
            .filter(|(_, l, _, _)| !l.trim().is_empty())
            .filter(|(n, l, t, _)| picker.custom_keycode_tab(n, l, t) == KeycodeTab::Custom)
            .map(|(_, _, _, v)| PickerAction::Assign(KeyBinding::Vial(*v)))
            .collect();
        assert_eq!(unrelated, original_order);
    }

    #[test]
    fn aliases_accumulate_and_match_alongside_the_caption() {
        let row = PickerRow::new(
            KeycodeTab::Rgb,
            "",
            "Bright\n-",
            "Decrease brightness",
            PickerAction::Assign(KeyBinding::Vial(0x7A08)),
        )
        .with_aliases("Brightness -")
        .with_aliases("  ")
        .with_aliases("RGB_VAD");
        assert_eq!(row.aliases, "Brightness - RGB_VAD");
        assert!(row.matches("brightness -", None));
        assert!(row.matches("bright -", None));
        assert!(!row.matches("hue", None));
    }
}
