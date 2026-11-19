//! Keyboard layout currently selected in the desktop session.
//!
//! The Layout Indicator can follow it, so its legends match what the host
//! actually types: Latin letters in an English layout, Cyrillic in a Russian
//! one. Only Hyprland is supported for now; it reports layout switches over
//! its IPC event socket. Elsewhere the layout stays unknown and the indicator
//! keeps using the "Key legends" setting.

use crate::keycode::KeyOutputLayout;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

const UNKNOWN: u8 = 0;
const ENGLISH: u8 = 1;
const RUSSIAN: u8 = 2;

fn encode(layout: Option<KeyOutputLayout>) -> u8 {
    match layout {
        None => UNKNOWN,
        Some(KeyOutputLayout::English) => ENGLISH,
        Some(KeyOutputLayout::Russian) => RUSSIAN,
    }
}

fn decode(value: u8) -> Option<KeyOutputLayout> {
    match value {
        ENGLISH => Some(KeyOutputLayout::English),
        RUSSIAN => Some(KeyOutputLayout::Russian),
        _ => None,
    }
}

/// Maps an XKB layout description ("English (US)", "Russian (phonetic)") to
/// the output layout whose legends the indicator can draw. Any layout other
/// than Russian types Latin letters on the alpha keys, so it maps to English.
pub(crate) fn output_layout_for_layout_name(name: &str) -> KeyOutputLayout {
    if name.trim_start().starts_with("Russian") {
        KeyOutputLayout::Russian
    } else {
        KeyOutputLayout::English
    }
}

/// Layout name from a Hyprland `activelayout>>KEYBOARD,LAYOUT` event line.
/// Keyboard names never contain commas, layout descriptions may.
pub(crate) fn hyprland_active_layout_from_event(line: &str) -> Option<&str> {
    let payload = line.trim_end().strip_prefix("activelayout>>")?;
    let (_, layout) = payload.split_once(',')?;
    (!layout.is_empty()).then_some(layout)
}

/// Active keymap of the main keyboard in Hyprland's `j/devices` reply.
pub(crate) fn hyprland_main_keymap_from_devices(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let keyboards = value.get("keyboards")?.as_array()?;
    let keymap = |keyboard: &serde_json::Value| {
        keyboard
            .get("active_keymap")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    keyboards
        .iter()
        .find(|keyboard| keyboard.get("main").and_then(serde_json::Value::as_bool) == Some(true))
        .and_then(keymap)
        .or_else(|| keyboards.first().and_then(keymap))
}

/// Watches the host layout on a background thread once started.
#[derive(Default)]
pub(crate) struct HostKeyboardLayoutWatcher {
    state: Arc<AtomicU8>,
    started: bool,
}

impl HostKeyboardLayoutWatcher {
    /// The current host layout, or `None` when it cannot be detected.
    pub(crate) fn current(&self) -> Option<KeyOutputLayout> {
        decode(self.state.load(Ordering::Relaxed))
    }

    /// Starts watching on first call; later calls do nothing. Each change
    /// repaints the root viewport and `viewport`, so a window showing
    /// legends redraws them.
    pub(crate) fn ensure_started(&mut self, ctx: &egui::Context, viewport: egui::ViewportId) {
        if self.started {
            return;
        }
        self.started = true;
        #[cfg(target_os = "linux")]
        hyprland::spawn(self.state.clone(), ctx.clone(), viewport);
        #[cfg(not(target_os = "linux"))]
        let _ = (ctx, viewport);
    }
}

#[cfg(target_os = "linux")]
mod hyprland {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::Duration;

    const RECONNECT_DELAY: Duration = Duration::from_secs(5);

    fn socket_dir() -> Option<PathBuf> {
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        Some(runtime_dir.join("hypr").join(signature))
    }

    fn query_main_keymap(dir: &std::path::Path) -> Option<String> {
        let mut stream = UnixStream::connect(dir.join(".socket.sock")).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
        stream.write_all(b"j/devices").ok()?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply).ok()?;
        hyprland_main_keymap_from_devices(&reply)
    }

    fn repaint(ctx: &egui::Context, viewport: egui::ViewportId) {
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        ctx.request_repaint_of(viewport);
    }

    fn store(
        state: &AtomicU8,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        layout: Option<KeyOutputLayout>,
    ) {
        let value = encode(layout);
        if state.swap(value, Ordering::Relaxed) != value {
            repaint(ctx, viewport);
        }
    }

    pub(super) fn spawn(state: Arc<AtomicU8>, ctx: egui::Context, viewport: egui::ViewportId) {
        let Some(dir) = socket_dir() else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("hyprland-layout-watcher".to_owned())
            .spawn(move || loop {
                if let Ok(events) = UnixStream::connect(dir.join(".socket2.sock")) {
                    if let Some(keymap) = query_main_keymap(&dir) {
                        let layout = output_layout_for_layout_name(&keymap);
                        store(&state, &ctx, viewport, Some(layout));
                    }
                    for line in BufReader::new(events).lines() {
                        let Ok(line) = line else {
                            break;
                        };
                        if let Some(layout) = hyprland_active_layout_from_event(&line) {
                            let layout = output_layout_for_layout_name(layout);
                            store(&state, &ctx, viewport, Some(layout));
                        }
                    }
                }
                // Hyprland restarted or the socket is not up yet: forget the
                // stale layout and try again.
                store(&state, &ctx, viewport, None);
                if !dir.exists() {
                    return;
                }
                std::thread::sleep(RECONNECT_DELAY);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_names_map_to_output_layouts() {
        assert_eq!(
            output_layout_for_layout_name("Russian"),
            KeyOutputLayout::Russian
        );
        assert_eq!(
            output_layout_for_layout_name("Russian (phonetic)"),
            KeyOutputLayout::Russian
        );
        assert_eq!(
            output_layout_for_layout_name("English (US)"),
            KeyOutputLayout::English
        );
        assert_eq!(
            output_layout_for_layout_name("German"),
            KeyOutputLayout::English
        );
    }

    #[test]
    fn activelayout_events_yield_the_layout_name() {
        assert_eq!(
            hyprland_active_layout_from_event("activelayout>>ergohaven-imperial44-v1/v2,Russian\n"),
            Some("Russian")
        );
        assert_eq!(
            hyprland_active_layout_from_event(
                "activelayout>>at-keyboard,English (US, intl., with dead keys)"
            ),
            Some("English (US, intl., with dead keys)")
        );
        assert_eq!(hyprland_active_layout_from_event("workspace>>2"), None);
        assert_eq!(hyprland_active_layout_from_event("activelayout>>kbd,"), None);
    }

    #[test]
    fn devices_reply_prefers_the_main_keyboard() {
        let json = r#"{"keyboards":[
            {"name":"other","active_keymap":"English (US)","main":false},
            {"name":"imperial44","active_keymap":"Russian","main":true}
        ]}"#;
        assert_eq!(
            hyprland_main_keymap_from_devices(json).as_deref(),
            Some("Russian")
        );
        let no_main = r#"{"keyboards":[{"name":"a","active_keymap":"English (US)"}]}"#;
        assert_eq!(
            hyprland_main_keymap_from_devices(no_main).as_deref(),
            Some("English (US)")
        );
        assert_eq!(hyprland_main_keymap_from_devices("not json"), None);
    }

    #[test]
    fn state_round_trips() {
        for layout in [
            None,
            Some(KeyOutputLayout::English),
            Some(KeyOutputLayout::Russian),
        ] {
            assert_eq!(decode(encode(layout)), layout);
        }
    }
}
