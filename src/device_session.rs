//! The connected keyboard as the UI sees it, whatever protocol configures it.
//!
//! Every write the UI makes goes through a method here, so the protocol is
//! chosen in one place. The methods speak Entropy's model — Vial keycodes,
//! Vial macro bytecode, QMK setting ids — because that is what the editors
//! hold; a session for another protocol translates at this boundary. Loaders
//! and features that exist only on Vial take the transport from
//! [`DeviceSession::as_vial`].

use anyhow::{bail, Context, Result};

use crate::hid::HidDevice;
use crate::keyboard::KeyBinding;
use crate::rynk::session::RynkSession;

/// Unlocked, unlocking, and the keys to hold.
pub(crate) type UnlockStatus = (bool, bool, Vec<(u8, u8)>);

pub(crate) enum DeviceSession {
    Vial(HidDevice),
    Rynk(RynkSession),
}

impl From<HidDevice> for DeviceSession {
    fn from(hid: HidDevice) -> Self {
        Self::Vial(hid)
    }
}

impl From<RynkSession> for DeviceSession {
    fn from(session: RynkSession) -> Self {
        Self::Rynk(session)
    }
}

/// QMK setting values by id, as the settings loaders read them. A Vial board
/// and a Rynk board serve the same ids, so the loaders are shared.
pub(crate) trait QmkSettingsSource {
    fn get_qmk_setting_u8(&self, qsid: u16) -> Result<u8>;
    fn get_qmk_setting_u16(&self, qsid: u16) -> Result<u16>;
}

impl QmkSettingsSource for HidDevice {
    fn get_qmk_setting_u8(&self, qsid: u16) -> Result<u8> {
        HidDevice::get_qmk_setting_u8(self, qsid)
    }

    fn get_qmk_setting_u16(&self, qsid: u16) -> Result<u16> {
        HidDevice::get_qmk_setting_u16(self, qsid)
    }
}

impl QmkSettingsSource for RynkSession {
    fn get_qmk_setting_u8(&self, qsid: u16) -> Result<u8> {
        RynkSession::get_qmk_setting_u8(self, qsid)
    }

    fn get_qmk_setting_u16(&self, qsid: u16) -> Result<u16> {
        RynkSession::get_qmk_setting_u16(self, qsid)
    }
}

impl DeviceSession {
    /// The Vial transport, for Vial-only loaders and features.
    pub(crate) fn as_vial(&self) -> Option<&HidDevice> {
        match self {
            Self::Vial(hid) => Some(hid),
            Self::Rynk(_) => None,
        }
    }

    /// Hands the Vial transport to a new owner, or gives the session back.
    pub(crate) fn into_vial(self) -> Result<HidDevice, Box<Self>> {
        match self {
            Self::Vial(hid) => Ok(hid),
            other => Err(Box::new(other)),
        }
    }

    /// [`Self::as_vial`] for an operation that only exists on Vial.
    pub(crate) fn require_vial(&self) -> Result<&HidDevice> {
        self.as_vial()
            .context("this keyboard does not support the operation")
    }

    pub(crate) fn as_rynk(&self) -> Option<&RynkSession> {
        match self {
            Self::Rynk(session) => Some(session),
            Self::Vial(_) => None,
        }
    }

    pub(crate) fn is_rynk(&self) -> bool {
        self.as_rynk().is_some()
    }

