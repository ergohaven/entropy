//! The tray icon on Linux: a StatusNotifierItem, which Plasma, the GNOME
//! AppIndicator extension, swaybar and waybar show.
//!
//! Hidden to the tray, the main window has no window at all: eframe drops it,
//! keeps the app running and creates it again on the way back, with the XDG
//! activation token the panel handed over with the click (see eframe's
//! `linux_root`). Without a panel to show the icon, the window only minimizes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::i18n::Language;

/// What a window does about the tray on a frame.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TraySync {
    Stay,
    /// The panel went away with the icon: the window must not stay out of reach.
    Restore,
    /// A panel came up while the window waits minimized for one, as at login.
    Hide,
}

pub(crate) fn tray_sync(hidden: bool, online: bool, in_background: bool) -> TraySync {
    match (hidden, online) {
        (true, false) => TraySync::Restore,
        (false, true) if in_background => TraySync::Hide,
        _ => TraySync::Stay,
    }
}

/// How the window leaves the screen: hidden to the tray where a panel shows
/// the icon to bring it back, minimized where none does.
pub(crate) fn background_command(tray_online: bool) -> egui::ViewportCommand {
    if tray_online {
        egui::ViewportCommand::Visible(false)
    } else {
        egui::ViewportCommand::Minimized(true)
    }
}

/// The tray icon. Dropping it takes the icon off the panel.
pub(crate) struct LinuxTray {
    handle: ksni::blocking::Handle<EntropyTray>,
    online: Arc<AtomicBool>,
    language: Language,
}

impl LinuxTray {
    /// Shows the icon, or `None` without a D-Bus session. With no panel up
    /// that shows StatusNotifierItems, as before Plasma starts at login, or on
    /// GNOME without its AppIndicator extension, the icon waits offline.
    pub(crate) fn spawn(ctx: &egui::Context, language: Language) -> Option<Self> {
        let online = Arc::new(AtomicBool::new(true));
        let tray = EntropyTray {
            ctx: ctx.clone(),
            language,
            online: online.clone(),
            restore: &crate::app::TRAY_RESTORE_REQUESTED,
            quit: &crate::app::TRAY_QUIT_REQUESTED,
        };
        match spawn_tray(tray) {
            Ok(handle) => Some(Self {
                handle,
                online,
                language,
            }),
            Err(error) => {
                log::info!("Tray: cannot offer a tray icon ({error}); minimizing instead");
                None
            }
        }
    }

    /// Whether a panel shows the icon. It goes away when the panel does, as
    /// when plasmashell restarts, and comes back with it.
    pub(crate) fn online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    /// Relabels the menu in `language`.
    pub(crate) fn set_language(&mut self, language: Language) {
        if language != self.language {
            self.language = language;
            self.handle.update(|tray| tray.language = language);
        }
    }
}

impl Drop for LinuxTray {
    fn drop(&mut self) {
        // Nothing waits for the service to stop: exit must not hang on D-Bus.
        let _ = self.handle.shutdown();
    }
}

// Tests run beside a live desktop: they must not put an icon on its panel.
#[cfg(not(test))]
fn spawn_tray(tray: EntropyTray) -> Result<ksni::blocking::Handle<EntropyTray>, ksni::Error> {
    use ksni::blocking::TrayMethods as _;

    // Entropy may start before the panel does: the icon then waits for it.
    tray.assume_sni_available(true).spawn()
}

#[cfg(test)]
fn spawn_tray(_tray: EntropyTray) -> Result<ksni::blocking::Handle<EntropyTray>, ksni::Error> {
    Err(ksni::Error::WontShow)
}

/// Whether a panel in this session shows StatusNotifierItems, before there is
/// an icon to tell: such a panel runs the watcher the icon registers with.
#[cfg(not(test))]
pub(crate) fn host_running() -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(&connection) else {
        return false;
    };
    let Ok(watcher) = zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher") else {
        return false;
    };
    dbus.name_has_owner(watcher).unwrap_or(false)
}

#[cfg(test)]
pub(crate) fn host_running() -> bool {
    false
}

struct EntropyTray {
    ctx: egui::Context,
    language: Language,
    online: Arc<AtomicBool>,
    restore: &'static AtomicBool,
    quit: &'static AtomicBool,
}

impl EntropyTray {
    fn request(&self, flag: &AtomicBool) {
        flag.store(true, Ordering::Relaxed);
        self.ctx.request_repaint();
    }

    fn restore(&self) {
        // The window comes back as a new one: the token lets the compositor
        // give it the focus the click asked for.
        if let Some(token) = activation_token() {
            eframe::set_root_activation_token(token);
        }
        self.request(self.restore);
    }
}

/// Plasma may hand the token over just after the click it belongs to.
fn activation_token() -> Option<String> {
    (0..5).find_map(|attempt| {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        ksni::current_xdg_activation_token()
    })
}

