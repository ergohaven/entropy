//! Read-only Rynk sessions for `--export-layout`, the Rynk side of
//! `hid::enforce_read_only_hid`: once enforced, every frame the process sends
//! must be a known read, or it never reaches the transport.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use rmk_types_rynk::protocol::rynk::Cmd;

#[derive(Clone, Default)]
pub(crate) struct ReadOnlyRynkSession {
    ledger: Arc<Mutex<ReadOnlyRynkLedger>>,
}

#[derive(Default)]
struct ReadOnlyRynkLedger {
    refused: Vec<u16>,
    failed_reads: Vec<u16>,
}

static READ_ONLY_RYNK_SESSION: OnceLock<ReadOnlyRynkSession> = OnceLock::new();

/// Makes every Rynk request this process sends from now on read-only. There
/// is no way back: the process is expected to exit when its read is done.
pub(crate) fn enforce_read_only_rynk() -> ReadOnlyRynkSession {
    READ_ONLY_RYNK_SESSION.get_or_init(Default::default).clone()
}

pub(super) fn read_only_rynk_session() -> Option<ReadOnlyRynkSession> {
    READ_ONLY_RYNK_SESSION.get().cloned()
}

impl ReadOnlyRynkSession {
    fn ledger(&self) -> MutexGuard<'_, ReadOnlyRynkLedger> {
        self.ledger
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// `false` for a command that is not a known read, which is then
    /// recorded as refused.
    pub(super) fn admit(&self, cmd: Cmd) -> bool {
        let admitted = is_read_cmd(cmd);
        if !admitted {
            self.ledger().refused.push(cmd.raw());
        }
        admitted
    }

    pub(super) fn record_failed_read(&self, cmd: Cmd) {
        self.ledger().failed_reads.push(cmd.raw());
    }

    /// Commands refused as writes (or as unknown commands).
    pub(crate) fn refused_requests(&self) -> Vec<u16> {
        self.ledger().refused.clone()
    }

    /// Reads that were sent and failed.
    pub(crate) fn failed_reads(&self) -> Vec<u16> {
        self.ledger().failed_reads.clone()
    }
}

/// Commands that only read the board. Anything else fails closed, a command
/// added to a table later included. Unlock polling arms an unlock attempt and
/// `GetBatteryHalves` wakes the halves, so neither is a read.
pub(crate) fn is_read_cmd(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::GetVersion
            | Cmd::GetCapabilities
            | Cmd::GetLockStatus
            | Cmd::GetLayout
            | Cmd::GetDeviceInfo
            | Cmd::GetKeyAction
            | Cmd::GetDefaultLayer
            | Cmd::GetEncoderAction
            | Cmd::GetKeymapBulk
            | Cmd::GetMacro
            | Cmd::GetCombo
            | Cmd::GetComboBulk
            | Cmd::GetMorse
            | Cmd::GetMorseBulk
            | Cmd::GetFork
            | Cmd::GetBehaviorConfig
            | Cmd::GetConnectionType
            | Cmd::GetConnectionStatus
            | Cmd::GetBleStatus
            | Cmd::GetCurrentLayer
            | Cmd::GetMatrixState
            | Cmd::GetBatteryStatus
            | Cmd::GetPeripheralStatus
            | Cmd::GetWpm
            | Cmd::GetSleepState
            | Cmd::GetLedIndicator
            | Cmd::GetErgohavenInfo
            | Cmd::GetBoardDefinition
            | Cmd::GetSettingKeys
            | Cmd::GetSetting
            | Cmd::GetLayoutOption
            | Cmd::GetWriteStatus
    )
}
