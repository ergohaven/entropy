use super::*;
use crate::device_session::DeviceSession;
use crate::firmware_flash::{self as flash, Event, Outcome, Part, Prepared, Role, Stage};
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError},
};

#[derive(Default)]
pub(crate) struct FirmwareFlashUi {
    dialog: Option<Dialog>,
    minimized: bool,
}

struct Dialog {
    device: Device,
    profile: flash::profiles::Profile,
    generation: u64,
    release: FirmwareUpdateCheckResult,
    current_version: String,
    backup: Option<PathBuf>,
    error: Option<String>,
    phase: Phase,
    part: Part,
    origin_part: Part,
    next: Option<Prepared>,
    retry: Option<Prepared>,
    post_write_check: bool,
    steps: Vec<(Part, Outcome)>,
}

enum Phase {
    Confirm,
    Preparing(Receiver<Result<Prepared, String>>),
    Ready(Prepared),
    Flashing {
        receiver: Receiver<Event>,
        stage: Stage,
    },
    Finished(Result<Outcome, String>),
}

impl FirmwareFlashUi {
    pub(super) fn owns_device(&self) -> bool {
        self.dialog
            .as_ref()
            .is_some_and(|d| matches!(d.phase, Phase::Flashing { .. }))
    }
}

fn text<'a>(language: crate::i18n::Language, en: &'a str, ru: &'a str) -> &'a str {
    match language {
        crate::i18n::Language::English => en,
        crate::i18n::Language::Russian => ru,
    }
}

pub(super) fn update_profile(
    device: &Device,
    target: &FirmwareReleaseTarget,
    release: &FirmwareUpdateCheckResult,
) -> Option<flash::profiles::Profile> {
    use crate::app::FirmwareReleaseFamily;

    if !flash::supported(device) || release.relation != VersionRelation::UpdateAvailable {
        return None;
    }
    match (device.firmware, target.family) {
        (crate::firmware::FirmwareProtocol::Vial, FirmwareReleaseFamily::Qmk)
            if flash::qmk_version(&target.current_version)? < flash::qmk_version(&release.latest_version)? =>
        {
            let profile = flash::profiles::qmk_profile(device.product_id, &target.asset_prefix)?;
            (release.asset.name == format!("{}_{}.uf2", release.latest_version, profile.archive))
                .then_some(profile)
        }
        (crate::firmware::FirmwareProtocol::Rynk, FirmwareReleaseFamily::Rynk)
            if flash::compatible_version(&target.current_version)
                && flash::compatible_version(&release.latest_version) =>
        {
            let profile = flash::profiles::profile_for_archive(device.product_id, &target.asset_prefix)?;
            (release.asset.name == format!("{}-{}.zip", profile.archive, release.latest_version))
                .then_some(profile)
        }
        _ => None,
    }
}

impl Dialog {
    fn standalone(&self) -> bool {
        self.profile.has_peripherals() && !self.profile.has_dongle()
    }

    fn moved_target(&self) -> bool {
        self.part != self.origin_part
    }

    fn requires_original_owner(&self) -> bool {
        self.steps.is_empty() && !(self.profile.qmk && self.retry.is_some())
    }

    fn physical_recovery(&self, app: &EntropyApp, physical_reset: bool) -> bool {
        !self.profile.qmk
            && physical_reset
            && self.retry.is_some()
            && self.backup.is_some()
            && app.hid_device.is_none()
            && app.selected_device.is_none()
    }

    fn session_part(profile: flash::profiles::Profile, session: &DeviceSession) -> Option<Part> {
        if profile.qmk {
            session.as_vial()?;
            return profile.parts.first().copied();
        }
        profile.runtime_part(session.as_rynk()?.link().board())
    }

    fn part_label(&self, language: crate::i18n::Language, part: Part) -> &'static str {
        text(language, part.label.0, part.label.1)
    }

    fn next_connection_instruction(
        &self,
        language: crate::i18n::Language,
        next: Part,
    ) -> &'static str {
        if !self.profile.qmk && next != self.origin_part {
            text(
                language,
                "Continue and start the next transfer. When prompted, move the USB cable to the part named above. Use the same port on the computer.",
                "Продолжите и запустите передачу. По подсказке переставьте USB-кабель в указанную выше часть. Используйте тот же порт компьютера.",
            )
        } else {
            text(
                language,
                "Move the USB cable to the part named above. Use the same port on the computer. Continue, then select the connected device in Entropy.",
                "Переставьте USB-кабель в указанную выше часть, оставив его в том же порту компьютера. Продолжите и выберите подключённое устройство в Entropy.",
            )
        }
    }
}

impl EntropyApp {
    pub(super) fn open_firmware_flash(&mut self, release: FirmwareUpdateCheckResult) {
        let Some(device) = self
            .selected_device
            .and_then(|i| self.device_manager.devices().get(i))
            .cloned()
            .filter(flash::supported)
        else {
            return;
        };
        if self.firmware_flash.dialog.is_some() {
            return;
        }
        let Some(target) = self
            .device_about_info
            .as_ref()
            .and_then(|i| i.firmware_update_target.as_ref())
        else {
            return;
        };
        let Some(profile) = update_profile(&device, target, &release) else {
            return;
        };
        let Some(origin_part) = self
            .hid_device
            .as_ref()
            .and_then(|session| Dialog::session_part(profile, session))
        else {
            self.status_msg = text(
                self.app_settings.language,
                "This USB component could not be identified for a firmware update.",
                "Не удалось определить подключённую по USB часть для обновления.",
            ).into();
            return;
        };
        let first = if profile.qmk {
            origin_part
        } else {
            profile.parts.iter().copied().find(|part| *part != origin_part).unwrap_or(origin_part)
        };
        self.firmware_flash.dialog = Some(Dialog {
            part: first,
            origin_part,
            profile,
            device,
            generation: self.connection_generation,
            release,
            current_version: target.current_version.clone(),
            backup: None,
            error: None,
            phase: Phase::Confirm,
            next: None,
            retry: None,
            post_write_check: false,
            steps: Vec::new(),
        });
    }

    fn firmware_flash_owner_matches(&self, dialog: &Dialog) -> bool {
        self.connection_generation == dialog.generation
            && self
                .selected_device
                .and_then(|i| self.device_manager.devices().get(i))
                .is_some_and(|d| {
                    d.path == dialog.device.path
                        && d.instance_token == dialog.device.instance_token
                        && d.permits_hid_target(&dialog.device)
                })
    }

    fn firmware_flash_session_matches(&self, dialog: &Dialog) -> bool {
        let part = self.hid_device.as_ref()
            .and_then(|session| Dialog::session_part(dialog.profile, session));
        part == Some(dialog.origin_part)
            && (!dialog.profile.qmk || self.device_about_info.as_ref()
                .and_then(|info| info.firmware_update_target.as_ref())
                .is_some_and(|target| {
                    target.family == crate::app::FirmwareReleaseFamily::Qmk
                        && target.asset_prefix == dialog.profile.archive
                }))
    }

    fn firmware_flash_can_prepare(&self, dialog: &Dialog) -> bool {
        !self.hid_write_lifecycle_busy()
            && !self.keycode_picker.macros_dirty
            && !self.keycode_picker.tap_dance_dirty
            && !self.combo_dirty
            && !self.combo_term_dirty
            && self.firmware_flash_session_matches(dialog)
            && matches!(self.connect_state, ConnectState::Idle)
    }

