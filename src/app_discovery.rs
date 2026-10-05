use crate::application_layouts::DetectedApplication;
use std::process::Command;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::ffi::{CStr, CString};
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxForegroundBackend {
    X11,
    GnomeWayland,
    KdeWayland,
    Hyprland,
    Sway,
    UnsupportedWayland,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ForegroundState {
    Focused(DetectedApplication),
    UnidentifiedWindow(String),
    NoFocusedWindow,
    BackendUnavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForegroundStatus {
    pub(crate) backend: String,
    pub(crate) state: ForegroundState,
}

impl Default for ForegroundStatus {
    fn default() -> Self {
        Self {
            backend: "starting".to_owned(),
            state: ForegroundState::BackendUnavailable("Foreground monitor is starting".to_owned()),
        }
    }
}

impl ForegroundStatus {
    fn focused(&self) -> Option<&DetectedApplication> {
        match &self.state {
            ForegroundState::Focused(application) => Some(application),
            ForegroundState::UnidentifiedWindow(_)
            | ForegroundState::NoFocusedWindow
            | ForegroundState::BackendUnavailable(_) => None,
        }
    }

    fn with_focused(mut self, application: DetectedApplication) -> Self {
        self.state = ForegroundState::Focused(application);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ApplicationDiscoverySnapshot {
    pub(crate) foreground: Option<DetectedApplication>,
    pub(crate) foreground_status: ForegroundStatus,
    pub(crate) available: Vec<DetectedApplication>,
    #[cfg(target_os = "macos")]
    pub(crate) text_expander_names: Vec<String>,
}

#[derive(Default)]
struct ApplicationScan {
    available: Vec<DetectedApplication>,
    #[cfg(target_os = "macos")]
    text_expander_names: Vec<String>,
}

struct DiscoveryState {
    receiver: Option<mpsc::Receiver<ApplicationScan>>,
    available: Vec<DetectedApplication>,
    #[cfg(target_os = "macos")]
    text_expander_names: Vec<String>,
    next_scan: Instant,
    force_refresh: bool,
}

const APPLICATION_RESCAN_INTERVAL: Duration = Duration::from_secs(1);

static DISCOVERY: OnceLock<Mutex<DiscoveryState>> = OnceLock::new();
static FOREGROUND_MONITOR: OnceLock<Mutex<ForegroundStatus>> = OnceLock::new();
static FOREGROUND_MONITOR_STARTED: OnceLock<()> = OnceLock::new();

fn foreground_monitor_snapshot() -> ForegroundStatus {
    let state = FOREGROUND_MONITOR.get_or_init(|| Mutex::new(ForegroundStatus::default()));
    FOREGROUND_MONITOR_STARTED.get_or_init(|| {
        let state = state;
        if let Err(error) = std::thread::Builder::new()
            .name("foreground-monitor".to_owned())
            .spawn(move || foreground_monitor_loop(state))
        {
            if let Ok(mut status) = state.lock() {
                *status = ForegroundStatus {
                    backend: "unavailable".to_owned(),
                    state: ForegroundState::BackendUnavailable(format!(
                        "Cannot start foreground monitor: {error}"
                    )),
                };
            }
        }
    });
    state
        .lock()
        .map(|status| status.clone())
        .unwrap_or_else(|_| ForegroundStatus {
            backend: "unavailable".to_owned(),
            state: ForegroundState::BackendUnavailable(
                "Foreground monitor state is poisoned".to_owned(),
            ),
        })
}

fn publish_foreground_status(state: &'static Mutex<ForegroundStatus>, current: ForegroundStatus) {
    let Ok(mut status) = state.lock() else {
        return;
    };
    if *status == current {
        return;
    }
    log::info!(
        "Application layouts: foreground backend={} state={:?}",
        current.backend,
        current.state
    );
    *status = current;
}

fn polling_foreground_monitor_loop(state: &'static Mutex<ForegroundStatus>) -> ! {
    loop {
        let current = platform_foreground_status();
        let unavailable = matches!(current.state, ForegroundState::BackendUnavailable(_));
        publish_foreground_status(state, current);
        std::thread::sleep(if unavailable {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(200)
        });
    }
}

#[cfg(target_os = "linux")]
fn foreground_monitor_loop(state: &'static Mutex<ForegroundStatus>) {
    match linux_foreground_backend() {
        LinuxForegroundBackend::GnomeWayland => linux_gnome_wayland_monitor_loop(state),
        LinuxForegroundBackend::X11 => {}
        LinuxForegroundBackend::KdeWayland
        | LinuxForegroundBackend::Hyprland
        | LinuxForegroundBackend::Sway
        | LinuxForegroundBackend::UnsupportedWayland
        | LinuxForegroundBackend::Unavailable => polling_foreground_monitor_loop(state),
    }

    loop {
        let Some(reader) = LinuxX11Applications::open() else {
            publish_foreground_status(
                state,
                ForegroundStatus {
                    backend: "Linux X11".to_owned(),
                    state: ForegroundState::BackendUnavailable(
                        "Cannot open the X11 display; check DISPLAY and Xauthority".to_owned(),
                    ),
                },
            );
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };

        let result = reader.monitor_foreground(|foreground| {
            publish_foreground_status(
                state,
                ForegroundStatus {
                    backend: "Linux X11 (events)".to_owned(),
                    state: foreground,
                },
            );
        });
        let error = match result {
            Ok(()) => "X11 foreground monitor stopped unexpectedly".to_owned(),
            Err(error) => error,
        };
        publish_foreground_status(
            state,
            ForegroundStatus {
                backend: "Linux X11 (events)".to_owned(),
                state: ForegroundState::BackendUnavailable(error),
            },
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(not(target_os = "linux"))]
fn foreground_monitor_loop(state: &'static Mutex<ForegroundStatus>) {
    polling_foreground_monitor_loop(state);
}

pub(crate) fn application_discovery_snapshot() -> ApplicationDiscoverySnapshot {
    let state = DISCOVERY.get_or_init(|| {
        Mutex::new(DiscoveryState {
            receiver: None,
            available: Vec::new(),
            #[cfg(target_os = "macos")]
            text_expander_names: Vec::new(),
            next_scan: Instant::now(),
            force_refresh: false,
        })
    });
    let Ok(mut state) = state.lock() else {
        return ApplicationDiscoverySnapshot::default();
    };
    if let Some(receiver) = state.receiver.take() {
        match receiver.try_recv() {
            Ok(mut scan) => {
                scan.available.retain(platform_application_is_user_facing);
                state.available = scan.available;
                #[cfg(target_os = "macos")]
                {
                    state.text_expander_names = scan.text_expander_names;
                }
                state.next_scan = Instant::now() + APPLICATION_RESCAN_INTERVAL;
            }
            Err(mpsc::TryRecvError::Empty) => state.receiver = Some(receiver),
            Err(mpsc::TryRecvError::Disconnected) => {
                state.next_scan = Instant::now() + Duration::from_secs(1);
            }
        }
    }
    if state.receiver.is_none() && (state.force_refresh || Instant::now() >= state.next_scan) {
        state.force_refresh = false;
        let (sender, receiver) = mpsc::channel();
        if std::thread::Builder::new()
            .name("application-discovery".to_owned())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                let scan = platform_available_apps();
                #[cfg(not(target_os = "macos"))]
                let scan = ApplicationScan {
                    available: platform_available_apps(),
                };
                let _ = sender.send(scan);
            })
            .is_ok()
        {
            state.receiver = Some(receiver);
        } else {
            state.next_scan = Instant::now() + Duration::from_secs(1);
        }
    }
    let mut status = foreground_monitor_snapshot();
    if let Some(mut application) = status.focused().cloned() {
        enrich_application_from_platform_catalog(&mut application);
        enrich_application_from_catalog(&mut application, &state.available);
        status = status.with_focused(application.clone());
        if platform_application_is_user_facing(&application) {
            merge_applications(&mut state.available, [application]);
        }
    }
    ApplicationDiscoverySnapshot {
        foreground: status.focused().cloned(),
        foreground_status: status,
        available: state.available.clone(),
        #[cfg(target_os = "macos")]
        text_expander_names: state.text_expander_names.clone(),
    }
}

#[cfg(target_os = "linux")]
fn enrich_application_from_platform_catalog(application: &mut DetectedApplication) {
    if let Some(catalog) = LINUX_APPLICATION_CATALOG.get() {
        enrich_application_from_catalog(application, &catalog.visible);
    }
}

#[cfg(not(target_os = "linux"))]
fn enrich_application_from_platform_catalog(_application: &mut DetectedApplication) {}

pub(crate) fn running_application_choices(
    applications: &[DetectedApplication],
) -> Vec<DetectedApplication> {
    let mut choices = Vec::<DetectedApplication>::new();
    for application in applications {
        if application.executable.trim().is_empty() {
            continue;
        }
        let mut application = application.clone();
        application.window_title.clear();
        if let Some(existing) = choices.iter_mut().find(|existing| {
            crate::application_layouts::application_identities_match(
                std::iter::once(existing.executable.as_str())
                    .chain(existing.identities.iter().map(String::as_str)),
                std::iter::once(application.executable.as_str())
                    .chain(application.identities.iter().map(String::as_str)),
            )
        }) {
            if existing.display_name.trim().is_empty()
                || existing.display_name == existing.executable
            {
                existing.display_name = application.display_name;
            }
            existing.identities = crate::application_layouts::normalized_identity_values(
                std::iter::once(existing.executable.as_str())
                    .chain(existing.identities.iter().map(String::as_str))
                    .chain(std::iter::once(application.executable.as_str()))
                    .chain(application.identities.iter().map(String::as_str)),
            );
        } else {
            choices.push(application);
        }
    }
    choices.sort_by_cached_key(|application| {
        (
            application.display_name.to_ascii_lowercase(),
            application.executable.to_ascii_lowercase(),
        )
    });
    choices
}

#[cfg(target_os = "linux")]
fn platform_application_is_user_facing(application: &DetectedApplication) -> bool {
    linux_application_is_user_facing_with_catalog(application, linux_application_catalog())
}

#[cfg(not(target_os = "linux"))]
fn platform_application_is_user_facing(_application: &DetectedApplication) -> bool {
    true
}

pub(crate) fn refresh_application_discovery() {
    if let Some(state) = DISCOVERY.get() {
        if let Ok(mut state) = state.lock() {
            state.force_refresh = true;
        }
    }
}

fn enrich_application_from_catalog(
    application: &mut DetectedApplication,
    catalog: &[DetectedApplication],
) {
    let Some(best) = catalog.iter().max_by_key(|candidate| {
        crate::application_layouts::application_identity_match_score(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str)),
            std::iter::once(candidate.executable.as_str())
                .chain(candidate.identities.iter().map(String::as_str)),
        )
    }) else {
        return;
    };
    if !crate::application_layouts::application_identities_match(
        std::iter::once(application.executable.as_str())
            .chain(application.identities.iter().map(String::as_str)),
        std::iter::once(best.executable.as_str()).chain(best.identities.iter().map(String::as_str)),
    ) {
        return;
    }
    application.identities = crate::application_layouts::normalized_identity_values(
        std::iter::once(application.executable.as_str())
            .chain(application.identities.iter().map(String::as_str))
            .chain(std::iter::once(best.executable.as_str()))
            .chain(best.identities.iter().map(String::as_str)),
    );
    if application.display_name.trim().is_empty()
        || application.display_name == application.executable
    {
        application.display_name = best.display_name.clone();
    }
}

fn merge_applications(
    applications: &mut Vec<DetectedApplication>,
    additional: impl IntoIterator<Item = DetectedApplication>,
) {
    for application in additional {
        if application.executable.trim().is_empty() {
            continue;
        }
        if let Some(existing) = applications.iter_mut().find(|existing| {
            crate::application_layouts::application_identities_match(
                std::iter::once(existing.executable.as_str())
                    .chain(existing.identities.iter().map(String::as_str)),
                std::iter::once(application.executable.as_str())
                    .chain(application.identities.iter().map(String::as_str)),
            ) && existing.window_title == application.window_title
        }) {
            if existing.display_name.is_empty() {
                existing.display_name = application.display_name;
            }
            existing.identities = crate::application_layouts::normalized_identity_values(
                std::iter::once(existing.executable.as_str())
                    .chain(existing.identities.iter().map(String::as_str))
                    .chain(std::iter::once(application.executable.as_str()))
                    .chain(application.identities.iter().map(String::as_str)),
            );
        } else {
            applications.push(application);
        }
    }
    applications.sort_by_cached_key(|application| {
        (
            application.display_name.to_ascii_lowercase(),
            application.executable.to_ascii_lowercase(),
            application.window_title.to_ascii_lowercase(),
        )
    });
    // Installed desktop catalogs can legitimately contain more than 128
    // launchers. Keep enough entries so applications late in the alphabet
    // (for example Telegram or VS Code) are not discarded before search.
    applications.truncate(512);
}

fn status_from_application(
    backend: impl Into<String>,
    application: Option<DetectedApplication>,
) -> ForegroundStatus {
    ForegroundStatus {
        backend: backend.into(),
        state: application
            .map(ForegroundState::Focused)
            .unwrap_or(ForegroundState::NoFocusedWindow),
    }
}

#[cfg(target_os = "linux")]
fn platform_foreground_status() -> ForegroundStatus {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    match linux_foreground_backend() {
        LinuxForegroundBackend::Hyprland => linux_hyprland_status(),
        LinuxForegroundBackend::Sway => linux_sway_status(),
        LinuxForegroundBackend::GnomeWayland => linux_gnome_wayland_status(),
        LinuxForegroundBackend::KdeWayland => linux_kde_wayland_status(),
        LinuxForegroundBackend::UnsupportedWayland => ForegroundStatus {
            backend: "Wayland".to_owned(),
            state: ForegroundState::BackendUnavailable(format!(
                "No foreground-window adapter is available for desktop environment '{desktop}'"
            )),
        },
        LinuxForegroundBackend::X11 => match LinuxX11Applications::open() {
            Some(reader) => ForegroundStatus {
                backend: "Linux X11".to_owned(),
                state: reader.active_status(),
            },
            None => ForegroundStatus {
                backend: "Linux X11".to_owned(),
                state: ForegroundState::BackendUnavailable(
                    "Cannot open the X11 display; check DISPLAY and Xauthority".to_owned(),
                ),
            },
        },
        LinuxForegroundBackend::Unavailable => ForegroundStatus {
            backend: "Linux".to_owned(),
            state: ForegroundState::BackendUnavailable(
                "Cannot determine the display server from XDG_SESSION_TYPE, DISPLAY or WAYLAND_DISPLAY"
                    .to_owned(),
            ),
        },
    }
}

#[cfg(target_os = "linux")]
fn linux_foreground_backend() -> LinuxForegroundBackend {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    linux_foreground_backend_for(
        &desktop,
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        std::env::var_os("SWAYSOCK").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    )
}

#[cfg(target_os = "linux")]
fn linux_foreground_backend_for(
    desktop: &str,
    hyprland: bool,
    sway: bool,
    wayland_display: bool,
    x11_display: bool,
    session_type: Option<&str>,
) -> LinuxForegroundBackend {
    let explicit_session = session_type
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if explicit_session.is_some_and(|value| value.eq_ignore_ascii_case("x11")) {
        return LinuxForegroundBackend::X11;
    }

    let wayland = explicit_session.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || (explicit_session.is_none() && (wayland_display || hyprland || sway));
    if wayland {
        if hyprland {
            return LinuxForegroundBackend::Hyprland;
        }
        if sway {
            return LinuxForegroundBackend::Sway;
        }
        // KDE must win over stale/secondary GNOME markers. Some Plasma
        // sessions inherit a colon-separated desktop value from the login
        // manager; showing the GNOME installer there cannot help KWin.
        if desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("kde"))
        {
            return LinuxForegroundBackend::KdeWayland;
        }
        if desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("gnome") || name.eq_ignore_ascii_case("ubuntu"))
        {
            return LinuxForegroundBackend::GnomeWayland;
        }
        return LinuxForegroundBackend::UnsupportedWayland;
    }

    if x11_display {
        LinuxForegroundBackend::X11
    } else {
        LinuxForegroundBackend::Unavailable
    }
}