impl ksni::Tray for EntropyTray {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }

    fn title(&self) -> String {
        crate::APP_TITLE.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [22, 32, 48]
            .into_iter()
            .map(|size| ksni::Icon {
                width: size as i32,
                height: size as i32,
                data: argb_from_rgba(&crate::app_icon::rgba_icon(size)),
            })
            .collect()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: crate::APP_TITLE.into(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.restore();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: crate::i18n::tr_catalog(self.language, "tray.open").into(),
                activate: Box::new(|tray: &mut Self| tray.restore()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: crate::i18n::tr_catalog(self.language, "tray.quit").into(),
                activate: Box::new(|tray: &mut Self| tray.request(tray.quit)),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_online(&self) {
        self.online.store(true, Ordering::Relaxed);
        self.ctx.request_repaint();
    }

    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        self.online.store(false, Ordering::Relaxed);
        self.ctx.request_repaint();
        // Keep the service: the panel may come back and show the icon again.
        true
    }
}

/// StatusNotifierItem pixmaps are ARGB32 in network byte order.
fn argb_from_rgba(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray;

    #[test]
    fn the_window_hides_only_where_a_panel_shows_the_icon() {
        assert_eq!(
            background_command(true),
            egui::ViewportCommand::Visible(false)
        );
        assert_eq!(
            background_command(false),
            egui::ViewportCommand::Minimized(true)
        );
    }

    #[test]
    fn the_window_comes_back_when_the_panel_goes_and_hides_when_one_comes() {
        assert_eq!(tray_sync(true, false, true), TraySync::Restore);
        assert_eq!(tray_sync(true, true, true), TraySync::Stay);
        assert_eq!(tray_sync(false, true, true), TraySync::Hide);
        // A window in front stays there, whatever the panel does.
        assert_eq!(tray_sync(false, true, false), TraySync::Stay);
        assert_eq!(tray_sync(false, false, true), TraySync::Stay);
        assert_eq!(tray_sync(false, false, false), TraySync::Stay);
    }

    static RESTORE: AtomicBool = AtomicBool::new(false);
    static QUIT: AtomicBool = AtomicBool::new(false);

    fn tray(language: Language) -> EntropyTray {
        EntropyTray {
            ctx: egui::Context::default(),
            language,
            online: Arc::new(AtomicBool::new(true)),
            restore: &RESTORE,
            quit: &QUIT,
        }
    }

    fn standard_items(tray: &EntropyTray) -> Vec<ksni::menu::StandardItem<EntropyTray>> {
        tray.menu()
            .into_iter()
            .filter_map(|item| match item {
                ksni::MenuItem::Standard(item) => Some(item),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_menu_opens_and_quits_entropy_in_its_language() {
        for language in [Language::English, Language::Russian] {
            let labels: Vec<String> = standard_items(&tray(language))
                .into_iter()
                .map(|item| item.label)
                .collect();
            assert_eq!(
                labels,
                [
                    crate::i18n::tr_catalog(language, "tray.open"),
                    crate::i18n::tr_catalog(language, "tray.quit"),
                ]
            );
            assert!(
                labels.iter().all(|label| !label.contains('.')),
                "{labels:?}"
            );
        }
        let english = standard_items(&tray(Language::English));
        let russian = standard_items(&tray(Language::Russian));
        assert_ne!(english[0].label, russian[0].label);
    }

    #[test]
    fn a_click_or_open_restores_the_window_and_quit_asks_to_quit() {
        let mut tray = tray(Language::English);
        let items = standard_items(&tray);

        tray.activate(0, 0);
        assert!(RESTORE.swap(false, Ordering::Relaxed));
        assert!(!QUIT.load(Ordering::Relaxed));

        (items[0].activate)(&mut tray);
        assert!(RESTORE.swap(false, Ordering::Relaxed));
        assert!(!QUIT.load(Ordering::Relaxed));

        (items[1].activate)(&mut tray);
        assert!(QUIT.swap(false, Ordering::Relaxed));
        assert!(!RESTORE.load(Ordering::Relaxed));
    }

    #[test]
    fn the_icon_goes_offline_with_the_panel_and_comes_back_with_it() {
        let tray = tray(Language::English);
        assert!(tray.watcher_offline(ksni::OfflineReason::No));
        assert!(!tray.online.load(Ordering::Relaxed));
        tray.watcher_online();
        assert!(tray.online.load(Ordering::Relaxed));
    }

    #[test]
    fn pixmaps_are_argb_in_network_order() {
        assert_eq!(
            argb_from_rgba(&[1, 2, 3, 4, 5, 6, 7, 8]),
            [4, 1, 2, 3, 8, 5, 6, 7]
        );
        for icon in tray(Language::English).icon_pixmap() {
            assert_eq!(icon.data.len(), (icon.width * icon.height * 4) as usize);
        }
    }
}
