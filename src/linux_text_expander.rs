//! Where Text Expander stands in this Linux session: whether IBus runs and
//! serves the Entropy engine, whether the desktop hands keys to it, and
//! whether an Entropy source is the one in use.

use std::path::Path;

/// The desktops whose own settings decide whether IBus gets the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinuxDesktop {
    /// GNOME Shell runs IBus itself and switches only the sources listed in
    /// `org.gnome.desktop.input-sources`.
    Gnome,
    /// KWin starts the input method chosen under Virtual Keyboard.
    KdeWayland,
    Other,
}

pub(crate) fn linux_desktop() -> LinuxDesktop {
    if crate::qmk_hid_host::gnome_desktop_session() {
        LinuxDesktop::Gnome
    } else if crate::qmk_hid_host::kde_desktop_session()
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
    {
        LinuxDesktop::KdeWayland
    } else {
        LinuxDesktop::Other
    }
}

/// What GNOME's input sources hold for the Entropy layouts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GnomeSources {
    /// An Entropy source is listed.
    pub(crate) entropy: bool,
    /// English or Russian is listed without its Entropy source, so
    /// `gnome-input-sources.sh use` has something to change.
    pub(crate) replaceable: bool,
    pub(crate) writable: bool,
}

/// Reads `gsettings get org.gnome.desktop.input-sources sources` as
/// GSettings prints it, e.g. `[('xkb', 'us'), ('ibus', 'mozc-jp')]`.
/// Mirrors the conditions `gnome-input-sources.sh use` acts on.
pub(crate) fn gnome_sources_from(printed: &str, writable: bool) -> GnomeSources {
    let entropy = |layout: &str| printed.contains(&format!("'entropy-universal-symbols-{layout}'"));
    let replaceable = |layout: &str| {
        !entropy(layout)
            && (printed.contains(&format!("('xkb', '{layout}')"))
                || printed.contains(&format!("('xkb', '{layout}+")))
    };
    GnomeSources {
        entropy: entropy("us") || entropy("ru"),
        replaceable: replaceable("us") || replaceable("ru"),
        writable,
    }
}

fn gnome_sources() -> Option<GnomeSources> {
    let gsettings = |args: &[&str]| {
        std::process::Command::new("gsettings")
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let printed = gsettings(&["get", "org.gnome.desktop.input-sources", "sources"])?;
    let writable = gsettings(&["writable", "org.gnome.desktop.input-sources", "sources"])
        .is_some_and(|writable| writable == "true");
    Some(gnome_sources_from(&printed, writable))
}

/// Whether kwinrc has KWin start the IBus Wayland panel as the input method.
/// `None` when kwinrc names no input method at all.
pub(crate) fn kwin_runs_ibus_from(kwinrc: &str) -> Option<bool> {
    let mut in_wayland = false;
    let mut input_method = None;
    for line in kwinrc.lines().map(str::trim) {
        if line.starts_with('[') {
            in_wayland = line == "[Wayland]";
        } else if let Some((key, value)) = line.split_once('=').filter(|_| in_wayland) {
            if matches!(key.trim(), "InputMethod" | "InputMethod[$e]") && !value.trim().is_empty() {
                input_method = Some(
                    value
                        .trim()
                        .ends_with("org.freedesktop.IBus.Panel.Wayland.Gtk3.desktop"),
                );
            }
        }
    }
    input_method
}

fn kwin_runs_ibus() -> Option<bool> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))?;
    kwin_runs_ibus_from(&std::fs::read_to_string(config_home.join("kwinrc")).ok()?)
}

/// One look at the session, taken off the UI thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextExpanderProbe {
    pub(crate) desktop: LinuxDesktop,
    /// `None` when no ibus-daemon runs.
    pub(crate) ibus: Option<IbusSession>,
    /// GNOME only; `None` when the sources could not be read.
    pub(crate) gnome_sources: Option<GnomeSources>,
    /// KDE Plasma Wayland only; see [`kwin_runs_ibus_from`].
    pub(crate) kwin_runs_ibus: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IbusSession {
    /// The daemon lists the Entropy engine.
    pub(crate) engine_listed: bool,
    /// The source in use is an Entropy one, with its base layout (0 for
    /// English, 1 for Russian) when IBus says.
    pub(crate) active: Option<Option<u8>>,
}

pub(crate) fn probe() -> TextExpanderProbe {
    let desktop = linux_desktop();
    TextExpanderProbe {
        desktop,
        ibus: crate::smart_input::running_ibus().map(|connection| IbusSession {
            engine_listed: crate::smart_input::ibus_lists_entropy_engine(&connection)
                .unwrap_or(false),
            active: crate::smart_input::ibus_global_engine(&connection)
                .filter(|engine| engine.entropy)
                .map(|engine| engine.layout),
        }),
        gnome_sources: (desktop == LinuxDesktop::Gnome)
            .then(gnome_sources)
            .flatten(),
        kwin_runs_ibus: (desktop == LinuxDesktop::KdeWayland)
            .then(kwin_runs_ibus)
            .flatten(),
    }
}

/// NixOS wraps ibus-daemon with a fixed IBUS_COMPONENT_PATH, so an engine
/// registered for one user is never loaded there; only the NixOS module can.
pub(crate) fn nixos() -> bool {
    Path::new("/etc/NIXOS").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_sources_name_what_use_would_change() {
        let sources = |printed| gnome_sources_from(printed, true);
        assert_eq!(
            sources("[('xkb', 'us'), ('xkb', 'ru')]"),
            GnomeSources {
                entropy: false,
                replaceable: true,
                writable: true
            }
        );
        assert_eq!(
            sources("[('ibus', 'entropy-universal-symbols-us'), ('xkb', 'ru+winkeys')]"),
            GnomeSources {
                entropy: true,
                replaceable: true,
                writable: true
            }
        );
        assert_eq!(
            sources(
                "[('ibus', 'entropy-universal-symbols-us'), ('ibus', 'entropy-universal-symbols-ru')]"
            ),
            GnomeSources { entropy: true, replaceable: false, writable: true }
        );
        assert_eq!(
            sources("[('xkb', 'de'), ('xkb', 'usx')]"),
            GnomeSources {
                entropy: false,
                replaceable: false,
                writable: true
            },
            "neither English nor Russian is listed"
        );
        assert_eq!(
            sources("@a(ss) []"),
            GnomeSources {
                writable: true,
                ..Default::default()
            }
        );
        assert!(!gnome_sources_from("[('xkb', 'us')]", false).writable);
    }

    #[test]
    fn kwinrc_names_the_input_method_kwin_starts() {
        assert_eq!(
            kwin_runs_ibus_from(
                "[Wayland]\nInputMethod[$e]=/run/current-system/sw/share/applications/org.freedesktop.IBus.Panel.Wayland.Gtk3.desktop\n"
            ),
            Some(true)
        );
        assert_eq!(
            kwin_runs_ibus_from(
                "[Plugins]\nfoo=1\n\n[Wayland]\nInputMethod=/usr/share/applications/org.kde.plasma.keyboard.desktop\nVirtualKeyboardEnabled=true\n"
            ),
            Some(false)
        );
        assert_eq!(
            kwin_runs_ibus_from("[Wayland]\nVirtualKeyboardEnabled=true\n"),
            None
        );
        assert_eq!(
            kwin_runs_ibus_from(
                "[Other]\nInputMethod=/usr/share/applications/org.freedesktop.IBus.Panel.Wayland.Gtk3.desktop\n"
            ),
            None,
            "only the Wayland group counts"
        );
    }
}
