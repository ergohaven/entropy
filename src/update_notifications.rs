//! Platform-independent update notification policy: opt-in, eligibility, deduplication,
//! delivery history, and click routing. Release checks remain owned by app_update; no timer here.
//!
//! Each platform backend implements initialize/available/request_permission/authorize/submit/drain.
//! Authorization and delivery report events asynchronously; the policy rechecks eligibility before
//! submission and records a release only after delivery succeeds. An abrupt exit before
//! the next event poll can still repeat an accepted alert on the following launch. To support another OS, add a
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
    Failed {
        key: String,
        version: String,
        error: String,
    },
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
    in_flight: BTreeSet<(String, String)>,
    pending_click: Option<PendingClick>,
}

impl UpdateNotifications {
    pub(super) fn initialize(&mut self, ctx: &egui::Context, history_path: PathBuf) {
        self.backend.initialize(ctx);
        if !self.backend.available() {
            return;
        }
        self.load_history(history_path);
    }

    fn load_history(&mut self, history_path: PathBuf) {
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
            && !self
                .in_flight
                .contains(&(key.to_owned(), version.to_owned()))
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

    fn submit(&mut self, notification: Notification) -> bool {
        if self.delivered.get(&notification.key) == Some(&notification.version)
            || !self
                .in_flight
                .insert((notification.key.clone(), notification.version.clone()))
        {
            return false;
        }
        self.backend.submit(notification);
        true
    }

    fn drain(&mut self) -> Vec<Event> {
        let events = self.backend.drain();
        self.record_events(&events);
        events
    }

    fn record_events(&mut self, events: &[Event]) {
        let mut changed = false;
        for event in events {
            if let Event::Delivered(key, version) = event {
                self.in_flight.remove(&(key.clone(), version.clone()));
                self.delivered.insert(key.clone(), version.clone());
                changed = true;
            } else if let Event::Failed { key, version, .. } = event {
                self.in_flight.remove(&(key.clone(), version.clone()));
            }
        }
        if changed {
            if let Some(path) = &self.history_path {
                let result = (|| -> std::io::Result<()> {
                    let data =
                        serde_json::to_vec(&self.delivered).map_err(std::io::Error::other)?;
                    let parent = path
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                        .unwrap_or_else(|| std::path::Path::new("."));
                    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
                    std::io::Write::write_all(&mut temporary, &data)?;
                    temporary.as_file().sync_all()?;
                    temporary.persist(path).map_err(|error| error.error)?;
                    #[cfg(unix)]
                    std::fs::File::open(parent)?.sync_all()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    log::warn!("Notification history save failed: {error}");
                }
            }
        }
    }
}

fn firmware_key(target: &FirmwareReleaseTarget) -> String {
    let family = match target.family {
        FirmwareReleaseFamily::Rmk => "rmk",
        FirmwareReleaseFamily::Qmk => "qmk",
    };
    format!("{family}:{}", target.asset_prefix)
}

#[derive(Debug, PartialEq, Eq)]
enum ClickRoute {
    AboutEntropy,
    AboutDevice,
    Release(String),
    Pending,
}

struct PendingClick {
    key: String,
    version: String,
    queued_at: std::time::Instant,
    main_menu_tab: MainMenuTab,
    settings_tab: SettingsTab,
}

fn notification_release_url(key: &str, version: &str) -> Option<String> {
    let repository = key.split_once(':').and_then(|(family, _)| match family {
        "qmk" => Some("vial-qmk"),
        "rmk" => Some("rmk"),
        _ => None,
    })?;
    let mut url = url::Url::parse(&format!(
        "https://github.com/ergohaven/{repository}/releases/tag/"
    ))
    .ok()?;
    url.path_segments_mut().ok()?.pop_if_empty().push(version);
    Some(url.into())
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

    fn notification_click_route(&self, key: &str, version: &str) -> Option<ClickRoute> {
        if key == "app" {
            return Some(ClickRoute::AboutEntropy);
        }
        if self
            .device_about_info
            .as_ref()
            .and_then(|info| info.firmware_update_target.as_ref())
            .is_some_and(|target| firmware_key(target) == key)
        {
            return Some(ClickRoute::AboutDevice);
        }
        let release_url = notification_release_url(key, version)?;
        // A cold-launch response precedes the initial asynchronous device probe. Keep it
        // pending until discovery/identification settles, while making the app visible now.
        if self.scan_frame == 0
            || matches!(
                self.device_scan_state,
                DeviceScanState::Scanning {
                    timeout_logged: false,
                    ..
                }
            )
            || matches!(self.connect_state, ConnectState::Loading { .. })
            || self.pending_device_connect.is_some()
        {
            return Some(ClickRoute::Pending);
        }
        Some(ClickRoute::Release(release_url))
    }

    fn pending_notification_click_route(&self, click: &PendingClick) -> Option<ClickRoute> {
        if self.main_menu_tab != click.main_menu_tab || self.settings_tab != click.settings_tab {
            return None;
        }
        let route = self.notification_click_route(&click.key, &click.version)?;
        if route == ClickRoute::Pending
            && click.queued_at.elapsed() >= std::time::Duration::from_secs(15)
        {
            return notification_release_url(&click.key, &click.version).map(ClickRoute::Release);
        }
        Some(route)
    }

    pub(super) fn authorize_notification_setting(&mut self, firmware: bool, before: bool) {
        let enabled = if firmware {
            self.app_settings.firmware_update_notifications
        } else {
            self.app_settings.app_update_notifications
        };
        if enabled != before {
            // An explicit settings change permits a new attempt after denial or failure,
            // without retrying every frame or resetting the other notification category.
            self.update_notifications.attempted.retain(|(key, _)| {
                if firmware {
                    key == "app"
                } else {
                    key != "app"
                }
            });
        }
        if enabled && !self.update_notifications.request_permission() {
            if firmware {
                self.app_settings.firmware_update_notifications = before;
            } else {
                self.app_settings.app_update_notifications = before;
            }
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "notifications.unbundled")
                    .into();
        }
    }

    fn resolve_notification_click(&mut self, key: String, version: String) {
        // The latest click supersedes an older click that is still waiting for discovery.
        self.update_notifications.pending_click = None;
        if let Some(route) = self.notification_click_route(&key, &version) {
            self.apply_notification_click_route(route, key, version);
        }
    }

    fn apply_notification_click_route(&mut self, route: ClickRoute, key: String, version: String) {
        match route {
            ClickRoute::AboutEntropy => {
                self.main_menu_tab = MainMenuTab::Settings;
                self.settings_tab = SettingsTab::AboutEntropy;
            }
            ClickRoute::AboutDevice => {
                self.main_menu_tab = MainMenuTab::Settings;
                self.settings_tab = SettingsTab::AboutDevice;
            }
            ClickRoute::Release(url) => {
                open_url_in_browser(&url);
            }
            ClickRoute::Pending => {
                self.update_notifications.pending_click = Some(PendingClick {
                    key,
                    version,
                    queued_at: std::time::Instant::now(),
                    main_menu_tab: self.main_menu_tab,
                    settings_tab: self.settings_tab,
                });
            }
        }
    }

    fn handle_notification_event(&mut self, ctx: &egui::Context, event: Event) {
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
                self.restore_from_tray(ctx);
                self.resolve_notification_click(key, version);
            }
            Event::PermissionDenied => {
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "notifications.permission_denied",
                )
                .into()
            }
            Event::Failed { error, .. } => log::warn!("Native update notification failed: {error}"),
            Event::Delivered(..) => {}
        }
    }

    pub(super) fn poll_update_notifications(&mut self, ctx: &egui::Context) {
        if self.headless {
            return;
        }
        let lang = self.app_settings.language;
        for event in self.update_notifications.drain() {
            self.handle_notification_event(ctx, event);
        }
        if let Some(click) = self.update_notifications.pending_click.take() {
            match self.pending_notification_click_route(&click) {
                Some(ClickRoute::Pending) => self.update_notifications.pending_click = Some(click),
                Some(route) => self.apply_notification_click_route(route, click.key, click.version),
                None => {}
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
    fn cold_launch_firmware_click_waits_for_initial_device_discovery() {
        let mut app = EntropyApp::new_inert_for_test();
        assert_eq!(
            app.notification_click_route("qmk:phenom_v1", "4.1.0"),
            Some(ClickRoute::Pending)
        );
        app.scan_frame = 1;
        assert!(matches!(
            app.notification_click_route("qmk:phenom_v1", "4.1.0"),
            Some(ClickRoute::Release(_))
        ));
    }

    #[test]
    fn toggling_off_resets_only_that_category_for_an_explicit_retry() {
        let mut app = EntropyApp::new_inert_for_test();
        assert!(app.update_notifications.claim(true, "app", "1.0.0"));
        assert!(app
            .update_notifications
            .claim(true, "qmk:phenom_v1", "4.1.0"));
        app.authorize_notification_setting(false, true);
        assert!(app.update_notifications.claim(true, "app", "1.0.0"));
        assert!(!app
            .update_notifications
            .claim(true, "qmk:phenom_v1", "4.1.0"));
        app.authorize_notification_setting(true, true);
        assert!(app
            .update_notifications
            .claim(true, "qmk:phenom_v1", "4.1.0"));
    }

    #[test]
    fn unavailable_backend_does_not_enable_a_notification_setting() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.app_update_notifications = true;
        app.authorize_notification_setting(false, false);
        assert!(!app.app_settings.app_update_notifications);
    }

    #[test]
    fn delivery_history_replaces_the_file_atomically() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let original = br#"{"app":"1.0.0"}"#;
        std::fs::write(&path, original).unwrap();
        let mut previous_file = std::fs::File::open(&path).unwrap();
        let mut notifications = UpdateNotifications::default();
        notifications.load_history(path.clone());
        notifications.record_events(&[Event::Delivered("app".into(), "2.0.0".into())]);
        let mut previous_contents = Vec::new();
        previous_file.read_to_end(&mut previous_contents).unwrap();
        assert_eq!(
            previous_contents, original,
            "an open reader must retain the complete previous history"
        );
        let restored: BTreeMap<String, String> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(restored.get("app").map(String::as_str), Some("2.0.0"));
    }

    #[test]
    fn disconnected_click_encodes_the_release_tag_as_one_url_segment() {
        let mut app = EntropyApp::new_inert_for_test();
        app.scan_frame = 1;
        let Some(ClickRoute::Release(release)) =
            app.notification_click_route("qmk:phenom_v1", "4.1.0#release")
        else {
            panic!("expected release route")
        };
        let url = url::Url::parse(&release).unwrap();
        assert!(url.fragment().is_none());
        assert!(url.path().ends_with("/4.1.0%23release"));
    }

    #[test]
    fn clearing_attempts_during_submission_cannot_authorize_a_duplicate() {
        let mut notifications = UpdateNotifications::default();
        let notification = Notification {
            key: "app".into(),
            version: "1.0.0".into(),
            title: String::new(),
            body: String::new(),
        };
        assert!(notifications.claim(true, "app", "1.0.0"));
        notifications.submit(notification);
        notifications.attempted.clear();
        assert!(!notifications.claim(true, "app", "1.0.0"));
    }

    #[test]
    fn repeated_authorization_completions_submit_only_once_and_failure_allows_explicit_retry() {
        let mut notifications = UpdateNotifications::default();
        let notification = Notification {
            key: "app".into(),
            version: "1.0.0".into(),
            title: String::new(),
            body: String::new(),
        };
        assert!(notifications.submit(notification.clone()));
        assert!(!notifications.submit(notification.clone()));
        notifications.record_events(&[Event::Failed {
            key: "app".into(),
            version: "1.0.0".into(),
            error: "transient failure".into(),
        }]);
        assert!(notifications.submit(notification.clone()));
        notifications.record_events(&[Event::Delivered("app".into(), "1.0.0".into())]);
        assert!(!notifications.submit(notification));
        assert!(notifications.in_flight.is_empty());
    }

    #[test]
    fn pending_click_respects_user_navigation_and_expires_without_restarting_its_wait() {
        let mut app = EntropyApp::new_inert_for_test();
        app.resolve_notification_click("qmk:phenom_v1".into(), "4.1.0".into());
        let mut click = app.update_notifications.pending_click.take().unwrap();
        assert_eq!(
            app.pending_notification_click_route(&click),
            Some(ClickRoute::Pending)
        );
        click.queued_at = std::time::Instant::now() - std::time::Duration::from_secs(16);
        assert!(matches!(
            app.pending_notification_click_route(&click),
            Some(ClickRoute::Release(_))
        ));
        app.main_menu_tab = MainMenuTab::Settings;
        assert!(app.pending_notification_click_route(&click).is_none());
    }

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
    fn failed_delivery_and_denied_permission_do_not_persist_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut notifications = UpdateNotifications::default();
        notifications.load_history(path.clone());
        assert!(notifications.claim(true, "app", "1.0.0"));
        notifications.record_events(&[
            Event::Failed {
                key: "app".into(),
                version: "1.0.0".into(),
                error: "test failure".into(),
            },
            Event::PermissionDenied,
        ]);
        assert!(notifications.delivered.is_empty());
        assert!(!path.exists());
        notifications.record_events(&[Event::Delivered("app".into(), "1.0.0".into())]);
        let mut relaunched = UpdateNotifications::default();
        relaunched.load_history(path);
        assert!(!relaunched.claim(true, "app", "1.0.0"));
        assert!(relaunched.claim(true, "app", "1.0.1"));
    }

    #[test]
    fn corrupt_history_recovers_without_marking_any_release_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, b"truncated history").unwrap();
        let mut notifications = UpdateNotifications::default();
        notifications.load_history(path.clone());
        assert!(notifications.claim(true, "app", "1.0.0"));
        notifications.record_events(&[Event::Delivered("app".into(), "1.0.0".into())]);
        let mut relaunched = UpdateNotifications::default();
        relaunched.load_history(path);
        assert!(!relaunched.claim(true, "app", "1.0.0"));
    }

    #[test]
    fn stale_authorization_releases_the_attempt_without_submitting() {
        let mut app = EntropyApp::new_inert_for_test();
        assert!(app.update_notifications.claim(true, "app", "1.0.0"));
        app.handle_notification_event(
            &egui::Context::default(),
            Event::Authorized(Notification {
                key: "app".into(),
                version: "1.0.0".into(),
                title: String::new(),
                body: String::new(),
            }),
        );
        assert!(app.update_notifications.claim(true, "app", "1.0.0"));
        assert!(app.update_notifications.delivered.is_empty());
    }

    #[test]
    fn a_new_click_supersedes_an_older_pending_firmware_click() {
        let mut app = EntropyApp::new_inert_for_test();
        app.resolve_notification_click("qmk:phenom_v1".into(), "4.1.0".into());
        assert!(app.update_notifications.pending_click.is_some());
        app.resolve_notification_click("app".into(), "1.0.0".into());
        assert!(app.update_notifications.pending_click.is_none());
        assert!(matches!(app.settings_tab, SettingsTab::AboutEntropy));
    }

    #[test]
    fn click_routes_to_about_tabs_and_defers_until_firmware_identification() {
        let mut app = EntropyApp::new_inert_for_test();
        assert_eq!(
            app.notification_click_route("app", "1.0.0"),
            Some(ClickRoute::AboutEntropy)
        );
        app.resolve_notification_click("qmk:phenom_v1".into(), "4.1.0".into());
        assert!(app.update_notifications.pending_click.is_some());
        app.scan_frame = 1;
        app.device_about_info = Some(DeviceAboutInfo {
            firmware_update_target: Some(FirmwareReleaseTarget {
                family: FirmwareReleaseFamily::Qmk,
                current_version: "4.0.6".into(),
                asset_prefix: "phenom_v1".into(),
            }),
            ..Default::default()
        });
        let click = app.update_notifications.pending_click.take().unwrap();
        app.resolve_notification_click(click.key, click.version);
        assert!(matches!(app.main_menu_tab, MainMenuTab::Settings));
        assert!(matches!(app.settings_tab, SettingsTab::AboutDevice));
        assert!(app.update_notifications.pending_click.is_none());
        app.resolve_notification_click("app".into(), "1.0.0".into());
        assert!(matches!(app.settings_tab, SettingsTab::AboutEntropy));
        assert!(app
            .notification_click_route("unknown:board", "1.0.0")
            .is_none());
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
