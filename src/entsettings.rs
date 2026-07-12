use super::*;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ENTSETTINGS_FORMAT: &str = "entropy.app-settings";
const ENTSETTINGS_VERSION: u16 = 1;

#[derive(Clone, Serialize, Deserialize)]
struct EntSettingsFile {
    format: String,
    version: u16,
    created_by: String,
    created_at: String,
    settings: AppSettings,
    #[serde(default)]
    text_expander_extra_files: Vec<EntSettingsTextExpanderRuleFile>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntSettingsTextExpanderRuleFile {
    file_name: String,
    rules: Vec<crate::text_expander::TextExpansionRule>,
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn export_entsettings_dialog(&mut self) {
        let bundle = self.entsettings_snapshot();
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Entropy app settings", &["entsettings"])
            .set_file_name("entropy-app-settings.entsettings")
            .save_file()
        else {
            return;
        };
        match write_entsettings_file(&path, &bundle, self.app_settings.language) {
            Ok(()) => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entsettings.exported_app_settings",
                    &[("path", &path.display().to_string())],
                )
            }
            Err(e) => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entsettings.export_app_settings_failed",
                    &[("error", &e.to_string())],
                )
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_entsettings_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Entropy app settings", &["entsettings"])
            .pick_file()
        else {
            return;
        };
        self.pending_entsettings_import_path = Some(path);
        self.import_progress_started_at = None;
        self.import_progress_title = crate::i18n::tr_catalog(
            self.app_settings.language,
            "entsettings.importing_app_settings",
        )
        .into();
        self.import_progress_body = crate::i18n::tr_catalog(
            self.app_settings.language,
            "entsettings.applying_app_settings",
        )
        .into();
        ctx.request_repaint();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_entsettings_from_path(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
    ) -> Result<String> {
        let lang = self.app_settings.language;
        let data = std::fs::read_to_string(path).with_context(|| {
            crate::i18n::tr_catalog_format(
                lang,
                "entsettings.failed_to_read",
                &[("path", &path.display().to_string())],
            )
        })?;
        let bundle: EntSettingsFile = serde_json::from_str(&data).with_context(|| {
            crate::i18n::tr_catalog_format(
                lang,
                "entsettings.failed_to_parse",
                &[("path", &path.display().to_string())],
            )
        })?;
        validate_entsettings_file(&bundle, lang)?;
        let backup_path = write_entsettings_auto_backup(&self.entsettings_snapshot(), lang)?;
        self.apply_entsettings(ctx, bundle)?;
        Ok(crate::i18n::tr_catalog_format(
            lang,
            "entsettings.import_complete_report",
            &[
                ("path", &path.display().to_string()),
                ("backup", &backup_path.display().to_string()),
            ],
        ))
    }

    fn entsettings_snapshot(&self) -> EntSettingsFile {
        let mut settings = self.app_settings.clone();
        settings.ui_scale = clamp_ui_scale(settings.ui_scale);
        settings.text_expander_rule_files =
            normalize_text_expander_rule_files(&settings.text_expander_rule_files);

        let text_expander_extra_files = settings
            .text_expander_rule_files
            .iter()
            .filter_map(|file_name| {
                load_text_expansion_rules_from_path(&text_expander_extra_rules_path(file_name)).map(
                    |rules| EntSettingsTextExpanderRuleFile {
                        file_name: file_name.clone(),
                        rules,
                    },
                )
            })
            .collect();

        EntSettingsFile {
            format: ENTSETTINGS_FORMAT.to_owned(),
            version: ENTSETTINGS_VERSION,
            created_by: "Entropy".to_owned(),
            created_at: chrono::Local::now().to_rfc3339(),
            settings,
            text_expander_extra_files,
        }
    }

    fn apply_entsettings(&mut self, ctx: &egui::Context, bundle: EntSettingsFile) -> Result<()> {
        let mut settings = bundle.settings;
        settings.ui_scale = clamp_ui_scale(settings.ui_scale);
        settings.text_expander_rule_files =
            normalize_text_expander_rule_files(&settings.text_expander_rule_files);

        self.app_settings = settings;
        save_app_settings(&self.app_settings);
        save_text_expansion_rules(&self.app_settings.text_expansion_rules);

        for file in &bundle.text_expander_extra_files {
            if let Some(file_name) = normalize_text_expander_rules_file_name(&file.file_name) {
                let path = text_expander_extra_rules_path(&file_name);
                let json =
                    serde_json::to_string_pretty(&file.rules).context(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "entsettings.failed_to_serialize_rules",
                    ))?;
                std::fs::write(&path, json).with_context(|| {
                    crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "entsettings.failed_to_write",
                        &[("path", &path.display().to_string())],
                    )
                })?;
            }
        }

        crate::ui_style::set_accent(self.app_settings.accent_color.color());
        ctx.set_zoom_factor(self.app_settings.ui_scale);
        self.sticky_layout_last_size = None;
        if !self.app_settings.layer_hover_preview {
            self.hover_layer = None;
        }
        #[cfg(target_os = "windows")]
        if !self.app_settings.minimize_to_tray_on_close {
            self.tray_icon = None;
        }
        self.text_expander_rules_signature =
            text_expander_rules_signature(&self.app_settings.text_expander_rule_files);
        self.sync_text_expander_runtime();
        ctx.request_repaint();
        Ok(())
    }
}

fn validate_entsettings_file(
    bundle: &EntSettingsFile,
    language: crate::i18n::Language,
) -> Result<()> {
    if bundle.format != ENTSETTINGS_FORMAT {
        bail!(
            "{}",
            crate::i18n::tr_catalog_format(
                language,
                "entsettings.unsupported_format",
                &[("format", &bundle.format)]
            )
        );
    }
    if bundle.version == 0 || bundle.version > ENTSETTINGS_VERSION {
        bail!(
            "{}",
            crate::i18n::tr_catalog_format(
                language,
                "entsettings.unsupported_version",
                &[("version", &bundle.version.to_string())]
            )
        );
    }
    Ok(())
}

fn write_entsettings_auto_backup(
    bundle: &EntSettingsFile,
    language: crate::i18n::Language,
) -> Result<PathBuf> {
    let base_dir = app_settings_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base_dir.join("backups");
    std::fs::create_dir_all(&dir).with_context(|| {
        crate::i18n::tr_catalog_format(
            language,
            "entsettings.failed_to_create_backup_dir",
            &[("path", &dir.display().to_string())],
        )
    })?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("auto-backup-app-settings-{stamp}.entsettings"));
    write_entsettings_file(&path, bundle, language)?;
    Ok(path)
}

fn write_entsettings_file(
    path: &Path,
    bundle: &EntSettingsFile,
    language: crate::i18n::Language,
) -> Result<()> {
    let json = serde_json::to_string_pretty(bundle).context(crate::i18n::tr_catalog(
        language,
        "entsettings.failed_to_serialize",
    ))?;
    std::fs::write(path, json).with_context(|| {
        crate::i18n::tr_catalog_format(
            language,
            "entsettings.failed_to_write",
            &[("path", &path.display().to_string())],
        )
    })
}
