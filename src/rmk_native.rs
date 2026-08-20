use anyhow::{bail, Context, Result};
use rmk_types::action::KeyAction;

const MSG_LEN: usize = 32;
const CMD_VIA_CUSTOM_SET_VALUE: u8 = 0x07;
const CMD_VIA_CUSTOM_GET_VALUE: u8 = 0x08;
const ERGOHAVEN_CUSTOM_NAMESPACE: u8 = 0xE8;
const ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS: u8 = 0x02;
const ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION: u8 = 0x03;
const ERGOHAVEN_CUSTOM_NEXT_NATIVE_KEY_ACTION: u8 = 0x04;
const ERGOHAVEN_CUSTOM_NATIVE_DYNAMIC_ACTION: u8 = 0x05;
const ERGOHAVEN_CUSTOM_NEXT_NATIVE_DYNAMIC_ACTION: u8 = 0x06;
const ERGOHAVEN_CUSTOM_COMBO_LAYER: u8 = 0x07;
const ERGOHAVEN_NATIVE_KEY_ACTION_VERSION: u8 = 0x01;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_GET_SET: u16 = 0x0001;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_UNIVERSAL_SYMBOLS: u16 = 0x0002;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_RUSSIAN_LETTERS: u16 = 0x0004;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_COMBO_OUTPUT: u16 = 0x0008;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_MORSE_ACTIONS: u16 = 0x0010;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_COMBO_LAYER: u16 = 0x0020;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_VIAL_MACRO_EXT: u16 = 0x0040;
const ERGOHAVEN_NATIVE_KEY_ACTION_CAP_REPEAT_KEY: u16 = 0x0080;
const NATIVE_DYNAMIC_ACTION_KIND_COMBO_OUTPUT: u8 = 0x00;
const NATIVE_DYNAMIC_ACTION_KIND_MORSE: u8 = 0x01;
const NATIVE_KEY_ACTION_STATUS_OK: u8 = 0x00;
const NATIVE_KEY_ACTION_STATUS_END: u8 = 0x01;
const NATIVE_KEY_ACTION_STATUS_UNSUPPORTED_VERSION: u8 = 0x02;
const NATIVE_KEY_ACTION_STATUS_INVALID_POSITION: u8 = 0x03;
const NATIVE_KEY_ACTION_STATUS_INVALID_PAYLOAD: u8 = 0x04;
const NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET: usize = 6;
const NATIVE_KEY_ACTION_SET_PAYLOAD_OFFSET: usize = 8;
const NATIVE_KEY_ACTION_NEXT_PAYLOAD_OFFSET: usize = 8;
const NATIVE_KEY_ACTION_MAX_PAYLOAD: usize = MSG_LEN - NATIVE_KEY_ACTION_SET_PAYLOAD_OFFSET;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RmkNativeCapabilities {
    pub(crate) key_actions: bool,
    pub(crate) universal_symbols: bool,
    pub(crate) russian_letters: bool,
    pub(crate) combo_output: bool,
    pub(crate) tap_dance_actions: bool,
    pub(crate) combo_layers: bool,
    pub(crate) vial_macro_ext: bool,
    pub(crate) repeat_key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RmkNativeActionAt {
    pub(crate) flat_index: usize,
    pub(crate) action: KeyAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RmkNativeDynamicActionAt {
    pub(crate) flat_index: usize,
    pub(crate) action: KeyAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RmkModTapParts {
    tap_value: u16,
    hold_modifier_bits: u8,
}

impl RmkModTapParts {
    pub(crate) fn tap_value(self) -> u16 {
        self.tap_value
    }

    pub(crate) fn hold_modifier_bits(self) -> u8 {
        self.hold_modifier_bits
    }

    pub(crate) fn vial_base(self) -> u16 {
        0x2000 | ((self.hold_modifier_bits as u16) << 8)
    }
}

pub(crate) fn rmk_mod_tap_parts(action: KeyAction) -> Option<RmkModTapParts> {
    use rmk_types::action::Action;

    let KeyAction::TapHold(
        Action::KeyWithModifier(key, tap_modifiers),
        Action::Modifier(hold_modifiers),
        _,
    ) = action
    else {
        return None;
    };

    Some(RmkModTapParts {
        tap_value: ((tap_modifiers.into_packed_bits() as u16) << 8) | key as u16,
        hold_modifier_bits: hold_modifiers.into_packed_bits(),
    })
}

pub(crate) fn apply_rmk_native_actions(
    layout: &mut crate::keyboard::KeyboardLayout,
    actions: &[RmkNativeActionAt],
) -> usize {
    let matrix_size = layout.rows.saturating_mul(layout.cols);
    if matrix_size == 0 {
        return 0;
    }

    let mut applied = 0;
    for native in actions {
        let layer = native.flat_index / matrix_size;
        let matrix_index = native.flat_index % matrix_size;
        let row = matrix_index / layout.cols;
        let col = matrix_index % layout.cols;
        let Some(key_index) = layout
            .keys
            .iter()
            .position(|key| key.row as usize == row && key.col as usize == col)
        else {
            continue;
        };
        layout.set_rmk_key_action(layer, key_index, native.action);
        applied += 1;
    }
    applied
}

pub(crate) fn apply_rmk_native_dynamic_actions(
    combos: &mut [crate::app::ComboEntry],
    tap_dances: &mut [crate::keycode_picker::TapDanceEntry],
    combo_count: usize,
    actions: &[RmkNativeDynamicActionAt],
) -> usize {
    let mut applied = 0;
    for native in actions {
        if native.flat_index < combo_count {
            if let Some(combo) = combos.get_mut(native.flat_index) {
                combo.output = crate::keyboard::KeyBinding::Rmk(native.action);
                applied += 1;
            }
            continue;
        }

        let morse_flat = native.flat_index - combo_count;
        let entry_index = morse_flat / 4;
        let field = morse_flat % 4;
        let Some(entry) = tap_dances.get_mut(entry_index) else {
            continue;
        };
        let binding = crate::keyboard::KeyBinding::Rmk(native.action);
        match field {
            0 => entry.on_tap = binding,
            1 => entry.on_hold = binding,
            2 => entry.on_double_tap = binding,
            3 => entry.on_tap_hold = binding,
            _ => continue,
        }
        applied += 1;
    }
    applied
}

pub(crate) fn toggle_handed_key_action(action: KeyAction) -> KeyAction {
    use rmk_types::action::Action;
    use rmk_types::modifier::ModifierCombination;

    fn swap_modifiers(modifiers: ModifierCombination) -> ModifierCombination {
        let bits = modifiers.into_bits();
        ModifierCombination::from_bits(bits.rotate_right(4))
    }

    fn swap_action(action: Action) -> Action {
        match action {
            Action::Modifier(modifiers) => Action::Modifier(swap_modifiers(modifiers)),
            Action::KeyWithModifier(key, modifiers) => {
                Action::KeyWithModifier(key, swap_modifiers(modifiers))
            }
            other => other,
        }
    }

    match action {
        KeyAction::Single(action) => KeyAction::Single(swap_action(action)),
        KeyAction::Tap(action) => KeyAction::Tap(swap_action(action)),
        KeyAction::TapHold(tap, hold, profile) => {
            KeyAction::TapHold(swap_action(tap), swap_action(hold), profile)
        }
        other => other,
    }
}

fn native_status_error(status: u8) -> &'static str {
    match status {
        NATIVE_KEY_ACTION_STATUS_UNSUPPORTED_VERSION => "unsupported protocol version",
        NATIVE_KEY_ACTION_STATUS_INVALID_POSITION => "invalid key position",
        NATIVE_KEY_ACTION_STATUS_INVALID_PAYLOAD => "invalid key action payload",
        _ => "unknown firmware error",
    }
}

fn native_response_header_matches(response: &[u8; MSG_LEN], subcommand: u8) -> bool {
    (response[0] == CMD_VIA_CUSTOM_GET_VALUE || response[0] == CMD_VIA_CUSTOM_SET_VALUE)
        && response[1] == ERGOHAVEN_CUSTOM_NAMESPACE
        && response[2] == subcommand
}

pub(crate) fn matches_rmk_native_response(
    command: &[u8],
    response: &[u8; MSG_LEN],
) -> Option<bool> {
    if !matches!(
        command.first(),
        Some(&CMD_VIA_CUSTOM_GET_VALUE) | Some(&CMD_VIA_CUSTOM_SET_VALUE)
    ) || command.get(1) != Some(&ERGOHAVEN_CUSTOM_NAMESPACE)
    {
        return None;
    }
    let subcommand = *command.get(2)?;
    if !matches!(
        subcommand,
        ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS
            | ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION
            | ERGOHAVEN_CUSTOM_NEXT_NATIVE_KEY_ACTION
            | ERGOHAVEN_CUSTOM_NATIVE_DYNAMIC_ACTION
            | ERGOHAVEN_CUSTOM_NEXT_NATIVE_DYNAMIC_ACTION
            | ERGOHAVEN_CUSTOM_COMBO_LAYER
    ) {
        return None;
    }

    // Standard QMK-Vial echoes unknown custom GET commands unchanged. Treat an
    // exact capabilities probe echo as a terminal "RMK unsupported" response;
    // the decoder below will return empty capabilities without transport retries.
    if subcommand == ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS
        && command.first() == Some(&CMD_VIA_CUSTOM_GET_VALUE)
        && command.len() == response.len()
        && command == response
    {
        return Some(true);
    }

    let header_matches = response[0] == command[0]
        && response[1] == ERGOHAVEN_CUSTOM_NAMESPACE
        && response[2] == subcommand
        && response[3] == ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
    if subcommand == ERGOHAVEN_CUSTOM_COMBO_LAYER {
        return Some(header_matches && response[7] == command.get(4).copied().unwrap_or(u8::MAX));
    }
    if !header_matches
        || command.first() == Some(&CMD_VIA_CUSTOM_SET_VALUE)
        || !matches!(
            subcommand,
            ERGOHAVEN_CUSTOM_NEXT_NATIVE_KEY_ACTION | ERGOHAVEN_CUSTOM_NEXT_NATIVE_DYNAMIC_ACTION
        )
    {
        return Some(header_matches);
    }

    if response[4] != NATIVE_KEY_ACTION_STATUS_OK {
        return Some(true);
    }
    let requested_cursor = command
        .get(4..6)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))?;
    let returned_index = u16::from_le_bytes([response[5], response[6]]);
    Some(returned_index >= requested_cursor)
}

