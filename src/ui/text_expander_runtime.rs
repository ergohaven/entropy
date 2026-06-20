use super::*;

impl EntropyApp {
    pub(super) fn text_expander_rule_issue(
        &self,
        idx: usize,
        rule: &crate::text_expander::TextExpansionRule,
    ) -> Option<&'static str> {
        if rule.trigger.is_empty() {
            return Some("text_expander.issue_empty_trigger");
        }
        if !crate::text_expander::valid_trigger(&rule.trigger) {
            return Some("text_expander.issue_invalid_trigger");
        }
        if crate::text_expander::prepare_replacement(&rule.replacement)
            .0
            .is_empty()
        {
            return Some("text_expander.issue_empty_replacement");
        }
        if self
            .app_settings
            .text_expansion_rules
            .iter()
            .enumerate()
            .any(|(other_idx, other)| other_idx != idx && other.trigger == rule.trigger)
        {
            return Some("text_expander.issue_duplicate_trigger");
        }
        None
    }

    pub(super) fn active_text_expansion_rules(
        &self,
    ) -> Vec<crate::text_expander::TextExpansionRule> {
        let mut rules = self.app_settings.text_expansion_rules.clone();
        rules.extend(load_extra_text_expansion_rules(
            &self.app_settings.text_expander_rule_files,
        ));
        rules
    }

    pub(super) fn sync_text_expander_runtime(&mut self) {
        crate::smart_input::set_text_expander_config(
            self.app_settings.text_expander_enabled,
            self.active_text_expansion_rules(),
            parse_text_expander_blacklist(&self.app_settings.text_expander_app_blacklist),
        );
    }

    pub(super) fn save_text_expander_settings(&mut self) {
        self.text_expander_settings_save_pending = false;
        save_text_expansion_rules(&self.app_settings.text_expansion_rules);
        self.text_expander_rules_signature =
            text_expander_rules_signature(&self.app_settings.text_expander_rule_files);
        save_app_settings(&self.app_settings);
        self.sync_text_expander_runtime();
    }

    pub(super) fn queue_text_expander_settings_save(&mut self, now: f64) {
        self.text_expander_settings_save_pending = true;
        self.text_expander_settings_last_edit_at = now;
    }

    pub(super) fn flush_pending_text_expander_settings(&mut self) {
        if self.text_expander_settings_save_pending {
            self.save_text_expander_settings();
        }
    }

    pub(super) fn poll_text_expander_deferred_save(&mut self, now: f64) {
        if self.text_expander_settings_save_pending
            && now - self.text_expander_settings_last_edit_at >= TEXT_EXPANDER_SAVE_DEBOUNCE_SECS
        {
            self.save_text_expander_settings();
        }
    }

    pub(super) fn add_text_expander_blacklist_app(&mut self, app_name: &str) -> bool {
        let Some(app_name) = normalize_text_expander_app_name(app_name) else {
            return false;
        };
        let mut entries =
            parse_text_expander_blacklist(&self.app_settings.text_expander_app_blacklist);
        if entries.iter().any(|entry| entry == &app_name) {
            return false;
        }
        entries.push(app_name);
        self.app_settings.text_expander_app_blacklist = format_text_expander_blacklist(&entries);
        self.save_text_expander_settings();
        true
    }

    pub(super) fn remove_text_expander_blacklist_app(&mut self, app_name: &str) -> bool {
        let Some(app_name) = normalize_text_expander_app_name(app_name) else {
            return false;
        };
        let mut entries =
            parse_text_expander_blacklist(&self.app_settings.text_expander_app_blacklist);
        let old_len = entries.len();
        entries.retain(|entry| entry != &app_name);
        if entries.len() == old_len {
            return false;
        }
        self.app_settings.text_expander_app_blacklist = format_text_expander_blacklist(&entries);
        self.save_text_expander_settings();
        true
    }

    pub(super) fn text_expander_window_candidates(
        &self,
        blacklist_entries: &[String],
    ) -> Vec<crate::smart_input::TextExpanderAppCandidate> {
        let mut candidates = Vec::new();
        for candidate in crate::smart_input::text_expander_app_candidates() {
            let Some(exe) = normalize_text_expander_app_name(&candidate.exe) else {
                continue;
            };
            if blacklist_entries.iter().any(|blocked| blocked == &exe) {
                continue;
            }
            if candidates
                .iter()
                .any(|existing: &crate::smart_input::TextExpanderAppCandidate| existing.exe == exe)
            {
                continue;
            }
            candidates.push(crate::smart_input::TextExpanderAppCandidate {
                exe,
                title: candidate.title,
            });
        }
        candidates.truncate(8);
        candidates
    }

    pub(super) fn reload_text_expander_rules_file(&mut self) -> bool {
        if let Some(rules) = load_text_expansion_rules() {
            self.app_settings.text_expansion_rules = rules;
            self.save_text_expander_settings();
            true
        } else {
            false
        }
    }

    pub(super) fn auto_reload_text_expander_rules_file(&mut self, now: f64) {
        if self.text_expander_settings_save_pending {
            return;
        }
        if now - self.text_expander_rules_last_check_at < 1.0 {
            return;
        }
        self.text_expander_rules_last_check_at = now;
        let current_signature =
            text_expander_rules_signature(&self.app_settings.text_expander_rule_files);
        if current_signature != self.text_expander_rules_signature {
            if self.reload_text_expander_rules_file() {
                self.sync_text_expander_runtime();
                self.text_expander_rules_signature = current_signature;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "text_expander.rules_auto_reloaded_status",
                )
                .to_owned();
            } else {
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "text_expander.rules_reload_failed_status",
                )
                .to_owned();
                self.text_expander_rules_signature = current_signature;
            }
        }
    }

    pub(super) fn open_text_expander_rules_folder(&mut self) {
        ensure_text_expander_rules_file(&self.app_settings.text_expansion_rules);
        let path = text_expander_rules_dir();
        let result = open_path_in_system_editor(&path);
        self.status_msg = if result {
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "text_expander.rules_file_opened_status",
            )
            .to_owned()
        } else {
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "text_expander.rules_file_open_failed_status",
            )
            .to_owned()
        };
    }

    pub(super) fn remove_text_expander_rules_file(&mut self, remove_idx: usize) {
        if remove_idx >= self.app_settings.text_expander_rule_files.len() {
            return;
        }
        self.app_settings
            .text_expander_rule_files
            .remove(remove_idx);
        self.text_expander_rules_signature =
            text_expander_rules_signature(&self.app_settings.text_expander_rule_files);
        save_app_settings(&self.app_settings);
        self.sync_text_expander_runtime();
    }
}
