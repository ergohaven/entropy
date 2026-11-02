//! The Rynk side of [`DeviceSession`]: the editor's Vial-shaped values turned
//! into Rynk requests on the board's shared link.
//!
//! Tap dances and macros are written as a whole on Rynk, while the editor
//! hands over a part (four tap dance patterns, the macro buffer). The session
//! keeps what it last read or wrote of them, so a write sends the rest as the
//! board has it and leaves unchanged macros alone.
//!
//! [`DeviceSession`]: crate::device_session::DeviceSession

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rmk_types_rynk::action::{EncoderAction, KeyAction};
use rmk_types_rynk::combo::Combo;
use rmk_types_rynk::fork::Fork;
use rmk_types_rynk::keyboard_macros::MacroOp;
use rmk_types_rynk::morse::Morse;
use rmk_types_rynk::protocol::rynk::ergohaven::ErgohavenInfo;
use rmk_types_rynk::protocol::rynk::{
    command, DeviceCapabilities, GetEncoderRequest, KeyPosition, SetComboRequest,
    SetEncoderRequest, SetForkRequest, SetKeyRequest, SetMorseRequest,
};

use super::link::RynkLink;
use super::macros::MacroLoss;
use super::{dynamic, keycode, macros};
use crate::app::{ComboEntry, KeyOverrideEntry};
use crate::hid::BatteryHalves;
use crate::keyboard::KeyBinding;
use crate::keycode_picker::TapDanceEntry;

/// How often a commit asks whether staged writes reached flash, and how long
/// it waits for them: the firmware itself drains for up to a minute.
const COMMIT_POLL: Duration = Duration::from_millis(100);
const COMMIT_LIMIT: Duration = Duration::from_secs(60);

