//! Platform-independent update notification policy: opt-in, eligibility, deduplication,
//! delivery history, and click routing. Release checks remain owned by app_update; no timer here.
//!
//! Each platform backend implements initialize/available/request_permission/authorize/submit/drain.
//! Authorization and delivery report events asynchronously; the policy rechecks eligibility before
//! submission and records a release only after delivery succeeds. To support another OS, add a
//! backend module and select it below. Settings rows follow the backend's SUPPORTED capability.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
#[path = "update_notifications/macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "update_notifications/unsupported.rs"]
mod platform;
// Compile and test the disabled backend on macOS as well.
#[cfg(all(test, target_os = "macos"))]
#[path = "update_notifications/unsupported.rs"]
mod unsupported;

pub(super) const SUPPORTED: bool = platform::SUPPORTED;
#[cfg(target_os = "macos")]
pub(crate) use platform::prepare as prepare_macos_update_notifications;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
enum Event {
    Authorized(Notification),
    Delivered(String, String),
    Clicked(String, String),
    PermissionDenied,
    Failed(String),
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone)]
struct Notification {
    key: String,
    version: String,
    title: String,
    body: String,
}

#[derive(Default)]
pub(super) struct UpdateNotifications {
    backend: platform::Backend,
    history_path: Option<PathBuf>,
    delivered: BTreeMap<String, String>,
    attempted: BTreeSet<(String, String)>,
}

impl UpdateNotifications {
    pub(super) fn initialize(&mut self, ctx: &egui::Context, history_path: PathBuf) {
        self.backend.initialize(ctx);
        if !self.backend.available() {
            return;
        }
        self.delivered = std::fs::read(&history_path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        self.history_path = Some(history_path);
    }

    pub(super) fn request_permission(&self) -> bool {
        self.backend.request_permission()
    }

    fn claim(&mut self, enabled: bool, key: &str, version: &str) -> bool {
        enabled
            && self.delivered.get(key).map(String::as_str) != Some(version)
            && self.attempted.insert((key.to_owned(), version.to_owned()))
    }

    fn send(
        &mut self,
        enabled: bool,
        key: &str,
        version: &str,
        text: impl FnOnce() -> (String, String),
    ) {
        if !self.backend.available() || !self.claim(enabled, key, version) {
            return;
        }
        let (title, body) = text();
        self.backend.authorize(Notification {
            key: key.into(),
            version: version.into(),
            title,
            body,
        });
    }

    fn submit(&self, notification: Notification) {
        self.backend.submit(notification);
    }

    fn drain(&mut self) -> Vec<Event> {
        let events = self.backend.drain();
        let mut changed = false;
        for event in &events {
            if let Event::Delivered(key, version) = event {
                self.delivered.insert(key.clone(), version.clone());
                changed = true;
            }
        }
        if changed {
            if let Some(path) = &self.history_path {
                let result = serde_json::to_vec(&self.delivered)
                    .map_err(std::io::Error::other)
                    .and_then(|data| std::fs::write(path, data));
                if let Err(error) = result {
                    log::warn!("Notification history save failed: {error}");
                }
            }
        }
        events
    }
}

fn firmware_key(target: &FirmwareReleaseTarget) -> String {
    let family = match target.family {
        FirmwareReleaseFamily::Rmk => "rmk",
        FirmwareReleaseFamily::Qmk => "qmk",
    };
    format!("{family}:{}", target.asset_prefix)
}

impl EntropyApp {
    fn notification_is_current(&self, notification: &Notification) -> bool {
        if notification.key == "app" {
            return self.app_settings.app_update_notifications
                && matches!(&self.update_check, UpdateCheckState::Ready(result)
                    if result.relation == VersionRelation::UpdateAvailable
                    && result.latest_version == notification.version);
        }
        self.app_settings.firmware_update_notifications
            && matches!((&self.device_about_info, &self.firmware_update_check),
                (Some(info), FirmwareUpdateCheckState::Ready { target, result })
                if info.firmware_update_target.as_ref() == Some(target)
                && firmware_key(target) == notification.key
                && result.relation == VersionRelation::UpdateAvailable
                && result.latest_version == notification.version)
    }