#[cfg(target_os = "windows")]
fn platform_foreground_status() -> ForegroundStatus {
    windows_foreground_status()
}

#[cfg(target_os = "macos")]
fn platform_foreground_status() -> ForegroundStatus {
    macos_foreground_status()
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_foreground_status() -> ForegroundStatus {
    ForegroundStatus {
        backend: std::env::consts::OS.to_owned(),
        state: ForegroundState::BackendUnavailable(
            "Foreground monitoring is not implemented on this operating system".to_owned(),
        ),
    }
}

#[cfg(target_os = "linux")]
fn linux_gnome_wayland_session(desktop: &str) -> bool {
    linux_foreground_backend_for(
        desktop,
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        std::env::var_os("SWAYSOCK").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    ) == LinuxForegroundBackend::GnomeWayland
}

#[cfg(target_os = "linux")]
fn linux_gnome_wayland_session_for(
    desktop: &str,
    wayland_display: bool,
    session_type: Option<&str>,
) -> bool {
    linux_foreground_backend_for(
        desktop,
        false,
        false,
        wayland_display,
        !wayland_display,
        session_type,
    ) == LinuxForegroundBackend::GnomeWayland
}

#[cfg(target_os = "linux")]
const GNOME_EXTENSION_UUID: &str = "entropy-foreground@ergohaven.com";
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_BUS: &str = "org.ergohaven.Entropy.Foreground1";
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_PATH: &str = "/org/ergohaven/Entropy/Foreground1";
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_INTERFACE: &str = "org.ergohaven.Entropy.Foreground1";
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_PROTOCOL_VERSION: u32 = 4;
#[cfg(target_os = "linux")]
static GNOME_SHELL_CONNECTION: OnceLock<zbus::blocking::Connection> = OnceLock::new();

#[cfg(target_os = "linux")]
const GNOME_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[cfg(target_os = "linux")]
fn gnome_shell_status_from_payload(
    payload: (bool, String, String, String, u32),
) -> ForegroundStatus {
    let (focused, app_id, wm_class, title, pid) = payload;
    ForegroundStatus {
        backend: "GNOME Wayland / Entropy Shell (events)".to_owned(),
        state: if focused {
            gnome_shell_application(app_id, wm_class, title, pid)
        } else {
            ForegroundState::NoFocusedWindow
        },
    }
}

#[cfg(target_os = "linux")]
fn linux_gnome_wayland_monitor_loop(state: &'static Mutex<ForegroundStatus>) -> ! {
    loop {
        let result = (|| -> Result<(), String> {
            let connection = zbus::blocking::Connection::session()
                .map_err(|error| format!("Cannot connect to the session bus: {error}"))?;
            let proxy = zbus::blocking::Proxy::new(
                &connection,
                GNOME_EXTENSION_BUS,
                GNOME_EXTENSION_PATH,
                GNOME_EXTENSION_INTERFACE,
            )
            .map_err(|error| format!("GNOME integration is not available: {error}"))?;
            let protocol_version = proxy
                .call::<_, _, u32>("GetProtocolVersion", &())
                .map_err(|_| "GNOME integration must be updated".to_owned())?;
            if protocol_version != GNOME_EXTENSION_PROTOCOL_VERSION {
                return Err(format!(
                    "Unsupported GNOME integration protocol {protocol_version}"
                ));
            }

            // Subscribe before reading the initial snapshot, so a focus change
            // between these operations cannot be lost.
            let mut signals = proxy
                .receive_signal("ActiveWindowChanged")
                .map_err(|error| format!("Cannot subscribe to GNOME focus events: {error}"))?;
            let initial = proxy
                .call::<_, _, (bool, String, String, String, u32)>("GetActiveWindow", &())
                .map_err(|error| format!("Cannot read the focused GNOME window: {error}"))?;
            publish_foreground_status(state, gnome_shell_status_from_payload(initial));

            for message in &mut signals {
                let payload = message
                    .body()
                    .deserialize::<(bool, String, String, String, u32)>()
                    .map_err(|error| format!("Invalid GNOME focus event: {error}"))?;
                publish_foreground_status(state, gnome_shell_status_from_payload(payload));
            }
            Err("GNOME focus event stream stopped".to_owned())
        })();

        let error = result
            .err()
            .unwrap_or_else(|| "GNOME focus monitor stopped unexpectedly".to_owned());
        publish_foreground_status(
            state,
            ForegroundStatus {
                backend: "GNOME Wayland / Entropy Shell".to_owned(),
                state: ForegroundState::BackendUnavailable(
                    "Install or enable the Entropy GNOME integration".to_owned(),
                ),
            },
        );
        log::debug!("Application layouts: {error}; retrying in {GNOME_RECONNECT_DELAY:?}");
        std::thread::sleep(GNOME_RECONNECT_DELAY);
    }
}

#[cfg(target_os = "linux")]
fn linux_gnome_wayland_status() -> ForegroundStatus {
    let connection = match GNOME_SHELL_CONNECTION.get() {
        Some(connection) => connection,
        None => match zbus::blocking::Connection::session() {
            Ok(connection) => GNOME_SHELL_CONNECTION.get_or_init(|| connection),
            Err(error) => {
                return ForegroundStatus {
                    backend: "GNOME Wayland / Entropy Shell".to_owned(),
                    state: ForegroundState::BackendUnavailable(format!(
                        "Cannot connect to the session bus: {error}"
                    )),
                };
            }
        },
    };
    let result = zbus::blocking::Proxy::new(
        connection,
        GNOME_EXTENSION_BUS,
        GNOME_EXTENSION_PATH,
        GNOME_EXTENSION_INTERFACE,
    )
    .and_then(|proxy| {
        proxy.call::<_, _, (bool, String, String, String, u32)>("GetActiveWindow", &())
    });

    let state = match result {
        Ok(payload) => return gnome_shell_status_from_payload(payload),
        Err(error) => {
            log::debug!("Application layouts: GNOME Shell integration unavailable: {error}");
            ForegroundState::BackendUnavailable(
                "Install or enable the Entropy GNOME integration".to_owned(),
            )
        }
    };
    ForegroundStatus {
        backend: "GNOME Wayland / Entropy Shell".to_owned(),
        state,
    }
}

#[cfg(target_os = "linux")]
fn gnome_shell_application(
    app_id: String,
    wm_class: String,
    title: String,
    pid: u32,
) -> ForegroundState {
    let mut application = (pid != 0)
        .then(|| linux_app_from_pid(pid, &title))
        .flatten()
        .or_else(|| {
            let executable = if app_id.trim().is_empty() {
                wm_class.trim()
            } else {
                app_id.trim()
            };
            (!executable.is_empty()).then(|| DetectedApplication {
                executable: executable.to_owned(),
                identities: Vec::new(),
                display_name: if wm_class.trim().is_empty() {
                    executable.to_owned()
                } else {
                    wm_class.trim().to_owned()
                },
                window_title: title.trim().to_owned(),
            })
        });
    let Some(application) = application.as_mut() else {
        return ForegroundState::UnidentifiedWindow(title);
    };
    application.identities = crate::application_layouts::normalized_identity_values(
        std::iter::once(application.executable.as_str())
            .chain(application.identities.iter().map(String::as_str))
            .chain([app_id.as_str(), wm_class.as_str()]),
    );
    ForegroundState::Focused(application.clone())
}

#[cfg(target_os = "linux")]
fn linux_gnome_wayland_applications() -> Vec<DetectedApplication> {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return Vec::new();
    };
    let Ok(proxy) = zbus::blocking::Proxy::new(
        &connection,
        GNOME_EXTENSION_BUS,
        GNOME_EXTENSION_PATH,
        GNOME_EXTENSION_INTERFACE,
    ) else {
        return Vec::new();
    };
    let Ok(windows) = proxy.call::<_, _, Vec<(String, String, String, u32)>>("GetOpenWindows", &())
    else {
        return Vec::new();
    };
    gnome_shell_applications_from_payload(windows)
}