    fn prepare_firmware_flash(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut Dialog,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            dialog.profile.product_id == dialog.device.product_id,
            "Update profile does not match the selected keyboard"
        );
        anyhow::ensure!(
            self.firmware_flash_owner_matches(dialog),
            "Keyboard changed; start the update again"
        );
        anyhow::ensure!(
            self.firmware_flash_can_prepare(dialog),
            "Wait for pending keyboard edits to finish"
        );
        let keyboard_uid = if dialog.profile.qmk {
            self.current_keyboard_id.filter(|uid| *uid != 0)
                .ok_or_else(|| anyhow::anyhow!("QMK keyboard identity is missing"))?.to_le_bytes()
        } else {
            self.hid_device.as_ref().and_then(DeviceSession::as_rynk)
                .ok_or_else(|| anyhow::anyhow!("Firmware updates require Rynk"))?.info().keyboard_uid
        };
        dialog.backup = Some(self.write_firmware_update_backup()?);
        let (sender, receiver) = mpsc::channel();
        let profile = dialog.profile;
        let release = dialog.release.clone();
        let origin_part = dialog.origin_part;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = flash::prepare(
                profile,
                &release.latest_version,
                &release.asset.name,
                &release.asset.url,
                keyboard_uid,
                origin_part,
            )
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        dialog.phase = Phase::Preparing(receiver);
        Ok(())
    }

    fn begin_firmware_flash(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut Dialog,
        physical_reset: bool,
    ) -> anyhow::Result<()> {
        let Phase::Ready(prepared) = &dialog.phase else {
            anyhow::bail!("Firmware is not prepared");
        };
        anyhow::ensure!(!dialog.profile.qmk || !physical_reset, "Select and unlock the QMK keyboard first");
        let recovery = dialog.physical_recovery(self, physical_reset);
        let needs_session = !recovery && (dialog.profile.qmk || dialog.steps.is_empty() || !dialog.moved_target());
        if dialog.requires_original_owner() && !recovery {
            anyhow::ensure!(
                self.firmware_flash_owner_matches(dialog),
                "Keyboard changed; start the update again"
            );
        } else if needs_session {
            // Peripheral transfers intentionally disconnect the original HID.
            // Admit the freshly loaded USB central of the same model; the
            // worker also checks the original physical port and fresh version.
            anyhow::ensure!(
                self.selected_device
                    .and_then(|i| self.device_manager.devices().get(i))
                    .is_some_and(
                        |d| flash::supported(d) && d.product_id == dialog.device.product_id
                    ),
                "Reconnect and select the requested keyboard component first"
            );
        }
        anyhow::ensure!(
            matches!(self.device_scan_state, DeviceScanState::Idle)
                && !self.hid_write_lifecycle_busy(),
            "Keyboard is busy; wait and try again"
        );
        if needs_session {
            anyhow::ensure!(
                self.firmware_flash_can_prepare(dialog),
                "Wait for pending keyboard edits to finish"
            );
        } else {
            anyhow::ensure!(
                self.hid_device.is_none()
                    && self.selected_device.is_none()
                    && matches!(
                        self.connect_state,
                        ConnectState::Idle | ConnectState::SelectingDevice
                    ),
                "Disconnect other keyboard sessions before transferring to the requested part"
            );
        }
        anyhow::ensure!(
            (dialog.moved_target() && !dialog.profile.qmk)
                || physical_reset
                || (self.vial_unlocked == Some(true) && !self.vial_unlock_polling),
            "Unlock the keyboard first"
        );
        let mut prepared = prepared.clone();
        if dialog.profile.qmk {
            let device = self.selected_device
                .and_then(|i| self.device_manager.devices().get(i))
                .ok_or_else(|| anyhow::anyhow!("Keyboard disconnected"))?;
            prepared.bind_qmk_serial(&device.serial_number)?;
        }
        // Save again immediately before relinquishing HID, including any edits
        // made during physical unlocking. Backup failure never authorizes reboot.
        let hid = if needs_session {
            dialog.backup = Some(self.write_firmware_update_backup()?);
            let session = self
                .hid_device
                .take()
                .ok_or_else(|| anyhow::anyhow!("Keyboard disconnected"))?;
            Some(session)
        } else {
            anyhow::ensure!(dialog.backup.is_some(), "Configuration backup is missing");
            None
        };
        dialog.next = prepared.next();
        dialog.retry = Some(prepared.clone());
        dialog.post_write_check = false;
        // Revoke the selected background publisher before the worker reboots
        // its device. Other connected keyboards keep their existing publishers.
        if hid.is_some() {
            self.clear_connected_keyboard_state("Firmware update in progress");
            self.selected_device = None;
            self.connect_state = ConnectState::SelectingDevice;
        }
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                flash::flash(prepared, hid, physical_reset, &sender).map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Finished(result));
            ctx.request_repaint();
        });
        dialog.phase = Phase::Flashing {
            receiver,
            stage: Stage::CheckingConnection,
        };
        Ok(())
    }

    fn firmware_flash_can_check_version(&self) -> bool {
        matches!(self.device_scan_state, DeviceScanState::Idle)
            && !self.hid_write_lifecycle_busy()
            && self.hid_device.is_none()
            && self.selected_device.is_none()
            && matches!(self.connect_state, ConnectState::Idle | ConnectState::SelectingDevice)
    }

    fn begin_firmware_version_check(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut Dialog,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            dialog.post_write_check && matches!(dialog.phase, Phase::Finished(Err(_))),
            "No firmware version check is available"
        );
        anyhow::ensure!(
            self.firmware_flash_can_check_version(),
            "Wait for keyboard discovery to finish and leave the target unselected in Entropy"
        );
        let prepared = dialog
            .retry
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Firmware is not prepared"))?;
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = flash::verify_after_reconnect(&prepared).map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Finished(result));
            ctx.request_repaint();
        });
        dialog.phase = Phase::Flashing {
            receiver,
            stage: Stage::WaitingReconnect,
        };
        Ok(())
    }

    pub(super) fn poll_firmware_flash(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.firmware_flash.dialog.take() else {
            return;
        };
        let mut replacement = None;
        match &mut dialog.phase {
            Phase::Preparing(receiver) => match receiver.try_recv() {
                Ok(Ok(prepared)) if self.firmware_flash_owner_matches(&dialog) => {
                    replacement = Some(Phase::Ready(prepared))
                }
                Ok(Ok(_)) => {
                    replacement = Some(Phase::Finished(Err(
                        "Keyboard changed during download; start again".into(),
                    )))
                }
                Ok(Err(error)) => replacement = Some(Phase::Finished(Err(error))),
                Err(TryRecvError::Disconnected) => {
                    replacement = Some(Phase::Finished(Err(
                        "Firmware preparation worker stopped".into()
                    )))
                }
                Err(TryRecvError::Empty) => {}
            },
            Phase::Flashing { receiver, stage } => {
                let result = loop {
                    match receiver.try_recv() {
                        Ok(Event::Progress(progress)) => *stage = progress,
                        Ok(Event::Finished(result)) => {
                            self.last_device_scan_at = 0.0;
                            break Some(result);
                        }
                        Err(TryRecvError::Disconnected) => {
                            break Some(Err(
                                "Firmware update worker stopped; reconnect the half before retrying"
                                    .into(),
                            ));
                        }
                        Err(TryRecvError::Empty) => break None,
                    }
                };
                if let Some(result) = result {
                    dialog.post_write_check = result.is_err()
                        && dialog.retry.is_some()
                        && (matches!(stage, Stage::WaitingDisconnect | Stage::WaitingReconnect)
                            || (dialog.profile.qmk && matches!(stage,
                                Stage::EnteringBootloader | Stage::WaitingBootloader | Stage::Writing)));
                    if let Ok(outcome) = result {
                        dialog.steps.push((dialog.part, outcome));
                        dialog.retry = None;
                    }
                    replacement = Some(Phase::Finished(result));
                }
            }
            _ => {}
        }
        if let Some(phase) = replacement {
            dialog.phase = phase;
        }
        if matches!(dialog.phase, Phase::Preparing(_) | Phase::Flashing { .. }) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.firmware_flash.dialog = Some(dialog);
    }

    pub(super) fn draw_firmware_flash(&mut self, ctx: &egui::Context) {
        // Let the existing physical-unlock overlay own input until it finishes.
        if self.unlock_open || self.vial_unlock_polling {
            return;
        }
        let Some(mut dialog) = self.firmware_flash.dialog.take() else {
            return;
        };
        let lang = self.app_settings.language;
        let session_ready = self.hid_device.is_some()
            && self
                .selected_device
                .and_then(|i| self.device_manager.devices().get(i))
                .is_some_and(|d| flash::supported(d) && d.product_id == dialog.device.product_id)
            && self.firmware_flash_session_matches(&dialog)
            && (!dialog.requires_original_owner() || self.firmware_flash_owner_matches(&dialog));
        if self.firmware_flash.minimized {
            let mut discard = false;
            egui::Window::new(text(lang, "Firmware update", "Обновление прошивки"))
                .collapsible(false).show(ctx, |ui| {
                    ui.label(if dialog.profile.qmk {
                        text(lang, "Connect the requested part to the same USB port, select it in Entropy, then resume the guide.", "Подключите нужную часть к тому же USB-порту, выберите её в Entropy и вернитесь к инструкции.")
                    } else {
                        text(lang, "Reconnect and select the primary half or Qube USB dongle, then resume the guide.", "Подключите и выберите основную половинку или USB-донгл Qube, затем вернитесь к инструкции.")
                    });
                    if ui.button(text(lang, "Resume guide", "Вернуться к инструкции")).clicked() {
                        self.firmware_flash.minimized = false;
                    }
                    discard = ui.button(text(lang, "Discard guide", "Завершить инструкцию")).clicked();
                });
            if !discard {
                self.firmware_flash.dialog = Some(dialog);
            } else {
                self.firmware_flash.minimized = false;
            }
            return;
        }
        enum Action {
            Prepare,
            Unlock,
            Flash,
            PhysicalReset,
            Next,
            Select,
            CheckVersion,
            Retry,
        }
        let mut action = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("firmware_update")).show(ctx, |ui| {
            ui.set_width(440.0_f32.min((ctx.content_rect().width() - 48.0).max(180.0)));
            // A later step can be taller than the previous modal contents.
            ui.set_max_height((ctx.content_rect().height() - 96.0).max(100.0));
            egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(text(lang, "Update firmware", "Обновить прошивку"));
            ui.label(format!("{} · {} → {}", dialog.device.name, dialog.current_version, dialog.release.latest_version));
            ui.add_space(6.0);
            let total = dialog.profile.parts.len();
            if total > 1 {
                let done = dialog.steps.len();
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).text(match lang {
                    crate::i18n::Language::English => format!("Components processed: {done} of {total}"),
                    crate::i18n::Language::Russian => format!("Обработано компонентов: {done} из {total}"),
                }));
            }
            {
                if !matches!(dialog.phase, Phase::Finished(Ok(_))) {
                    ui.label(format!("{} · {}: {}", dialog.profile.archive, text(lang, "Update target", "Обновляем"), dialog.part_label(lang, dialog.part)));
                }
                for (part, outcome) in &dialog.steps {
                    ui.label(format!("{}: {}", dialog.part_label(lang, *part), match outcome {
                        Outcome::Verified => text(lang, "version verified", "версия проверена"),
                        Outcome::TransferredUnverified => text(lang, "transfer complete, version unverified", "передача завершена, версия не проверена"),
                    }));
                }
            }
            ui.separator();
            if !dialog.profile.qmk && matches!(dialog.phase, Phase::Confirm) {
                ui.label(text(lang, "Requires Entropy 0.6.0+ and RMK 0.3.0+ using Rynk. Update every component from the same release.", "Требуются Entropy 0.6.0+ и RMK 0.3.0+ с Rynk. Обновите все части прошивкой одного релиза."));
            }
            if let Some((en, ru)) = dialog.profile.connection_note.filter(|_| {
                !matches!(dialog.phase, Phase::Finished(Ok(_))) || dialog.next.is_some()
            }) {
                ui.label(text(lang, en, ru));
            }
            ui.add_space(8.0);
            match &dialog.phase {
                Phase::Confirm => {
                    ui.label(if !dialog.profile.has_peripherals() {
                        text(lang, "Keep the keyboard connected while Entropy backs up your configuration and verifies the firmware image.", "Оставьте клавиатуру подключённой, пока Entropy сохраняет настройки и проверяет образ прошивки.")
                    } else if dialog.profile.qmk {
                        text(lang, "Entropy backs up your configuration and updates the USB-connected half first. When prompted, move the computer’s USB cable to the other half and select it in Entropy.", "Entropy сохранит настройки и сначала обновит половинку, подключённую по USB. Затем переставьте USB-кабель компьютера во вторую половинку и выберите её в Entropy.")
                    } else if dialog.standalone() {
                        text(lang, "The half already connected by USB is the primary half; the other is the secondary half. Keep the primary connected while Entropy backs up your configuration and prepares both images. When prompted, disconnect the primary from USB and connect the secondary. The primary will be updated last. Entropy verifies the version on halves that support USB maintenance.", "Половинка, уже подключённая по USB, — основная; другая — вторая. Оставьте основную подключённой, пока Entropy сохраняет настройки и готовит оба образа. По инструкции отключите основную от USB и подключите вторую. Основная обновится последней. Entropy проверяет версию половинок, поддерживающих обслуживание по USB.")
                    } else {
                        text(lang, "Qube is already connected by USB. Keep it connected while Entropy backs up your configuration and verifies every image. The guide updates the left half, the right half, then Qube. Connect only the requested part to the same USB port. Entropy verifies the version on halves that support USB maintenance.", "Qube уже подключён по USB. Оставьте его подключённым для резервного копирования и проверки всех образов. Обновим левую половинку, правую, затем Qube. Подключайте только указанную часть к тому же USB-порту. Entropy проверяет версию половинок, поддерживающих обслуживание по USB.")
                    });
                    if ui.button(text(lang, "Back up and prepare", "Создать копию и подготовить")).clicked() { action = Some(Action::Prepare); }
                },
                Phase::Preparing(_) => { ui.spinner(); ui.label(text(lang, "Downloading and verifying firmware…", "Скачиваем и проверяем прошивку…")); },
                Phase::Ready(_) if dialog.profile.qmk => {
                    if !session_ready {
                        ui.label(text(lang, "Connect the requested part to the original USB port and select it in Entropy.", "Подключите нужную часть к исходному USB-порту и выберите её в Entropy."));
                        if ui.button(text(lang, "Select keyboard", "Выбрать клавиатуру")).clicked() {
                            action = Some(Action::Select);
                        }
                    }
                    ui.label(text(lang, "Firmware image verified. Unlock with the keys shown by Entropy to enter update mode.", "Образ прошивки проверен. Для перехода в режим прошивки удерживайте клавиши, указанные Entropy."));
                    ui.add_enabled_ui(session_ready, |ui| {
                        if self.vial_unlocked != Some(true) {
                            if ui.button(text(lang, "Unlock keyboard", "Разблокировать клавиатуру")).clicked() {
                                action = Some(Action::Unlock);
                            }
                        } else if ui.button(text(lang, "Flash this target", "Прошить эту часть")).clicked() {
                            action = Some(Action::Flash);
                        }
                    });
                },
                Phase::Ready(_) if dialog.moved_target() => {
                    ui.label(text(lang,
                        "Start the transfer and wait for Entropy to ask you to move the cable. Connect only the named half to the original USB port. Entropy will prompt for physical unlock keys if this half supports USB maintenance. Older firmware requires BOOT or Reset.",
                        "Запустите передачу и дождитесь, пока Entropy попросит переставить кабель. Подключите только указанную половинку к исходному USB-порту. Если половинка поддерживает обслуживание по USB, Entropy покажет клавиши для разблокировки. Для старой прошивки понадобится BOOT или Reset.",
                    ));
                    if ui.button(format!("{}: {}", text(lang, "Start transfer", "Начать передачу"), dialog.part_label(lang, dialog.part))).clicked() { action = Some(Action::Flash); }
                },
                Phase::Ready(_) => {
                    if !session_ready && !dialog.physical_recovery(self, true) {
                        ui.label(if !dialog.profile.has_peripherals() {
                            text(lang, "Reconnect the keyboard to the original USB port and select it in Entropy.", "Подключите клавиатуру к исходному USB-порту и выберите её в Entropy.")
                        } else if dialog.part.role == Role::Dongle {
                            text(lang, "Reconnect Qube to the original USB port and select it in Entropy. Entropy will back up the freshly loaded configuration before updating it.", "Подключите Qube к исходному USB-порту и выберите его в Entropy. Перед обновлением Entropy сохранит загруженные настройки.")
                        } else {
                            text(lang, "Disconnect the secondary half from USB. Reconnect the primary half to the original USB port and select it in Entropy. Entropy will back up its freshly loaded configuration before updating it.", "Отключите вторую половинку от USB. Подключите основную половинку к исходному USB-порту и выберите её в Entropy. Перед обновлением Entropy сохранит загруженные настройки.")
                        });
                        let select_label = if !dialog.profile.has_peripherals() {
                            text(lang, "Select keyboard", "Выбрать клавиатуру")
                        } else if dialog.part.role == Role::Dongle {
                            text(lang, "Select Qube USB dongle", "Выбрать USB-донгл Qube")
                        } else {
                            text(lang, "Select primary half", "Выбрать основную половинку")
                        };
                        if ui.button(select_label).clicked() { action = Some(Action::Select); }
                    }
                    if dialog.physical_recovery(self, true) {
                        ui.label(text(lang, "The previous attempt's firmware image and backup are retained. Use physical update mode below to retry on the original USB port.", "Образ прошивки и резервная копия предыдущей попытки сохранены. Используйте ручной вход в режим прошивки ниже для повтора в исходном USB-порту."));
                    }
                    ui.label(format!("{}: {}", dialog.part_label(lang, dialog.part), if dialog.part.rp2040() { text(lang, "Firmware image verified. Connect and select the named update target on the original USB port. Unlock it to enter update mode, or hold BOOT while reconnecting it.", "Образ прошивки проверен. Подключите и выберите указанную обновляемую часть в исходном USB-порту. Разблокируйте её для перехода в режим прошивки или удерживайте BOOT при подключении.") } else { text(lang, "Firmware image verified. Once the update target is connected to the original USB port and selected, unlock it to enter update mode or use the physical Reset option below.", "Образ прошивки проверен. Подключите обновляемую часть к исходному USB-порту и выберите её, затем разблокируйте для перехода в режим прошивки или используйте кнопку Reset ниже.") }));
                    ui.add_enabled_ui(session_ready, |ui| {
                    if self.vial_unlocked != Some(true) {
                        if ui.button(text(lang, "Unlock keyboard", "Разблокировать клавиатуру")).clicked() { action = Some(Action::Unlock); }
                    } else if ui.button(text(lang, "Flash this target", "Прошить эту часть")).clicked() { action = Some(Action::Flash); }
                    });
                    ui.add_enabled_ui(session_ready || dialog.physical_recovery(self, true), |ui| {
                    if ui.button(if dialog.part.rp2040() { text(lang, "Use physical BOOT", "Использовать кнопку BOOT") } else { text(lang, "Use physical Reset", "Использовать кнопку Reset") }).clicked() { action = Some(Action::PhysicalReset); }
                    });
                },
                Phase::Flashing { .. } if dialog.post_write_check => {
                    ui.spinner();
                    ui.label(text(lang, "Checking the firmware version…", "Проверяем версию прошивки…"));
                },
                Phase::Flashing { stage, .. } => {
                    ui.spinner();
                    if let Stage::Unlocking(keys) = stage {
                        if let Some(keys) = dialog.part.unlock_keys(keys) {
                            ui.label(match lang {
                                crate::i18n::Language::English => format!("Hold the physical {keys} keys to unlock."),
                                crate::i18n::Language::Russian => format!("Удерживайте клавиши {keys} для разблокировки."),
                            });
                        } else {
                            ui.label(format!("{} {keys:?}", text(lang, "Hold the physical unlock keys at these matrix positions:", "Удерживайте клавиши разблокировки в этих позициях матрицы:")));
                        }
                        return;
                    }
                    let instruction = match *stage {
                        Stage::Unlocking(_) => unreachable!("unlock instructions are shown above"),
                        Stage::WaitingComponent if dialog.part.rp2040() => text(lang, "Disconnect the previous USB component. Connect only the named half to the original USB port. For older firmware, hold BOOT while plugging in and then release it. Check the physical side before using BOOT.", "Отключите предыдущую часть от USB. Подключите только указанную половинку к исходному USB-порту. Для старой прошивки удерживайте BOOT во время подключения, затем отпустите. Перед использованием BOOT проверьте сторону половинки."),
                        Stage::WaitingComponent => text(lang, "Disconnect the previous USB component. Connect only the named half to the original USB port. For older firmware, double-press Reset on that half. Check the physical side before pressing Reset.", "Отключите предыдущую часть от USB. Подключите только указанную половинку к исходному USB-порту. Для старой прошивки дважды нажмите Reset на этой половинке. Перед нажатием Reset проверьте сторону половинки."),
                        Stage::WaitingBootloader if dialog.profile.qmk => text(lang, "Waiting for the keyboard to enter update mode. Keep the USB cable connected.", "Ждём перехода клавиатуры в режим прошивки. Оставьте USB-кабель подключённым."),
                        Stage::WaitingBootloader if dialog.part.rp2040() => text(lang, "Waiting for update mode on the named target. If needed, hold BOOT while reconnecting it to the original USB port.", "Ждём режим прошивки указанной части. При необходимости удерживайте BOOT при подключении к исходному USB-порту."),
                        Stage::WaitingBootloader => text(lang, "Waiting for update mode. If needed, double-press Reset on this target. Keep the USB cable connected to the same port.", "Ждём режим прошивки. При необходимости дважды нажмите Reset на этой части. Оставьте USB-кабель в том же порту."),
                        Stage::Writing => text(lang, "Writing firmware. Keep the keyboard connected.", "Записываем прошивку. Не отключайте клавиатуру."),
                        Stage::WaitingReconnect => text(lang, "Waiting for the target to restart and report its firmware version…", "Ждём перезапуска обновляемой части и проверяем версию прошивки…"),
                        Stage::CheckingConnection => text(lang, "Keep the currently connected component plugged in while Entropy checks the USB connection.", "Не отключайте подключённую часть, пока Entropy проверяет USB-соединение."),
                        Stage::EnteringBootloader => text(lang, "Entering update mode…", "Переходим в режим прошивки…"),
                        Stage::WaitingDisconnect => text(lang, "Waiting for the target to leave update mode…", "Ждём выхода обновляемой части из режима прошивки…"),
                    };
                    if matches!(stage, Stage::WaitingBootloader | Stage::WaitingComponent) {
                        ui.label(format!("{}: {instruction}", dialog.part_label(lang, dialog.part)));
                    } else {
                        ui.label(instruction);
                    }
                },
                Phase::Finished(Ok(outcome)) => {
                    ui.label(match outcome {
                        Outcome::Verified => text(lang, "The update target returned with the expected firmware version.", "Обновляемая часть вернулась с ожидаемой версией прошивки."),
                        Outcome::TransferredUnverified => text(lang, "Transfer complete, version unverified. Entropy cannot read this target's installed firmware version over USB.", "Передача завершена, версия не проверена. Entropy не может прочитать установленную версию прошивки этой половинки через USB."),
                    });
                    if let Some(next) = &dialog.next {
                        ui.add_space(6.0);
                        ui.strong(format!("{}: {}", text(lang, "Next", "Далее"), dialog.part_label(lang, next.part)));
                        ui.label(dialog.next_connection_instruction(lang, next.part));
                        if ui.button(format!("{}: {}", text(lang, "Continue", "Продолжить"), dialog.part_label(lang, next.part))).clicked() { action = Some(Action::Next); }
                    } else {
                        ui.label(if !dialog.profile.has_peripherals() {
                            text(lang, "The keyboard's firmware version is verified. Import the saved layout if needed.", "Версия прошивки клавиатуры проверена. При необходимости импортируйте сохранённую раскладку.")
                        } else {
                            text(lang, "All guided steps are finished. Restore your normal keyboard connection and check both halves. Import the saved layout if needed.", "Все этапы инструкции завершены. Верните обычное подключение и проверьте обе половинки. При необходимости импортируйте сохранённую раскладку.")
                        });
                        if dialog.profile.host_bluetooth {
                            ui.add_space(6.0);
                            ui.strong(text(lang, "Bluetooth connection", "Подключение по Bluetooth"));
                            ui.label(text(lang,
                                "If the keyboard no longer connects over Bluetooth, forget it in your system’s Bluetooth settings and pair it again.",
                                "Если клавиатура перестала подключаться по Bluetooth, удалите её из списка устройств в настройках системы и выполните сопряжение заново.",
                            ));
                        }
                    }
                },
                Phase::Finished(Err(error)) => {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if dialog.profile.qmk && dialog.retry.is_some() {
                        if dialog.post_write_check {
                            ui.label(text(lang, "If this part stays in BOOTSEL, follow the manual firmware guide for your exact model and revision, then check its version here.", "Если эта часть осталась в режиме BOOTSEL, обновите её по инструкции для вашей модели и ревизии, затем проверьте версию здесь."));
                            ui.hyperlink_to(text(lang, "Manual firmware guide", "Инструкция по ручному обновлению"), "https://docs.eh.works/firmware/");
                        } else {
                            ui.label(text(lang, "Reconnect and select the same QMK keyboard part before retrying.", "Подключите и выберите ту же часть клавиатуры с QMK перед повтором."));
                        }
                    }
                    if dialog.post_write_check {
                        ui.label(format!("{}: {}", dialog.part_label(lang, dialog.part), text(lang,
                            "To check the version, reconnect only this target to the original USB port without using BOOT or Reset.",
                            "Для проверки версии подключите заново только эту часть к исходному USB-порту, не используя BOOT или Reset.",
                        )));
                        if ui.add_enabled(
                            self.firmware_flash_can_check_version(),
                            egui::Button::new(text(lang, "Check firmware version", "Проверить версию прошивки")),
                        ).on_disabled_hover_text(text(lang,
                            "Wait for keyboard discovery to finish and leave the target unselected in Entropy.",
                            "Дождитесь завершения поиска клавиатур и не выбирайте эту часть в Entropy.",
                        )).clicked() { action = Some(Action::CheckVersion); }
                    } else if !dialog.profile.qmk {
                        ui.label(if dialog.part.rp2040() { text(lang, "No successful update is recorded for this part. Reconnect it before retrying; hold BOOT while reconnecting if it remains in update mode.", "Успешное обновление этой части не подтверждено. Подключите её заново перед повтором; если она осталась в режиме прошивки, удерживайте BOOT при подключении.") } else { text(lang, "No successful update is recorded for this part. Reconnect it before retrying; use physical Reset if it remains in update mode.", "Успешное обновление этой части не подтверждено. Перед повтором подключите её заново; если она осталась в режиме прошивки, используйте Reset.") });
                    }
                    if dialog.retry.is_some() {
                        if ui.button(text(lang, "Retry this target", "Повторить эту часть")).clicked() { action = Some(Action::Retry); }
                    } else if dialog.profile.qmk {
                        ui.label(text(lang, "Reconnect and select the keyboard, then open About device → Update firmware to start again.", "Подключите и выберите клавиатуру, затем снова откройте «Об устройстве → Обновить прошивку»."));
                    } else {
                        ui.label(text(lang, "Reconnect and select the primary half or Qube USB dongle, then open About device → Update firmware again to restart the guide.", "Подключите и выберите основную половинку или USB-донгл Qube, затем снова откройте «Об устройстве → Обновить прошивку», чтобы начать инструкцию заново."));
                    }
                },
            }
            if cfg!(target_os = "macos") && matches!(dialog.phase, Phase::Ready(_)) {
                ui.add_space(8.0);
                ui.add(egui::Label::new(text(lang,
                    "After a UF2 copy, the bootloader drive (such as NRF52BOOT) disconnects to restart. macOS may show “Disk Not Ejected Properly”. This notification alone does not mean flashing failed. Check the connected device's firmware version after it restarts.",
                    "После копирования UF2 загрузочный диск (например, NRF52BOOT) отключается для перезапуска. macOS может показать «Диск извлечён неправильно». Это уведомление само по себе не означает ошибку прошивки. После перезапуска проверьте версию подключённого устройства.")).wrap());
                ui.hyperlink_to(text(lang, "Firmware update instructions", "Инструкция по обновлению прошивки"), "https://docs.eh.works/firmware/");
            }
            if let Some(error) = &dialog.error { ui.colored_label(ui.visuals().error_fg_color, error); }
            if let Some(path) = &dialog.backup {
                ui.add_space(8.0);
                ui.label(text(lang, "Configuration backup:", "Резервная копия настроек:"));
                ui.label(path.display().to_string());
            }
            if !matches!(dialog.phase, Phase::Flashing { .. }) && ui.button(text(lang, "Close", "Закрыть")).clicked() { close = true; }
            });
        });
        if let Some(action) = action {
            let result = match action {
                Action::Prepare => self.prepare_firmware_flash(ctx, &mut dialog),
                Action::Unlock => {
                    self.unlock_open = true;
                    Ok(())
                }
                Action::Flash => self.begin_firmware_flash(ctx, &mut dialog, false),
                Action::PhysicalReset => self.begin_firmware_flash(ctx, &mut dialog, true),
                Action::Next => {
                    if let Some(next) = dialog.next.take() {
                        dialog.part = next.part;
                        dialog.phase = Phase::Ready(next);
                    }
                    Ok(())
                }
                Action::Select => {
                    self.firmware_flash.minimized = true;
                    Ok(())
                }
                Action::CheckVersion => self.begin_firmware_version_check(ctx, &mut dialog),
                Action::Retry => {
                    if let Some(prepared) = dialog.retry.clone() {
                        dialog.phase = Phase::Ready(prepared);
                        dialog.post_write_check = false;
                    }
                    Ok(())
                }
            };
            dialog.error = result.err().map(|error| format!("{error:#}"));
        }
        if !close {
            self.firmware_flash.dialog = Some(dialog);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_update::{FirmwareReleaseFamily, UpdateAsset, VersionRelation};
    use flash::profiles::*;

    use crate::rynk::client::testing::{handshake, info, ok, FakeBoard};
    use rmk_types_rynk::protocol::rynk::ergohaven::ERGOHAVEN_RYNK_REVISION;
    use rmk_types_rynk::protocol::rynk::{Cmd, FirmwareVersion};

    fn fixture() -> (EntropyApp, FakeBoard) {
        let mut app = EntropyApp::new_inert_for_test();
        let mut device = crate::device::test_usb_device("usb:test", 1, 0x52);
        device.firmware = crate::firmware::FirmwareProtocol::Rynk;
        app.device_manager.replace_devices(vec![device]);
        app.selected_device = Some(0);
        app.device_about_info = Some(DeviceAboutInfo {
            firmware_update_target: Some(FirmwareReleaseTarget {
                family: FirmwareReleaseFamily::Rynk,
                current_version: "0.3.0".into(),
                asset_prefix: "phenom".into(),
            }),
            ..Default::default()
        });
        let board = FakeBoard::new(|h, _| {
            if h.cmd == Cmd::GetErgohavenInfo {
                let mut value = info(ERGOHAVEN_RYNK_REVISION);
                value.firmware = FirmwareVersion {
                    major: 0,
                    minor: 3,
                    patch: 0,
                };
                vec![ok(h, value)]
            } else {
                vec![handshake(h, 4128).expect("unexpected device operation")]
            }
        });
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let link = crate::rynk::session::testing::link(&format!("usb:flash-test-{id}"), &board);
        app.hid_device = Some(crate::rynk::session::RynkSession::new(link).into());
        app.open_firmware_flash(FirmwareUpdateCheckResult {
            latest_version: "v0.3.1".into(),
            release_url: String::new(),
            asset: UpdateAsset {
                name: "phenom-v0.3.1.zip".into(),
                url: String::new(),
            },
            relation: VersionRelation::UpdateAvailable,
        });
        (app, board)
    }

    #[test]
    fn firmware_flash_qmk_profiles_include_split_and_single_boards_in_the_existing_guide() {
        for (pid, asset, parts) in [
            (0x42, "macropad_v3", 1),
            (0xb2, "sm62_v1", 2),
            (0x70, "k03_v3_v4", 2),
        ] {
            let _config = ScopedTempConfig::new();
            let mut app = EntropyApp::new_inert_for_test();
            let device = crate::device::test_usb_device("usb:qmk-test", 1, pid);
            let mut target = FirmwareReleaseTarget {
                family: FirmwareReleaseFamily::Qmk,
                current_version: "4.1.0".into(),
                asset_prefix: asset.into(),
            };
            let release = FirmwareUpdateCheckResult {
                latest_version: "4.2.0".into(),
                release_url: String::new(),
                asset: UpdateAsset { name: format!("4.2.0_{asset}.uf2"), url: String::new() },
                relation: VersionRelation::UpdateAvailable,
            };
            assert_eq!(update_profile(&device, &target, &release).unwrap().parts.len(), parts);
            target.family = FirmwareReleaseFamily::QmkMigration;
            assert!(update_profile(&device, &target, &release).is_none());
            target.family = FirmwareReleaseFamily::Rynk;
            assert!(update_profile(&device, &target, &release).is_none());
            target.family = FirmwareReleaseFamily::Qmk;
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            app.device_manager.replace_devices(vec![device]);
            app.selected_device = Some(0);
            app.device_about_info = Some(DeviceAboutInfo {
                firmware_update_target: Some(target),
                ..Default::default()
            });
            app.open_firmware_flash(release);
            let dialog = app.firmware_flash.dialog.as_ref().unwrap();
            assert_eq!(dialog.part, dialog.origin_part);
            assert_eq!(dialog.profile.parts.len(), parts);
            assert!(app.firmware_flash_session_matches(dialog));
            assert!(recorder.requests().is_empty());
        }
    }

    #[test]
    fn firmware_flash_about_rejects_the_other_topology_at_the_same_pid() {
        let mut device = crate::device::test_usb_device("usb:test", 1, 0xbe);
        device.firmware = crate::firmware::FirmwareProtocol::Rynk;
        let target = FirmwareReleaseTarget {
            family: FirmwareReleaseFamily::Rynk,
            current_version: "0.3.0".into(),
            asset_prefix: "velvet".into(),
        };
        let mut release = FirmwareUpdateCheckResult {
            latest_version: "v0.3.1".into(),
            release_url: String::new(),
            asset: UpdateAsset { name: "velvet-qube-v0.3.1.zip".into(), url: String::new() },
            relation: VersionRelation::UpdateAvailable,
        };
        assert!(update_profile(&device, &target, &release).is_none());
        release.asset.name = "velvet-v0.3.1.zip".into();
        let profile = update_profile(&device, &target, &release).unwrap();
        assert_eq!(profile.parts[0].label.0, "Left half");
        assert_eq!(profile.parts[1].label.0, "Right half");
    }

    #[test]
    fn firmware_flash_about_button_opens_a_localized_confirmation_without_device_io() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for (lang, update_label, prepare_label) in [
            (
                crate::i18n::Language::English,
                "Update firmware…",
                "Back up and prepare",
            ),
            (
                crate::i18n::Language::Russian,
                "Обновить прошивку…",
                "Создать копию и подготовить",
            ),
        ] {
            let (mut app, recorder) = fixture();
            let dialog = app.firmware_flash.dialog.take().unwrap();
            app.app_settings.language = lang;
            app.firmware_update_check = FirmwareUpdateCheckState::Ready {
                target: app
                    .device_about_info
                    .as_ref()
                    .unwrap()
                    .firmware_update_target
                    .clone()
                    .unwrap(),
                result: dialog.release,
            };
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(640.0, 480.0))
                .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                .build_ui_state(
                    |ui, app: &mut EntropyApp| {
                        app.draw_about_device_page(ui, ui.max_rect());
                        app.draw_firmware_flash(ui.ctx());
                    },
                    app,
                );
            harness.run();
            harness.get_by_label(update_label).click();
            harness.run();
            harness.get_by_label(prepare_label);
            harness.get_by_label("USB identity fixture · 0.3.0 → v0.3.1");
            assert!(matches!(
                harness
                    .state()
                    .firmware_flash
                    .dialog
                    .as_ref()
                    .unwrap()
                    .phase,
                Phase::Confirm
            ));
            assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
        }
    }

    #[test]
    fn firmware_flash_restart_hint_only_appears_after_preparation() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for lang in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
            for ready in [false, true] {
                let (mut app, board) = fixture();
                app.app_settings.language = lang;
                if ready {
                    let dialog = app.firmware_flash.dialog.as_mut().unwrap();
                    dialog.phase = Phase::Ready(Prepared {
                        profile: dialog.profile,
                        qmk_serial: None,
                        keyboard_uid: [1; 8], bytes: vec![7], version: "0.3.1".into(),
                        part: dialog.part, remaining: vec![],
                        origin: flash::UsbTarget {
                            port: "preview".into(), vendor: 0xe126, product: 0x52, mounts: vec![],
                        },
                    });
                }
                let mut harness = egui_kittest::Harness::builder()
                    .with_size(egui::vec2(900.0, 900.0))
                    .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                    .build_ui_state(|ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()), app);
                harness.run_steps(2);
                let label = text(lang,
                    "After a UF2 copy, the bootloader drive (such as NRF52BOOT) disconnects to restart. macOS may show “Disk Not Ejected Properly”. This notification alone does not mean flashing failed. Check the connected device's firmware version after it restarts.",
                    "После копирования UF2 загрузочный диск (например, NRF52BOOT) отключается для перезапуска. macOS может показать «Диск извлечён неправильно». Это уведомление само по себе не означает ошибку прошивки. После перезапуска проверьте версию подключённого устройства.");
                assert_eq!(harness.query_by_label(label).is_some(), ready && cfg!(target_os = "macos"));
                if ready && cfg!(target_os = "macos") {
                    assert!(harness.get_by_label(label).rect().width() <= 440.0);
                }
                assert!(board.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
            }
        }
    }

    #[test]
    fn firmware_flash_phenom_guides_the_matching_rmk_pair() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for lang in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            let (mut app, board) = fixture();
            app.app_settings.language = lang;
            assert_eq!(
                app.firmware_flash.dialog.as_ref().unwrap().part,
                PHENOM_PERIPHERAL
            );
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(640.0, 800.0))
                .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                .build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()),
                    app,
                );
            harness.run_steps(2);
            harness.get_by_label(text(lang, "Requires Entropy 0.6.0+ and RMK 0.3.0+ using Rynk. Update every component from the same release.", "Требуются Entropy 0.6.0+ и RMK 0.3.0+ с Rynk. Обновите все части прошивкой одного релиза."));
            harness.get_by_label(text(lang, "Keep the Phenom halves linked. Move only the computer's USB cable when the guide asks.", "Оставьте половинки Phenom соединёнными. По инструкции переставляйте только USB-кабель компьютера."));
            assert!(board.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
        }
    }

    #[test]
    fn firmware_flash_legacy_protocol_and_old_firmware_cannot_open_the_guide() {
        let _config = ScopedTempConfig::new();
        for (protocol, version) in [
            (crate::firmware::FirmwareProtocol::Vial, "0.3.0"),
            (crate::firmware::FirmwareProtocol::Rynk, "0.2.0"),
        ] {
            let (mut app, board) = fixture();
            let release = app.firmware_flash.dialog.take().unwrap().release;
            let mut device = crate::device::test_usb_device("usb:test", 1, 0x52);
            device.firmware = protocol;
            app.device_manager.replace_devices(vec![device]);
            app.device_about_info
                .as_mut()
                .unwrap()
                .firmware_update_target
                .as_mut()
                .unwrap()
                .current_version = version.into();
            app.open_firmware_flash(release);
            assert!(app.firmware_flash.dialog.is_none());
            assert!(board.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
        }
    }

    #[test]
    fn firmware_flash_native_backup_preserves_rynk_actions_without_reboot() {
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let mut layout = KeyboardLayout::from_vial_json(&serde_json::json!({
            "matrix": {"rows": 1, "cols": 1},
            "layouts": {"keymap": [["0,0"]]}
        }))
        .unwrap();
        layout.firmware = crate::firmware::FirmwareProtocol::Rynk;
        layout.layers = vec![vec![crate::keyboard::KeyBinding::Rynk(
            rmk_types_rynk::action::KeyAction::Single(rmk_types_rynk::action::Action::User(90)),
        )]];
        app.layout = Some(layout);
        app.layer_count = 1;
        let path = app.write_firmware_update_backup().unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let action =
            rmk_types_rynk::action::KeyAction::Single(rmk_types_rynk::action::Action::User(90));
        assert_eq!(
            saved["data"]["rynk_keymap"][0][0],
            serde_json::to_value(action).unwrap()
        );
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_stale_owner_and_incomplete_backup_never_take_hid_or_reboot() {
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let mut dialog = app.firmware_flash.dialog.take().unwrap();
        dialog.part = PHENOM_CENTRAL;
        dialog.phase = Phase::Ready(Prepared {
            profile: dialog.profile,
            qmk_serial: None,
            keyboard_uid: [0; 8],
            bytes: vec![],
            version: "0.3.1".into(),
            part: PHENOM_CENTRAL,
            remaining: vec![],
            origin: flash::UsbTarget {
                port: "test".into(),
                vendor: 0xe126,
                product: 0x52,
                mounts: vec![],
            },
        });
        app.connection_generation += 1;
        assert!(app
            .begin_firmware_flash(&egui::Context::default(), &mut dialog, true)
            .unwrap_err()
            .to_string()
            .contains("Keyboard changed"));
        app.connection_generation = dialog.generation;
        // No full layout exists: backup must fail before HID is relinquished.
        assert!(app
            .begin_firmware_flash(&egui::Context::default(), &mut dialog, true)
            .is_err());
        assert!(app.hid_device.is_some());
        assert_eq!(app.selected_device, Some(0));
        assert!(matches!(dialog.phase, Phase::Ready(_)));
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_ignores_stale_download_and_suppresses_scan_during_transfer() {
        let _config = ScopedTempConfig::new();
        let (mut app, _) = fixture();
        let (tx, rx) = mpsc::channel();
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.phase = Phase::Preparing(rx);
        tx.send(Ok(Prepared {
            profile: dialog.profile,
            qmk_serial: None,
            keyboard_uid: [0; 8],
            bytes: vec![],
            version: "0.3.1".into(),
            part: PHENOM_CENTRAL,
            remaining: vec![],
            origin: flash::UsbTarget {
                port: "test".into(),
                vendor: 0xe126,
                product: 0x52,
                mounts: vec![],
            },
        }))
        .unwrap();
        app.connection_generation += 1;
        let ctx = egui::Context::default();
        app.poll_firmware_flash(&ctx);
        assert!(matches!(
            app.firmware_flash.dialog.as_ref().unwrap().phase,
            Phase::Finished(Err(_))
        ));
        let (_tx, rx) = mpsc::channel();
        app.firmware_flash.dialog.as_mut().unwrap().phase = Phase::Flashing {
            receiver: rx,
            stage: Stage::Writing,
        };
        let (scans, requests) = mpsc::channel();
        app.test_device_scan_requests = Some(scans);
        app.last_device_scan_at = 0.0;
        app.tick_background(&ctx);
        assert!(requests.try_recv().is_err());
        assert!(app.hid_write_lifecycle_busy());
    }

    #[test]
    fn firmware_flash_post_write_check_preserves_progress_and_can_continue() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        app.app_settings.language = crate::i18n::Language::English;
        app.hid_device = None;
        app.selected_device = None;
        app.connect_state = ConnectState::SelectingDevice;
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.device.product_id = 0x71;
        dialog.profile = flash::profile(0x71).unwrap();
        dialog.origin_part = K04_DONGLE;
        dialog.part = K04_RIGHT_PERIPHERAL;
        let previous_step = (K04_LEFT_PERIPHERAL, Outcome::TransferredUnverified);
        dialog.steps.push(previous_step);
        let backup = PathBuf::from("saved.entlayout");
        dialog.backup = Some(backup.clone());
        let prepared = Prepared {
            profile: dialog.profile,
            qmk_serial: None,
            keyboard_uid: [1; 8],
            bytes: vec![7],
            version: "0.3.1".into(),
            part: dialog.part,
            remaining: vec![(K04_DONGLE, vec![9])],
            origin: flash::UsbTarget {
                port: "original-port".into(),
                vendor: 0xe126,
                product: 0x71,
                mounts: vec![],
            },
        };
        dialog.next = prepared.next();
        dialog.retry = Some(prepared);

        let ctx = egui::Context::default();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(640.0, 780.0))
            .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
            .build_ui_state(|ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()), app);
        for (last_stage, offers_check) in [
            (Stage::WaitingBootloader, false),
            (Stage::Writing, false),
            (Stage::WaitingDisconnect, true),
            (Stage::WaitingReconnect, true),
        ] {
            let (tx, receiver) = mpsc::channel();
            let app = harness.state_mut();
            app.firmware_flash.dialog.as_mut().unwrap().phase = Phase::Flashing {
                receiver,
                stage: Stage::WaitingComponent,
            };
            tx.send(Event::Progress(last_stage)).unwrap();
            tx.send(Event::Finished(Err("not verified".into()))).unwrap();
            app.poll_firmware_flash(&ctx);
            harness.run();
            assert_eq!(
                harness.query_by_label("Check firmware version").is_some(),
                offers_check
            );
            harness.get_by_label("Retry this target");
        }
        harness.get_by_label("Right half: To check the version, reconnect only this target to the original USB port without using BOOT or Reset.");
        let dialog = harness.state().firmware_flash.dialog.as_ref().unwrap();
        assert_eq!(dialog.steps, vec![previous_step]);
        assert_eq!(dialog.retry.as_ref().unwrap().bytes, vec![7]);
        assert_eq!(dialog.next.as_ref().unwrap().bytes, vec![9]);
        assert_eq!(dialog.backup.as_ref(), Some(&backup));

        // Deliver readback results through the normal worker channel, without USB I/O.
        for result in [Err("readback failed".into()), Ok(Outcome::Verified)] {
            let (tx, receiver) = mpsc::channel();
            let app = harness.state_mut();
            app.firmware_flash.dialog.as_mut().unwrap().phase = Phase::Flashing {
                receiver,
                stage: Stage::WaitingReconnect,
            };
            assert!(app.firmware_flash.owns_device());
            assert!(!app.firmware_flash_can_check_version());
            harness.run_steps(2);
            harness.get_by_label("Checking the firmware version…");
            let failed = result.is_err();
            tx.send(Event::Finished(result)).unwrap();
            harness.state_mut().poll_firmware_flash(&ctx);
            harness.run();
            assert_eq!(
                harness.query_by_label("Check firmware version").is_some(),
                failed
            );
            if failed {
                let dialog = harness.state().firmware_flash.dialog.as_ref().unwrap();
                assert_eq!(dialog.steps, vec![previous_step]);
                assert_eq!(dialog.retry.as_ref().unwrap().bytes, vec![7]);
                harness.get_by_label("Retry this target");
            }
        }
        let dialog = harness.state().firmware_flash.dialog.as_ref().unwrap();
        assert_eq!(
            dialog.steps,
            vec![previous_step, (K04_RIGHT_PERIPHERAL, Outcome::Verified)]
        );
        assert!(dialog.retry.is_none());
        assert_eq!(dialog.backup.as_ref(), Some(&backup));
        let continue_label = format!("Continue: {}", K04_DONGLE.label.0);
        harness.get_by_label(continue_label.as_str()).click();
        harness.run();
        let dialog = harness.state().firmware_flash.dialog.as_ref().unwrap();
        assert!(matches!(&dialog.phase, Phase::Ready(prepared) if prepared.part == K04_DONGLE));
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_version_check_requires_exclusive_device_access() {
        let _config = ScopedTempConfig::new();
        let (mut app, _) = fixture();
        app.connect_state = ConnectState::Idle;
        assert!(!app.firmware_flash_can_check_version());
        app.selected_device = None;
        assert!(!app.firmware_flash_can_check_version());
        app.hid_device = None;
        assert!(app.firmware_flash_can_check_version());

        let (_tx, rx) = mpsc::channel();
        app.device_scan_state = DeviceScanState::Scanning {
            rx,
            started_at: std::time::Instant::now(),
            generation: 0,
            timeout_logged: false,
            usb_only: true,
        };
        assert!(!app.firmware_flash_can_check_version());
    }

    #[test]
    fn firmware_flash_future_single_board_guide_needs_no_half_swapping() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for profile in [fixtures::SINGLE_RP2040, fixtures::SINGLE_NRF52840] {
            let (mut app, recorder) = fixture();
            let dialog = app.firmware_flash.dialog.as_mut().unwrap();
            dialog.device.product_id = profile.product_id;
            dialog.profile = profile;
            dialog.part = profile.parts[0];
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(640.0, 700.0))
                .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                .build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()),
                    app,
                );
            harness.run();
            harness.get_by_label("future-board · Update target: Keyboard");
            harness.get_by_label("Keep the keyboard connected while Entropy backs up your configuration and verifies the firmware image.");
            let dialog = harness.state_mut().firmware_flash.dialog.as_mut().unwrap();
            assert!(!dialog.standalone());
            let (_tx, receiver) = mpsc::channel();
            dialog.phase = Phase::Flashing {
                receiver,
                stage: Stage::WaitingBootloader,
            };
            harness.run_steps(2);
            let waiting = if profile.parts[0].rp2040() {
                "Keyboard: Waiting for update mode on the named target. If needed, hold BOOT while reconnecting it to the original USB port."
            } else {
                "Keyboard: Waiting for update mode. If needed, double-press Reset on this target. Keep the USB cable connected to the same port."
            };
            harness.get_by_label(waiting);
            harness
                .state_mut()
                .firmware_flash
                .dialog
                .as_mut()
                .unwrap()
                .phase = Phase::Finished(Ok(Outcome::Verified));
            harness.run();
            harness.get_by_label(
                "The keyboard's firmware version is verified. Import the saved layout if needed.",
            );
            assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
        }
    }

    #[test]
    fn firmware_flash_native_peripheral_remains_unverified_and_central_can_be_selected() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for (pid, peripheral, central, continue_label) in [
            (
                0x74,
                K04_SECONDARY,
                K04_LEFT_CENTRAL,
                "Continue: Primary half",
            ),
            (
                0x52,
                PHENOM_PERIPHERAL,
                PHENOM_CENTRAL,
                "Continue: Left half",
            ),
        ] {
            let (mut app, recorder) = fixture();
            let mut dialog = app.firmware_flash.dialog.take().unwrap();
            dialog.device.product_id = pid;
            dialog.profile = flash::profile(pid).unwrap();
            dialog.part = peripheral;
            dialog.origin_part = central;
            dialog.next = Some(Prepared {
                profile: dialog.profile,
                qmk_serial: None,
                keyboard_uid: [0; 8],
                bytes: vec![],
                origin: flash::UsbTarget {
                    port: "original-port".into(),
                    vendor: 0xe126,
                    product: pid,
                    mounts: vec![],
                },
                version: "0.3.1".into(),
                part: central,
                remaining: vec![],
            });
            let (tx, rx) = mpsc::channel();
            dialog.phase = Phase::Flashing {
                receiver: rx,
                stage: Stage::WaitingDisconnect,
            };
            app.firmware_flash.dialog = Some(dialog);
            tx.send(Event::Finished(Ok(Outcome::TransferredUnverified)))
                .unwrap();
            app.poll_firmware_flash(&egui::Context::default());
            assert_eq!(
                app.firmware_flash.dialog.as_ref().unwrap().steps,
                vec![(peripheral, Outcome::TransferredUnverified)]
            );

            // The real worker relinquishes the primary before a peripheral
            // transfer; completion alone must not resurrect that old session.
            app.hid_device = None;
            app.selected_device = None;
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(640.0, 700.0))
                .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                .build_ui_state(
                    |ui, app: &mut EntropyApp| {
                        let _ = ui.button("Device selector");
                        app.draw_firmware_flash(ui.ctx());
                    },
                    app,
                );
            harness.run();
            harness.get_by_label("Transfer complete, version unverified. Entropy cannot read this target's installed firmware version over USB.");
            harness.get_by_label(format!("Next: {}", central.label.0).as_str());
            harness.get_by_label("Move the USB cable to the part named above. Use the same port on the computer. Continue, then select the connected device in Entropy.");
            assert!(harness.query_by_label("Bluetooth connection").is_none());
            harness.get_by_label(continue_label).click();
            harness.run();
            assert_eq!(
                harness.state().firmware_flash.dialog.as_ref().unwrap().part,
                central
            );
            harness.get_by_label("Select primary half").click();
            harness.run();
            assert!(harness.state().firmware_flash.minimized);
            harness.get_by_label("Device selector");
            harness.get_by_label("Resume guide").click();
            harness.run();
            assert!(!harness.state().firmware_flash.minimized);
            let mut dialog = harness.state_mut().firmware_flash.dialog.take().unwrap();
            // Rebinding must not accept a different USB model after swapping parts.
            harness.state_mut().device_manager.replace_devices(vec![]);
            assert!(harness
                .state_mut()
                .begin_firmware_flash(&egui::Context::default(), &mut dialog, true)
                .unwrap_err()
                .to_string()
                .contains("Reconnect and select"));
            assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
        }
    }

    #[test]
    fn firmware_flash_pairing_help_is_only_shown_at_completion_for_host_bluetooth() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for (pid, bluetooth) in [(0x74, true), (0x71, false), (0x52, false)] {
            let (mut app, _) = fixture();
            let dialog = app.firmware_flash.dialog.as_mut().unwrap();
            dialog.profile = flash::profile(pid).unwrap();
            dialog.part = *dialog.profile.parts.last().unwrap();
            dialog.steps = dialog
                .profile
                .parts
                .iter()
                .copied()
                .map(|part| {
                    let outcome = if part.peripheral() {
                        Outcome::TransferredUnverified
                    } else {
                        Outcome::Verified
                    };
                    (part, outcome)
                })
                .collect();
            dialog.phase = Phase::Finished(Ok(Outcome::Verified));
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(640.0, 700.0))
                .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                .build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()),
                    app,
                );
            harness.run_steps(2);
            assert_eq!(harness.query_by_label("Bluetooth connection").is_some(), bluetooth);
            if bluetooth {
                harness.get_by_label("If the keyboard no longer connects over Bluetooth, forget it in your system’s Bluetooth settings and pair it again.");
            }
        }
    }

    #[test]
    fn firmware_flash_failed_primary_retains_physical_recovery_without_a_session() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for (pid, part, peripheral, button) in [
            (0x52, PHENOM_CENTRAL, PHENOM_PERIPHERAL, "Use physical BOOT"),
            (0x74, K04_LEFT_CENTRAL, K04_SECONDARY, "Use physical Reset"),
            (0x71, K04_DONGLE, K04_RIGHT_PERIPHERAL, "Use physical Reset"),
        ] {
            let (mut app, recorder) = fixture();
            app.hid_device = None;
            app.selected_device = None;
            let dialog = app.firmware_flash.dialog.as_mut().unwrap();
            dialog.device.product_id = pid;
            dialog.profile = flash::profile(pid).unwrap();
            dialog.origin_part = part;
            dialog.part = part;
            dialog
                .steps
                .push((peripheral, Outcome::TransferredUnverified));
            dialog.backup = Some(PathBuf::from("saved.entlayout"));
            dialog.retry = Some(Prepared {
                profile: dialog.profile,
                qmk_serial: None,
                keyboard_uid: [1; 8],
                bytes: vec![7],
                version: "0.3.1".into(),
                part,
                remaining: vec![],
                origin: flash::UsbTarget {
                    port: "original-port".into(),
                    vendor: 0xe126,
                    product: pid,
                    mounts: vec![],
                },
            });
            dialog.phase = Phase::Finished(Err("primary transfer failed".into()));
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(640.0, 780.0))
                .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
                .build_ui_state(
                    |ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()),
                    app,
                );
            harness.run();
            harness.get_by_label("Retry this target").click();
            harness.run();
            harness.get_by_label(button);
            let app = harness.state();
            let dialog = app.firmware_flash.dialog.as_ref().unwrap();
            assert!(matches!(dialog.phase, Phase::Ready(_)));
            assert_eq!(dialog.retry.as_ref().unwrap().bytes, vec![7]);
            assert!(dialog.physical_recovery(app, true));
            assert!(!dialog.physical_recovery(app, false));
            assert_eq!(
                dialog.steps,
                vec![(peripheral, Outcome::TransferredUnverified)]
            );
            assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
        }
    }

    #[test]
    fn firmware_flash_k04_waiting_screen_keeps_the_cable_swap_instructions_visible() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let (tx, rx) = mpsc::channel();
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.device.product_id = 0x74;
        dialog.profile = flash::profile(0x74).unwrap();
        dialog.part = K04_SECONDARY;
        dialog.phase = Phase::Flashing {
            receiver: rx,
            stage: Stage::CheckingConnection,
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(640.0, 700.0))
            .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
            .build_ui_state(
                |ui, app: &mut EntropyApp| {
                    app.poll_firmware_flash(ui.ctx());
                    app.draw_firmware_flash(ui.ctx());
                },
                app,
            );
        let swap = "Secondary half: Disconnect the previous USB component. Connect only the named half to the original USB port. For older firmware, double-press Reset on that half. Check the physical side before pressing Reset.";
        harness.run_steps(2);
        harness.get_by_label("Keep the currently connected component plugged in while Entropy checks the USB connection.");
        assert!(harness.query_by_label(swap).is_none());
        tx.send(Event::Progress(Stage::WaitingComponent)).unwrap();
        harness.run_steps(2);
        harness.get_by_label("k04 · Update target: Secondary half");
        harness.get_by_label(swap);
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_k04_maintenance_reports_local_unlock_keys() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let (_tx, rx) = mpsc::channel();
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.device.product_id = 0x74;
        dialog.profile = flash::profile(0x74).unwrap();
        dialog.part = K04_SECONDARY;
        dialog.phase = Phase::Flashing {
            receiver: rx,
            stage: Stage::Unlocking(vec![(2, 1), (2, 2)]),
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(640.0, 700.0))
            .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
            .build_ui_state(|ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()), app);
        harness.run_steps(2);
        harness.get_by_label("Hold the physical J + K keys to unlock.");
        assert!(recorder.sent().iter().all(|header| header.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_k04_later_peripheral_retry_preserves_prior_unverified_progress() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.device.product_id = 0x71;
        dialog.profile = flash::profile(0x71).unwrap();
        dialog.part = K04_RIGHT_PERIPHERAL;
        dialog
            .steps
            .push((K04_LEFT_PERIPHERAL, Outcome::TransferredUnverified));
        dialog.retry = Some(Prepared {
            profile: dialog.profile,
            qmk_serial: None,
            keyboard_uid: [0; 8],
            bytes: vec![7],
            version: "0.3.1".into(),
            part: K04_RIGHT_PERIPHERAL,
            remaining: vec![(K04_DONGLE, vec![9])],
            origin: flash::UsbTarget {
                port: "original-port".into(),
                vendor: 0xe126,
                product: 0x71,
                mounts: vec![],
            },
        });
        let (tx, rx) = mpsc::channel();
        dialog.phase = Phase::Flashing {
            receiver: rx,
            stage: Stage::Writing,
        };
        tx.send(Event::Finished(Err("flush failed".into())))
            .unwrap();
        app.poll_firmware_flash(&egui::Context::default());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(640.0, 700.0))
            .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
            .build_ui_state(
                |ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()),
                app,
            );
        harness.run();
        harness.get_by_label("Retry this target").click();
        harness.run();
        harness.get_by_label("k04-qube · Update target: Right half");
        let dialog = harness.state().firmware_flash.dialog.as_ref().unwrap();
        let Phase::Ready(prepared) = &dialog.phase else {
            panic!("retry must return to confirmation");
        };
        assert_eq!(prepared.part, K04_RIGHT_PERIPHERAL);
        assert_eq!(prepared.bytes, vec![7]);
        assert_eq!(prepared.remaining, vec![(K04_DONGLE, vec![9])]);
        assert_eq!(
            dialog.steps,
            vec![(K04_LEFT_PERIPHERAL, Outcome::TransferredUnverified)]
        );
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }
}
