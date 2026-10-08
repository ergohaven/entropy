use super::*;

// Picker search. Pipeline: query -> normalize -> memo check -> rows of every
// visible tab plus the macro and Tap Dance slots -> substring match -> one
// hit per action -> cached results -> results grouped by tab and section.
// The memo covers both the query and the picker state the rows derive from,
// so results follow device data that arrives while the picker is open.

/// Search state of the picker: the live query plus results cached for it.
#[derive(Default)]
pub(super) struct PickerSearch {
    /// Text bound to the search field.
    pub(super) query: String,
    /// Normalized query the cached results were computed for.
    resolved_query: String,
    /// `KeycodePicker::row_fingerprint` the cached results were computed for.
    resolved_fingerprint: Option<u64>,
    results: Vec<PickerRow>,
}

impl PickerSearch {
    pub(super) fn is_active(&self) -> bool {
        !self.resolved_query.is_empty()
    }

    pub(super) fn results(&self) -> &[PickerRow] {
        &self.results
    }

    pub(super) fn reset(&mut self) {
        self.query.clear();
        self.resolved_query.clear();
        self.resolved_fingerprint = None;
        self.results.clear();
    }
}

fn normalized_query(query: &str) -> String {
    query.trim().replace('\n', " ").to_lowercase()
}

/// Disambiguate equal captions within a search group with their QMK names.
/// The regular tab keeps its compact keycaps; search can put distinct actions
/// (e.g. media Stop and browser Stop) right next to one another.
fn search_result_row(row: &PickerRow, group: &[PickerRow]) -> PickerRow {
    let mut result = row.clone();
    if group
        .iter()
        .any(|other| other.action != row.action && other.label == row.label)
    {
        if let Some(name) = row
            .aliases
            .split_whitespace()
            .find(|name| name.starts_with("KC_"))
        {
            let caption = row.label.rsplit('\n').next().unwrap_or(&row.label);
            result.label = format!("{}\n{}", name.trim_start_matches("KC_"), caption);
        }
    }
    result
}

/// Case-insensitive substring match over several texts; newlines count as
/// spaces. `needle_lower` must already be normalized.
pub(super) fn search_text_matches(needle_lower: &str, haystacks: &[&str]) -> bool {
    haystacks.iter().any(|text| {
        text.replace('\n', " ")
            .to_lowercase()
            .contains(needle_lower)
    })
}

impl KeycodePicker {
    /// Clear the search field and cached results (e.g. when the host page
    /// hands the picker a new target).
    pub(crate) fn reset_search(&mut self) {
        self.search.reset();
    }

    /// Recompute cached search results when the query or the searchable
    /// picker state changed (device data, slot names, language...).
    pub(super) fn refresh_vial_search_results(&mut self) {
        let query = normalized_query(&self.search.query);
        let fingerprint = self.row_fingerprint();
        if query == self.search.resolved_query
            && self.search.resolved_fingerprint == Some(fingerprint)
        {
            return;
        }
        self.search.resolved_query = query.clone();
        self.search.resolved_fingerprint = Some(fingerprint);
        self.search.results.clear();
        if query.is_empty() {
            return;
        }

        let mut results: Vec<PickerRow> = Vec::new();
        for row in self.search_rows() {
            // A key visible on two tabs is listed once, under the first tab.
            if row.matches(&query, self.tab_heading(row.tab))
                && !results.iter().any(|hit| hit.action == row.action)
            {
                results.push(row);
            }
        }
        self.search.results = results;
    }