#[cfg(target_os = "linux")]
fn gnome_shell_applications_from_payload(
    windows: Vec<(String, String, String, u32)>,
) -> Vec<DetectedApplication> {
    windows
        .into_iter()
        .filter_map(|(app_id, wm_class, title, pid)| {
            match gnome_shell_application(app_id, wm_class, title, pid) {
                ForegroundState::Focused(application) => Some(application),
                ForegroundState::UnidentifiedWindow(_)
                | ForegroundState::NoFocusedWindow
                | ForegroundState::BackendUnavailable(_) => None,
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub(crate) fn gnome_shell_integration_needed() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    linux_gnome_wayland_session(&desktop)
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GnomeIntegrationInstallReport {
    pub(crate) message: String,
    pub(crate) enabled: bool,
    pub(crate) active: bool,
    pub(crate) restart_required: bool,
}

#[cfg(target_os = "linux")]
pub(crate) struct GnomeIntegrationInstallTask {
    receiver: mpsc::Receiver<Result<GnomeIntegrationInstallReport, String>>,
}

#[cfg(target_os = "linux")]
impl GnomeIntegrationInstallTask {
    pub(crate) fn try_recv(
        &self,
    ) -> Result<Result<GnomeIntegrationInstallReport, String>, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn start_gnome_shell_integration_install() -> Result<GnomeIntegrationInstallTask, String>
{
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("gnome-integration-install".to_owned())
        .spawn(move || {
            let result = install_gnome_shell_integration_detailed();
            if result.as_ref().is_ok_and(|report| report.active) {
                refresh_foreground_monitor_now();
            }
            let _ = sender.send(result);
        })
        .map_err(|error| format!("Cannot start GNOME integration installer: {error}"))?;
    Ok(GnomeIntegrationInstallTask { receiver })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn gnome_shell_integration_needed() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(crate) fn install_gnome_shell_integration() -> Result<String, String> {
    install_gnome_shell_integration_detailed().map(|report| report.message)
}

#[cfg(target_os = "linux")]
fn install_gnome_shell_integration_detailed() -> Result<GnomeIntegrationInstallReport, String> {
    let shell_version = Command::new("gnome-shell")
        .arg("--version")
        .output()
        .map_err(|error| format!("Cannot query GNOME Shell version: {error}"))?;
    let version_text = String::from_utf8_lossy(&shell_version.stdout);
    let major = version_text
        .split_whitespace()
        .find_map(|part| part.split('.').next()?.parse::<u32>().ok())
        .ok_or_else(|| format!("Cannot parse GNOME Shell version: {version_text}"))?;
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| "HOME and XDG_DATA_HOME are unavailable".to_owned())?;
    let directory = data_home
        .join("gnome-shell/extensions")
        .join(GNOME_EXTENSION_UUID);
    // Updating an enabled extension in place leaves the previous JavaScript
    // object alive. Disable it first so re-enabling can load the new bridge.
    let _ = Command::new("gnome-extensions")
        .args(["disable", GNOME_EXTENSION_UUID])
        .output();
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Cannot create GNOME integration directory: {error}"))?;
    let extension = if major >= 45 {
        include_str!("../assets/gnome-shell-extension/extension-modern.js")
    } else {
        include_str!("../assets/gnome-shell-extension/extension-legacy.js")
    };
    std::fs::write(directory.join("extension.js"), extension)
        .map_err(|error| format!("Cannot install GNOME integration: {error}"))?;
    let metadata = gnome_extension_metadata(major);
    let metadata = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("Cannot build GNOME integration metadata: {error}"))?;
    std::fs::write(directory.join("metadata.json"), metadata)
        .map_err(|error| format!("Cannot install GNOME integration metadata: {error}"))?;

    // Validate before enabling. The old installer wrote package version 2 but
    // immediately required protocol 3, producing the reported false failure.
    verify_gnome_integration_files(&directory)?;

    let enabled_by_command = Command::new("gnome-extensions")
        .args(["enable", GNOME_EXTENSION_UUID])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !enabled_by_command {
        enable_gnome_extension_after_login()?;
    }
    let enabled = gnome_extension_is_enabled()?;
    if !enabled {
        return Err(
            "GNOME integration was installed but is not present in enabled-extensions".to_owned(),
        );
    }
    let active = gnome_extension_protocol_available();
    let restart_required = !active;
    let message = if active {
        "GNOME integration installed, enabled, and running".to_owned()
    } else {
        "GNOME integration installed and enabled; sign out of Ubuntu and sign back in once"
            .to_owned()
    };
    Ok(GnomeIntegrationInstallReport {
        message,
        enabled,
        active,
        restart_required,
    })
}

#[cfg(target_os = "linux")]
fn gnome_extension_metadata(major: u32) -> serde_json::Value {
    serde_json::json!({
        "uuid": GNOME_EXTENSION_UUID,
        "name": "Entropy application focus",
        "description": "Reports the focused GNOME window to Entropy without accessibility access",
        "version": GNOME_EXTENSION_PROTOCOL_VERSION,
        "shell-version": [major.to_string()]
    })
}

#[cfg(target_os = "linux")]
fn verify_gnome_integration_files(directory: &Path) -> Result<(), String> {
    let extension = directory.join("extension.js");
    let metadata = directory.join("metadata.json");
    if !extension.is_file() {
        return Err(format!(
            "GNOME integration file was not created: {}",
            extension.display()
        ));
    }
    let metadata_bytes = std::fs::read(&metadata).map_err(|error| {
        format!(
            "Cannot verify GNOME integration metadata {}: {error}",
            metadata.display()
        )
    })?;
    let metadata_json: serde_json::Value = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("Invalid GNOME integration metadata: {error}"))?;
    if metadata_json
        .get("uuid")
        .and_then(serde_json::Value::as_str)
        != Some(GNOME_EXTENSION_UUID)
    {
        return Err("Installed GNOME integration has the wrong UUID".to_owned());
    }
    if metadata_json
        .get("version")
        .and_then(serde_json::Value::as_u64)
        != Some(u64::from(GNOME_EXTENSION_PROTOCOL_VERSION))
    {
        return Err("Installed GNOME integration has the wrong protocol version".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn gnome_extension_is_enabled() -> Result<bool, String> {
    let current = Command::new("gsettings")
        .args(["get", "org.gnome.shell", "enabled-extensions"])
        .output()
        .map_err(|error| format!("Cannot read enabled GNOME extensions: {error}"))?;
    if !current.status.success() {
        return Err(format!(
            "Cannot read enabled GNOME extensions: {}",
            String::from_utf8_lossy(&current.stderr).trim()
        ));
    }
    Ok(
        parse_gsettings_string_array(&String::from_utf8_lossy(&current.stdout))
            .iter()
            .any(|extension| extension == GNOME_EXTENSION_UUID),
    )
}

#[cfg(target_os = "linux")]
fn gnome_extension_protocol_available() -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(proxy) = zbus::blocking::Proxy::new(
        &connection,
        GNOME_EXTENSION_BUS,
        GNOME_EXTENSION_PATH,
        GNOME_EXTENSION_INTERFACE,
    ) else {
        return false;
    };
    proxy
        .call::<_, _, u32>("GetProtocolVersion", &())
        .is_ok_and(|version| version == GNOME_EXTENSION_PROTOCOL_VERSION)
}

#[cfg(target_os = "linux")]
pub(crate) fn refresh_foreground_monitor_now() {
    let Some(state) = FOREGROUND_MONITOR.get() else {
        return;
    };
    publish_foreground_status(state, platform_foreground_status());
}

#[cfg(target_os = "linux")]
fn enable_gnome_extension_after_login() -> Result<(), String> {
    let current = Command::new("gsettings")
        .args(["get", "org.gnome.shell", "enabled-extensions"])
        .output()
        .map_err(|error| format!("Cannot read enabled GNOME extensions: {error}"))?;
    if !current.status.success() {
        return Err("Cannot read enabled GNOME extensions".to_owned());
    }
    let current = String::from_utf8_lossy(&current.stdout);
    let mut extensions = parse_gsettings_string_array(&current);
    if !extensions.iter().any(|value| value == GNOME_EXTENSION_UUID) {
        extensions.push(GNOME_EXTENSION_UUID.to_owned());
    }
    let value = format!(
        "[{}]",
        extensions
            .iter()
            .map(|extension| format!("'{}'", extension.replace('\'', "\\'")))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let status = Command::new("gsettings")
        .args([
            "set",
            "org.gnome.shell",
            "enabled-extensions",
            value.as_str(),
        ])
        .status()
        .map_err(|error| format!("Cannot enable GNOME integration: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "Cannot enable GNOME integration".to_owned())
}

#[cfg(target_os = "linux")]
fn parse_gsettings_string_array(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '\'' {
            if quoted {
                values.push(std::mem::take(&mut current));
            }
            quoted = !quoted;
        } else if quoted {
            current.push(character);
        }
    }
    values
}

#[cfg(target_os = "linux")]
fn platform_available_apps() -> Vec<DetectedApplication> {
    // Only applications with visible windows belong in the picker. The
    // installed desktop catalog is kept as metadata so runtime process IDs
    // still receive friendly names and stable identities without exposing
    // control panels and helper launchers to the user.
    let mut applications = Vec::new();
    if linux_foreground_backend() == LinuxForegroundBackend::GnomeWayland {
        merge_applications(&mut applications, linux_gnome_wayland_applications());
    }
    if let Some(output) = command_stdout("hyprctl", &["-j", "clients"]) {
        merge_applications(&mut applications, parse_hyprland_clients(&output));
    }
    if let Some(output) = command_stdout("swaymsg", &["-t", "get_tree", "-r"]) {
        merge_applications(&mut applications, parse_sway_apps(&output));
    }
    if let Some(output) = command_stdout("wmctrl", &["-lp"]) {
        merge_applications(&mut applications, parse_wmctrl_apps(&output));
    }
    merge_applications(&mut applications, linux_x11_applications());
    let catalog = linux_installed_apps();
    for application in &mut applications {
        enrich_application_from_catalog(application, &catalog);
    }
    applications
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct LinuxApplicationCatalog {
    visible: Vec<DetectedApplication>,
    hidden: Vec<DetectedApplication>,
}

#[cfg(target_os = "linux")]
static LINUX_APPLICATION_CATALOG: OnceLock<LinuxApplicationCatalog> = OnceLock::new();

#[cfg(target_os = "linux")]
fn linux_application_catalog() -> &'static LinuxApplicationCatalog {
    LINUX_APPLICATION_CATALOG.get_or_init(scan_linux_desktop_entries)
}

#[cfg(target_os = "linux")]
fn linux_installed_apps() -> Vec<DetectedApplication> {
    linux_application_catalog().visible.clone()
}

#[cfg(target_os = "linux")]
fn linux_application_is_user_facing_with_catalog(
    application: &DetectedApplication,
    catalog: &LinuxApplicationCatalog,
) -> bool {
    let matches = |candidate: &DetectedApplication| {
        crate::application_layouts::application_identities_match(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str)),
            std::iter::once(candidate.executable.as_str())
                .chain(candidate.identities.iter().map(String::as_str)),
        )
    };

    // A visible launcher wins over a hidden helper entry from the same package.
    // Unknown applications remain visible so portable binaries and AppImages
    // with a real window are not accidentally discarded.
    catalog.visible.iter().any(matches) || !catalog.hidden.iter().any(matches)
}

#[cfg(target_os = "linux")]
fn scan_linux_desktop_entries() -> LinuxApplicationCatalog {
    let mut data_roots = Vec::<PathBuf>::new();
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        data_roots.push(PathBuf::from(data_home));
    } else if let Some(home) = std::env::var_os("HOME") {
        data_roots.push(PathBuf::from(home).join(".local/share"));
    }
    let system_roots =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data_roots.extend(std::env::split_paths(&system_roots));

    let mut catalog = LinuxApplicationCatalog::default();
    for root in data_roots {
        collect_desktop_entries(&root.join("applications"), 0, &mut catalog);
    }
    let mut visible = Vec::new();
    merge_applications(&mut visible, catalog.visible);
    let mut hidden = Vec::new();
    merge_applications(&mut hidden, catalog.hidden);
    LinuxApplicationCatalog { visible, hidden }
}

#[cfg(target_os = "linux")]
fn collect_desktop_entries(directory: &Path, depth: usize, catalog: &mut LinuxApplicationCatalog) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_entries(&path, depth + 1, catalog);
        } else if path.extension().and_then(|value| value.to_str()) == Some("desktop") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                let desktop_id = path.file_stem().and_then(|value| value.to_str());
                if let Some((application, visible)) =
                    parse_desktop_entry_metadata_with_id(&contents, desktop_id)
                {
                    if visible {
                        catalog.visible.push(application);
                    } else {
                        catalog.hidden.push(application);
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_desktop_entry(contents: &str) -> Option<DetectedApplication> {
    parse_desktop_entry_with_id(contents, None)
}

#[cfg(target_os = "linux")]
fn parse_desktop_entry_with_id(
    contents: &str,
    desktop_id: Option<&str>,
) -> Option<DetectedApplication> {
    parse_desktop_entry_metadata_with_id(contents, desktop_id)
        .and_then(|(application, visible)| visible.then_some(application))
}

#[cfg(target_os = "linux")]
fn parse_desktop_entry_metadata_with_id(
    contents: &str,
    desktop_id: Option<&str>,
) -> Option<(DetectedApplication, bool)> {
    let mut in_desktop_entry = false;
    let mut name = String::new();
    let mut executable = String::new();
    let mut try_exec = String::new();
    let mut startup_class = String::new();
    let mut application_type = String::new();
    let mut hidden = false;
    let mut no_display = false;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if in_desktop_entry {
                break;
            }
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => name = value.to_owned(),
            "Exec" => executable = desktop_exec_executable(value).unwrap_or_default(),
            "TryExec" => try_exec = desktop_exec_executable(value).unwrap_or_default(),
            "StartupWMClass" => startup_class = value.to_owned(),
            "Type" => application_type = value.to_owned(),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if application_type != "Application" || name.trim().is_empty() {
        return None;
    }
    let primary_executable = [
        executable.as_str(),
        try_exec.as_str(),
        startup_class.as_str(),
    ]
    .into_iter()
    .find(|value| !value.trim().is_empty())?
    .to_owned();
    let identities = crate::application_layouts::normalized_identity_values(
        [
            Some(primary_executable.as_str()),
            Some(executable.as_str()),
            Some(try_exec.as_str()),
            Some(startup_class.as_str()),
            desktop_id,
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty()),
    );
    Some((
        DetectedApplication {
            executable: primary_executable,
            identities,
            display_name: name,
            window_title: String::new(),
        },
        !hidden && !no_display,
    ))
}

#[cfg(target_os = "linux")]
fn desktop_exec_executable(command: &str) -> Option<String> {
    let tokens = split_desktop_exec(command);
    let mut index = 0usize;
    if tokens.first().is_some_and(|token| {
        Path::new(token)
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == "env")
    }) {
        index += 1;
        while tokens.get(index).is_some_and(|token| {
            token.starts_with('-')
                || token
                    .split_once('=')
                    .is_some_and(|(name, _)| !name.is_empty())
        }) {
            index += 1;
        }
    }
    let command = tokens.get(index)?.trim();
    let command_name = Path::new(command)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(command);
    if command_name == "flatpak" && tokens.get(index + 1).is_some_and(|value| value == "run") {
        return tokens[index + 2..]
            .iter()
            .find(|value| !value.starts_with('-') && !value.starts_with('%'))
            .cloned();
    }
    if command_name == "snap" && tokens.get(index + 1).is_some_and(|value| value == "run") {
        return tokens.get(index + 2).cloned();
    }
    (!command_name.is_empty() && !command_name.starts_with('%')).then(|| command_name.to_owned())
}

#[cfg(target_os = "linux")]
fn split_desktop_exec(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            token.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                token.push(character);
            }
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(character);
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

#[cfg(target_os = "linux")]
struct LinuxX11Applications {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
}

#[cfg(target_os = "linux")]
const LINUX_X11_USER_WINDOW_TYPES: &[&str] = &[
    "_NET_WM_WINDOW_TYPE_NORMAL",
    "_NET_WM_WINDOW_TYPE_DIALOG",
    "_NET_WM_WINDOW_TYPE_UTILITY",
];

#[cfg(target_os = "linux")]
static LINUX_X11_DISCOVERY_DISPLAYS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();

#[cfg(target_os = "linux")]
fn linux_x11_discovery_displays() -> &'static Mutex<HashSet<usize>> {
    LINUX_X11_DISCOVERY_DISPLAYS.get_or_init(|| {
        // Window ids may become stale between reading _NET_CLIENT_LIST and
        // querying their properties. Handle errors only for our private Xlib
        // connection; winit retains normal handling for its own connection.
        winit::platform::x11::register_xlib_error_hook(Box::new(|display, _error| {
            LINUX_X11_DISCOVERY_DISPLAYS
                .get()
                .and_then(|displays| displays.lock().ok())
                .is_some_and(|displays| displays.contains(&(display as usize)))
        }));
        Mutex::new(HashSet::new())
    })
}

#[cfg(target_os = "linux")]
impl LinuxX11Applications {
    fn open() -> Option<Self> {
        unsafe {
            let xlib = x11_dl::xlib::Xlib::open().ok()?;
            let tracked_displays = linux_x11_discovery_displays();
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return None;
            }
            tracked_displays.lock().ok()?.insert(display as usize);
            Some(Self { xlib, display })
        }
    }

    fn atom(&self, name: &str) -> Option<x11_dl::xlib::Atom> {
        let name = CString::new(name).ok()?;
        let atom = unsafe { (self.xlib.XInternAtom)(self.display, name.as_ptr(), 1) };
        (atom != 0).then_some(atom)
    }

    fn property_u32(&self, window: x11_dl::xlib::Window, name: &str) -> Vec<u32> {
        let Some(property) = self.atom(name) else {
            return Vec::new();
        };
        unsafe {
            let mut actual_type = 0;
            let mut actual_format = 0;
            let mut item_count = 0;
            let mut bytes_after = 0;
            let mut data = std::ptr::null_mut();
            let status = (self.xlib.XGetWindowProperty)(
                self.display,
                window,
                property,
                0,
                4096,
                x11_dl::xlib::False,
                x11_dl::xlib::AnyPropertyType as x11_dl::xlib::Atom,
                &mut actual_type,
                &mut actual_format,
                &mut item_count,
                &mut bytes_after,
                &mut data,
            );
            if status != 0 || data.is_null() || actual_format != 32 {
                if !data.is_null() {
                    (self.xlib.XFree)(data.cast());
                }
                return Vec::new();
            }
            let values = std::slice::from_raw_parts(
                data.cast::<std::os::raw::c_ulong>(),
                item_count as usize,
            )
            .iter()
            .map(|value| *value as u32)
            .collect();
            (self.xlib.XFree)(data.cast());
            values
        }
    }

    fn property_text(&self, window: x11_dl::xlib::Window, name: &str) -> Option<String> {
        let property = self.atom(name)?;
        unsafe {
            let mut actual_type = 0;
            let mut actual_format = 0;
            let mut item_count = 0;
            let mut bytes_after = 0;
            let mut data = std::ptr::null_mut();
            let status = (self.xlib.XGetWindowProperty)(
                self.display,
                window,
                property,
                0,
                4096,
                x11_dl::xlib::False,
                x11_dl::xlib::AnyPropertyType as x11_dl::xlib::Atom,
                &mut actual_type,
                &mut actual_format,
                &mut item_count,
                &mut bytes_after,
                &mut data,
            );
            if status != 0 || data.is_null() || actual_format != 8 {
                if !data.is_null() {
                    (self.xlib.XFree)(data.cast());
                }
                return None;
            }
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), item_count as usize);
            let value = String::from_utf8_lossy(bytes)
                .trim_matches('\0')
                .trim()
                .to_owned();
            (self.xlib.XFree)(data.cast());
            (!value.is_empty()).then_some(value)
        }
    }

    fn window_classes(&self, window: x11_dl::xlib::Window) -> Vec<String> {
        unsafe {
            let mut hint: x11_dl::xlib::XClassHint = std::mem::zeroed();
            if (self.xlib.XGetClassHint)(self.display, window, &mut hint) == 0 {
                return Vec::new();
            }
            let class = if hint.res_class.is_null() {
                String::new()
            } else {
                CStr::from_ptr(hint.res_class)
                    .to_string_lossy()
                    .into_owned()
            };
            let name = if hint.res_name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(hint.res_name).to_string_lossy().into_owned()
            };
            if !hint.res_class.is_null() {
                (self.xlib.XFree)(hint.res_class.cast());
            }
            if !hint.res_name.is_null() {
                (self.xlib.XFree)(hint.res_name.cast());
            }
            [class, name]
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .collect()
        }
    }

    fn window_is_viewable(&self, window: x11_dl::xlib::Window) -> bool {
        if window == 0 {
            return false;
        }
        unsafe {
            let mut attributes: x11_dl::xlib::XWindowAttributes = std::mem::zeroed();
            (self.xlib.XGetWindowAttributes)(self.display, window, &mut attributes) != 0
                && attributes.map_state == x11_dl::xlib::IsViewable
        }
    }

    fn window_is_user_facing(&self, window: x11_dl::xlib::Window) -> bool {
        let window_types = self.property_u32(window, "_NET_WM_WINDOW_TYPE");
        if window_types.is_empty() {
            // Older applications may not publish EWMH type metadata. They are
            // still valid ICCCM clients, so keep them instead of hiding them.
            return true;
        }
        LINUX_X11_USER_WINDOW_TYPES
            .iter()
            .filter_map(|name| self.atom(name))
            .any(|allowed| window_types.contains(&(allowed as u32)))
    }

    fn child_windows(&self, window: x11_dl::xlib::Window) -> Vec<x11_dl::xlib::Window> {
        unsafe {
            let mut returned_root = 0;
            let mut parent = 0;
            let mut children = std::ptr::null_mut();
            let mut child_count = 0;
            let status = (self.xlib.XQueryTree)(
                self.display,
                window,
                &mut returned_root,
                &mut parent,
                &mut children,
                &mut child_count,
            );
            if status == 0 || children.is_null() {
                if !children.is_null() {
                    (self.xlib.XFree)(children.cast());
                }
                return Vec::new();
            }
            let result = std::slice::from_raw_parts(children, child_count as usize).to_vec();
            (self.xlib.XFree)(children.cast());
            result
        }
    }

    fn is_icccm_client(&self, window: x11_dl::xlib::Window) -> bool {
        !self.property_u32(window, "WM_STATE").is_empty()
    }

    fn client_descendant(
        &self,
        window: x11_dl::xlib::Window,
        depth: usize,
    ) -> Option<x11_dl::xlib::Window> {
        if self.is_icccm_client(window) {
            return Some(window);
        }
        if depth == 0 {
            return None;
        }
        self.child_windows(window)
            .into_iter()
            .find_map(|child| self.client_descendant(child, depth - 1))
    }

    fn client_window_for_focus(&self, mut window: x11_dl::xlib::Window) -> x11_dl::xlib::Window {
        let root = unsafe { (self.xlib.XDefaultRootWindow)(self.display) };
        if window == 0 || window == root {
            return window;
        }
        if let Some(client) = self.client_descendant(window, 4) {
            return client;
        }
        let fallback = window;
        loop {
            if self.is_icccm_client(window) {
                return window;
            }
            unsafe {
                let mut returned_root = 0;
                let mut parent = 0;
                let mut children = std::ptr::null_mut();
                let mut child_count = 0;
                let status = (self.xlib.XQueryTree)(
                    self.display,
                    window,
                    &mut returned_root,
                    &mut parent,
                    &mut children,
                    &mut child_count,
                );
                if !children.is_null() {
                    (self.xlib.XFree)(children.cast());
                }
                if status == 0 || parent == 0 || parent == root {
                    return fallback;
                }
                window = parent;
            }
        }
    }

    fn active_window(&self) -> Option<x11_dl::xlib::Window> {
        let root = unsafe { (self.xlib.XDefaultRootWindow)(self.display) };
        if let Some(window) = self
            .property_u32(root, "_NET_ACTIVE_WINDOW")
            .first()
            .copied()
            .map(x11_dl::xlib::Window::from)
            .filter(|window| *window != 0 && self.window_is_viewable(*window))
        {
            return Some(window);
        }

        unsafe {
            let mut focused = 0;
            let mut revert_to = 0;
            (self.xlib.XGetInputFocus)(self.display, &mut focused, &mut revert_to);
            if focused == 0
                || focused == root
                || focused == x11_dl::xlib::PointerRoot as x11_dl::xlib::Window
            {
                return None;
            }
            let focused = self.client_window_for_focus(focused);
            self.window_is_viewable(focused).then_some(focused)
        }
    }

    fn application_for_window(&self, window: x11_dl::xlib::Window) -> Option<DetectedApplication> {
        if window == 0 || !self.window_is_user_facing(window) {
            return None;
        }
        let title = self
            .property_text(window, "_NET_WM_NAME")
            .or_else(|| self.property_text(window, "WM_NAME"))
            .unwrap_or_default();
        let classes = self.window_classes(window);
        let class = classes.first().cloned().unwrap_or_default();
        let mut application = self
            .property_u32(window, "_NET_WM_PID")
            .first()
            .copied()
            .and_then(|pid| linux_app_from_pid(pid, &title))
            .or_else(|| {
                (!class.is_empty()).then(|| DetectedApplication {
                    executable: class.clone(),
                    identities: classes.clone(),
                    display_name: class.clone(),
                    window_title: title.clone(),
                })
            })?;
        if !class.is_empty() {
            application.display_name = class;
        }
        application.identities = crate::application_layouts::normalized_identity_values(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str))
                .chain(classes.iter().map(String::as_str)),
        );
        Some(application)
    }

    fn active_status(&self) -> ForegroundState {
        let Some(window) = self.active_window() else {
            return ForegroundState::NoFocusedWindow;
        };
        if let Some(application) = self.application_for_window(window) {
            return ForegroundState::Focused(application);
        }
        let title = self
            .property_text(window, "_NET_WM_NAME")
            .or_else(|| self.property_text(window, "WM_NAME"))
            .unwrap_or_default();
        ForegroundState::UnidentifiedWindow(title)
    }

    fn subscribe_foreground_events(&self) -> Result<(), String> {
        let root = unsafe { (self.xlib.XDefaultRootWindow)(self.display) };
        if root == 0 {
            return Err("X11 did not provide a root window".to_owned());
        }

        unsafe {
            // _NET_ACTIVE_WINDOW is a property of the root window. The event
            // mask belongs only to this private X11 connection and does not
            // modify winit's connection or the window manager's subscriptions.
            (self.xlib.XSelectInput)(
                self.display,
                root,
                x11_dl::xlib::PropertyChangeMask | x11_dl::xlib::FocusChangeMask,
            );
            (self.xlib.XSync)(self.display, x11_dl::xlib::False);
        }
        Ok(())
    }

    fn wait_for_foreground_event(&self, timeout_ms: i32) -> Result<bool, String> {
        let connection = unsafe { (self.xlib.XConnectionNumber)(self.display) };
        if connection < 0 {
            return Err("X11 did not provide a connection descriptor".to_owned());
        }
        if unsafe { (self.xlib.XPending)(self.display) } == 0 {
            let mut descriptor = libc::pollfd {
                fd: connection,
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    return Ok(false);
                }
                return Err(format!("X11 event wait failed: {error}"));
            }
            if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(format!(
                    "X11 connection closed (poll flags: 0x{:x})",
                    descriptor.revents
                ));
            }
            if result == 0 {
                return Ok(false);
            }
        }

        let mut received_event = false;
        unsafe {
            while (self.xlib.XPending)(self.display) > 0 {
                let mut event: x11_dl::xlib::XEvent = std::mem::zeroed();
                (self.xlib.XNextEvent)(self.display, &mut event);
                received_event = true;
            }
        }
        Ok(received_event)
    }

    fn monitor_foreground(&self, mut publish: impl FnMut(ForegroundState)) -> Result<(), String> {
        self.subscribe_foreground_events()?;
        publish(self.active_status());
        loop {
            // Non-EWMH window managers may not update _NET_ACTIVE_WINDOW.
            // A one-second timeout validates XGetInputFocus as a compatibility
            // fallback; normal GNOME X11 switches arrive as root events.
            let _received_event = self.wait_for_foreground_event(1_000)?;
            publish(self.active_status());
        }
    }

    fn applications(&self) -> Vec<DetectedApplication> {
        let root = unsafe { (self.xlib.XDefaultRootWindow)(self.display) };
        let windows = {
            let stacking = self.property_u32(root, "_NET_CLIENT_LIST_STACKING");
            if stacking.is_empty() {
                self.property_u32(root, "_NET_CLIENT_LIST")
            } else {
                stacking
            }
        };
        windows
            .into_iter()
            .filter(|window| *window != 0)
            .filter_map(|window| self.application_for_window(window.into()))
            .collect()
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxX11Applications {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XCloseDisplay)(self.display);
        }
        if let Ok(mut displays) = linux_x11_discovery_displays().lock() {
            displays.remove(&(self.display as usize));
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_x11_applications() -> Vec<DetectedApplication> {
    LinuxX11Applications::open()
        .map(|reader| reader.applications())
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn linux_kde_wayland_status() -> ForegroundStatus {
    let info = match kdotool::get_active_window_info() {
        Ok(info) => info,
        Err(error) => {
            return ForegroundStatus {
                backend: "KDE Wayland / KWin".to_owned(),
                state: ForegroundState::BackendUnavailable(error.to_string()),
            };
        }
    };
    let title = info.title.trim().to_owned();
    let class = info.class_name.trim().to_owned();
    let application = linux_app_from_pid(info.pid, title.clone()).or_else(|| {
        (!class.is_empty()).then(|| DetectedApplication {
            executable: class.clone(),
            identities: vec![class.clone()],
            display_name: class.clone(),
            window_title: title.clone(),
        })
    });
    let Some(mut application) = application else {
        return ForegroundStatus {
            backend: "KDE Wayland / KWin".to_owned(),
            state: ForegroundState::UnidentifiedWindow(title),
        };
    };
    if !class.is_empty() {
        application.identities = crate::application_layouts::normalized_identity_values(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str))
                .chain(std::iter::once(class.as_str())),
        );
    }
    ForegroundStatus {
        backend: "KDE Wayland / KWin".to_owned(),
        state: ForegroundState::Focused(application),
    }
}