    pub(crate) fn is_bluetooth_transport(&self) -> bool {
        match self {
            Self::Vial(hid) => hid.is_bluetooth_transport(),
            Self::Rynk(session) => crate::rynk::transport::is_bluetooth(session.link().id()),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn macos_hid_operation_lock(&self) -> Option<std::sync::MutexGuard<'static, ()>> {
        match self {
            Self::Vial(hid) => hid.macos_hid_operation_lock(),
            Self::Rynk(_) => None,
        }
    }

    pub(crate) fn is_transport_available(&self) -> bool {
        match self {
            Self::Vial(hid) => hid.is_transport_available(),
            Self::Rynk(session) => session.is_transport_available(),
        }
    }

    /// A Rynk board whose link broke: nothing reaches the board through this
    /// session any more, even while the board stays plugged in.
    pub(crate) fn link_lost(&self) -> bool {
        self.as_rynk()
            .is_some_and(|session| !session.is_transport_available())
    }

    /// Rynk host data waits its turn between requests, so only Vial pauses it.
    pub(crate) fn with_host_output_paused_for_settings<T>(
        &self,
        operation: impl FnOnce() -> T,
    ) -> T {
        match self {
            Self::Vial(hid) => hid.with_host_output_paused_for_settings(operation),
            Self::Rynk(_) => operation(),
        }
    }

    pub(crate) fn set_keycode(&self, layer: u8, row: u8, col: u8, keycode: u16) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_keycode(layer, row, col, keycode),
            Self::Rynk(session) => {
                session.set_key_binding(layer, row, col, KeyBinding::Vial(keycode))
            }
        }
    }

    pub(crate) fn set_rmk_key_action(
        &self,
        layer: u8,
        row: u8,
        col: u8,
        action: rmk_types::action::KeyAction,
    ) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_rmk_key_action(layer, row, col, action),
            Self::Rynk(session) => {
                session.set_key_binding(layer, row, col, KeyBinding::Rmk(action))
            }
        }
    }

    pub(crate) fn set_key_binding(
        &self,
        layer: u8,
        row: u8,
        col: u8,
        binding: KeyBinding,
    ) -> Result<()> {
        if let Self::Rynk(session) = self {
            return session.set_key_binding(layer, row, col, binding);
        }
        match binding {
            KeyBinding::Vial(keycode) => self.set_keycode(layer, row, col, keycode),
            KeyBinding::Rmk(action) => self.set_rmk_key_action(layer, row, col, action),
            binding @ KeyBinding::VialTapHold { .. } => self
                .require_vial()?
                .set_rmk_key_action(layer, row, col, binding),
            KeyBinding::Rynk(_) => {
                anyhow::bail!("Rynk key action cannot be written to a Vial board")
            }
        }
    }

    /// Lands what the board staged for flash. Vial boards write as they go, so
    /// for them there is nothing to wait for; `progress` counts what is still
    /// pending on boards that stage.
    pub(crate) fn commit_writes(&self, progress: &std::sync::atomic::AtomicU32) -> Result<()> {
        match self {
            Self::Vial(_) => Ok(()),
            Self::Rynk(session) => session.commit_writes(progress),
        }
    }

    /// What a Rynk board can hold and its morse keys as read, to check an
    /// import before it writes anything. `None` for Vial boards.
    pub(crate) fn rynk_import_context(
        &self,
    ) -> Option<(
        rmk_types_rynk::protocol::rynk::DeviceCapabilities,
        Vec<rmk_types_rynk::morse::Morse>,
    )> {
        match self {
            Self::Vial(_) => None,
            Self::Rynk(session) => Some((*session.capabilities(), session.morses())),
        }
    }

    pub(crate) fn set_encoder(
        &self,
        layer: u8,
        idx: u8,
        direction: u8,
        keycode: u16,
    ) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_encoder(layer, idx, direction, keycode),
            Self::Rynk(session) => session.set_encoder(layer, idx, direction, keycode),
        }
    }

    pub(crate) fn get_macro_buffer_size(&self) -> Result<u16> {
        match self {
            Self::Vial(hid) => hid.get_macro_buffer_size(),
            Self::Rynk(session) => Ok(session.capabilities().macro_space_size),
        }
    }

    pub(crate) fn get_macro_buffer(&self, size: u16, count: u8) -> Result<Vec<u8>> {
        match self {
            Self::Vial(hid) => hid.get_macro_buffer(size, count),
            Self::Rynk(session) => session.macro_buffer(size, count),
        }
    }

    /// The macros in Vial's buffer for `set_macro_buffer`. A Vial board's
    /// buffer is `size` bytes; a Rynk board counts its own pool of ops when it
    /// takes them, so Vial's bytes are no limit there.
    pub(crate) fn encode_macros(&self, macros: &[Vec<u8>], size: u16) -> Result<Vec<u8>> {
        let size = match self {
            Self::Vial(_) => size,
            Self::Rynk(_) => u16::MAX,
        };
        crate::hid::HidDevice::encode_macros(macros, size)
    }

    /// Writes the macro buffer and returns, by macro, what of it the board
    /// does not run: a Vial board stores the bytes as they are.
    pub(crate) fn set_macro_buffer(
        &self,
        data: &[u8],
    ) -> Result<Vec<(usize, crate::rynk::macros::MacroLoss)>> {
        match self {
            Self::Vial(hid) => hid.set_macro_buffer(data).map(|()| Vec::new()),
            Self::Rynk(session) => session.set_macro_buffer(data),
        }
    }

    /// Writes what changed of a combo: Vial takes keys and output apart from
    /// the layer, a Rynk board the whole combo in one write.
    pub(crate) fn set_combo_entry(
        &self,
        idx: u8,
        entry: &crate::app::ComboEntry,
        binding_changed: bool,
        layer_changed: bool,
    ) -> Result<()> {
        match self {
            Self::Vial(hid) => {
                if binding_changed {
                    hid.set_combo_binding(idx, entry.keys, entry.output)?;
                }
                if layer_changed {
                    hid.set_rmk_combo_layer(idx, entry.layer)?;
                }
                Ok(())
            }
            Self::Rynk(session) if binding_changed || layer_changed => {
                session.set_combo(idx, entry)
            }
            Self::Rynk(_) => Ok(()),
        }
    }

    pub(crate) fn set_tap_dance_bindings(
        &self,
        idx: u8,
        actions: [KeyBinding; 4],
        tapping_term: u16,
    ) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_tap_dance_bindings(idx, actions, tapping_term),
            Self::Rynk(session) => {
                let [on_tap, on_hold, on_double_tap, on_tap_hold] = actions;
                session.set_tap_dance(
                    idx,
                    &crate::keycode_picker::TapDanceEntry {
                        on_tap,
                        on_hold,
                        on_double_tap,
                        on_tap_hold,
                        tapping_term,
                    },
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set_key_override(
        &self,
        idx: u8,
        trigger: u16,
        replacement: u16,
        layers: u16,
        trigger_mods: u8,
        negative_mod_mask: u8,
        suppressed_mods: u8,
        options: u8,
    ) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_key_override(
                idx,
                trigger,
                replacement,
                layers,
                trigger_mods,
                negative_mod_mask,
                suppressed_mods,
                options,
            ),
            Self::Rynk(session) => session.set_key_override(
                idx,
                &crate::app::KeyOverrideEntry {
                    trigger,
                    replacement,
                    layers,
                    trigger_mods,
                    negative_mod_mask,
                    suppressed_mods,
                    options: crate::app::KeyOverrideOptionsState::from_bits(options),
                },
            ),
        }
    }

    pub(crate) fn set_alt_repeat_key(
        &self,
        idx: u8,
        keycode: u16,
        alt_keycode: u16,
        allowed_mods: u8,
        options: u8,
    ) -> Result<()> {
        match self {
            Self::Vial(hid) => {
                hid.set_alt_repeat_key(idx, keycode, alt_keycode, allowed_mods, options)
            }
            Self::Rynk(_) => bail!("Rynk boards have no alt repeat keys"),
        }
    }

    pub(crate) fn get_qmk_setting_u8(&self, qsid: u16) -> Result<u8> {
        match self {
            Self::Vial(hid) => hid.get_qmk_setting_u8(qsid),
            Self::Rynk(session) => session.get_qmk_setting_u8(qsid),
        }
    }

    pub(crate) fn set_qmk_setting_u8(&self, qsid: u16, value: u8) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_qmk_setting_u8(qsid, value),
            Self::Rynk(session) => session.set_setting(qsid, vec![value]),
        }
    }

    pub(crate) fn get_qmk_setting_u16(&self, qsid: u16) -> Result<u16> {
        match self {
            Self::Vial(hid) => hid.get_qmk_setting_u16(qsid),
            Self::Rynk(session) => session.get_qmk_setting_u16(qsid),
        }
    }

    pub(crate) fn set_qmk_setting_u16(&self, qsid: u16, value: u16) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_qmk_setting_u16(qsid, value),
            Self::Rynk(session) => session.set_setting(qsid, value.to_le_bytes().to_vec()),
        }
    }

    pub(crate) fn get_qmk_setting_string(&self, qsid: u16) -> Result<String> {
        match self {
            Self::Vial(hid) => hid.get_qmk_setting_string(qsid),
            Self::Rynk(session) => session.get_qmk_setting_string(qsid),
        }
    }

    pub(crate) fn set_qmk_setting_string(&self, qsid: u16, value: &str) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_qmk_setting_string(qsid, value),
            Self::Rynk(session) => {
                session.set_setting(qsid, crate::hid::qmk_setting_string_value(value))
            }
        }
    }

    pub(crate) fn set_layout_options(&self, options: u32) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.set_layout_options(options),
            Self::Rynk(session) => session.set_layout_options(options),
        }
    }

    pub(crate) fn get_unlock_status_with_progress(&self) -> Result<UnlockStatus> {
        match self {
            Self::Vial(hid) => hid.get_unlock_status_with_progress(),
            Self::Rynk(session) => session.unlock_status(),
        }
    }

    pub(crate) fn unlock_start(&self) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.unlock_start(),
            // A Rynk board starts unlocking at its first poll.
            Self::Rynk(session) => session.unlock_poll().map(|_| ()),
        }
    }

    pub(crate) fn unlock_poll(&self) -> Result<(bool, bool, u8)> {
        match self {
            Self::Vial(hid) => hid.unlock_poll(),
            Self::Rynk(session) => session.unlock_poll(),
        }
    }

    pub(crate) fn lock(&self) -> Result<()> {
        match self {
            Self::Vial(hid) => hid.lock(),
            Self::Rynk(session) => session.lock(),
        }
    }

    pub(crate) fn get_switch_matrix(&self, rows: usize, cols: usize) -> Result<Vec<bool>> {
        match self {
            Self::Vial(hid) => hid.get_switch_matrix(rows, cols),
            Self::Rynk(session) => session.switch_matrix(rows, cols),
        }
    }

    pub(crate) fn get_battery_halves(&self) -> Result<Option<crate::hid::BatteryHalves>> {
        match self {
            Self::Vial(hid) => hid.get_battery_halves(),
            Self::Rynk(session) => session.battery_halves().map(Some),
        }
    }
}