pub(crate) struct RynkSession {
    link: Arc<RynkLink>,
    morses: Mutex<Vec<Option<Morse>>>,
    macros: Mutex<Vec<Option<Vec<MacroOp>>>>,
    /// Settings read ahead in one go while connecting, by id; a write drops
    /// its id. Empty once the connect is done.
    read_ahead: Mutex<HashMap<u16, Vec<u8>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl RynkSession {
    pub(crate) fn new(link: Arc<RynkLink>) -> Self {
        Self {
            link,
            morses: Mutex::new(Vec::new()),
            macros: Mutex::new(Vec::new()),
            read_ahead: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn link(&self) -> &Arc<RynkLink> {
        &self.link
    }

    pub(crate) fn info(&self) -> &ErgohavenInfo {
        &self.link.board().info
    }

    pub(crate) fn capabilities(&self) -> &DeviceCapabilities {
        &self.link.board().capabilities
    }

    pub(crate) fn is_transport_available(&self) -> bool {
        self.link.is_open()
    }

    pub(crate) fn set_key_binding(
        &self,
        layer: u8,
        row: u8,
        col: u8,
        binding: KeyBinding,
    ) -> Result<()> {
        let action = keycode::binding_action(binding)?;
        self.link.request::<command::SetKeyAction>(SetKeyRequest {
            position: KeyPosition { layer, row, col },
            action,
        })?;
        Ok(())
    }

    /// Vial sets one direction, 1 being clockwise; the board stores both, so
    /// the other one is written back as read.
    pub(crate) fn set_encoder(
        &self,
        layer: u8,
        idx: u8,
        direction: u8,
        keycode: u16,
    ) -> Result<()> {
        let action = keycode::binding_action(KeyBinding::Vial(keycode))?;
        self.link.run(move |client| {
            let request = GetEncoderRequest {
                encoder_id: idx,
                layer,
            };
            let mut encoder = client.request::<command::GetEncoderAction>(&request)?;
            if direction == 1 {
                encoder.clockwise = action;
            } else {
                encoder.counter_clockwise = action;
            }
            client.request::<command::SetEncoderAction>(&SetEncoderRequest {
                encoder_id: idx,
                layer,
                action: encoder,
            })
        })?;
        Ok(())
    }

    /// Keys, output and layer of a combo in one write.
    pub(crate) fn set_combo(&self, idx: u8, entry: &ComboEntry) -> Result<()> {
        let config = dynamic::combo(entry, self.capabilities().max_combo_keys as usize)?;
        self.link
            .request::<command::SetCombo>(SetComboRequest { index: idx, config })?;
        Ok(())
    }

    /// The morse keys as the board has them, remembered for the tap dance
    /// writes that follow.
    pub(crate) fn read_morses(&self) -> Result<Vec<Morse>> {
        let count = self.capabilities().max_morse;
        let morses = self.link.run(move |client| client.read_morses(count))?;
        *lock(&self.morses) = morses.iter().cloned().map(Some).collect();
        Ok(morses)
    }

    pub(crate) fn morses(&self) -> Vec<Morse> {
        lock(&self.morses).iter().flatten().cloned().collect()
    }

    /// A tap dance's four patterns and term over the morse key, whose other
    /// patterns are kept.
    pub(crate) fn set_tap_dance(&self, idx: u8, entry: &TapDanceEntry) -> Result<()> {
        let mut morses = lock(&self.morses);
        let known = morses.get(idx as usize).cloned().flatten();
        let current = match known {
            Some(morse) => morse,
            None => self.link.request::<command::GetMorse>(idx)?,
        };
        let max_patterns = self.capabilities().max_patterns_per_key as usize;
        let config = dynamic::merge_tap_dance(&current, entry, max_patterns)?;
        self.link.request::<command::SetMorse>(SetMorseRequest {
            index: idx,
            config: config.clone(),
        })?;
        if morses.len() <= idx as usize {
            morses.resize(idx as usize + 1, None);
        }
        morses[idx as usize] = Some(config);
        Ok(())
    }

    pub(crate) fn set_key_override(&self, idx: u8, entry: &KeyOverrideEntry) -> Result<()> {
        let config = dynamic::fork(entry)?;
        self.link
            .request::<command::SetFork>(SetForkRequest { index: idx, config })?;
        Ok(())
    }

    /// The board's macros in Vial's buffer: each one's bytes and a NUL, then
    /// zeros up to `size` when they are shorter.
    pub(crate) fn macro_buffer(&self, size: u16, count: u8) -> Result<Vec<u8>> {
        let count = count.min(self.capabilities().max_macros);
        let read = self.link.run(move |client| client.read_macros(count))?;
        let mut buffer = Vec::new();
        for ops in &read {
            buffer.extend(macros::macro_vial_bytes(ops).0);
            buffer.push(0);
        }
        if buffer.len() < size as usize {
            buffer.resize(size as usize, 0);
        }
        *lock(&self.macros) = read.into_iter().map(Some).collect();
        Ok(buffer)
    }

    /// Writes the macros of a Vial buffer that differ from what the board
    /// was last known to hold, and returns what of the buffer the board does
    /// not run, by macro: the editor keeps Vial bytes, which can say more.
    pub(crate) fn set_macro_buffer(&self, data: &[u8]) -> Result<Vec<(usize, MacroLoss)>> {
        if !self.capabilities().macros_writable {
            bail!("this keyboard keeps its macros read-only");
        }
        let max = self.capabilities().max_macros as usize;
        let mut known = lock(&self.macros);
        let mut losses = Vec::new();
        let wanted: Vec<Vec<MacroOp>> = data
            .split(|&byte| byte == 0)
            .take(max)
            .enumerate()
            .map(|(index, bytes)| {
                let (ops, lost) = macros::vial_macro_ops(bytes);
                losses.extend(lost.into_iter().map(|loss| (index, loss)));
                ops
            })
            .collect();
        // The board would refuse the SetMacro that passes its pool, after the
        // ones before it went through.
        let used = macros::macro_space_used(wanted.iter().map(Vec::as_slice), max);
        let available = self.capabilities().macro_space_size as usize;
        if used > available {
            bail!("macros need {used} bytes, but the keyboard's macro memory is {available} bytes");
        }
        let changed: Vec<(u8, Vec<MacroOp>)> = wanted
            .iter()
            .enumerate()
            .filter(|(index, ops)| known.get(*index).cloned().flatten().as_ref() != Some(*ops))
            .map(|(index, ops)| (index as u8, ops.clone()))
            .collect();
        let written = changed.clone();
        self.link.run(move |client| {
            for (index, ops) in &written {
                client.write_macro(*index, ops)?;
            }
            Ok(())
        })?;
        for (index, ops) in changed {
            if known.len() <= index as usize {
                known.resize(index as usize + 1, None);
            }
            known[index as usize] = Some(ops);
        }
        Ok(losses)
    }

    /// Every key of the first `layers` layers, row by row, a layer each.
    pub(crate) fn keymap(&self, layers: u8) -> Result<Vec<Vec<KeyAction>>> {
        let (rows, cols) = (self.capabilities().num_rows, self.capabilities().num_cols);
        let keys = self
            .link
            .run(move |client| client.read_keymap(layers, rows, cols))?;
        let layer_len = (rows as usize * cols as usize).max(1);
        Ok(keys.chunks(layer_len).map(<[KeyAction]>::to_vec).collect())
    }

    /// The first `count` encoders of the first `layers` layers, a layer each.
    pub(crate) fn encoders(&self, layers: u8, count: u8) -> Result<Vec<Vec<EncoderAction>>> {
        let requests: Vec<GetEncoderRequest> = (0..layers)
            .flat_map(|layer| {
                (0..count).map(move |encoder_id| GetEncoderRequest { encoder_id, layer })
            })
            .collect();
        let actions = self
            .link
            .run(move |client| client.request_all::<command::GetEncoderAction>(&requests))?;
        Ok(actions
            .chunks(usize::from(count).max(1))
            .map(<[EncoderAction]>::to_vec)
            .collect())
    }

    pub(crate) fn read_combos(&self) -> Result<Vec<Combo>> {
        let count = self.capabilities().max_combos;
        Ok(self.link.run(move |client| client.read_combos(count))?)
    }

    pub(crate) fn read_forks(&self) -> Result<Vec<Fork>> {
        let indices: Vec<u8> = (0..self.capabilities().max_forks).collect();
        Ok(self
            .link
            .run(move |client| client.request_all::<command::GetFork>(&indices))?)
    }

    pub(crate) fn layout_options(&self) -> Result<u32> {
        Ok(self.link.run(|client| client.layout_options())?)
    }

    /// A setting in the bytes Vial carries for the same id.
    pub(crate) fn setting(&self, qsid: u16) -> Result<Vec<u8>> {
        if let Some(value) = lock(&self.read_ahead).get(&qsid) {
            return Ok(value.clone());
        }
        Ok(self.link.run(move |client| client.setting(qsid))?)
    }

    /// Reads the settings `ids` name in one go for the reads of a connect
    /// that follow, which over BLE would each wait for their own reply.
    pub(crate) fn read_settings_ahead(&self, ids: &[u16]) -> Result<()> {
        let wanted = ids.to_vec();
        let values = self.link.run(move |client| client.settings(&wanted))?;
        lock(&self.read_ahead).extend(ids.iter().copied().zip(values));
        Ok(())
    }

    /// Ends the read-ahead: later reads ask the board again.
    pub(crate) fn forget_settings_read_ahead(&self) {
        lock(&self.read_ahead).clear();
    }

    /// Every setting id the board serves.
    pub(crate) fn setting_keys(&self) -> Result<Vec<u16>> {
        Ok(self.link.run(|client| client.setting_keys())?)
    }

    pub(crate) fn get_qmk_setting_u8(&self, qsid: u16) -> Result<u8> {
        self.setting(qsid)?
            .first()
            .copied()
            .with_context(|| format!("setting {qsid} came back empty"))
    }

    /// The board sends a setting's own length, so a one-byte value reads with
    /// a zero high byte, as Vial's zero-padded report does.
    pub(crate) fn get_qmk_setting_u16(&self, qsid: u16) -> Result<u16> {
        let value = self.setting(qsid)?;
        let low = *value
            .first()
            .with_context(|| format!("setting {qsid} came back empty"))?;
        Ok(u16::from_le_bytes([
            low,
            value.get(1).copied().unwrap_or(0),
        ]))
    }

    pub(crate) fn get_qmk_setting_string(&self, qsid: u16) -> Result<String> {
        let value = self.setting(qsid)?;
        let end = value
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(value.len());
        let text = std::str::from_utf8(&value[..end])
            .with_context(|| format!("setting {qsid} is not UTF-8"))?;
        Ok(text.trim().to_owned())
    }

    pub(crate) fn set_setting(&self, qsid: u16, value: Vec<u8>) -> Result<()> {
        lock(&self.read_ahead).remove(&qsid);
        Ok(self
            .link
            .run(move |client| client.set_setting(qsid, &value))?)
    }

    pub(crate) fn set_layout_options(&self, options: u32) -> Result<()> {
        Ok(self
            .link
            .run(move |client| client.set_layout_options(options))?)
    }

    /// Unlocked, unlocking, and the keys to hold.
    pub(crate) fn unlock_status(&self) -> Result<crate::device_session::UnlockStatus> {
        let status = self.link.request::<command::GetLockStatus>(())?;
        Ok((
            !status.locked,
            status.unlocking,
            status.key_positions.to_vec(),
        ))
    }

    /// Unlocked, unlocking, and how many keys are still to be held.
    pub(crate) fn unlock_poll(&self) -> Result<(bool, bool, u8)> {
        let status = self.link.request::<command::UnlockPoll>(())?;
        Ok((!status.locked, status.unlocking, status.remaining_keys))
    }

    pub(crate) fn lock(&self) -> Result<()> {
        Ok(self.link.request::<command::Lock>(())?)
    }

    /// Pressed keys, row by row, from the board's bitmap: each row takes
    /// whole bytes, column 0 in bit 0.
    pub(crate) fn switch_matrix(&self, rows: usize, cols: usize) -> Result<Vec<bool>> {
        let bitmap = self
            .link
            .request::<command::GetMatrixState>(())?
            .pressed_bitmap;
        let stride = cols.div_ceil(8);
        Ok((0..rows * cols)
            .map(|at| {
                let (row, col) = (at / cols, at % cols);
                bitmap
                    .get(row * stride + col / 8)
                    .is_some_and(|byte| byte >> (col % 8) & 1 != 0)
            })
            .collect())
    }

    pub(crate) fn battery_halves(&self) -> Result<BatteryHalves> {
        let halves = self.link.run(|client| client.battery_halves())?;
        let level = |known: bool, level: u8| (known && level != 0xFF).then_some(level);
        Ok(BatteryHalves {
            left: level(halves.mask & 1 != 0, halves.left),
            right: level(halves.mask & 2 != 0, halves.right),
        })
    }

    /// Lands staged writes, keeping `progress` at the count still pending.
    pub(crate) fn commit_writes(&self, progress: &AtomicU32) -> Result<()> {
        let (sender, statuses) = std::sync::mpsc::channel();
        let committed = self.link.run(move |client| {
            client.commit_writes(COMMIT_POLL, COMMIT_LIMIT, |status| {
                let _ = sender.send(status);
            })
        });
        for status in statuses.try_iter() {
            let pending = status.pending_keys as u32 + status.pending_macro_chunks as u32;
            progress.store(pending, Ordering::Relaxed);
        }
        committed.context("staged writes did not reach flash")
    }
}

/// A board that keeps what it is sent, for tests of the session and connect.
#[cfg(test)]
pub(crate) mod testing {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use rmk_types_rynk::action::{EncoderAction, KeyAction};
    use rmk_types_rynk::combo::Combo;
    use rmk_types_rynk::fork::Fork;
    use rmk_types_rynk::keyboard_macros::{Macro, MacroOp};
    use rmk_types_rynk::morse::Morse;
    use rmk_types_rynk::protocol::rynk::ergohaven::{
        BatteryHalves, ErgohavenInfo, SetSettingRequest, SettingKeysPage, SettingValue,
        WriteStatus, ERGOHAVEN_RYNK_REVISION,
    };
    use rmk_types_rynk::protocol::rynk::{
        Cmd, DeviceCapabilities, GetComboBulkRequest, GetComboBulkResponse, GetEncoderRequest,
        GetKeymapBulkRequest, GetKeymapBulkResponse, GetMorseBulkRequest, GetMorseBulkResponse,
        KeyPosition, LayoutChunk, LockStatus, MatrixState, ProtocolVersion, RynkError, RynkHeader,
        SetComboRequest, SetEncoderRequest, SetForkRequest, SetKeyRequest, SetMacroRequest,
        SetMorseRequest,
    };
    use serde::de::DeserializeOwned;

    use super::super::client::fnv1a;
    use super::super::client::testing::{capabilities, err, info, ok, quick, FakeBoard};
    use super::super::link::{BoxedTransport, PhysicalId, RynkLink};
    use super::super::read_only::is_read_cmd;

    /// What the board holds; tests read it back to see what was written.
    #[derive(Default)]
    pub(crate) struct Stored {
        pub(crate) caps: u32,
        pub(crate) keyboard_uid: [u8; 8],
        pub(crate) definition: Vec<u8>,
        pub(crate) capabilities: DeviceCapabilities,
        /// Layer after layer, each row by row.
        pub(crate) keymap: Vec<KeyAction>,
        pub(crate) encoders: BTreeMap<(u8, u8), EncoderAction>,
        pub(crate) combos: Vec<Combo>,
        pub(crate) morses: Vec<Morse>,
        pub(crate) forks: Vec<Fork>,
        pub(crate) macros: Vec<Vec<MacroOp>>,
        pub(crate) settings: BTreeMap<u16, Vec<u8>>,
        pub(crate) layout_options: u32,
        pub(crate) lock: Option<LockStatus>,
        pub(crate) matrix: Vec<u8>,
        pub(crate) battery: Option<BatteryHalves>,
        /// Commands after the handshake, in order.
        pub(crate) received: Vec<Cmd>,
    }

    impl Stored {
        pub(crate) fn new(rows: u8, cols: u8, layers: u8) -> Self {
            Self {
                capabilities: DeviceCapabilities {
                    num_layers: layers,
                    num_rows: rows,
                    num_cols: cols,
                    max_combo_keys: 4,
                    macro_space_size: 256,
                    macros_writable: true,
                    max_patterns_per_key: 8,
                    ..capabilities(512)
                },
                keymap: vec![KeyAction::No; rows as usize * cols as usize * layers as usize],
                ..Default::default()
            }
        }

        /// Commands received that change what the board holds.
        pub(crate) fn writes(&self) -> Vec<Cmd> {
            self.received
                .iter()
                .copied()
                // Battery readings ask the halves for fresh ones but store nothing.
                .filter(|&cmd| !is_read_cmd(cmd) && cmd != Cmd::GetBatteryHalves)
                .collect()
        }
    }

    fn request<T: DeserializeOwned>(payload: &[u8]) -> T {
        postcard::from_bytes(payload).unwrap()
    }

    fn serve(stored: &mut Stored, header: RynkHeader, payload: &[u8]) -> Vec<u8> {
        let cmd = header.cmd;
        match cmd {
            Cmd::GetVersion => return ok(header, ProtocolVersion::CURRENT),
            Cmd::GetErgohavenInfo => {
                return ok(
                    header,
                    ErgohavenInfo {
                        caps: stored.caps,
                        keyboard_uid: stored.keyboard_uid,
                        definition_hash: fnv1a(&stored.definition),
                        ..info(ERGOHAVEN_RYNK_REVISION)
                    },
                )
            }
            Cmd::GetCapabilities => return ok(header, stored.capabilities),
            _ => stored.received.push(cmd),
        }
        let cols = stored.capabilities.num_cols as usize;
        let layer_len = stored.capabilities.num_rows as usize * cols;
        match cmd {
            Cmd::GetBoardDefinition => {
                let offset = request::<u32>(payload) as usize;
                let end = (offset + 200).min(stored.definition.len());
                ok(
                    header,
                    LayoutChunk {
                        total_len: stored.definition.len() as u32,
                        bytes: stored.definition[offset..end].try_into().unwrap(),
                    },
                )
            }
            Cmd::GetSettingKeys => {
                let after = request::<u16>(payload);
                let keys = stored.settings.keys().copied().filter(|&id| id > after);
                ok(header, SettingKeysPage(keys.take(4).collect()))
            }
            Cmd::GetSetting => match stored.settings.get(&request::<u16>(payload)) {
                Some(value) => ok(header, SettingValue(value.as_slice().try_into().unwrap())),
                None => err(header, RynkError::Invalid),
            },
            Cmd::SetSetting => {
                let set: SetSettingRequest = request(payload);
                stored.settings.insert(set.id, set.value.0.to_vec());
                ok(header, ())
            }
            Cmd::GetKeymapBulk => {
                let at: GetKeymapBulkRequest = request(payload);
                let start = at.layer as usize * layer_len
                    + at.start_row as usize * cols
                    + at.start_col as usize;
                // As the firmware pages: on across layers, to the keymap's end.
                let end = (start + 3).min(stored.keymap.len());
                let actions = stored.keymap[start..end].to_vec();
                ok(header, GetKeymapBulkResponse { actions })
            }
            Cmd::GetKeyAction => {
                let at: KeyPosition = request(payload);
                let index =
                    at.layer as usize * layer_len + at.row as usize * cols + at.col as usize;
                ok(header, stored.keymap[index])
            }
            Cmd::SetKeyAction => {
                let set: SetKeyRequest = request(payload);
                let at = set.position;
                let index =
                    at.layer as usize * layer_len + at.row as usize * cols + at.col as usize;
                stored.keymap[index] = set.action;
                ok(header, ())
            }
            Cmd::GetEncoderAction => {
                let at: GetEncoderRequest = request(payload);
                let encoder = stored.encoders.get(&(at.layer, at.encoder_id)).copied();
                ok(
                    header,
                    encoder.unwrap_or(EncoderAction::new(KeyAction::No, KeyAction::No)),
                )
            }
            Cmd::SetEncoderAction => {
                let set: SetEncoderRequest = request(payload);
                stored
                    .encoders
                    .insert((set.layer, set.encoder_id), set.action);
                ok(header, ())
            }
            Cmd::GetComboBulk => {
                let at: GetComboBulkRequest = request(payload);
                let configs = stored.combos.iter().skip(at.start_index as usize).take(2);
                ok(
                    header,
                    GetComboBulkResponse {
                        configs: configs.cloned().collect(),
                    },
                )
            }
            Cmd::SetCombo => {
                let set: SetComboRequest = request(payload);
                stored.combos[set.index as usize] = set.config;
                ok(header, ())
            }
            Cmd::GetMorseBulk => {
                let at: GetMorseBulkRequest = request(payload);
                let configs = stored.morses.iter().skip(at.start_index as usize).take(2);
                ok(
                    header,
                    GetMorseBulkResponse {
                        configs: configs.cloned().collect(),
                    },
                )
            }
            Cmd::GetMorse => ok(
                header,
                stored.morses[request::<u8>(payload) as usize].clone(),
            ),
            Cmd::SetMorse => {
                let set: SetMorseRequest = request(payload);
                stored.morses[set.index as usize] = set.config;
                ok(header, ())
            }
            Cmd::GetFork => ok(header, stored.forks[request::<u8>(payload) as usize]),
            Cmd::SetFork => {
                let set: SetForkRequest = request(payload);
                stored.forks[set.index as usize] = set.config;
                ok(header, ())
            }
            Cmd::GetMacro => {
                let ops = &stored.macros[request::<u8>(payload) as usize];
                ok(header, Macro::from_slice(ops).unwrap())
            }
            Cmd::SetMacro => {
                let set: SetMacroRequest = request(payload);
                stored.macros[set.index as usize] = set.macro_ops.ops().collect();
                ok(header, ())
            }
            Cmd::GetLayoutOption => ok(header, stored.layout_options),
            Cmd::SetLayoutOption => {
                stored.layout_options = request(payload);
                ok(header, ())
            }
            Cmd::GetBatteryHalves => match stored.battery {
                Some(halves) => ok(header, halves),
                None => err(header, RynkError::Invalid),
            },
            Cmd::GetLockStatus | Cmd::UnlockPoll => ok(
                header,
                stored.lock.clone().unwrap_or(LockStatus {
                    locked: false,
                    unlocking: false,
                    remaining_keys: 0,
                    key_positions: Default::default(),
                }),
            ),
            Cmd::GetMatrixState => ok(
                header,
                MatrixState {
                    pressed_bitmap: stored.matrix.as_slice().try_into().unwrap(),
                },
            ),
            Cmd::CommitWrites | Cmd::GetWriteStatus => ok(
                header,
                WriteStatus {
                    pending_keys: 0,
                    pending_macro_chunks: 0,
                    fault: false,
                },
            ),
            other => panic!("the test board does not serve {other:?}"),
        }
    }

    /// A board serving `stored`, and the state it keeps.
    pub(crate) fn board(stored: Stored) -> (FakeBoard, Arc<Mutex<Stored>>) {
        let state = Arc::new(Mutex::new(stored));
        let shared = state.clone();
        let board = FakeBoard::new(move |header, payload| {
            vec![serve(&mut shared.lock().unwrap(), header, payload)]
        });
        (board, state)
    }

    /// The board's link under `id`. Holding it makes a connect that opens
    /// `id` share it instead of looking for the board on a bus.
    pub(crate) fn link(id: &str, board: &FakeBoard) -> Arc<RynkLink> {
        RynkLink::acquire(PhysicalId(id.to_owned()), quick(), || {
            Ok(Box::new(board.transport()) as BoxedTransport)
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use rmk_types_rynk::action::Action;
    use rmk_types_rynk::keycode::{HidKeyCode, KeyCode};
    use rmk_types_rynk::morse::{MorsePattern, HOLD, TAP};
    use rmk_types_rynk::protocol::rynk::Cmd;

    use super::testing::{board, link, Stored};
    use super::*;
    use crate::device_session::DeviceSession;

    const KC_A: u16 = 0x04;
    const KC_B: u16 = 0x05;
    const KC_C: u16 = 0x06;
    fn triple_tap() -> MorsePattern {
        MorsePattern::from_u16(0b1000)
    }

    fn key(code: HidKeyCode) -> KeyAction {
        KeyAction::Single(Action::Key(KeyCode::Hid(code)))
    }

    fn session(name: &str, stored: Stored) -> (RynkSession, Arc<Mutex<Stored>>) {
        let (board, state) = board(stored);
        let session = RynkSession::new(link(&format!("session-{name}"), &board));
        (session, state)
    }

    #[test]
    fn a_combo_goes_to_the_board_whole_in_one_write() {
        let mut stored = Stored::new(1, 2, 1);
        stored.combos = vec![Combo::empty(); 2];
        let (session, state) = session("combo", stored);
        let entry = ComboEntry {
            keys: [KC_A, KC_B, 0, 0],
            output: KeyBinding::Vial(KC_C),
            layer: Some(1),
        };
        let device = DeviceSession::from(session);

        device.set_combo_entry(1, &entry, true, true).unwrap();
        device.set_combo_entry(1, &entry, false, false).unwrap();

        let state = state.lock().unwrap();
        assert_eq!(state.writes(), [Cmd::SetCombo]);
        let combo = &state.combos[1];
        assert_eq!(
            combo.actions.as_slice(),
            [key(HidKeyCode::A), key(HidKeyCode::B)]
        );
        assert_eq!(combo.output, key(HidKeyCode::C));
        assert_eq!(combo.layer, Some(1));
    }

    #[test]
    fn a_tap_dance_write_keeps_the_patterns_the_editor_cannot_show() {
        let mut stored = Stored::new(1, 2, 1);
        let mut morse = Morse::default();
        morse
            .put(TAP, Action::Key(KeyCode::Hid(HidKeyCode::A)))
            .unwrap();
        morse
            .put(triple_tap(), Action::Key(KeyCode::Hid(HidKeyCode::Z)))
            .unwrap();
        stored.morses = vec![morse];
        stored.capabilities.max_morse = 1;
        let (session, state) = session("tap-dance", stored);
        session.read_morses().unwrap();

        let device = DeviceSession::from(session);
        let fields = [
            KeyBinding::Vial(KC_B),
            KeyBinding::Vial(KC_C),
            KeyBinding::Vial(0),
            KeyBinding::Vial(0),
        ];
        device.set_tap_dance_bindings(0, fields, 250).unwrap();

        let state = state.lock().unwrap();
        let written = &state.morses[0];
        assert_eq!(
            written.get(TAP),
            Some(Action::Key(KeyCode::Hid(HidKeyCode::B)))
        );
        assert_eq!(
            written.get(HOLD),
            Some(Action::Key(KeyCode::Hid(HidKeyCode::C)))
        );
        assert_eq!(
            written.get(triple_tap()),
            Some(Action::Key(KeyCode::Hid(HidKeyCode::Z))),
            "a pattern Vial has no field for is kept"
        );
        let device_morses = device.rynk_import_context().unwrap().1;
        assert_eq!(
            device_morses[0].get(TAP),
            Some(Action::Key(KeyCode::Hid(HidKeyCode::B))),
            "the session remembers what it wrote"
        );
    }

    #[test]
    fn only_the_macros_that_changed_are_written() {
        let mut stored = Stored::new(1, 2, 1);
        stored.macros = vec![
            vec![MacroOp::Char(b'h'), MacroOp::Char(b'i')],
            vec![MacroOp::Char(b'o')],
            Vec::new(),
        ];
        stored.capabilities.max_macros = 3;
        let (session, state) = session("macros", stored);
        let buffer = session.macro_buffer(32, 3).unwrap();
        assert_eq!(&buffer[..6], b"hi\0o\0\0");
        assert_eq!(buffer.len(), 32);

        let mut edited = buffer.clone();
        edited[3] = b'k';
        session.set_macro_buffer(&edited).unwrap();

        let state = state.lock().unwrap();
        assert_eq!(state.writes(), [Cmd::SetMacro]);
        assert_eq!(state.macros[1], [MacroOp::Char(b'k')]);
        assert_eq!(state.macros[0], [MacroOp::Char(b'h'), MacroOp::Char(b'i')]);
    }

    #[test]
    fn a_macro_write_returns_what_the_board_does_not_type() {
        let mut stored = Stored::new(1, 2, 1);
        stored.macros = vec![Vec::new(), Vec::new()];
        stored.capabilities.max_macros = 2;
        let (session, state) = session("macro losses", stored);

        let mut buffer = b"ok\0".to_vec();
        buffer.extend_from_slice("a\u{416}".as_bytes());
        buffer.push(0);
        let losses = session.set_macro_buffer(&buffer).unwrap();

        assert_eq!(
            losses,
            [(1, MacroLoss::Byte(0xD0)), (1, MacroLoss::Byte(0x96))],
            "the Cyrillic letter's two bytes, both in the second macro"
        );
        let state = state.lock().unwrap();
        assert_eq!(state.macros[0], [MacroOp::Char(b'o'), MacroOp::Char(b'k')]);
        assert_eq!(state.macros[1], [MacroOp::Char(b'a')]);
    }

    #[test]
    fn a_macro_write_past_the_pool_is_refused_before_any_write() {
        let mut stored = Stored::new(1, 2, 1);
        stored.macros = vec![Vec::new()];
        stored.capabilities.max_macros = 1;
        stored.capabilities.macro_space_size = 8;
        let (session, state) = session("macro pool", stored);

        // Four characters of two bytes each and the slot's length byte.
        let error = session.set_macro_buffer(b"abcd\0").unwrap_err();
        assert_eq!(
            error.to_string(),
            "macros need 9 bytes, but the keyboard's macro memory is 8 bytes"
        );
        assert!(state.lock().unwrap().writes().is_empty());

        session.set_macro_buffer(b"abc\0").unwrap();
        assert_eq!(state.lock().unwrap().macros[0].len(), 3);
    }

    #[test]
    fn vial_bytes_are_no_limit_for_a_rynk_board_pool() {
        let mut stored = Stored::new(1, 2, 1);
        stored.macros = vec![Vec::new()];
        stored.capabilities.max_macros = 1;
        stored.capabilities.macro_space_size = 16;
        let (session, state) = session("macro delays", stored);
        let device = DeviceSession::from(session);

        // Five 10 ms delays: 21 bytes in Vial's buffer, 11 of the board's pool.
        let delays: Vec<u8> = (0..5).flat_map(|_| [0x01, 0x04, 11, 1]).collect();
        let size = device.get_macro_buffer_size().unwrap();
        let buffer = device.encode_macros(&[delays], size).unwrap();
        device.set_macro_buffer(&buffer).unwrap();

        assert_eq!(state.lock().unwrap().macros[0], vec![MacroOp::Delay(10); 5]);
    }

    #[test]
    fn settings_read_ahead_serve_the_connect_until_written_or_forgotten() {
        let mut stored = Stored::new(1, 2, 1);
        stored.settings.insert(7, vec![1]);
        stored.settings.insert(8, vec![2]);
        let (session, state) = session("read-ahead", stored);
        let asked = |state: &Mutex<Stored>| {
            let state = state.lock().unwrap();
            state
                .received
                .iter()
                .filter(|cmd| **cmd == Cmd::GetSetting)
                .count()
        };

        session.read_settings_ahead(&[7, 8]).unwrap();
        state.lock().unwrap().settings.insert(7, vec![9]);
        assert_eq!(session.setting(7).unwrap(), [1], "as read ahead");
        assert_eq!(session.setting(7).unwrap(), [1], "and again");
        assert_eq!(asked(&state), 2, "both read once, ahead");

        session.set_setting(8, vec![3]).unwrap();
        assert_eq!(session.setting(8).unwrap(), [3], "a write is read back");
        session.forget_settings_read_ahead();
        assert_eq!(
            session.setting(7).unwrap(),
            [9],
            "the board's own once forgotten"
        );
        assert_eq!(asked(&state), 4);
    }

    #[test]
    fn settings_carry_the_bytes_vial_carries() {
        let mut stored = Stored::new(1, 2, 1);
        stored.settings.insert(7, vec![0x2A]);
        let (session, state) = session("settings", stored);
        let device = DeviceSession::from(session);

        device.set_qmk_setting_u16(2, 0x01F4).unwrap();
        device.set_qmk_setting_string(200, "50%").unwrap();
        assert_eq!(device.get_qmk_setting_u16(7).unwrap(), 0x2A);
        assert_eq!(device.get_qmk_setting_u16(2).unwrap(), 0x01F4);
        assert_eq!(device.get_qmk_setting_string(200).unwrap(), "50%%");

        let state = state.lock().unwrap();
        assert_eq!(state.settings[&2], [0xF4, 0x01]);
        assert_eq!(state.settings[&200], b"50%%\0");
    }

    #[test]
    fn an_encoder_write_keeps_the_other_direction() {
        let mut stored = Stored::new(1, 2, 1);
        stored.encoders.insert(
            (0, 1),
            EncoderAction::new(key(HidKeyCode::A), KeyAction::Tap(Action::No)),
        );
        let (session, state) = session("encoder", stored);

        session.set_encoder(0, 1, 1, KC_B).unwrap();

        let encoder = state.lock().unwrap().encoders[&(0, 1)];
        assert_eq!(encoder.clockwise, key(HidKeyCode::B));
        assert_eq!(encoder.counter_clockwise, KeyAction::Tap(Action::No));
    }

    #[test]
    fn the_switch_matrix_is_read_row_by_row() {
        let mut stored = Stored::new(2, 10, 1);
        // Row 0: columns 0 and 9; row 1: column 8. Each row takes two bytes.
        stored.matrix = vec![0b0000_0001, 0b0000_0010, 0, 0b0000_0001];
        let (session, _) = session("matrix", stored);

        let pressed = session.switch_matrix(2, 10).unwrap();
        let at: Vec<usize> = (0..20).filter(|&i| pressed[i]).collect();
        assert_eq!(at, [0, 9, 18]);
    }

    #[test]
    fn a_half_without_a_reading_has_no_battery_level() {
        let mut stored = Stored::new(1, 2, 1);
        stored.battery = Some(rmk_types_rynk::protocol::rynk::ergohaven::BatteryHalves {
            mask: 0b01,
            left: 77,
            right: 0xFF,
        });
        let (session, _) = session("battery", stored);
        let halves = session.battery_halves().unwrap();
        assert_eq!((halves.left, halves.right), (Some(77), None));
    }
}