#[cfg(target_os = "linux")]
fn linux_hyprland_status() -> ForegroundStatus {
    let backend = "Hyprland IPC";
    let output = match Command::new("hyprctl")
        .args(["-j", "activewindow"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return ForegroundStatus {
                backend: backend.to_owned(),
                state: ForegroundState::BackendUnavailable(format!(
                    "hyprctl exited with status {}",
                    output.status
                )),
            };
        }
        Err(error) => {
            return ForegroundStatus {
                backend: backend.to_owned(),
                state: ForegroundState::BackendUnavailable(format!("cannot run hyprctl: {error}")),
            };
        }
    };
    let value = match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
        Ok(value) => value,
        Err(error) => {
            return ForegroundStatus {
                backend: backend.to_owned(),
                state: ForegroundState::BackendUnavailable(format!(
                    "hyprctl returned invalid JSON: {error}"
                )),
            };
        }
    };
    let state = if value.as_object().is_some_and(serde_json::Map::is_empty) {
        ForegroundState::NoFocusedWindow
    } else if let Some(application) = parse_hyprland_window(&value) {
        ForegroundState::Focused(application)
    } else {
        ForegroundState::UnidentifiedWindow(
            value
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        )
    };
    ForegroundStatus {
        backend: backend.to_owned(),
        state,
    }
}

