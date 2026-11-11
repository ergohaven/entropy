//! Local test-only UI diagnostic; never opens a HID endpoint or changes settings.
use super::*;
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[path = "sm62_keymap_trace.rs"]
mod keymap_trace;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    Preparing(Instant),
    ArmPeer,
    ArmMaster,
    Measuring(Instant),
    Reading(u8, u8),
    Finished,
}

pub(super) struct Session {
    pub(super) phase: Phase,
    device: crate::device::Device,
    generation: u64,
    expires: Instant,
    owner: crate::hid::SharedHidOutput,
    pub(super) sequence: u8,
    pub(super) pending: Option<[u8; 32]>,
    pub(super) cancelled: Arc<AtomicBool>,
    sides: [u8; 2],
    words: [[u32; 48]; 2],
    // Separate protocol and storage: never interpret a single frame as wire-v3 FPS.
    trace: Option<Box<[u32; 792]>>,
    path: std::path::PathBuf,
    file: std::fs::File,
    message: String,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Session {
    fn record(&mut self, text: &str) -> std::io::Result<()> {
        writeln!(self.file, "{text}")?;
        self.file.flush()
    }
    fn stop(&mut self, reason: &str) {
        self.cancelled.store(true, Ordering::Release);
        self.phase = Phase::Finished;
        self.message = reason.into();
        if let Err(e) = self.record(&format!("RESULT: {reason}")) {
            self.message = format!("{reason}; log write failed: {e}");
        }
    }
    fn matches(&self, app: &EntropyApp) -> bool {
        self.generation == app.connection_generation
            && app
                .selected_device
                .and_then(|i| app.device_manager.devices().get(i))
                .is_some_and(|d| {
                    d.path == self.device.path
                        && d.instance_token == self.device.instance_token
                        && d.permits_hid_target(&self.device)
                })
            && app
                .shared_hid_output
                .as_ref()
                .is_none_or(|o| self.owner.shares_owner_with(o))
            && app.hid_device.as_ref().is_none_or(|h| {
                h.shared_output()
                    .is_some_and(|o| self.owner.shares_owner_with(&o))
            })
            && self.owner.is_available()
    }
}

fn request(command: u8, target: u8, seq: u8, page: u8) -> [u8; 32] {
    let mut q = [0; 32];
    q[..8].copy_from_slice(&[0xd7, 0x62, 3, command, target, seq, page, 0]);
    q
}
fn valid_snapshot(w: &[u32; 48]) -> bool {
    w[2] == 10000
        && w[1].wrapping_sub(w[0]) == 10000
        && (10000..60000).contains(&w[37].wrapping_sub(w[0]))
        && w[3] == 3
        && w[5] == 3
        && w[7] == 8
        && [8, 9, 10, 12, 33, 40].iter().all(|&i| w[i] == 0)
        && w[11] >= 3
        && w[13] >= 2
        && w[16..24].iter().map(|&n| n as u64).sum::<u64>() == w[13] as u64
}
fn snapshot_text(target: usize, side: u8, w: &[u32; 48]) -> String {
    let mut text=format!("\nOWNER={} side={} (1=LEFT,2=RIGHT)\nlocal arm={} deadline={} publication={} window={}ms\nactual screen start={} end={} mask={:#X}; configured mode start={} end={} (mode is NOT actual screen; OFF=4)\nframes clock/player/keymap/ORBIT/off={}/{}/{}/{}/{}\nORBIT intervals n={} sum={} max={}ms; first={} last={}\nhistogram <=16/33/50/66/100/150/250/>250={:?}\n",
        if target==0 {"MASTER/USB"} else {"PEER/no USB"},side,w[0],w[1],w[37],w[2],w[3],w[5],w[7],w[4],w[6],w[8],w[9],w[10],w[11],w[12],w[13],w[14],w[15],w[38],w[39],&w[16..24]);
    for (i, name) in [
        (24, "app split task"),
        (27, "app image capture"),
        (30, "main-loop gap"),
        (40, "diagnostic RPC"),
    ] {
        text.push_str(&format!(
            "{name}: n={} sum={} mean={:.3} max={}ms\n",
            w[i],
            w[i + 1],
            if w[i] > 0 {
                w[i + 1] as f64 / w[i] as f64
            } else {
                0.0
            },
            w[i + 2]
        ));
    }
    text.push_str(&format!(
        "diagnostic requests in window={} transitions={} crossing deadline={}\nRaw words: {w:?}\n",
        w[33], w[34], w[35]
    ));
    // Never label a non-ORBIT/partial/contaminated capture as an FPS measurement.
    if valid_snapshot(w) {
        text.push_str(&format!(
            "ORBIT {:.3} completed frames/s; mean interval {:.3}ms\n",
            w[11] as f64 * 1000.0 / w[2] as f64,
            w[14] as f64 / w[13] as f64
        ));
    } else {
        text.push_str("INVALID capture: no FPS calculated (not complete clean ORBIT).\n");
    }
    text
}

impl EntropyApp {
    pub(super) fn sm62_fps_eligible(&self) -> bool {
        self.firmware == FirmwareProtocol::Vial
            && self.layout.is_some()
            && self
                .selected_device
                .and_then(|i| self.device_manager.devices().get(i))
                .is_some_and(|d| {
                    d.vendor_id == 0xe126
                        && d.product_id == 0x00b2
                        && d.firmware == FirmwareProtocol::Vial
                        && !d.bus_type.to_ascii_lowercase().contains("bluetooth")
                })
            && self
                .hid_device
                .as_ref()
                .is_some_and(|h| h.shared_output().is_some_and(|o| o.is_available()))
            && self.pending_device_connect.is_none()
            && self.deferred_full_layout_action.is_none()
            && !self.hid_write_task_active()
            && !self.hid_write_lifecycle_busy()
            && !self.unlock_open
            && !self.vial_unlock_polling
            && !self.vial_unlock_session_started
            && self.vial_unlock_event_pause.is_none()
            && !matches!(
                self.connect_state,
                ConnectState::Loading { .. } | ConnectState::Reconnecting(_)
            )
    }
    pub(super) fn begin_sm62_fps(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let path = crate::diagnostics::active_log_path().with_file_name(format!(
            "SM62-FPS-{}-{:09}.log",
            now.as_secs(),
            now.subsec_nanos()
        ));
        self.begin_sm62_fps_to(path);
    }
    fn begin_sm62_fps_to(&mut self, path: std::path::PathBuf) {
        self.begin_sm62_capture_to(path, false);
    }
    fn begin_sm62_trace(&mut self) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let path = crate::diagnostics::active_log_path().with_file_name(format!("SM62-KEYMAP-TRACE-{}-{:09}.log", now.as_secs(), now.subsec_nanos()));
        self.begin_sm62_capture_to(path, true);
    }
    fn begin_sm62_capture_to(&mut self, path: std::path::PathBuf, trace: bool) {
        if self
            .sm62_fps
            .as_ref()
            .is_some_and(|s| s.phase != Phase::Finished)
            || !self.sm62_fps_eligible()
        {
            self.status_msg = "SM62 FPS: устройство не готово / device not ready".into();
            return;
        }
        let device = self.device_manager.devices()[self.selected_device.unwrap()].clone();
        let owner = self.hid_device.as_ref().unwrap().shared_output().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) => {
                self.status_msg = format!("SM62 FPS: log creation failed: {e}");
                return;
            }
        };
        let mut session = Session {
            phase: Phase::Preparing(Instant::now() + Duration::from_secs(5)),
            device,
            generation: self.connection_generation,
            expires: Instant::now() + Duration::from_secs(120),
            owner,
            sequence: ((now.subsec_nanos() % 255) + 1) as u8,
            pending: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            sides: [0; 2],
            words: [[0; 48]; 2],
            trace: trace.then(|| Box::new([0; 792])),
            path,
            file,
            message: String::new(),
        };
        let header=format!("Entropy {} integrated SM62 FPS v1 / wire v3 / collector v3.1 semantics\nRAM only; firmware unchanged. Existing exclusive HID owner.\nVID={:04X} PID={:04X} path={} instance={} generation={} sequence={}\nPreparation 5s. No SM62 keypress needed. Wait for ORBIT on BOTH screens.\nNormal E6/event feedback/layout/media/application host traffic remains enabled during the measurement.\nIndependent local clocks do not prove simultaneity.\n",env!("CARGO_PKG_VERSION"),session.device.vendor_id,session.device.product_id,session.device.path,session.device.instance_token,session.generation,session.sequence);
        let header = if trace { format!("Entropy {} KEYMAP single-frame trace / wire 0x19 / not FPS\nExisting exclusive owner; firmware unchanged; USB half only.\nVID={:04X} PID={:04X} path={} instance={} generation={} sequence={}\nARM waits for first actual KEYMAP; press after 5s preparation, wait 6s without polls then read 132 frozen pages. Normal host traffic remains enabled, not proof of on-wire rate.\nScene IDs are render-owner actual KEYMAP at both frame boundaries; source completion IDs are words17/18. ORBIT before the keypress is ignored.\nSPI success is firmware API completion, NOT physical DMA-error integrity certification.\n", env!("CARGO_PKG_VERSION"),session.device.vendor_id,session.device.product_id,session.device.path,session.device.instance_token,session.generation,session.sequence) } else { header };
        if let Err(e) = session.record(&header) {
            session.stop(&format!("ERROR log write: {e}"));
        }
        self.status_msg = format!("SM62 FPS: {}", session.path.display());
        self.sm62_fps = Some(session);
    }
    pub(super) fn cancel_sm62_fps(&mut self, reason: &str) {
        if let Some(s) = self
            .sm62_fps
            .as_mut()
            .filter(|s| s.phase != Phase::Finished)
        {
            s.stop(reason);
            self.status_msg = format!("SM62 FPS: {} — {}", s.message, s.path.display());
        }
    }
    pub(super) fn tick_sm62_fps(&mut self, ctx: &egui::Context) {
        let Some(s) = self
            .sm62_fps
            .as_ref()
            .filter(|s| s.phase != Phase::Finished)
        else {
            return;
        };
        if Instant::now() >= s.expires {
            self.cancel_sm62_fps("ERROR: overall 120s measurement deadline expired");
            return;
        }
        if !s.matches(self)
            || self.unlock_open
            || self.vial_unlock_polling
            || self.vial_unlock_session_started
        {
            self.cancel_sm62_fps(
                "CANCELLED: device/owner/generation changed or physical unlock active",
            );
            return;
        }
        if s.pending.is_some() {
            return;
        }
        let now = Instant::now();
        let phase = s.phase;
        if let Phase::Preparing(due) | Phase::Measuring(due) = phase {
            if now < due {
                ctx.request_repaint_after(
                    due.saturating_duration_since(now)
                        .min(Duration::from_millis(100)),
                );
                return;
            }
        }
        let s = self.sm62_fps.as_mut().unwrap();
        s.phase = match phase {
            Phase::Preparing(_) => if s.trace.is_some() { Phase::ArmMaster } else { Phase::ArmPeer },
            Phase::Measuring(_) => Phase::Reading(0, 0),
            other => other,
        };
        let mut q = match s.phase {
            Phase::ArmPeer => request(1, 1, s.sequence, 0),
            Phase::ArmMaster => request(1, 0, s.sequence, 0),
            Phase::Reading(t, p) => request(2, t, s.sequence, p),
            _ => return,
        };
        if s.trace.is_some() { q[2] = 0x19; }
        let cancel = s.cancelled.clone();
        // Existing owner worker borrows/pauses only for this real ARM/GET exchange.
        // No worker, lock or event pause exists during the 11.5-second idle window.
        match self
            .start_vial_hid_operation(ctx, vial_hid_task::VialHidOperation::Sm62Fps { q, cancel })
        {
            vial_hid_task::VialHidTaskStart::Started => {
                self.sm62_fps.as_mut().unwrap().pending = Some(q)
            }
            vial_hid_task::VialHidTaskStart::Busy => {
                ctx.request_repaint_after(Duration::from_millis(50))
            }
            vial_hid_task::VialHidTaskStart::NoDevice => {
                self.cancel_sm62_fps("ERROR: device disconnected")
            }
        }
    }
    pub(super) fn finish_sm62_fps(&mut self, q: [u8; 32], reply: Result<[u8; 32], String>) {
        let Some(s) = self.sm62_fps.as_ref() else {
            return;
        };
        if s.phase == Phase::Finished || s.pending != Some(q) {
            return;
        }
        if !s.matches(self) {
            let reason = reply
                .as_ref()
                .err()
                .map(|e| format!("ERROR: {e}; device/owner no longer current"))
                .unwrap_or_else(|| "CANCELLED: stale worker result / owner changed".into());
            self.cancel_sm62_fps(&reason);
            return;
        }
        let s = self.sm62_fps.as_mut().unwrap();
        s.pending = None;
        let r = match reply {
            Ok(r) => r,
            Err(e) => {
                s.stop(&format!("ERROR: q={q:02X?}: {e}"));
                return;
            }
        };
        if let Err(e) = s.record(&format!("q={q:02X?}\nr={r:02X?}")) {
            s.stop(&format!("ERROR log write: {e}"));
            return;
        }
        if s.trace.is_some() {
            s.finish_trace(q, r);
            self.status_msg = format!("SM62 KEYMAP trace: {} — {}", s.message, s.path.display());
            return;
        }
        if crate::hid::HidDevice::sm62_fps_unsupported(&q, &r) {
            let message = application_layout_runtime::app_layout_text(
                self.app_settings.language,
                "Текущая прошивка не поддерживает этот замер FPS. Измерение не выполнено.",
                "Current firmware does not support this FPS measurement. No measurement was taken.",
            );
            s.stop(message);
            self.status_msg = format!("SM62 FPS: {} — {}", s.message, s.path.display());
            return;
        }
        let role = if q[4] == 0 { 1 } else { 2 };
        if r[..4] != q[..4]
            || r[4] != q[5]
            || r[5] != q[6]
            || r[6] >> 4 != role
            || !(1..=2).contains(&(r[6] & 15))
            || r[7] != q[3]
        {
            s.stop("ERROR: invalid command/version/sequence/page/role/side/status");
            return;
        }
        let target = q[4] as usize;
        let side = r[6] & 15;
        if q[3] == 1 {
            s.sides[target] = side;
            if target == 1 {
                s.phase = Phase::ArmMaster;
            } else if s.sides[0] == s.sides[1] {
                s.stop("ERROR: equal physical sides");
            } else {
                s.phase = Phase::Measuring(Instant::now() + Duration::from_millis(11500));
                if let Err(e) = s.record(
                    "MEASURING: 11.5s without diagnostic polls; normal background traffic enabled.",
                ) {
                    s.stop(&format!("ERROR log write: {e}"));
                }
            }
        } else {
            if s.sides[target] != side {
                s.stop("ERROR: physical side changed in frozen snapshot");
                return;
            }
            let page = q[6] as usize;
            for i in 0..6 {
                s.words[target][page * 6 + i] =
                    u32::from_le_bytes(r[8 + 4 * i..12 + 4 * i].try_into().unwrap());
            }
            s.phase = if page < 7 {
                Phase::Reading(target as u8, page as u8 + 1)
            } else if target == 0 {
                Phase::Reading(1, 0)
            } else {
                for t in 0..2 {
                    if let Err(e) = s.record(&snapshot_text(t, s.sides[t], &s.words[t])) {
                        s.stop(&format!("ERROR log write: {e}"));
                        return;
                    }
                }
                if s.words.iter().all(valid_snapshot) {
                    s.stop("SUCCESS: both complete clean ORBIT windows saved");
                } else {
                    s.stop("ERROR: not ORBIT, incomplete or contaminated window; no accepted measurement");
                }
                Phase::Finished
            };
        }
        self.status_msg = format!(
            "SM62 FPS: {} — {}",
            if s.phase == Phase::Finished {
                &s.message
            } else {
                "измерение / measurement"
            },
            s.path.display()
        );
    }
    #[cfg(test)]
    pub(super) fn draw_sm62_fps(&mut self, ui: &mut egui::Ui) {
        use application_layout_runtime::app_layout_text;
        let lang = self.app_settings.language;
        let active = self
            .sm62_fps
            .as_ref()
            .is_some_and(|s| s.phase != Phase::Finished);
        if ui.add_enabled(!active && self.sm62_fps_eligible(), egui::Button::new(app_layout_text(
            lang, "Кадр KEYMAP 0x19 (SM62, USB-половина)", "Frame trace 0x19 (SM62, USB half)"
        ))).clicked() { self.begin_sm62_trace(); }
        if let Some(s) = self.sm62_fps.as_ref().filter(|s| s.trace.is_some()) {
            ui.label(app_layout_text(lang,
                "Нужна диагностическая прошивка 0x19. После ARM нажмите клавишу в течение 5 с: ORBIT пропускается, захватывается первый KEYMAP. Только USB-половина; это не FPS. Обмен Entropy внутри захвата помечается загрязнением.",
                "Requires diagnostic firmware 0x19. After ARM press a key within 5s: ORBIT is ignored, first KEYMAP captured. USB half only; not FPS. Entropy traffic inside capture is marked contaminated."));
            ui.label(format!("{:?}", s.phase));
        }
        if ui
            .add_enabled(
                !active && self.sm62_fps_eligible(),
                egui::Button::new(app_layout_text(
                    lang,
                    "Замер FPS ORBIT (SM62)",
                    "Measure ORBIT FPS (SM62)",
                )),
            )
            .clicked()
        {
            self.begin_sm62_fps();
        }
        if let Some(s) = &self.sm62_fps {
            let text = match s.phase {
                Phase::Preparing(_) if s.trace.is_some() => app_layout_text(lang, "Подготовка: дождитесь ARM, затем нажмите клавишу SM62.", "Preparing: wait for ARM, then press an SM62 key.").into(),
                Phase::Measuring(_) if s.trace.is_some() => app_layout_text(lang, "ARM принят: нажмите клавишу сейчас; ждём первый KEYMAP.", "ARM accepted: press a key now; waiting for first KEYMAP.").into(),
                Phase::Preparing(d) => format!(
                    "{} {:.1}s",
                    app_layout_text(
                        lang,
                        "Ожидание: корабль на обоих экранах; не нажимайте клавиши.",
                        "Wait for ORBIT on both screens; do not press keys."
                    ),
                    d.saturating_duration_since(Instant::now()).as_secs_f32()
                ),
                Phase::Measuring(d) => format!(
                    "{} {:.1}s",
                    app_layout_text(
                        lang,
                        "Замер; обычный обмен Entropy включён.",
                        "Measuring; normal Entropy traffic enabled."
                    ),
                    d.saturating_duration_since(Instant::now()).as_secs_f32()
                ),
                Phase::Finished => s.message.clone(),
                _ => app_layout_text(
                    lang,
                    "Обмен через владельца HID…",
                    "Exchanging through HID owner…",
                )
                .into(),
            };
            ui.label(text);
            ui.label(format!(
                "{}: {}",
                app_layout_text(lang, "Лог", "Log"),
                s.path.display()
            ));
            if ui
                .button(app_layout_text(
                    lang,
                    "Копировать путь лога",
                    "Copy log path",
                ))
                .clicked()
            {
                ui.ctx().copy_text(s.path.to_string_lossy().into_owned());
            }
            if active
                && ui
                    .button(app_layout_text(
                        lang,
                        "Отменить замер",
                        "Cancel measurement",
                    ))
                    .clicked()
            {
                self.cancel_sm62_fps("CANCELLED by user");
            }
        }
    }
}

#[cfg(test)]
#[path = "sm62_fps_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_orbit_off_and_diagnostic_contamination_without_fake_fps() {
        let mut w = [0; 48];
        w[0] = 1;
        w[1] = 10001;
        w[2] = 10000;
        w[37] = 10002;
        w[3] = 3;
        w[5] = 3;
        w[7] = 8;
        w[11] = 3;
        w[13] = 2;
        w[16] = 2;
        assert!(valid_snapshot(&w));
        assert!(snapshot_text(0, 1, &w).contains("completed frames/s"));
        for i in [3, 5, 7, 8, 9, 10, 12, 13, 16, 33, 40] {
            let mut bad = w;
            bad[i] ^= 1;
            assert!(!valid_snapshot(&bad));
            assert!(!snapshot_text(0, 1, &bad).contains("completed frames/s"));
        }
        w[3] = 4;
        assert!(!valid_snapshot(&w));
    }
}