    pub(super) fn poll_update_notifications(&mut self, ctx: &egui::Context) {
        if self.headless {
            return;
        }
        let lang = self.app_settings.language;
        for event in self.update_notifications.drain() {
            match event {
                Event::Authorized(notification) => {
                    // Permission is asynchronous: recheck opt-in and the selected device before submitting.
                    if self.notification_is_current(&notification) {
                        self.update_notifications.submit(notification);
                    } else {
                        self.update_notifications
                            .attempted
                            .remove(&(notification.key, notification.version));
                    }
                }
                Event::Clicked(key, version) => {
                    if key != "app"
                        && !self
                            .device_about_info
                            .as_ref()
                            .and_then(|info| info.firmware_update_target.as_ref())
                            .is_some_and(|target| firmware_key(target) == key)
                    {
                        // The notification can outlive the connection it described.
                        if let Some(repository) =
                            key.split_once(':').and_then(|(family, _)| match family {
                                "qmk" => Some("vial-qmk"),
                                "rmk" => Some("rmk"),
                                _ => None,
                            })
                        {
                            open_url_in_browser(&format!(
                                "https://github.com/ergohaven/{repository}/releases/tag/{version}"
                            ));
                        }
                        continue;
                    }
                    self.main_menu_tab = MainMenuTab::Settings;
                    self.settings_tab = if key == "app" {
                        SettingsTab::AboutEntropy
                    } else {
                        SettingsTab::AboutDevice
                    };
                    self.restore_from_tray(ctx);
                }
                Event::PermissionDenied => {
                    self.status_msg =
                        crate::i18n::tr_catalog(lang, "notifications.permission_denied").into()
                }
                Event::Failed(error) => log::warn!("Native update notification failed: {error}"),
                Event::Delivered(..) => {}
            }
        }
        if let UpdateCheckState::Ready(result) = &self.update_check {
            if result.relation == VersionRelation::UpdateAvailable {
                self.update_notifications.send(
                    self.app_settings.app_update_notifications,
                    "app",
                    &result.latest_version,
                    || {
                        (
                            crate::i18n::tr_catalog(lang, "notifications.app_title").into(),
                            crate::i18n::tr_catalog_format(
                                lang,
                                "notifications.app_body",
                                &[("version", &result.latest_version)],
                            ),
                        )
                    },
                );
            }
        }
        if let (Some(info), FirmwareUpdateCheckState::Ready { target, result }) =
            (&self.device_about_info, &self.firmware_update_check)
        {
            if info.firmware_update_target.as_ref() == Some(target)
                && result.relation == VersionRelation::UpdateAvailable
            {
                self.update_notifications.send(
                    self.app_settings.firmware_update_notifications,
                    &firmware_key(target),
                    &result.latest_version,
                    || {
                        (
                            crate::i18n::tr_catalog(lang, "notifications.firmware_title").into(),
                            crate::i18n::tr_catalog_format(
                                lang,
                                "notifications.firmware_body",
                                &[
                                    ("version", &result.latest_version),
                                    ("device", &info.product),
                                ],
                            ),
                        )
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_are_opt_in_and_deduplicated_by_target_and_release() {
        let mut notifications = UpdateNotifications::default();
        assert!(!notifications.claim(false, "app", "1.0.0"));
        assert!(notifications.claim(true, "app", "1.0.0"));
        assert!(!notifications.claim(true, "app", "1.0.0"));
        assert!(notifications.claim(true, "qmk:phenom_v1", "1.0.0"));
        notifications.delivered.insert("app".into(), "1.0.0".into());
        notifications.attempted.clear();
        assert!(!notifications.claim(true, "app", "1.0.0"));
        assert!(notifications.claim(true, "app", "1.0.1"));
    }

    #[test]
    fn failed_delivery_is_not_persisted_as_delivered() {
        let mut notifications = UpdateNotifications::default();
        assert!(notifications.claim(true, "app", "1.0.0"));
        assert!(notifications.delivered.is_empty());
    }

    #[test]
    fn notification_preferences_are_independent_and_old_settings_remain_opt_in() {
        let settings = AppSettings {
            app_update_notifications: true,
            firmware_update_notifications: false,
            ..Default::default()
        };
        let mut json = serde_json::to_value(&settings).unwrap();
        let restored: AppSettings = serde_json::from_value(json.clone()).unwrap();
        assert!(restored.app_update_notifications);
        assert!(!restored.firmware_update_notifications);
        json.as_object_mut()
            .unwrap()
            .remove("app_update_notifications");
        json.as_object_mut()
            .unwrap()
            .remove("firmware_update_notifications");
        let legacy: AppSettings = serde_json::from_value(json).unwrap();
        assert!(!legacy.app_update_notifications);
        assert!(!legacy.firmware_update_notifications);
    }

    #[test]
    fn firmware_authorization_rechecks_preferences_connection_and_latest_release() {
        let mut app = EntropyApp::new_inert_for_test();
        let target = FirmwareReleaseTarget {
            family: FirmwareReleaseFamily::Qmk,
            current_version: "4.0.6".into(),
            asset_prefix: "phenom_v1".into(),
        };
        let notification = Notification {
            key: firmware_key(&target),
            version: "4.1.0".into(),
            title: String::new(),
            body: String::new(),
        };
        app.device_about_info = Some(DeviceAboutInfo {
            firmware_update_target: Some(target.clone()),
            ..Default::default()
        });
        app.firmware_update_check = FirmwareUpdateCheckState::Ready {
            target,
            result: FirmwareUpdateCheckResult {
                latest_version: "4.1.0".into(),
                release_url: "https://github.com/ergohaven/vial-qmk/releases/tag/4.1.0".into(),
                asset: UpdateAsset {
                    name: "4.1.0_phenom_v1.uf2".into(),
                    url: String::new(),
                },
                relation: VersionRelation::UpdateAvailable,
            },
        };
        app.app_settings.app_update_notifications = true;
        assert!(!app.notification_is_current(&notification));
        app.app_settings.firmware_update_notifications = true;
        app.app_settings.app_update_notifications = false;
        assert!(app.notification_is_current(&notification));
        app.device_about_info = None;
        assert!(!app.notification_is_current(&notification));
        app.device_about_info = Some(DeviceAboutInfo {
            firmware_update_target: firmware_update_target(&app.firmware_update_check).cloned(),
            ..Default::default()
        });
        if let FirmwareUpdateCheckState::Ready { result, .. } = &mut app.firmware_update_check {
            result.relation = VersionRelation::UpToDate;
        }
        assert!(!app.notification_is_current(&notification));
    }
}