#[cfg(target_os = "linux")]
fn linux_sway_status() -> ForegroundStatus {
    let backend = "Sway IPC";
    let output = match Command::new("swaymsg")
        .args(["-t", "get_tree", "-r"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return ForegroundStatus {
                backend: backend.to_owned(),
                state: ForegroundState::BackendUnavailable(format!(
                    "swaymsg exited with status {}",
                    output.status
                )),
            };
        }
        Err(error) => {
            return ForegroundStatus {
                backend: backend.to_owned(),
                state: ForegroundState::BackendUnavailable(format!("cannot run swaymsg: {error}")),
            };
        }
    };
    let value = match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
        Ok(value) => value,
        Err(error) => {
            return ForegroundStatus {
                backend: backend.to_owned(),
                state: ForegroundState::BackendUnavailable(format!(
                    "swaymsg returned invalid JSON: {error}"
                )),
            };
        }
    };
    let state = find_focused_sway_node(&value)
        .and_then(parse_sway_node)
        .map(ForegroundState::Focused)
        .unwrap_or(ForegroundState::NoFocusedWindow);
    ForegroundStatus {
        backend: backend.to_owned(),
        state,
    }
}

#[cfg(target_os = "linux")]
fn linux_app_from_pid(pid: u32, title: impl Into<String>) -> Option<DetectedApplication> {
    let executable = std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .ok()
                .and_then(|bytes| {
                    let command = bytes.split(|byte| *byte == 0).next()?;
                    let command = String::from_utf8_lossy(command);
                    Path::new(command.as_ref())
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
        })
        .or_else(|| {
            std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .map(|value| value.trim().to_owned())
        })?;
    (!executable.is_empty()).then(|| DetectedApplication {
        display_name: executable.clone(),
        identities: vec![executable.clone()],
        executable,
        window_title: title.into().trim().to_owned(),
    })
}

