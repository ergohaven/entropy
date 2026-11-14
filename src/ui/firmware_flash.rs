use super::*;
use crate::device_session::DeviceSession;
use crate::firmware_flash::{self as flash, Event, Outcome, Part, Prepared, Stage};
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
    generation: u64,
    release: FirmwareUpdateCheckResult,
    current_version: String,
    backup: Option<PathBuf>,
    error: Option<String>,
    phase: Phase,
    part: Part,
    next: Option<Prepared>,
    retry: Option<Prepared>,
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

impl Dialog {
    fn standalone(&self) -> bool {
        flash::profile(self.device.product_id).is_some_and(|p| p.parts.len() == 2)
    }

    fn physical_recovery(&self, app: &EntropyApp, physical_reset: bool) -> bool {
        physical_reset
            && self.retry.is_some()
            && self.backup.is_some()
            && app.hid_device.is_none()
            && app.selected_device.is_none()
    }

    fn part_label(&self, language: crate::i18n::Language, part: Part) -> &'static str {
        match part {
            Part::PhenomCentral => text(
                language,
                "Primary half (left)",
                "Основная половинка (левая)",
            ),
            Part::PhenomPeripheral => text(
                language,
                "Secondary half (right)",
                "Вторая половинка (правая)",
            ),
            Part::LeftCentral => text(language, "Primary half", "Основная половинка"),
            Part::RightPeripheral if self.standalone() => {
                text(language, "Secondary half", "Вторая половинка")
            }
            Part::LeftPeripheral => text(language, "Left half", "Левая половинка"),
            Part::RightPeripheral => text(language, "Right half", "Правая половинка"),
            Part::Dongle => text(language, "Qube USB dongle", "USB-донгл Qube"),
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
        if release.latest_version.trim_start_matches('v').is_empty()
            || self.firmware_flash.dialog.is_some()
            || release.relation != VersionRelation::UpdateAvailable
        {
            return;
        }
        let Some(current_version) = self
            .device_about_info
            .as_ref()
            .and_then(|i| i.firmware_update_target.as_ref())
            .map(|t| t.current_version.clone())
        else {
            return;
        };
        if !flash::compatible_version(&current_version)
            || !flash::compatible_version(&release.latest_version)
            || self
                .hid_device
                .as_ref()
                .and_then(DeviceSession::as_rynk)
                .is_none()
            || self
                .device_about_info
                .as_ref()
                .and_then(|i| i.firmware_update_target.as_ref())
                .is_none_or(|t| t.family != app_update::FirmwareReleaseFamily::Rynk)
        {
            return;
        }
        self.firmware_flash.dialog = Some(Dialog {
            part: flash::profile(device.product_id).map_or(Part::PhenomCentral, |p| p.parts[0]),
            device,
            generation: self.connection_generation,
            release,
            current_version,
            backup: None,
            error: None,
            phase: Phase::Confirm,
            next: None,
            retry: None,
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

    fn firmware_flash_can_prepare(&self) -> bool {
        !self.hid_write_lifecycle_busy()
            && !self.keycode_picker.macros_dirty
            && !self.keycode_picker.tap_dance_dirty
            && !self.combo_dirty
            && !self.combo_term_dirty
            && self
                .hid_device
                .as_ref()
                .and_then(DeviceSession::as_rynk)
                .is_some()
            && matches!(self.connect_state, ConnectState::Idle)
    }

    fn prepare_firmware_flash(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut Dialog,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.firmware_flash_owner_matches(dialog),
            "Keyboard changed; start the update again"
        );
        anyhow::ensure!(
            self.firmware_flash_can_prepare(),
            "Wait for pending keyboard edits to finish"
        );
        let keyboard_uid = self
            .hid_device
            .as_ref()
            .and_then(DeviceSession::as_rynk)
            .ok_or_else(|| anyhow::anyhow!("Firmware updates require Rynk"))?
            .info()
            .keyboard_uid;
        dialog.backup = Some(self.write_firmware_update_backup()?);
        let (sender, receiver) = mpsc::channel();
        let device = dialog.device.clone();
        let release = dialog.release.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = flash::prepare(
                &device,
                &release.latest_version,
                &release.asset.name,
                &release.asset.url,
                keyboard_uid,
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
        let Phase::Ready(_) = &dialog.phase else {
            anyhow::bail!("Firmware is not prepared");
        };
        let recovery = dialog.physical_recovery(self, physical_reset);
        let needs_session = !recovery && (dialog.steps.is_empty() || !dialog.part.peripheral());
        if dialog.steps.is_empty() && !recovery {
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
                "Reconnect and select this keyboard's primary half or Qube USB dongle first"
            );
        }
        anyhow::ensure!(
            matches!(self.device_scan_state, DeviceScanState::Idle)
                && !self.hid_write_lifecycle_busy(),
            "Keyboard is busy; wait and try again"
        );
        if needs_session {
            anyhow::ensure!(
                self.firmware_flash_can_prepare(),
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
            dialog.part.peripheral()
                || physical_reset
                || (self.vial_unlocked == Some(true) && !self.vial_unlock_polling),
            "Unlock the keyboard first"
        );
        // Save again immediately before relinquishing HID, including any edits
        // made during physical unlocking. Backup failure never authorizes reboot.
        let hid = if needs_session {
            dialog.backup = Some(self.write_firmware_update_backup()?);
            let session = self
                .hid_device
                .take()
                .ok_or_else(|| anyhow::anyhow!("Keyboard disconnected"))?;
            let hid = match session {
                DeviceSession::Rynk(session) => session,
                other => {
                    self.hid_device = Some(other);
                    anyhow::bail!("Firmware updates require Rynk");
                }
            };
            Some(hid)
        } else {
            anyhow::ensure!(dialog.backup.is_some(), "Configuration backup is missing");
            None
        };
        let Phase::Ready(prepared) = std::mem::replace(&mut dialog.phase, Phase::Confirm) else {
            unreachable!()
        };
        dialog.next = prepared.next();
        dialog.retry = Some(prepared.clone());
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
            stage: if dialog.part.peripheral() {
                Stage::WaitingBootloader
            } else {
                Stage::EnteringBootloader
            },
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
            Phase::Flashing { receiver, stage } => loop {
                match receiver.try_recv() {
                    Ok(Event::Progress(progress)) => *stage = progress,
                    Ok(Event::Finished(result)) => {
                        if let Ok(outcome) = result {
                            dialog.steps.push((dialog.part, outcome));
                            dialog.retry = None;
                        }
                        replacement = Some(Phase::Finished(result));
                        self.last_device_scan_at = 0.0;
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        replacement = Some(Phase::Finished(Err(
                            "Firmware update worker stopped; reconnect the half before retrying"
                                .into(),
                        )));
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            },
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
            && (!dialog.steps.is_empty() || self.firmware_flash_owner_matches(&dialog));
        if self.firmware_flash.minimized {
            let mut discard = false;
            egui::Window::new(text(lang, "Firmware update", "Обновление прошивки"))
                .collapsible(false).show(ctx, |ui| {
                    ui.label(text(lang, "Reconnect and select the primary half or Qube USB dongle, then resume the guide.", "Подключите и выберите основную половинку или USB-донгл Qube, затем вернитесь к инструкции."));
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
            Retry,
        }
        let mut action = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("firmware_update")).show(ctx, |ui| {
            ui.set_width(440.0_f32.min((ctx.content_rect().width() - 48.0).max(180.0)));
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 96.0).max(100.0))
                .show(ui, |ui| {
            ui.heading(text(lang, "Update firmware", "Обновить прошивку"));
            ui.label(&dialog.device.name);
            ui.label(format!("{} → {}", dialog.current_version, dialog.release.latest_version));
            if let Some(profile) = flash::profile(dialog.device.product_id) {
                ui.label(format!("{} · {}: {}", profile.archive, text(lang, "Update target", "Обновляем"), dialog.part_label(lang, dialog.part)));
                for (part, outcome) in &dialog.steps {
                    ui.label(format!("{}: {}", dialog.part_label(lang, *part), match outcome {
                        Outcome::Verified => text(lang, "version verified", "версия проверена"),
                        Outcome::TransferredUnverified => text(lang, "transfer complete, version unverified", "передача завершена, версия не проверена"),
                    }));
                }
            }
            ui.label(text(lang, "Requires Entropy 0.6.0+ and RMK 0.3.0+ using Rynk. Update every component from the same release.", "Требуются Entropy 0.6.0+ и RMK 0.3.0+ с Rynk. Обновите все части прошивкой одного релиза."));
            if dialog.part.rp2040() {
                ui.label(text(lang, "Phenom RMK uses the left half as primary and the right half as secondary. Keep the halves linked during normal use; move only the computer's USB cable when prompted.", "В RMK для Phenom левая половинка — основная, правая — вторая. Для обычной работы оставьте половинки соединёнными; по инструкции переставляйте только USB-кабель компьютера."));
            }
            ui.add_space(8.0);
            match &dialog.phase {
                Phase::Confirm => {
                    ui.label(if dialog.standalone() {
                        text(lang, "The half already connected by USB is the primary half; the other is the secondary half. Keep the primary connected while Entropy backs up your configuration and prepares both images. When prompted, disconnect the primary from USB and connect the secondary. The primary will be updated last. The secondary's installed version cannot be verified through USB.", "Половинка, уже подключённая по USB, — основная; другая — вторая. Оставьте основную подключённой, пока Entropy сохраняет настройки и готовит оба образа. По инструкции отключите основную от USB и подключите вторую. Основная обновится последней. Установленную версию второй половинки нельзя проверить через USB.")
                    } else {
                        text(lang, "Qube is already connected by USB. Keep it connected while Entropy backs up your configuration and verifies every image. The guide updates the left half, the right half, then Qube. Connect only the requested part to the same USB port. The halves' installed versions cannot be verified through USB.", "Qube уже подключён по USB. Оставьте его подключённым для резервного копирования и проверки всех образов. Обновим левую половинку, правую, затем Qube. Подключайте только указанную часть к тому же USB-порту. Установленные версии половинок нельзя проверить через USB.")
                    });
                    if ui.button(text(lang, "Back up and prepare", "Создать копию и подготовить")).clicked() { action = Some(Action::Prepare); }
                },
                Phase::Preparing(_) => { ui.spinner(); ui.label(text(lang, "Downloading and verifying firmware…", "Скачиваем и проверяем прошивку…")); },
                Phase::Ready(_) if dialog.part.peripheral() => {
                    ui.label(if dialog.part.rp2040() {
                        text(lang, "Start the transfer while the primary half is connected. Disconnect the primary from USB, hold BOOT on the RIGHT secondary half and connect it to the SAME USB port, then release BOOT. Entropy cannot identify left versus right in the bootloader; check the named half.", "Запустите передачу с подключённой основной половинкой. Отключите её от USB, удерживайте BOOT на ПРАВОЙ второй половинке и подключите её к ТОМУ ЖЕ USB-порту, затем отпустите BOOT. Entropy не различает половинки в загрузчике: проверьте указанную половинку.")
                    } else if dialog.standalone() {
                        text(lang, "Start the transfer below while the primary half is still connected. Then disconnect the primary from USB and connect only the secondary half to the SAME USB port. Double-press Reset on the secondary half. Entropy cannot identify halves in the bootloader; check that you connected the secondary half.", "Запустите передачу ниже, пока основная половинка ещё подключена. Затем отключите основную от USB и подключите только вторую к ТОМУ ЖЕ USB-порту. Дважды нажмите Reset на второй половинке. Entropy не различает половинки в загрузчике: проверьте, что подключили вторую.")
                    } else {
                        text(lang, "Start the transfer below, then disconnect Qube or the previous half from USB. Connect ONLY the named half to the SAME USB port and double-press its Reset button. Entropy cannot detect left versus right in the bootloader: check the named half before proceeding.", "Запустите передачу ниже, затем отключите Qube или предыдущую половинку от USB. Подключите ТОЛЬКО указанную половинку к ТОМУ ЖЕ USB-порту и дважды нажмите Reset. Entropy не различает левую и правую половинки в загрузчике: проверьте указанную половинку перед продолжением.")
                    });
                    ui.label(text(lang, "A completed transfer will remain version unverified.", "После передачи версия останется непроверенной."));
                    if ui.button(format!("{}: {}", text(lang, "Start transfer", "Начать передачу"), dialog.part_label(lang, dialog.part))).clicked() { action = Some(Action::PhysicalReset); }
                },
                Phase::Ready(_) => {
                    if !session_ready && !dialog.physical_recovery(self, true) {
                        ui.label(if dialog.part == Part::Dongle {
                            text(lang, "Reconnect Qube to the original USB port and select it in Entropy. Entropy will back up the freshly loaded configuration before updating it.", "Подключите Qube к исходному USB-порту и выберите его в Entropy. Перед обновлением Entropy сохранит загруженные настройки.")
                        } else {
                            text(lang, "Disconnect the secondary half from USB. Reconnect the primary half to the original USB port and select it in Entropy. Entropy will back up its freshly loaded configuration before updating it.", "Отключите вторую половинку от USB. Подключите основную половинку к исходному USB-порту и выберите её в Entropy. Перед обновлением Entropy сохранит загруженные настройки.")
                        });
                        let select_label = if dialog.part == Part::Dongle {
                            text(lang, "Select Qube USB dongle", "Выбрать USB-донгл Qube")
                        } else {
                            text(lang, "Select primary half", "Выбрать основную половинку")
                        };
                        if ui.button(select_label).clicked() { action = Some(Action::Select); }
                    }
                    if dialog.physical_recovery(self, true) {
                        ui.label(text(lang, "The previous attempt's firmware image and backup are retained. Use physical update mode below to retry on the original USB port.", "Образ прошивки и резервная копия предыдущей попытки сохранены. Используйте ручной вход в режим прошивки ниже для повтора в исходном USB-порту."));
                    }
                    ui.label(if dialog.part.rp2040() { text(lang, "Firmware image verified. Connect and select the LEFT primary half on the original USB port. Unlock it to enter update mode, or hold BOOT while reconnecting it.", "Образ прошивки проверен. Подключите и выберите ЛЕВУЮ основную половинку в исходном USB-порту. Разблокируйте её для перехода в режим прошивки или удерживайте BOOT при подключении.") } else { text(lang, "Firmware image verified. Once the update target is connected to the original USB port and selected, unlock it to enter update mode or use the physical Reset option below.", "Образ прошивки проверен. Подключите обновляемую часть к исходному USB-порту и выберите её, затем разблокируйте для перехода в режим прошивки или используйте кнопку Reset ниже.") });
                    ui.add_enabled_ui(session_ready, |ui| {
                    if self.vial_unlocked != Some(true) {
                        if ui.button(text(lang, "Unlock keyboard", "Разблокировать клавиатуру")).clicked() { action = Some(Action::Unlock); }
                    } else if ui.button(text(lang, "Flash this half", "Прошить эту половинку")).clicked() { action = Some(Action::Flash); }
                    });
                    ui.add_enabled_ui(session_ready || dialog.physical_recovery(self, true), |ui| {
                    if ui.button(if dialog.part.rp2040() { text(lang, "Use physical BOOT", "Использовать кнопку BOOT") } else { text(lang, "Use physical Reset", "Использовать кнопку Reset") }).clicked() { action = Some(Action::PhysicalReset); }
                    });
                },
                Phase::Flashing { stage, .. } => {
                    ui.spinner();
                    ui.label(match *stage {
                        Stage::WaitingBootloader if dialog.part.rp2040() && dialog.part.peripheral() => text(lang, "Disconnect the primary half from USB. Hold BOOT on the RIGHT secondary half while connecting it to the original USB port, then release BOOT.", "Отключите основную половинку от USB. Удерживайте BOOT на ПРАВОЙ второй половинке при подключении к исходному USB-порту, затем отпустите BOOT."),
                        Stage::WaitingBootloader if dialog.part.rp2040() => text(lang, "Waiting for update mode on the LEFT primary half. If needed, hold BOOT while reconnecting it to the original USB port.", "Ждём режим прошивки ЛЕВОЙ основной половинки. При необходимости удерживайте BOOT при подключении к исходному USB-порту."),
                        Stage::WaitingBootloader if dialog.part.peripheral() && dialog.standalone() => text(lang, "Disconnect the primary half from USB. Connect ONLY the secondary half to the original USB port and double-press its Reset button.", "Отключите основную половинку от USB. Подключите ТОЛЬКО вторую половинку к исходному USB-порту и дважды нажмите её кнопку Reset."),
                        Stage::WaitingBootloader if dialog.part.peripheral() => text(lang, "Disconnect Qube or the previous half from USB. Connect ONLY the named half to the original USB port and double-press its Reset button.", "Отключите Qube или предыдущую половинку от USB. Подключите ТОЛЬКО указанную половинку к исходному USB-порту и дважды нажмите её кнопку Reset."),
                        Stage::WaitingBootloader => text(lang, "Waiting for update mode. If needed, double-press Reset on this half. Keep the USB cable connected to the same port.", "Ждём режим прошивки. При необходимости дважды нажмите Reset на этой половинке. Оставьте USB-кабель в том же порту."),
                        Stage::Writing => text(lang, "Writing firmware. Keep the keyboard connected.", "Записываем прошивку. Не отключайте клавиатуру."),
                        Stage::WaitingReconnect => text(lang, "Waiting for the half to restart and report its firmware version…", "Ждём перезапуска половинки и проверяем версию прошивки…"),
                        Stage::EnteringBootloader => text(lang, "Entering update mode…", "Переходим в режим прошивки…"),
                        Stage::WaitingDisconnect => text(lang, "Firmware data sent. Waiting for the peripheral to leave update mode. Its application and installed version are unverified.", "Данные прошивки отправлены. Ждём выхода половинки из режима прошивки. Запуск и установленная версия не проверены."),
                    });
                },
                Phase::Finished(Ok(outcome)) => {
                    ui.label(match outcome {
                        Outcome::Verified => text(lang, "The primary half or Qube USB dongle returned with the expected firmware version.", "Основная половинка или USB-донгл Qube вернулись с ожидаемой версией прошивки."),
                        Outcome::TransferredUnverified => text(lang, "Transfer complete, version unverified. Entropy cannot read this half's installed firmware version over USB.", "Передача завершена, версия не проверена. Entropy не может прочитать установленную версию прошивки этой половинки через USB."),
                    });
                    if let Some(next) = &dialog.next {
                        if ui.button(format!("{}: {}", text(lang, "Continue", "Продолжить"), dialog.part_label(lang, next.part))).clicked() { action = Some(Action::Next); }
                    } else {
                        ui.label(if dialog.standalone() {
                            text(lang, "All guided steps are finished. The secondary half's version remains unverified. Restore your normal keyboard connection and check both halves; import the saved layout if needed.", "Все этапы инструкции завершены. Версия второй половинки остаётся непроверенной. Верните обычное подключение клавиатуры и проверьте обе половинки; при необходимости импортируйте сохранённую раскладку.")
                        } else {
                            text(lang, "All guided steps are finished. Peripheral versions remain unverified. Reconnect the keyboard and check both halves; import the saved layout if needed.", "Все этапы инструкции завершены. Версии периферийных половинок остаются непроверенными. Подключите клавиатуру и проверьте обе половинки; при необходимости импортируйте сохранённую раскладку.")
                        });
                    }
                },
                Phase::Finished(Err(error)) => {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui.label(if dialog.part.rp2040() { text(lang, "No successful update is recorded for this part. Reconnect it before retrying; hold BOOT while reconnecting if it remains in update mode.", "Успешное обновление этой части не подтверждено. Подключите её заново перед повтором; если она осталась в режиме прошивки, удерживайте BOOT при подключении.") } else { text(lang, "No successful update is recorded for this part. Reconnect it before retrying; use physical Reset if it remains in update mode.", "Успешное обновление этой части не подтверждено. Перед повтором подключите её заново; если она осталась в режиме прошивки, используйте Reset.") });
                    if dialog.retry.is_some() {
                        if ui.button(text(lang, "Retry this half", "Повторить эту половинку")).clicked() { action = Some(Action::Retry); }
                    } else {
                        ui.label(text(lang, "Reconnect and select the primary half or Qube USB dongle, then open About device → Update firmware again to restart the guide.", "Подключите и выберите основную половинку или USB-донгл Qube, затем снова откройте «Об устройстве → Обновить прошивку», чтобы начать инструкцию заново."));
                    }
                },
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
                Action::Retry => {
                    if let Some(prepared) = dialog.retry.clone() {
                        dialog.phase = Phase::Ready(prepared);
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
            harness.get_by_label("0.3.0 → v0.3.1");
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
                Part::PhenomPeripheral
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
            harness.get_by_label(text(lang, "Phenom RMK uses the left half as primary and the right half as secondary. Keep the halves linked during normal use; move only the computer's USB cable when prompted.", "В RMK для Phenom левая половинка — основная, правая — вторая. Для обычной работы оставьте половинки соединёнными; по инструкции переставляйте только USB-кабель компьютера."));
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
        dialog.part = Part::PhenomCentral;
        dialog.phase = Phase::Ready(Prepared {
            keyboard_uid: [0; 8],
            bytes: vec![],
            version: "0.3.1".into(),
            part: Part::PhenomCentral,
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
            keyboard_uid: [0; 8],
            bytes: vec![],
            version: "0.3.1".into(),
            part: Part::PhenomCentral,
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
    fn firmware_flash_failed_version_check_does_not_record_a_verified_step() {
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let (tx, rx) = mpsc::channel();
        app.firmware_flash.dialog.as_mut().unwrap().phase = Phase::Flashing {
            receiver: rx,
            stage: Stage::WaitingReconnect,
        };
        tx.send(Event::Finished(Err("not verified".into())))
            .unwrap();
        app.poll_firmware_flash(&egui::Context::default());
        let dialog = app.firmware_flash.dialog.as_ref().unwrap();
        assert!(matches!(&dialog.phase, Phase::Finished(Err(error)) if error == "not verified"));
        assert!(dialog.steps.is_empty());
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_native_peripheral_remains_unverified_and_central_can_be_selected() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for (pid, peripheral, central, continue_label) in [
            (
                0x74,
                Part::RightPeripheral,
                Part::LeftCentral,
                "Continue: Primary half",
            ),
            (
                0x52,
                Part::PhenomPeripheral,
                Part::PhenomCentral,
                "Continue: Primary half (left)",
            ),
        ] {
            let (mut app, recorder) = fixture();
            let mut dialog = app.firmware_flash.dialog.take().unwrap();
            dialog.device.product_id = pid;
            dialog.part = peripheral;
            dialog.next = Some(Prepared {
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
            harness.get_by_label("Transfer complete, version unverified. Entropy cannot read this half's installed firmware version over USB.");
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
    fn firmware_flash_failed_primary_retains_physical_recovery_without_a_session() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        for (pid, part, peripheral, button) in [
            (
                0x52,
                Part::PhenomCentral,
                Part::PhenomPeripheral,
                "Use physical BOOT",
            ),
            (
                0x74,
                Part::LeftCentral,
                Part::RightPeripheral,
                "Use physical Reset",
            ),
            (
                0x71,
                Part::Dongle,
                Part::RightPeripheral,
                "Use physical Reset",
            ),
        ] {
            let (mut app, recorder) = fixture();
            app.hid_device = None;
            app.selected_device = None;
            let dialog = app.firmware_flash.dialog.as_mut().unwrap();
            dialog.device.product_id = pid;
            dialog.part = part;
            dialog
                .steps
                .push((peripheral, Outcome::TransferredUnverified));
            dialog.backup = Some(PathBuf::from("saved.entlayout"));
            dialog.retry = Some(Prepared {
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
            harness.get_by_label("Retry this half").click();
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
        let (_tx, rx) = mpsc::channel();
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.device.product_id = 0x74;
        dialog.part = Part::RightPeripheral;
        dialog.phase = Phase::Flashing {
            receiver: rx,
            stage: Stage::WaitingBootloader,
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(640.0, 700.0))
            .with_context_setup(crate::keycode_picker::setup_picker_test_fonts)
            .build_ui_state(
                |ui, app: &mut EntropyApp| app.draw_firmware_flash(ui.ctx()),
                app,
            );
        harness.run_steps(2);
        harness.get_by_label("k04 · Update target: Secondary half");
        harness.get_by_label("Disconnect the primary half from USB. Connect ONLY the secondary half to the original USB port and double-press its Reset button.");
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }

    #[test]
    fn firmware_flash_k04_later_peripheral_retry_preserves_prior_unverified_progress() {
        use egui_kittest::kittest::Queryable;
        let _config = ScopedTempConfig::new();
        let (mut app, recorder) = fixture();
        let dialog = app.firmware_flash.dialog.as_mut().unwrap();
        dialog.device.product_id = 0x71;
        dialog.part = Part::RightPeripheral;
        dialog
            .steps
            .push((Part::LeftPeripheral, Outcome::TransferredUnverified));
        dialog.retry = Some(Prepared {
            keyboard_uid: [0; 8],
            bytes: vec![7],
            version: "0.3.1".into(),
            part: Part::RightPeripheral,
            remaining: vec![(Part::Dongle, vec![9])],
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
        harness.get_by_label("Retry this half").click();
        harness.run();
        harness.get_by_label("k04-qube · Update target: Right half");
        let dialog = harness.state().firmware_flash.dialog.as_ref().unwrap();
        let Phase::Ready(prepared) = &dialog.phase else {
            panic!("retry must return to confirmation");
        };
        assert_eq!(prepared.part, Part::RightPeripheral);
        assert_eq!(prepared.bytes, vec![7]);
        assert_eq!(prepared.remaining, vec![(Part::Dongle, vec![9])]);
        assert_eq!(
            dialog.steps,
            vec![(Part::LeftPeripheral, Outcome::TransferredUnverified)]
        );
        assert!(recorder.sent().iter().all(|h| h.cmd != Cmd::BootloaderJump));
    }
}