    pub(super) fn show_vial_search_results(&mut self, ui: &mut egui::Ui, states: SlotDataStates) {
        if self.search.results().is_empty() {
            ui.add_space(52.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr_picker(self.language, "key_picker.search_empty"))
                        .size(13.0)
                        .color(ui.visuals().weak_text_color()),
                );
            });
            return;
        }

        ui.label(
            RichText::new(crate::i18n::tr_catalog_format(
                self.language,
                "key_picker.search_matches",
                &[("count", self.search.results().len().to_string().as_str())],
            ))
            .size(11.0)
            .color(picker_heading_color(ui.visuals().dark_mode)),
        );
        ui.add_space(4.0);

        let results = self.search.results().to_vec();
        let dark = ui.visuals().dark_mode;

        // Group hits by (tab, section) in order of first appearance, so every
        // result row says where the key normally lives.
        let mut groups: Vec<((KeycodeTab, &'static str), Vec<PickerRow>)> = Vec::new();
        for row in results {
            let key = (row.tab, row.section);
            match groups.iter_mut().find(|(group_key, _)| *group_key == key) {
                Some((_, rows)) => rows.push(row),
                None => groups.push((key, vec![row])),
            }
        }

        for ((tab, section), rows) in groups {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(
                    RichText::new(tab.style().glyph)
                        .size(12.0)
                        .color(tab.style().tint(dark)),
                );
                let mut heading = picker_tab_label(self.language, tab).to_string();
                let section = if section.is_empty() {
                    self.tab_heading(tab).unwrap_or("")
                } else {
                    section
                };
                if !section.is_empty() {
                    heading.push_str(" · ");
                    heading.push_str(section);
                }
                ui.label(
                    RichText::new(heading)
                        .size(11.0)
                        .color(picker_heading_color(dark)),
                );
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for row in &rows {
                    self.show_picker_row(ui, &search_result_row(row, &rows), states);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;

    fn search(picker: &mut KeycodePicker, query: &str) -> Vec<PickerRow> {
        picker.search.query = query.into();
        picker.refresh_vial_search_results();
        picker.search.results().to_vec()
    }

    fn hit<'a>(results: &'a [PickerRow], action: PickerAction) -> Option<&'a PickerRow> {
        results.iter().find(|row| row.action == action)
    }

    fn results_contain(picker: &KeycodePicker, value: u16) -> bool {
        picker
            .search
            .results()
            .iter()
            .any(|hit| hit.action == PickerAction::Assign(KeyBinding::Vial(value)))
    }

    fn special_section(picker: &KeycodePicker, key: &'static str) -> &'static str {
        crate::i18n::tr_catalog(picker.language, key)
    }

    #[test]
    fn matches_texts_case_insensitively_with_newlines_as_spaces() {
        assert!(search_text_matches(
            "vol",
            &["Vol+\nUp", "KC_VOLU", "Volume up"]
        ));
        assert!(search_text_matches("vol+ up", &["Vol+\nUp", "KC_VOLU", ""]));
        assert!(search_text_matches("kc_volu", &["Vol+", "KC_VOLU", ""]));
        assert!(search_text_matches(
            "громкость",
            &["Vol+", "KC_VOLU", "Громкость +"]
        ));
        assert!(!search_text_matches(
            "bluetooth",
            &["Vol+", "KC_VOLU", "Volume up"]
        ));
    }

    #[test]
    fn collects_matches_across_keycodes_and_macro_names() {
        let mut picker = KeycodePicker {
            macro_count: 2,
            macro_names: vec!["Email signature".into(), String::new()],
            ..Default::default()
        };

        search(&mut picker, "email");
        assert!(results_contain(&picker, 0x7700));

        // QMK names stay searchable for keys of the Keys grid.
        search(&mut picker, "kc_escape");
        assert!(results_contain(&picker, 0x0029));

        search(&mut picker, "");
        assert!(picker.search.results().is_empty());
    }

    #[test]
    fn single_section_heading_is_searchable() {
        let mut picker = KeycodePicker::default();
        let basic_heading = picker.tab_heading(KeycodeTab::Basic).unwrap();
        let results = search(&mut picker, "standard keyboard layout");
        assert!(!results.is_empty());
        assert!(results.iter().all(|row| row.tab == KeycodeTab::Basic));
        assert!(results.iter().all(|row| row.section.is_empty()));
        assert!(results
            .iter()
            .all(|row| row.matches("standard keyboard layout", Some(basic_heading))));
    }

    #[test]
    fn every_row_of_every_visible_tab_is_indexed() {
        let mut picker = KeycodePicker {
            supports_rmk_native_key_actions: true,
            supports_universal_symbols: true,
            supports_universal_russian_letters: true,
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
            ..Default::default()
        };
        let corpus = picker.search_rows();
        for tab in picker.visible_vial_tabs() {
            for row in picker.tab_rows(tab) {
                assert!(
                    corpus.contains(&row),
                    "{tab:?} row {:?} is not searchable",
                    row.label
                );
            }
        }
    }

    #[test]
    fn finds_rows_the_static_keycode_table_does_not_know() {
        let mut picker = KeycodePicker::default();

        // RGB tab rows are hardcoded keycodes.
        let results = search(&mut picker, "backlight");
        let toggle = hit(&results, PickerAction::Assign(KeyBinding::Vial(0x7800)))
            .expect("backlight toggle should be searchable");
        assert_eq!(toggle.tab, KeycodeTab::Rgb);
        assert_eq!(
            toggle.section,
            special_section(&picker, "key_picker_text.backlight")
        );
        assert!(
            results_contain(&picker, 0x7A00) || {
                search(&mut picker, "rgb lighting");
                results_contain(&picker, 0x7A00)
            }
        );

        // Layer actions open the layer chooser.
        let results = search(&mut picker, "layer tg");
        assert!(hit(&results, PickerAction::PickLayer(0x5260)).is_some());
        let results = search(&mut picker, "lt");
        assert!(hit(&results, PickerAction::PickLayer(0x4000)).is_some());

        // Mod-Tap, Mod+Key and One-Shot modifiers.
        let results = search(&mut picker, "hold ctrl");
        assert!(hit(&results, PickerAction::PickModTap(0x2100)).is_some());
        let results = search(&mut picker, "mod+key shift");
        assert!(hit(&results, PickerAction::PickModKey(0x0200)).is_some());
        search(&mut picker, "osm");
        assert!(results_contain(&picker, 0x52A1));

        // OS editing shortcuts are composed chords.
        search(&mut picker, "undo");
        assert!(picker.search.results().iter().any(|row| {
            row.section == special_section(&picker, "key_picker_text.os_edit_shortcuts")
        }));
    }

    #[test]
    fn results_keep_the_home_tab_and_section_users_see() {
        let mut picker = KeycodePicker {
            supports_mouse_keys: false,
            ..Default::default()
        };

        // Two different "Stop" keys stay apart by their QMK names but share
        // the section they are shown in.
        let results = search(&mut picker, "stop");
        let media_section = special_section(&picker, "key_picker_text.media_apps_system");
        for value in [0x00AD, 0x00B8] {
            let row = hit(&results, PickerAction::Assign(KeyBinding::Vial(value)))
                .unwrap_or_else(|| panic!("{value:#06X} should be found by 'stop'"));
            assert_eq!(row.tab, KeycodeTab::Special);
            assert_eq!(row.section, media_section);
        }
        let media = hit(&results, PickerAction::Assign(KeyBinding::Vial(0x00AD))).unwrap();
        let browser = hit(&results, PickerAction::Assign(KeyBinding::Vial(0x00B8))).unwrap();
        let stop_group: Vec<PickerRow> = results
            .iter()
            .filter(|row| row.section == media_section)
            .cloned()
            .collect();
        assert_eq!(
            search_result_row(media, &stop_group).accessible_name(),
            "MSTP Stop"
        );
        assert_eq!(
            search_result_row(browser, &stop_group).accessible_name(),
            "WSTP Stop"
        );
        assert_ne!(
            search_result_row(media, &stop_group).label,
            search_result_row(browser, &stop_group).label
        );

        let results = search(&mut picker, "kana");
        let jis = hit(&results, PickerAction::Assign(KeyBinding::Vial(0x0088)))
            .expect("JIS Kana should be searchable");
        assert_eq!(jis.tab, KeycodeTab::Special);
        assert_eq!(
            jis.section,
            special_section(&picker, "key_picker_text.international")
        );

        // Magic and Space Cadet do not depend on mouse-key support.
        let results = search(&mut picker, "magic");
        assert!(hit(&results, PickerAction::Assign(KeyBinding::Vial(0x7000))).is_some());
        let results = search(&mut picker, "space cadet");
        assert!(hit(&results, PickerAction::Assign(KeyBinding::Vial(0x7C18))).is_some());

        // Mouse keys follow the tab: hidden when the firmware lacks them.
        search(&mut picker, "mouse");
        assert!(!picker
            .search
            .results()
            .iter()
            .any(|row| { row.section == special_section(&picker, "key_picker_text.mouse") }));
    }

    #[test]
    fn a_key_shown_on_two_tabs_is_listed_once_under_the_first() {
        let mut picker = KeycodePicker::default();
        let results = search(&mut picker, "shift");
        let shifts: Vec<&PickerRow> = results
            .iter()
            .filter(|row| row.action == PickerAction::Assign(KeyBinding::Vial(0x00E1)))
            .collect();
        assert_eq!(shifts.len(), 1);
        assert_eq!(shifts[0].tab, KeycodeTab::Basic);
    }

    #[test]
    fn results_follow_device_data_loaded_after_the_query() {
        // Macro names arrive with the deferred device load.
        let mut picker = KeycodePicker {
            macro_count: 0,
            macro_names: Vec::new(),
            ..Default::default()
        };
        assert!(search(&mut picker, "signature").is_empty());
        picker.macro_count = 1;
        picker.macro_names = vec!["Email signature".into()];
        picker.refresh_vial_search_results();
        assert!(results_contain(&picker, 0x7700));

        // Device-defined keycodes appear once the definition is parsed.
        assert!(search(&mut picker, "scroll mode").is_empty());
        picker.custom_keycodes.push((
            "EH_SCR".into(),
            "Scroll".into(),
            "Scroll mode".into(),
            0x7E01,
        ));
        picker.refresh_vial_search_results();
        assert!(results_contain(&picker, 0x7E01));

        // A Tap Dance renamed in its editor is found by the new name.
        picker.tap_dance_entries = vec![TapDanceEntry::default()];
        picker.tap_dance_names = vec![String::new()];
        assert!(search(&mut picker, "wave").is_empty());
        picker.tap_dance_names[0] = "Wave".into();
        picker.refresh_vial_search_results();
        assert!(results_contain(&picker, 0x5700));
    }

    #[test]
    fn results_follow_a_language_switch() {
        let mut picker = KeycodePicker {
            language: crate::i18n::Language::English,
            ..Default::default()
        };
        assert!(search(&mut picker, "нампад").is_empty());
        picker.language = crate::i18n::Language::Russian;
        picker.refresh_vial_search_results();
        assert!(picker
            .search
            .results()
            .iter()
            .any(|row| row.section == special_section(&picker, "key_picker_text.numpad")));
    }

    #[test]
    fn open_picker_shows_items_loaded_while_it_is_open() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;

        let mut picker = KeycodePicker {
            open: true,
            language: crate::i18n::Language::English,
            macro_count: 1,
            macro_names: vec![String::new()],
            ..Default::default()
        };
        picker.search.query = "signature".into();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1_400.0, 1_000.0))
            .build_state(
                |ctx, picker: &mut KeycodePicker| {
                    picker.show(
                        ctx,
                        DeferredPickerDataState::Ready,
                        DeferredPickerDataState::Ready,
                    );
                },
                picker,
            );
        harness.run();
        assert!(harness.state().search.results().is_empty());

        harness.state_mut().macro_names[0] = "Email signature".into();
        harness.run();
        let hit = harness.state().search.results()[0].clone();
        assert_eq!(hit.action, PickerAction::Assign(KeyBinding::Vial(0x7700)));
        let name = hit.accessible_name();
        assert!(
            harness
                .query_by_role_and_label(Role::Button, &name)
                .is_some(),
            "the new macro should be on screen without retyping the query"
        );
    }

    #[test]
    fn finds_universal_symbols_when_firmware_supports_them() {
        let mut picker = KeycodePicker {
            supports_universal_symbols: true,
            supports_rmk_native_key_actions: true,
            rmk_native_key_actions_allowed_for_target: true,
            supports_universal_russian_letters: true,
            ..Default::default()
        };
        let results = search(&mut picker, "universal");
        let sync = crate::universal_symbols::binding(crate::universal_symbols::USER_SYNC);
        let ru_letter =
            crate::universal_symbols::binding(crate::universal_symbols::USER_RUSSIAN_LETTER_START);
        let sync_row = hit(&results, PickerAction::Assign(sync)).expect("sync should be found");
        assert_eq!(sync_row.tab, KeycodeTab::UniversalSymbols);
        let letter_row =
            hit(&results, PickerAction::Assign(ru_letter)).expect("letter should be found");
        assert_eq!(letter_row.tab, KeycodeTab::Special);
        assert_eq!(
            letter_row.section,
            special_section(&picker, "key_picker_text.international")
        );

        // Without RMK actions allowed for the target, universal entries vanish.
        let mut gated = KeycodePicker {
            supports_universal_symbols: true,
            supports_rmk_native_key_actions: true,
            rmk_native_key_actions_allowed_for_target: false,
            ..Default::default()
        };
        let results = search(&mut gated, "universal");
        assert!(hit(&results, PickerAction::Assign(sync)).is_none());
    }

    #[test]
    fn finds_special_captions_and_slot_entries() {
        let mut picker = KeycodePicker {
            macro_count: 1,
            macro_names: vec![String::new()],
            tap_dance_entries: vec![TapDanceEntry::default(); 2],
            tap_dance_names: vec![String::new(), "Волна".into()],
            ..Default::default()
        };

        search(&mut picker, "tap dan");
        assert!(results_contain(&picker, 0x5700));
        assert!(results_contain(&picker, 0x5701));
        assert!(picker
            .search
            .results()
            .iter()
            .any(|row| { row.action == PickerAction::ChooseSlot(AdvancedSlotKind::TapDance) }));

        search(&mut picker, "волна");
        assert_eq!(picker.search.results().len(), 1);
        assert!(results_contain(&picker, 0x5701));

        search(&mut picker, "none");
        assert!(results_contain(&picker, 0x0000));

        search(&mut picker, "inherit");
        assert!(results_contain(&picker, 0x0001));

        search(&mut picker, "macro");
        assert!(results_contain(&picker, 0x7700));
        assert!(picker
            .search
            .results()
            .iter()
            .any(|row| { row.action == PickerAction::ChooseSlot(AdvancedSlotKind::Macro) }));
    }
}