#[cfg(target_os = "linux")]
fn parse_hyprland_window(value: &serde_json::Value) -> Option<DetectedApplication> {
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if let Some(mut application) = value
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .and_then(|pid| linux_app_from_pid(pid, title))
    {
        if let Some(class) = value.get("class").and_then(serde_json::Value::as_str) {
            application.identities = crate::application_layouts::normalized_identity_values(
                std::iter::once(application.executable.as_str())
                    .chain(application.identities.iter().map(String::as_str))
                    .chain(std::iter::once(class)),
            );
        }
        return Some(application);
    }
    let executable = value
        .get("class")
        .and_then(serde_json::Value::as_str)?
        .trim();
    (!executable.is_empty()).then(|| DetectedApplication {
        executable: executable.to_owned(),
        identities: vec![executable.to_owned()],
        display_name: executable.to_owned(),
        window_title: title.trim().to_owned(),
    })
}

#[cfg(target_os = "linux")]
fn parse_hyprland_clients(output: &str) -> Vec<DetectedApplication> {
    serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(parse_hyprland_window)
        .collect()
}

#[cfg(target_os = "linux")]
fn find_focused_sway_node(value: &serde_json::Value) -> Option<&serde_json::Value> {
    if value
        .get("focused")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Some(value);
    }
    ["nodes", "floating_nodes"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_array))
        .flat_map(|nodes| nodes.iter())
        .find_map(find_focused_sway_node)
}

#[cfg(target_os = "linux")]
fn parse_sway_node(value: &serde_json::Value) -> Option<DetectedApplication> {
    let title = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if let Some(mut application) = value
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .and_then(|pid| linux_app_from_pid(pid, title))
    {
        if let Some(app_id) = value
            .get("app_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                value
                    .get("window_properties")
                    .and_then(|properties| properties.get("class"))
                    .and_then(serde_json::Value::as_str)
            })
        {
            application.identities = crate::application_layouts::normalized_identity_values(
                std::iter::once(application.executable.as_str())
                    .chain(application.identities.iter().map(String::as_str))
                    .chain(std::iter::once(app_id)),
            );
        }
        return Some(application);
    }
    let executable = value
        .get("app_id")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("window_properties")
                .and_then(|properties| properties.get("class"))
                .and_then(serde_json::Value::as_str)
        })?
        .trim();
    (!executable.is_empty()).then(|| DetectedApplication {
        executable: executable.to_owned(),
        identities: vec![executable.to_owned()],
        display_name: executable.to_owned(),
        window_title: title.trim().to_owned(),
    })
}