fn decode_native_capabilities(response: &[u8; MSG_LEN]) -> RmkNativeCapabilities {
    if !native_response_header_matches(response, ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS)
        || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
    {
        return RmkNativeCapabilities::default();
    }
    let flags = u16::from_le_bytes([response[4], response[5]]);
    RmkNativeCapabilities {
        key_actions: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_GET_SET != 0,
        universal_symbols: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_UNIVERSAL_SYMBOLS != 0,
        russian_letters: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_RUSSIAN_LETTERS != 0,
        combo_output: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_COMBO_OUTPUT != 0,
        tap_dance_actions: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_MORSE_ACTIONS != 0,
        combo_layers: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_COMBO_LAYER != 0,
        vial_macro_ext: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_VIAL_MACRO_EXT != 0,
        repeat_key: flags & ERGOHAVEN_NATIVE_KEY_ACTION_CAP_REPEAT_KEY != 0,
    }
}

pub(crate) const fn supports_layout_sync(capabilities: RmkNativeCapabilities) -> bool {
    capabilities.universal_symbols
}

fn decode_native_action(
    response: &[u8; MSG_LEN],
    status_offset: usize,
    len_offset: usize,
    payload_offset: usize,
) -> Result<KeyAction> {
    let status = response[status_offset];
    if status != NATIVE_KEY_ACTION_STATUS_OK {
        bail!(
            "RMK native key action failed: {}",
            native_status_error(status)
        );
    }
    let payload_len = response[len_offset] as usize;
    if payload_len == 0 || payload_offset + payload_len > response.len() {
        bail!("invalid RMK native key action payload length: {payload_len}");
    }
    postcard::from_bytes(&response[payload_offset..payload_offset + payload_len])
        .context("failed to decode RMK native key action")
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::hid::HidDevice {
    pub(crate) fn get_rmk_native_capabilities(&self) -> Result<RmkNativeCapabilities> {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS;
        let response = self.usb_send(&command)?;
        Ok(decode_native_capabilities(&response))
    }

    pub(crate) fn get_rmk_key_action(&self, layer: u8, row: u8, col: u8) -> Result<KeyAction> {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = layer;
        command[5] = row;
        command[6] = col;
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
        {
            bail!("unexpected RMK native key action response");
        }
        decode_native_action(
            &response,
            4,
            NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET - 1,
            NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET,
        )
    }

    pub(crate) fn set_rmk_key_action(
        &self,
        layer: u8,
        row: u8,
        col: u8,
        action: KeyAction,
    ) -> Result<()> {
        let mut encoded = [0u8; NATIVE_KEY_ACTION_MAX_PAYLOAD];
        let payload =
            postcard::to_slice(&action, &mut encoded).context("failed to encode RMK key action")?;
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_SET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = layer;
        command[5] = row;
        command[6] = col;
        command[7] = payload.len() as u8;
        command[NATIVE_KEY_ACTION_SET_PAYLOAD_OFFSET
            ..NATIVE_KEY_ACTION_SET_PAYLOAD_OFFSET + payload.len()]
            .copy_from_slice(payload);
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
        {
            bail!("unexpected RMK native key action write response");
        }
        if response[4] != NATIVE_KEY_ACTION_STATUS_OK {
            bail!(
                "RMK native key action write failed: {}",
                native_status_error(response[4])
            );
        }
        let readback = self.get_rmk_key_action(layer, row, col)?;
        if readback != action {
            bail!("RMK native key action readback mismatch at layer {layer}, row {row}, col {col}");
        }
        Ok(())
    }

    pub(crate) fn get_next_rmk_native_key_action(
        &self,
        start_flat_index: usize,
    ) -> Result<Option<RmkNativeActionAt>> {
        let start = u16::try_from(start_flat_index)
            .context("RMK native key action cursor exceeds protocol range")?;
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NEXT_NATIVE_KEY_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4..6].copy_from_slice(&start.to_le_bytes());
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_NEXT_NATIVE_KEY_ACTION)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
        {
            bail!("unexpected RMK native key action scan response");
        }
        if response[4] == NATIVE_KEY_ACTION_STATUS_END {
            return Ok(None);
        }
        let action = decode_native_action(
            &response,
            4,
            NATIVE_KEY_ACTION_NEXT_PAYLOAD_OFFSET - 1,
            NATIVE_KEY_ACTION_NEXT_PAYLOAD_OFFSET,
        )?;
        Ok(Some(RmkNativeActionAt {
            flat_index: u16::from_le_bytes([response[5], response[6]]) as usize,
            action,
        }))
    }

    pub(crate) fn get_rmk_native_key_actions_in_range(
        &self,
        start_flat_index: usize,
        end_flat_index: usize,
    ) -> Result<Vec<RmkNativeActionAt>> {
        let mut actions = Vec::new();
        let mut cursor = start_flat_index;
        while cursor < end_flat_index {
            let Some(next) = self.get_next_rmk_native_key_action(cursor)? else {
                break;
            };
            if next.flat_index < cursor {
                bail!(
                    "RMK native key action scan did not advance: {} < {}",
                    next.flat_index,
                    cursor
                );
            }
            if next.flat_index >= end_flat_index {
                break;
            }
            actions.push(next);
            cursor = next.flat_index + 1;
        }
        Ok(actions)
    }

    fn get_rmk_native_dynamic_action(&self, kind: u8, index: u8, field: u8) -> Result<KeyAction> {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NATIVE_DYNAMIC_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = kind;
        command[5] = index;
        command[6] = field;
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_NATIVE_DYNAMIC_ACTION)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
        {
            bail!("unexpected RMK native dynamic action response");
        }
        decode_native_action(
            &response,
            4,
            NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET - 1,
            NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET,
        )
    }

    fn set_rmk_native_dynamic_action(
        &self,
        kind: u8,
        index: u8,
        field: u8,
        action: KeyAction,
    ) -> Result<()> {
        let mut encoded = [0u8; NATIVE_KEY_ACTION_MAX_PAYLOAD];
        let payload = postcard::to_slice(&action, &mut encoded)
            .context("failed to encode RMK native dynamic action")?;
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_SET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NATIVE_DYNAMIC_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = kind;
        command[5] = index;
        command[6] = field;
        command[7] = payload.len() as u8;
        command[NATIVE_KEY_ACTION_SET_PAYLOAD_OFFSET
            ..NATIVE_KEY_ACTION_SET_PAYLOAD_OFFSET + payload.len()]
            .copy_from_slice(payload);
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_NATIVE_DYNAMIC_ACTION)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
        {
            bail!("unexpected RMK native dynamic action write response");
        }
        if response[4] != NATIVE_KEY_ACTION_STATUS_OK {
            bail!(
                "RMK native dynamic action write failed: {}",
                native_status_error(response[4])
            );
        }
        let readback = self.get_rmk_native_dynamic_action(kind, index, field)?;
        if readback != action {
            bail!("RMK native dynamic action readback mismatch at kind {kind}, index {index}, field {field}");
        }
        Ok(())
    }

    pub(crate) fn set_rmk_combo_output(&self, index: u8, action: KeyAction) -> Result<()> {
        self.set_rmk_native_dynamic_action(
            NATIVE_DYNAMIC_ACTION_KIND_COMBO_OUTPUT,
            index,
            0,
            action,
        )
    }

    pub(crate) fn get_rmk_combo_layer(&self, index: u8) -> Result<Option<u8>> {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_COMBO_LAYER;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = index;
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_COMBO_LAYER)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
            || response[7] != index
        {
            bail!("unexpected RMK Combo layer response");
        }
        if response[4] != NATIVE_KEY_ACTION_STATUS_OK {
            bail!(
                "RMK Combo layer read failed: {}",
                native_status_error(response[4])
            );
        }
        match response[5] {
            0 => Ok(None),
            1 => Ok(Some(response[6])),
            marker => bail!("invalid RMK Combo layer marker: {marker}"),
        }
    }

    pub(crate) fn set_rmk_combo_layer(&self, index: u8, layer: Option<u8>) -> Result<()> {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_SET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_COMBO_LAYER;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = index;
        command[5] = u8::from(layer.is_some());
        command[6] = layer.unwrap_or(0);
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_COMBO_LAYER)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
            || response[7] != index
        {
            bail!("unexpected RMK Combo layer write response");
        }
        if response[4] != NATIVE_KEY_ACTION_STATUS_OK {
            bail!(
                "RMK Combo layer write failed: {}",
                native_status_error(response[4])
            );
        }
        let readback = self.get_rmk_combo_layer(index)?;
        if readback != layer {
            bail!(
                "RMK Combo layer readback mismatch at index {index}: wrote {layer:?}, read back {readback:?}"
            );
        }
        Ok(())
    }

    pub(crate) fn set_rmk_tap_dance_action(
        &self,
        index: u8,
        field: u8,
        action: KeyAction,
    ) -> Result<()> {
        self.set_rmk_native_dynamic_action(NATIVE_DYNAMIC_ACTION_KIND_MORSE, index, field, action)
    }

    pub(crate) fn get_next_rmk_native_dynamic_action(
        &self,
        start_flat_index: usize,
    ) -> Result<Option<RmkNativeDynamicActionAt>> {
        let start = u16::try_from(start_flat_index)
            .context("RMK native dynamic action cursor exceeds protocol range")?;
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NEXT_NATIVE_DYNAMIC_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4..6].copy_from_slice(&start.to_le_bytes());
        let response = self.usb_send(&command)?;
        if !native_response_header_matches(&response, ERGOHAVEN_CUSTOM_NEXT_NATIVE_DYNAMIC_ACTION)
            || response[3] != ERGOHAVEN_NATIVE_KEY_ACTION_VERSION
        {
            bail!("unexpected RMK native dynamic action scan response");
        }
        if response[4] == NATIVE_KEY_ACTION_STATUS_END {
            return Ok(None);
        }
        let action = decode_native_action(
            &response,
            4,
            NATIVE_KEY_ACTION_NEXT_PAYLOAD_OFFSET - 1,
            NATIVE_KEY_ACTION_NEXT_PAYLOAD_OFFSET,
        )?;
        Ok(Some(RmkNativeDynamicActionAt {
            flat_index: u16::from_le_bytes([response[5], response[6]]) as usize,
            action,
        }))
    }

    pub(crate) fn get_rmk_native_dynamic_actions(
        &self,
        combo_count: usize,
        tap_dance_count: usize,
    ) -> Result<Vec<RmkNativeDynamicActionAt>> {
        let end_flat_index = combo_count.saturating_add(tap_dance_count.saturating_mul(4));
        let mut actions = Vec::new();
        let mut cursor = 0;
        while cursor < end_flat_index {
            let Some(next) = self.get_next_rmk_native_dynamic_action(cursor)? else {
                break;
            };
            if next.flat_index < cursor {
                bail!(
                    "RMK native dynamic action scan did not advance: {} < {}",
                    next.flat_index,
                    cursor
                );
            }
            if next.flat_index >= end_flat_index {
                break;
            }
            actions.push(next);
            cursor = next.flat_index + 1;
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmk_types::action::Action;
    use rmk_types::keycode::HidKeyCode;
    use rmk_types::modifier::ModifierCombination;

    #[test]
    fn decodes_universal_symbols_capability_independently() {
        let mut response = [0u8; MSG_LEN];
        response[0] = CMD_VIA_CUSTOM_GET_VALUE;
        response[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        response[2] = ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS;
        response[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        response[4..6].copy_from_slice(
            &(ERGOHAVEN_NATIVE_KEY_ACTION_CAP_GET_SET
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_UNIVERSAL_SYMBOLS
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_RUSSIAN_LETTERS)
                .to_le_bytes(),
        );

        assert_eq!(
            decode_native_capabilities(&response),
            RmkNativeCapabilities {
                key_actions: true,
                universal_symbols: true,
                russian_letters: true,
                combo_output: false,
                tap_dance_actions: false,
                combo_layers: false,
                vial_macro_ext: false,
                repeat_key: false,
            }
        );
        response[4..6].copy_from_slice(
            &(ERGOHAVEN_NATIVE_KEY_ACTION_CAP_GET_SET
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_UNIVERSAL_SYMBOLS)
                .to_le_bytes(),
        );
        assert!(!decode_native_capabilities(&response).russian_letters);
        response[4..6].copy_from_slice(
            &(ERGOHAVEN_NATIVE_KEY_ACTION_CAP_GET_SET
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_COMBO_OUTPUT
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_MORSE_ACTIONS
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_COMBO_LAYER
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_VIAL_MACRO_EXT
                | ERGOHAVEN_NATIVE_KEY_ACTION_CAP_REPEAT_KEY)
                .to_le_bytes(),
        );
        let dynamic = decode_native_capabilities(&response);
        assert!(dynamic.combo_output);
        assert!(dynamic.tap_dance_actions);
        assert!(dynamic.combo_layers);
        assert!(dynamic.vial_macro_ext);
        assert!(dynamic.repeat_key);
        assert!(!dynamic.universal_symbols);
        response[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION + 1;
        assert_eq!(
            decode_native_capabilities(&response),
            RmkNativeCapabilities::default()
        );
    }

    #[test]
    fn universal_symbols_capability_enables_existing_layout_sync_bridge() {
        assert!(supports_layout_sync(RmkNativeCapabilities {
            key_actions: true,
            universal_symbols: true,
            russian_letters: false,
            combo_output: false,
            tap_dance_actions: false,
            combo_layers: false,
            vial_macro_ext: false,
            repeat_key: false,
        }));
        assert!(!supports_layout_sync(RmkNativeCapabilities {
            key_actions: true,
            universal_symbols: false,
            russian_letters: false,
            combo_output: false,
            tap_dance_actions: false,
            combo_layers: false,
            vial_macro_ext: false,
            repeat_key: false,
        }));
    }

    #[test]
    fn native_action_scan_response_must_reach_the_requested_cursor() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NEXT_NATIVE_KEY_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4..6].copy_from_slice(&59u16.to_le_bytes());

        let mut response = command;
        response[4] = NATIVE_KEY_ACTION_STATUS_OK;
        response[5..7].copy_from_slice(&58u16.to_le_bytes());
        assert_eq!(
            matches_rmk_native_response(&command, &response),
            Some(false)
        );

        response[5..7].copy_from_slice(&59u16.to_le_bytes());
        assert_eq!(matches_rmk_native_response(&command, &response), Some(true));
    }

    #[test]
    fn native_dynamic_action_scan_response_must_reach_the_requested_cursor() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NEXT_NATIVE_DYNAMIC_ACTION;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4..6].copy_from_slice(&12u16.to_le_bytes());

        let mut response = command;
        response[4] = NATIVE_KEY_ACTION_STATUS_OK;
        response[5..7].copy_from_slice(&11u16.to_le_bytes());
        assert_eq!(
            matches_rmk_native_response(&command, &response),
            Some(false)
        );

        response[5..7].copy_from_slice(&12u16.to_le_bytes());
        assert_eq!(matches_rmk_native_response(&command, &response), Some(true));
    }

    #[test]
    fn native_dynamic_actions_overlay_combo_and_tap_dance_fields() {
        let combo_action = KeyAction::Single(Action::User(0x80));
        let tap_action = KeyAction::Single(Action::User(0x81));
        let mut combos = vec![crate::app::ComboEntry::default(); 2];
        let mut tap_dances = vec![crate::keycode_picker::TapDanceEntry::default(); 1];
        let applied = apply_rmk_native_dynamic_actions(
            &mut combos,
            &mut tap_dances,
            2,
            &[
                RmkNativeDynamicActionAt {
                    flat_index: 1,
                    action: combo_action,
                },
                RmkNativeDynamicActionAt {
                    flat_index: 2 + 3,
                    action: tap_action,
                },
            ],
        );

        assert_eq!(applied, 2);
        assert_eq!(
            combos[1].output,
            crate::keyboard::KeyBinding::Rmk(combo_action)
        );
        assert_eq!(
            tap_dances[0].on_tap_hold,
            crate::keyboard::KeyBinding::Rmk(tap_action)
        );
    }

    #[test]
    fn combo_layer_response_must_echo_the_requested_slot() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_COMBO_LAYER;
        command[3] = ERGOHAVEN_NATIVE_KEY_ACTION_VERSION;
        command[4] = 3;

        let mut response = command;
        response[4] = NATIVE_KEY_ACTION_STATUS_OK;
        response[5] = 1;
        response[6] = 2;
        response[7] = 2;
        assert_eq!(
            matches_rmk_native_response(&command, &response),
            Some(false)
        );

        response[7] = 3;
        assert_eq!(matches_rmk_native_response(&command, &response), Some(true));

        command[0] = CMD_VIA_CUSTOM_SET_VALUE;
        response[0] = CMD_VIA_CUSTOM_SET_VALUE;
        assert_eq!(matches_rmk_native_response(&command, &response), Some(true));
    }

    #[test]
    fn native_capability_probe_accepts_exact_qmk_echo() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS;

        assert_eq!(matches_rmk_native_response(&command, &command), Some(true));
        assert_eq!(
            decode_native_capabilities(&command),
            RmkNativeCapabilities::default()
        );
    }

    #[test]
    fn decodes_native_mod_tap_payload() {
        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::Kc0, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::LCTRL),
            Default::default(),
        );
        let mut response = [0u8; MSG_LEN];
        let mut encoded = [0u8; NATIVE_KEY_ACTION_MAX_PAYLOAD];
        let payload = postcard::to_slice(&action, &mut encoded).unwrap();
        response[4] = NATIVE_KEY_ACTION_STATUS_OK;
        response[5] = payload.len() as u8;
        response[NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET
            ..NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET + payload.len()]
            .copy_from_slice(payload);
        assert_eq!(
            decode_native_action(
                &response,
                4,
                NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET - 1,
                NATIVE_KEY_ACTION_GET_PAYLOAD_OFFSET,
            )
            .unwrap(),
            action
        );
    }

    #[test]
    fn native_mod_tap_has_readable_layout_label_and_tooltip() {
        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::LeftBracket, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::RCTRL),
            Default::default(),
        );
        let binding = crate::keyboard::KeyBinding::Rmk(action);
        let label = crate::app::key_binding_label_with_macro_names(
            binding,
            &[],
            &[],
            &[],
            &[],
            crate::keycode::KeyLegendLayout::English,
        );
        let tooltip =
            crate::app::key_binding_tooltip_with_macro_names(binding, &[], &[], &[], &[], &[]);

        assert!(label.contains("Ctrl"), "{label}");
        assert!(label.contains('{'), "{label}");
        assert!(tooltip.contains("RMK Mod Tap"), "{tooltip}");
        assert!(tooltip.contains("Ctrl"), "{tooltip}");
        assert!(tooltip.contains("Right click"), "{tooltip}");
        let ru_tooltip = crate::i18n::tr_text(crate::i18n::Language::Russian, &tooltip);
        assert!(ru_tooltip.contains("Правый клик"), "{ru_tooltip}");
    }

    #[test]
    fn native_mod_tap_exposes_lossless_tap_value_and_vial_edit_base() {
        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::LeftBracket, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::RCTRL),
            Default::default(),
        );

        let parts = rmk_mod_tap_parts(action).expect("native Mod Tap should be recognized");

        assert_eq!(
            parts.tap_value(),
            ((ModifierCombination::LSHIFT.into_packed_bits() as u16) << 8)
                | HidKeyCode::LeftBracket as u16
        );
        assert_eq!(
            parts.vial_base(),
            0x2000 | ((ModifierCombination::RCTRL.into_packed_bits() as u16) << 8)
        );
    }

    #[test]
    fn native_mod_tap_mirroring_swaps_tap_and_hold_modifier_hands() {
        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::LeftBracket, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::RCTRL),
            Default::default(),
        );

        assert_eq!(
            toggle_handed_key_action(action),
            KeyAction::TapHold(
                Action::KeyWithModifier(HidKeyCode::LeftBracket, ModifierCombination::RSHIFT),
                Action::Modifier(ModifierCombination::LCTRL),
                Default::default(),
            )
        );
    }
}