#[cfg(target_os = "linux")]
fn collect_sway_apps(value: &serde_json::Value, applications: &mut Vec<DetectedApplication>) {
    if let Some(application) = parse_sway_node(value) {
        applications.push(application);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(nodes) = value.get(key).and_then(serde_json::Value::as_array) {
            for node in nodes {
                collect_sway_apps(node, applications);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_sway_apps(output: &str) -> Vec<DetectedApplication> {
    let Some(value) = serde_json::from_str::<serde_json::Value>(output).ok() else {
        return Vec::new();
    };
    let mut applications = Vec::new();
    collect_sway_apps(&value, &mut applications);
    applications
}

#[cfg(target_os = "linux")]
fn parse_wmctrl_apps(output: &str) -> Vec<DetectedApplication> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _window = fields.next()?;
            let _desktop = fields.next()?;
            let pid = fields.next()?.parse::<u32>().ok()?;
            let _host = fields.next()?;
            let title = fields.collect::<Vec<_>>().join(" ");
            linux_app_from_pid(pid, title)
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_foreground_status() -> ForegroundStatus {
    let Some(candidate) = crate::smart_input::native_foreground_app_candidate() else {
        return ForegroundStatus {
            backend: "Windows Win32".to_owned(),
            state: ForegroundState::NoFocusedWindow,
        };
    };
    let executable = candidate.exe;
    ForegroundStatus {
        backend: "Windows Win32".to_owned(),
        state: ForegroundState::Focused(DetectedApplication {
            display_name: executable.clone(),
            identities: vec![executable.clone()],
            executable,
            window_title: candidate.title,
        }),
    }
}

#[cfg(target_os = "windows")]
fn platform_available_apps() -> Vec<DetectedApplication> {
    crate::smart_input::native_open_window_app_candidates()
        .into_iter()
        .map(|candidate| DetectedApplication {
            display_name: candidate.exe.clone(),
            identities: vec![candidate.exe.clone()],
            executable: candidate.exe,
            window_title: candidate.title,
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn macos_ns_string(value: *mut objc::runtime::Object) -> Option<String> {
    use objc::{msg_send, sel, sel_impl};

    if value.is_null() {
        return None;
    }
    unsafe {
        let utf8: *const std::os::raw::c_char = msg_send![value, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(target_os = "macos")]
fn macos_app_is_pickable(activation_policy: isize, include_accessory: bool) -> bool {
    activation_policy == 0 || (include_accessory && activation_policy == 1)
}

#[cfg(target_os = "macos")]
fn macos_detected_application(
    application: *mut objc::runtime::Object,
    include_accessory: bool,
) -> Option<DetectedApplication> {
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        if application.is_null() {
            return None;
        }
        // Layouts only use regular GUI apps. Text expansion also offers
        // accessory/menu-bar apps, but never prohibited/background processes.
        let activation_policy: isize = msg_send![application, activationPolicy];
        if !macos_app_is_pickable(activation_policy, include_accessory) {
            return None;
        }
        let name: *mut objc::runtime::Object = msg_send![application, localizedName];
        let bundle: *mut objc::runtime::Object = msg_send![application, bundleIdentifier];
        let url: *mut objc::runtime::Object = msg_send![application, executableURL];
        let path: *mut objc::runtime::Object = if url.is_null() {
            std::ptr::null_mut()
        } else {
            msg_send![url, path]
        };
        let display_name = macos_ns_string(name).unwrap_or_default();
        let bundle_id = macos_ns_string(bundle).unwrap_or_default();
        let executable_path = macos_ns_string(path).unwrap_or_default();
        let executable = if !bundle_id.is_empty() {
            bundle_id.clone()
        } else if !executable_path.is_empty() {
            executable_path.clone()
        } else {
            display_name.clone()
        };
        (!executable.is_empty()).then(|| DetectedApplication {
            executable: executable.clone(),
            identities: crate::application_layouts::normalized_identity_values(
                [
                    executable.as_str(),
                    bundle_id.as_str(),
                    executable_path.as_str(),
                    display_name.as_str(),
                ]
                .into_iter()
                .filter(|value| !value.is_empty()),
            ),
            display_name,
            window_title: String::new(),
        })
    }
}

#[cfg(target_os = "macos")]
fn macos_foreground_status() -> ForegroundStatus {
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let Some(pool_class) = objc::runtime::Class::get("NSAutoreleasePool") else {
            return ForegroundStatus {
                backend: "macOS NSWorkspace".to_owned(),
                state: ForegroundState::BackendUnavailable(
                    "NSAutoreleasePool is unavailable".to_owned(),
                ),
            };
        };
        let pool: *mut objc::runtime::Object = msg_send![pool_class, new];
        let result = (|| {
            let workspace_class = objc::runtime::Class::get("NSWorkspace")?;
            let workspace: *mut objc::runtime::Object = msg_send![workspace_class, sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            let application: *mut objc::runtime::Object =
                msg_send![workspace, frontmostApplication];
            if application.is_null() {
                return None;
            }
            macos_detected_application(application, false)
        })();
        if !pool.is_null() {
            let _: () = msg_send![pool, drain];
        }
        status_from_application("macOS NSWorkspace", result)
    }
}

#[cfg(target_os = "macos")]
fn platform_available_apps() -> ApplicationScan {
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let Some(pool_class) = objc::runtime::Class::get("NSAutoreleasePool") else {
            return ApplicationScan::default();
        };
        let pool: *mut objc::runtime::Object = msg_send![pool_class, new];
        let applications = (|| {
            let workspace_class = objc::runtime::Class::get("NSWorkspace")?;
            let workspace: *mut objc::runtime::Object = msg_send![workspace_class, sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            let running: *mut objc::runtime::Object = msg_send![workspace, runningApplications];
            if running.is_null() {
                return None;
            }
            let count: usize = msg_send![running, count];
            let mut applications = ApplicationScan::default();
            for index in 0..count {
                let application: *mut objc::runtime::Object =
                    msg_send![running, objectAtIndex: index];
                let policy: isize = msg_send![application, activationPolicy];
                let pid: i32 = msg_send![application, processIdentifier];
                if let Some(detected) = macos_detected_application(application, true) {
                    // Menu-bar apps remain pickable for text expansion, without
                    // adding them to the application-layout catalog.
                    if pid as u32 != std::process::id() {
                        applications
                            .text_expander_names
                            .push(detected.display_name.clone());
                    }
                    if macos_app_is_pickable(policy, false) {
                        applications.available.push(detected);
                    }
                }
            }
            Some(applications)
        })()
        .unwrap_or_default();
        if !pool.is_null() {
            let _: () = msg_send![pool, drain];
        }
        applications
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_application_name_for_pid(pid: u32) -> Option<String> {
    use objc::{msg_send, sel, sel_impl};

    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    unsafe {
        let pool_class = objc::runtime::Class::get("NSAutoreleasePool")?;
        let pool: *mut objc::runtime::Object = msg_send![pool_class, new];
        let result = (|| {
            let application_class = objc::runtime::Class::get("NSRunningApplication")?;
            let application: *mut objc::runtime::Object =
                msg_send![application_class, runningApplicationWithProcessIdentifier: pid];
            if application.is_null() {
                return None;
            }
            let name: *mut objc::runtime::Object = msg_send![application, localizedName];
            macos_ns_string(name)
        })();
        if !pool.is_null() {
            let _: () = msg_send![pool, drain];
        }
        result
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_available_apps() -> Vec<DetectedApplication> {
    Vec::new()
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;

    #[test]
    fn text_expander_includes_accessory_apps_without_changing_layout_picker() {
        assert!(macos_app_is_pickable(0, true));
        assert!(macos_app_is_pickable(1, true));
        assert!(!macos_app_is_pickable(2, true));
        assert!(macos_app_is_pickable(0, false));
        assert!(!macos_app_is_pickable(1, false));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn parses_hyprland_clients() {
        let applications = parse_hyprland_clients(
            r#"[{"pid":0,"class":"com.figma.Desktop","title":"Draft — Figma"}]"#,
        );
        assert_eq!(applications[0].executable, "com.figma.Desktop");
        assert_eq!(applications[0].window_title, "Draft — Figma");
    }

    #[test]
    fn parses_nested_sway_applications() {
        let applications = parse_sway_apps(
            r#"{"nodes":[{"nodes":[{"pid":0,"app_id":"code","name":"main.rs"}]}]}"#,
        );
        assert!(applications
            .iter()
            .any(|application| application.executable == "code"));
    }

    #[test]
    fn parses_wmctrl_process_and_title() {
        let pid = std::process::id();
        let output = format!("0x01 0 {pid} host Entropy test window");
        let applications = parse_wmctrl_apps(&output);
        assert_eq!(applications.len(), 1);
        assert_eq!(applications[0].window_title, "Entropy test window");
    }

    #[test]
    fn parses_visible_desktop_application() {
        let application = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Visual Studio Code\nExec=/usr/share/code/code --unity-launch %F\n",
        )
        .unwrap();
        assert_eq!(application.display_name, "Visual Studio Code");
        assert_eq!(application.executable, "code");
    }

    #[test]
    fn installed_catalog_matches_presets_without_running_windows() {
        let audacity = parse_desktop_entry_with_id(
            "[Desktop Entry]\nType=Application\nName=Audacity\nExec=/usr/bin/audacity %F\n",
            Some("org.audacityteam.Audacity.desktop"),
        )
        .unwrap();
        let found = match_builtin_presets_from_catalog(&[audacity]);
        assert!(found.contains("audacity"));
        assert!(!found.contains("visual_studio_code"));
        assert!(!found.contains("figma"));
    }

    #[test]
    fn desktop_metadata_supplies_generic_runtime_identities() {
        let application = parse_desktop_entry_with_id(
            "[Desktop Entry]\nType=Application\nName=Example Paint\nExec=/opt/example/bin/example-paint %F\nTryExec=example-paint\nStartupWMClass=ExamplePaint\n",
            Some("com.example.Paint.desktop"),
        )
        .unwrap();

        assert!(application
            .identities
            .iter()
            .any(|value| value == "examplepaint"));
        assert!(crate::application_layouts::application_identities_match(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str)),
            ["ExamplePaint"],
        ));
    }

    #[test]
    fn parses_official_snap_ticktick_desktop_entry() {
        let application = parse_desktop_entry_with_id(
            "[Desktop Entry]\nName=TickTick\nExec=ticktick %U\nTerminal=false\nType=Application\nStartupWMClass=TickTick\nCategories=Office;\n",
            Some("ticktick_ticktick.desktop"),
        )
        .expect("official TickTick desktop entry must be visible");

        assert_eq!(application.executable, "ticktick");
        assert!(crate::application_layouts::application_identities_match(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str)),
            ["ticktick_ticktick.desktop", "TickTick"],
        ));
    }

    #[test]
    fn foreground_process_is_enriched_from_installed_launcher_metadata() {
        let mut foreground = DetectedApplication {
            executable: "gnome-terminal-server".to_owned(),
            identities: vec!["gnome-terminal-server".to_owned()],
            display_name: "gnome-terminal-server".to_owned(),
            window_title: "Terminal".to_owned(),
        };
        let catalog = [DetectedApplication {
            executable: "gnome-terminal".to_owned(),
            identities: vec![
                "org.gnome.Terminal.desktop".to_owned(),
                "Gnome-terminal".to_owned(),
            ],
            display_name: "Terminal".to_owned(),
            window_title: String::new(),
        }];

        enrich_application_from_catalog(&mut foreground, &catalog);

        assert_eq!(foreground.display_name, "Terminal");
        assert!(crate::application_layouts::application_identities_match(
            foreground.identities.iter().map(String::as_str),
            ["org.gnome.Terminal"]
        ));
    }

    #[test]
    fn running_choices_merge_windows_and_hide_window_titles() {
        let choices = running_application_choices(&[
            DetectedApplication {
                executable: "telegram-desktop".to_owned(),
                identities: vec!["org.telegram.desktop".to_owned()],
                display_name: "Telegram".to_owned(),
                window_title: "General".to_owned(),
            },
            DetectedApplication {
                executable: "org.telegram.desktop".to_owned(),
                identities: vec!["telegram-desktop".to_owned()],
                display_name: "Telegram".to_owned(),
                window_title: "Ergohaven".to_owned(),
            },
            DetectedApplication {
                executable: "blender".to_owned(),
                identities: vec!["blender".to_owned()],
                display_name: "Blender".to_owned(),
                window_title: "Scene".to_owned(),
            },
        ]);

        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].display_name, "Blender");
        assert_eq!(choices[1].display_name, "Telegram");
        assert!(choices
            .iter()
            .all(|application| application.window_title.is_empty()));
    }

    #[test]
    fn running_choices_include_entropy_itself() {
        let current = "Entropy-linux-x86_64".to_owned();
        let choices = running_application_choices(&[DetectedApplication {
            executable: current.clone(),
            identities: vec![current],
            display_name: "Entropy".to_owned(),
            window_title: "Entropy".to_owned(),
        }]);

        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].display_name, "Entropy");
        assert!(choices[0].window_title.is_empty());
    }

    #[test]
    fn skips_hidden_desktop_application() {
        assert!(parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Hidden helper\nExec=helper\nNoDisplay=true\n",
        )
        .is_none());
    }

    #[test]
    fn linux_picker_hides_helpers_but_keeps_visible_and_portable_apps() {
        let helper = parse_desktop_entry_metadata_with_id(
            "[Desktop Entry]\nType=Application\nName=Background helper\nExec=example-app\nNoDisplay=true\n",
            Some("com.example.App.Helper.desktop"),
        )
        .expect("hidden helper metadata must still be classified")
        .0;
        let visible = parse_desktop_entry_metadata_with_id(
            "[Desktop Entry]\nType=Application\nName=Example App\nExec=example-app\nStartupWMClass=ExampleApp\n",
            Some("com.example.App.desktop"),
        )
        .expect("visible launcher metadata must be classified")
        .0;
        let running = DetectedApplication {
            executable: "example-app".to_owned(),
            identities: vec!["ExampleApp".to_owned()],
            display_name: "Example App".to_owned(),
            window_title: "Document".to_owned(),
        };

        assert!(!linux_application_is_user_facing_with_catalog(
            &running,
            &LinuxApplicationCatalog {
                visible: Vec::new(),
                hidden: vec![helper.clone()],
            },
        ));
        assert!(linux_application_is_user_facing_with_catalog(
            &running,
            &LinuxApplicationCatalog {
                visible: vec![visible],
                hidden: vec![helper],
            },
        ));
        assert!(linux_application_is_user_facing_with_catalog(
            &DetectedApplication {
                executable: "Entropy-linux-x86_64".to_owned(),
                identities: vec!["Entropy-linux-x86_64".to_owned()],
                display_name: "Entropy".to_owned(),
                window_title: "Entropy".to_owned(),
            },
            &LinuxApplicationCatalog::default(),
        ));
    }

    #[test]
    fn x11_picker_accepts_app_windows_not_shell_surfaces() {
        assert!(LINUX_X11_USER_WINDOW_TYPES.contains(&"_NET_WM_WINDOW_TYPE_NORMAL"));
        assert!(LINUX_X11_USER_WINDOW_TYPES.contains(&"_NET_WM_WINDOW_TYPE_DIALOG"));
        for system_surface in [
            "_NET_WM_WINDOW_TYPE_DESKTOP",
            "_NET_WM_WINDOW_TYPE_DOCK",
            "_NET_WM_WINDOW_TYPE_MENU",
            "_NET_WM_WINDOW_TYPE_NOTIFICATION",
            "_NET_WM_WINDOW_TYPE_TOOLTIP",
        ] {
            assert!(!LINUX_X11_USER_WINDOW_TYPES.contains(&system_surface));
        }
    }

    #[test]
    fn parses_flatpak_and_quoted_desktop_commands() {
        assert_eq!(
            desktop_exec_executable("flatpak run --branch=stable org.telegram.desktop %u")
                .as_deref(),
            Some("org.telegram.desktop")
        );
        assert_eq!(
            desktop_exec_executable("\"/opt/arduino ide/arduino-ide\" %F").as_deref(),
            Some("arduino-ide")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn recognizes_ubuntu_gnome_wayland_without_mistaking_x11_or_kde() {
        assert!(linux_gnome_wayland_session_for(
            "ubuntu:GNOME",
            true,
            Some("wayland")
        ));
        assert!(linux_gnome_wayland_session_for(
            "GNOME",
            false,
            Some("Wayland")
        ));
        assert!(!linux_gnome_wayland_session_for(
            "ubuntu:GNOME",
            true,
            Some("x11")
        ));
        assert!(!linux_gnome_wayland_session_for(
            "KDE",
            true,
            Some("wayland")
        ));
        assert!(!linux_gnome_wayland_session_for(
            "KDE:GNOME",
            true,
            Some("wayland")
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bundled_gnome_metadata_matches_the_bridge_protocol() {
        let metadata = gnome_extension_metadata(46);
        assert_eq!(
            metadata.get("version").and_then(serde_json::Value::as_u64),
            Some(u64::from(GNOME_EXTENSION_PROTOCOL_VERSION))
        );
        assert_eq!(
            metadata
                .get("shell-version")
                .and_then(serde_json::Value::as_array)
                .and_then(|versions| versions.first())
                .and_then(serde_json::Value::as_str),
            Some("46")
        );
    }

    #[test]
    fn explicit_session_type_wins_over_stale_display_variables() {
        assert_eq!(
            linux_foreground_backend_for("ubuntu:GNOME", false, false, true, true, Some("x11")),
            LinuxForegroundBackend::X11
        );
        assert_eq!(
            linux_foreground_backend_for("ubuntu:GNOME", false, false, true, true, Some("wayland")),
            LinuxForegroundBackend::GnomeWayland
        );
        assert_eq!(
            linux_foreground_backend_for("ubuntu:GNOME", true, true, true, true, Some("x11")),
            LinuxForegroundBackend::X11
        );
    }

    #[test]
    fn infers_session_only_when_xdg_session_type_is_absent() {
        assert_eq!(
            linux_foreground_backend_for("GNOME", false, false, true, true, None),
            LinuxForegroundBackend::GnomeWayland
        );
        assert_eq!(
            linux_foreground_backend_for("GNOME", false, false, false, true, None),
            LinuxForegroundBackend::X11
        );
        assert_eq!(
            linux_foreground_backend_for("KDE", false, false, true, false, None),
            LinuxForegroundBackend::KdeWayland
        );
    }

    #[test]
    fn selects_compositor_adapter_only_inside_wayland_session() {
        assert_eq!(
            linux_foreground_backend_for("", true, false, true, false, Some("wayland")),
            LinuxForegroundBackend::Hyprland
        );
        assert_eq!(
            linux_foreground_backend_for("", false, true, true, false, Some("wayland")),
            LinuxForegroundBackend::Sway
        );
        assert_eq!(
            linux_foreground_backend_for("unknown", false, false, true, false, Some("wayland")),
            LinuxForegroundBackend::UnsupportedWayland
        );
        assert_eq!(
            linux_foreground_backend_for("", false, false, false, false, None),
            LinuxForegroundBackend::Unavailable
        );
    }

    #[test]
    fn legacy_x11_selection_cases_remain_supported() {
        assert_eq!(
            linux_foreground_backend_for("", false, false, false, true, Some("x11")),
            LinuxForegroundBackend::X11
        );
        assert_ne!(
            linux_foreground_backend_for("", false, false, true, false, Some("wayland")),
            LinuxForegroundBackend::X11
        );
        assert_eq!(
            linux_foreground_backend_for("", true, false, false, true, Some("x11")),
            LinuxForegroundBackend::X11
        );
        assert_eq!(
            linux_foreground_backend_for("", false, true, false, true, Some("x11")),
            LinuxForegroundBackend::X11
        );
    }

    #[test]
    fn detector_states_do_not_conflate_no_window_with_backend_failure() {
        let no_window = status_from_application("test backend", None);
        assert_eq!(no_window.backend, "test backend");
        assert!(matches!(no_window.state, ForegroundState::NoFocusedWindow));

        let unavailable = ForegroundStatus {
            backend: "test backend".to_owned(),
            state: ForegroundState::BackendUnavailable("offline".to_owned()),
        };
        assert_ne!(no_window, unavailable);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_shell_bridge_preserves_stable_window_identities() {
        let state = gnome_shell_application(
            "org.telegram.desktop".to_owned(),
            "TelegramDesktop".to_owned(),
            "Telegram".to_owned(),
            0,
        );
        let ForegroundState::Focused(application) = state else {
            panic!("GNOME Shell bridge did not return a focused application");
        };
        assert_eq!(application.window_title, "Telegram");
        assert!(crate::application_layouts::application_identities_match(
            application.identities.iter().map(String::as_str),
            ["org.telegram.desktop", "TelegramDesktop"]
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_shell_open_windows_include_apps_that_are_not_focused() {
        let applications = gnome_shell_applications_from_payload(vec![
            (
                "org.telegram.desktop".to_owned(),
                "TelegramDesktop".to_owned(),
                "Telegram".to_owned(),
                0,
            ),
            (
                "org.gnome.Nautilus".to_owned(),
                "org.gnome.Nautilus".to_owned(),
                "Downloads".to_owned(),
                0,
            ),
        ]);
        assert_eq!(applications.len(), 2);
        assert!(applications.iter().any(|application| {
            crate::application_layouts::application_identities_match(
                application.identities.iter().map(String::as_str),
                ["org.telegram.desktop", "TelegramDesktop"],
            )
        }));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bundled_gnome_extensions_use_shell_focus_not_accessibility() {
        for source in [
            include_str!("../assets/gnome-shell-extension/extension-modern.js"),
            include_str!("../assets/gnome-shell-extension/extension-legacy.js"),
        ] {
            assert!(source.contains("global.display.focus_window"));
            assert!(source.contains("GetActiveWindow"));
            assert!(source.contains("GetProtocolVersion"));
            assert!(source.contains("GetOpenWindows"));
            assert!(source.contains("global.get_window_actors()"));
            assert!(source.contains("ActiveWindowChanged"));
            assert!(source.contains("notify::focus-window"));
            assert!(source.contains("notify::focus-app"));
            assert!(source.contains("GLib.idle_add"));
            assert!(source.contains("notify::title"));
            assert!(source.contains("return 4"));
            assert!(!source.to_ascii_lowercase().contains("at-spi"));
            assert!(!source.to_ascii_lowercase().contains("accessible"));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_gsettings_enabled_extension_arrays() {
        assert_eq!(
            parse_gsettings_string_array("['first@example.com', 'second@example.com']"),
            ["first@example.com", "second@example.com"]
        );
        assert!(parse_gsettings_string_array("@as []").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "installs the per-user GNOME Shell integration"]
    fn live_install_gnome_shell_integration() {
        eprintln!(
            "{}",
            install_gnome_shell_integration().expect("GNOME integration install failed")
        );
    }

    #[test]
    #[ignore = "requires a live X11 desktop"]
    fn live_x11_backend_opens_without_external_window_tools() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let status = match LinuxX11Applications::open() {
            Some(reader) => ForegroundStatus {
                backend: "Linux X11".to_owned(),
                state: reader.active_status(),
            },
            None => panic!("could not open DISPLAY"),
        };
        eprintln!("live X11 status: {status:?}");
        assert!(!matches!(
            status.state,
            ForegroundState::BackendUnavailable(_)
        ));
    }

    #[test]
    #[ignore = "requires an isolated live X11 display"]
    fn live_x11_backend_receives_root_property_events() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let reader = LinuxX11Applications::open().expect("could not open reader DISPLAY");
        let writer = LinuxX11Applications::open().expect("could not open writer DISPLAY");
        reader
            .subscribe_foreground_events()
            .expect("could not subscribe to root events");

        let root = unsafe { (writer.xlib.XDefaultRootWindow)(writer.display) };
        let property_name = CString::new("_ENTROPY_FOREGROUND_MONITOR_TEST").unwrap();
        let property = unsafe {
            (writer.xlib.XInternAtom)(writer.display, property_name.as_ptr(), x11_dl::xlib::False)
        };
        assert_ne!(property, 0, "could not intern test atom");
        let value: std::os::raw::c_ulong = 1;
        unsafe {
            (writer.xlib.XChangeProperty)(
                writer.display,
                root,
                property,
                x11_dl::xlib::XA_CARDINAL,
                32,
                x11_dl::xlib::PropModeReplace,
                (&value as *const std::os::raw::c_ulong).cast(),
                1,
            );
            (writer.xlib.XFlush)(writer.display);
        }

        assert!(reader
            .wait_for_foreground_event(1_000)
            .expect("X11 event wait failed"));
        unsafe {
            (writer.xlib.XDeleteProperty)(writer.display, root, property);
            (writer.xlib.XFlush)(writer.display);
        }
    }
}

/// System installation scan, separate from the running-window picker. An error
/// means detection unavailable, never "not installed".
pub(crate) fn installed_builtin_presets() -> Result<std::collections::BTreeSet<String>, String> {
    static INSTALLED: OnceLock<Result<std::collections::BTreeSet<String>, String>> =
        OnceLock::new();
    INSTALLED
        .get_or_init(scan_installed_builtin_presets)
        .clone()
}

#[cfg(target_os = "linux")]
fn scan_installed_builtin_presets() -> Result<std::collections::BTreeSet<String>, String> {
    let mut roots = Vec::new();
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(data_home));
    } else if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share"));
    }
    roots.extend(std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    ));
    if !roots
        .iter()
        .any(|root| std::fs::read_dir(root.join("applications")).is_ok())
    {
        return Err("No readable desktop application catalog".to_owned());
    }
    Ok(match_builtin_presets_from_catalog(&linux_installed_apps()))
}

#[cfg(target_os = "linux")]
fn match_builtin_presets_from_catalog(
    catalog: &[DetectedApplication],
) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    for preset in crate::application_layouts::builtin_application_layout_presets() {
        let matches = catalog.iter().any(|application| {
            crate::application_layouts::executables_match(
                preset.executable,
                &application.executable,
            ) || crate::application_layouts::application_identities_match(
                std::iter::once(preset.executable).chain(preset.identities.iter().copied()),
                std::iter::once(application.executable.as_str())
                    .chain(application.identities.iter().map(String::as_str)),
            )
        });
        if matches {
            found.insert(preset.id.to_owned());
        }
    }
    found
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn scan_installed_builtin_presets() -> Result<std::collections::BTreeSet<String>, String> {
    use std::path::{Path, PathBuf};
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(key) {
                roots.push(PathBuf::from(root));
            }
        }
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(root).join("Programs"));
        }
        for key in ["APPDATA", "PROGRAMDATA"] {
            if let Some(root) = std::env::var_os(key) {
                roots.push(PathBuf::from(root).join("Microsoft/Windows/Start Menu/Programs"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Applications"));
        roots.push(PathBuf::from("/System/Applications"));
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
    }
    let mut names = std::collections::BTreeSet::new();
    let mut readable = false;
    fn collect(
        dir: &Path,
        depth: usize,
        names: &mut std::collections::BTreeSet<String>,
        readable: &mut bool,
    ) {
        if depth > 5 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        *readable = true;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let stem = Path::new(&name)
                .file_stem()
                .and_then(|part| part.to_str())
                .unwrap_or("");
            let ext = path
                .extension()
                .and_then(|part| part.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "exe" | "lnk" | "app") {
                names.insert(
                    stem.chars()
                        .filter(|ch| ch.is_ascii_alphanumeric())
                        .collect(),
                );
            }
            if path.is_dir() && ext != "app" {
                collect(&path, depth + 1, names, readable);
            }
        }
    }
    for root in roots {
        collect(&root, 0, &mut names, &mut readable);
    }
    if !readable {
        return Err("No readable system application directories".to_owned());
    }
    let mut found = std::collections::BTreeSet::new();
    for preset in crate::application_layouts::builtin_application_layout_presets() {
        let aliases: &[&str] = match preset.id {
            "obs_studio" => &["obs", "obs64", "obsstudio"],
            "visual_studio_code" => &["code", "visualstudiocode"],
            "blender" => &["blender"],
            "figma" => &["figma"],
            "adobe_photoshop" => &["photoshop", "adobephotoshop"],
            "audacity" => &["audacity"],
            "firefox" => &["firefox", "mozillafirefox"],
            "google_chrome" => &["chrome", "googlechrome"],
            "adobe_premiere_pro" => &["adobepremierepro", "premierepro"],
            "adobe_illustrator" => &["illustrator", "adobeillustrator"],
            "visual_studio" => &["devenv", "microsoftvisualstudio"],
            "intellij_idea" => &["idea64", "intellijidea"],
            "pycharm" => &["pycharm64", "pycharm"],
            "discord" => &["discord"],
            "streamlabs_desktop" => &["streamlabsdesktop", "streamlabsobs", "slobs"],
            _ => &[],
        };
        let versioned_name = match preset.id {
            "adobe_photoshop" => Some("adobephotoshop"),
            "adobe_premiere_pro" => Some("adobepremierepro"),
            "adobe_illustrator" => Some("adobeillustrator"),
            "visual_studio" => Some("microsoftvisualstudio"),
            "intellij_idea" => Some("intellijidea"),
            "pycharm" => Some("pycharm"),
            _ => None,
        };
        if aliases.iter().any(|alias| names.contains(*alias))
            || versioned_name.is_some_and(|prefix| {
                names.iter().any(|name| {
                    name.starts_with(prefix)
                        && name[prefix.len()..].chars().all(|ch| ch.is_ascii_digit())
                })
            })
        {
            found.insert(preset.id.to_owned());
        }
    }
    Ok(found)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn scan_installed_builtin_presets() -> Result<std::collections::BTreeSet<String>, String> {
    Err("System application catalog unsupported on this platform".to_owned())
}
