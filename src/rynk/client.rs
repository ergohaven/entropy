//! Synchronous Rynk client over any byte transport.
//!
//! One request at a time, or a window of reads where the transport pipelines;
//! the link owner serializes its callers and adds the window for requests
//! nobody waits on.
//! Requests are generic over `Endpoint`, so endpoint tables declared elsewhere
//! (the ergohaven vendor table, a private branch's) work without a change here.

use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::time::{Duration, Instant, SystemTime};

use rmk_types_rynk::action::KeyAction;
use rmk_types_rynk::combo::Combo;
use rmk_types_rynk::keyboard_macros::{Macro, MacroOp};
use rmk_types_rynk::morse::Morse;
use rmk_types_rynk::protocol::rynk::command::{self, Endpoint};
use rmk_types_rynk::protocol::rynk::ergohaven::{
    self, BatteryHalves, ErgohavenInfo, SetSettingRequest, SettingValue, WriteStatus,
    ERGOHAVEN_RYNK_REVISION,
};
use rmk_types_rynk::protocol::rynk::{
    encode_frame, max_wire_size, Cmd, Deframer, DeviceCapabilities, GetComboBulkRequest,
    GetKeymapBulkRequest, GetMorseBulkRequest, ProtocolVersion, RynkError, RynkHeader,
    SetMacroRequest, RYNK_HEADER_SIZE,
};
use serde::Serialize;

use super::read_only::{read_only_rynk_session, ReadOnlyRynkSession};

/// Payload budget before the board reports its own: enough for the handshake.
const HANDSHAKE_MAX_PAYLOAD: usize = 64;
/// Receive buffer: the largest frame a `u16` payload size allows, COBS-encoded.
const RX_BUFFER_SIZE: usize = max_wire_size(RYNK_HEADER_SIZE + u16::MAX as usize);
/// On connect, what an earlier session left in the link is read away until
/// the link stays quiet this long, within `DRAIN_LIMIT`.
const DRAIN_QUIET: Duration = Duration::from_millis(50);
const DRAIN_LIMIT: Duration = Duration::from_millis(500);
/// Requests in flight over a transport that pipelines, in wire bytes and in
/// frames: well inside the 512-byte pipe the board's BLE writes land in, so
/// a write is never held back, and a small tail parked in the board's buffer
/// while it answers the request in front.
const PIPELINE_BYTES: usize = 256;
const PIPELINE_FRAMES: usize = 32;

/// A byte link to one board: USB bulk endpoints, BLE GATT, or 32-byte HID
/// reports. The client only frames; the transport moves bytes.
pub(crate) trait RynkTransport {
    /// Sends one whole COBS frame, split as the medium needs.
    fn send(&mut self, frame: &[u8]) -> io::Result<()>;
    /// Bytes that arrived, waiting up to `timeout`; `Ok(0)` when none did.
    /// Zero padding after a frame, as HID reports carry, is fine.
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize>;
    /// Whether the board takes the next request before the host has read the
    /// reply to the last one. Over USB it does not: its session blocks on a
    /// reply until the host polls for it, and takes no frame meanwhile.
    fn pipelines(&self) -> bool {
        false
    }
    /// Several whole frames back to back, for a transport that pipelines.
    fn send_all(&mut self, frames: &[Vec<u8>]) -> io::Result<()> {
        frames.iter().try_for_each(|frame| self.send(frame))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RynkTimeouts {
    /// Until the first byte of a reply arrives.
    pub(crate) reply: Duration,
    /// Between bytes of one reply. A 4 KB reply over 32-byte HID reports is a
    /// hundred reports or more, so the reply as a whole has no fixed limit;
    /// it only has to keep moving.
    pub(crate) idle: Duration,
    /// Hard limit on one request, however slowly its reply moves.
    pub(crate) total: Duration,
}

impl Default for RynkTimeouts {
    fn default() -> Self {
        Self {
            reply: Duration::from_secs(8),
            idle: Duration::from_millis(2500),
            total: Duration::from_secs(60),
        }
    }
}

#[derive(Debug)]
pub(crate) enum RynkClientError {
    Transport(io::Error),
    Timeout {
        cmd: u16,
    },
    /// The board answered with an error.
    Device {
        cmd: u16,
        error: RynkError,
    },
    /// The reply does not decode as the endpoint's response, or has bytes
    /// left over: the board speaks another revision.
    Malformed {
        cmd: u16,
    },
    CmdMismatch {
        sent: u16,
        got: u16,
    },
    /// The request does not fit the board's frame.
    TooLarge {
        cmd: u16,
        len: usize,
        max: usize,
    },
    VersionMismatch {
        board: ProtocolVersion,
        host: ProtocolVersion,
    },
    /// The board speaks another ergohaven wire revision; `None` when it is
    /// no ergohaven firmware at all.
    RevisionMismatch {
        board: Option<u16>,
        host: u16,
    },
    /// The board definition read back does not hash to what the board reported.
    DefinitionHash {
        expected: u32,
        actual: u32,
    },
    /// Staged writes failed to reach flash.
    WriteFault,
    /// Staged writes were still pending when the wait ran out.
    WritesPending(WriteStatus),
    /// A bulk reply that cannot be right, such as an empty page short of the end.
    BulkProtocol {
        cmd: u16,
    },
    /// A read-only session refused a command that is not a known read.
    ReadOnly {
        cmd: u16,
    },
}

impl fmt::Display for RynkClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(f, "Rynk transport error: {error}"),
            Self::Timeout { cmd } => write!(f, "Rynk command {cmd:#06x} timed out"),
            Self::Device { cmd, error } => write!(f, "Rynk command {cmd:#06x} failed: {error:?}"),
            Self::Malformed { cmd } => write!(f, "Rynk command {cmd:#06x}: malformed reply"),
            Self::CmdMismatch { sent, got } => {
                write!(f, "Rynk command {sent:#06x} answered as {got:#06x}")
            }
            Self::TooLarge { cmd, len, max } => {
                write!(
                    f,
                    "Rynk command {cmd:#06x}: {len} bytes exceed the board's {max}"
                )
            }
            Self::VersionMismatch { board, host } => write!(
                f,
                "Rynk protocol {}.{} on the board, {}.{} in Entropy",
                board.major, board.minor, host.major, host.minor
            ),
            Self::RevisionMismatch {
                board: Some(board),
                host,
            } => {
                write!(
                    f,
                    "ergohaven Rynk revision {board} on the board, {host} in Entropy"
                )
            }
            Self::RevisionMismatch { board: None, host } => {
                write!(
                    f,
                    "the board has no ergohaven Rynk revision, Entropy needs {host}"
                )
            }
            Self::DefinitionHash { expected, actual } => write!(
                f,
                "board definition hashes to {actual:#010x}, the board reported {expected:#010x}"
            ),
            Self::WriteFault => f.write_str("the board failed to store staged writes"),
            Self::WritesPending(status) => write!(
                f,
                "{} keys and {} macro chunks still wait for flash",
                status.pending_keys, status.pending_macro_chunks
            ),
            Self::BulkProtocol { cmd } => write!(f, "Rynk command {cmd:#06x}: bad bulk page"),
            Self::ReadOnly { cmd } => {
                write!(f, "Read-only Rynk session refused command {cmd:#06x}")
            }
        }
    }
}

impl RynkClientError {
    /// The error for the user, in `language`, with what to update where an
    /// update helps. `Display` stays English for logs.
    pub(crate) fn describe(&self, language: crate::i18n::Language) -> String {
        let tr = |key, vars: &[(&str, &str)]| crate::i18n::tr_catalog_format(language, key, vars);
        let cmd = |cmd: &u16| format!("{cmd:#06x}");
        let version = |version: &ProtocolVersion| format!("{}.{}", version.major, version.minor);
        match self {
            Self::Transport(error) => {
                #[cfg(target_os = "linux")]
                if let Some(away) = error
                    .get_ref()
                    .and_then(|inner| inner.downcast_ref::<super::transport::NotConnected>())
                {
                    return tr("rynk_client.not_connected", &[("board", &away.board)]);
                }
                tr("rynk_client.transport", &[("error", &error.to_string())])
            }
            Self::Timeout { cmd: sent } => tr("rynk_client.timeout", &[("cmd", &cmd(sent))]),
            Self::Device { cmd: sent, error } => {
                let reason = match error {
                    RynkError::Malformed => tr("rynk_client.reason_malformed", &[]),
                    RynkError::NotReady => tr("rynk_client.reason_not_ready", &[]),
                    RynkError::StorageFault => tr("rynk_client.reason_storage_fault", &[]),
                    RynkError::Internal => tr("rynk_client.reason_internal", &[]),
                    RynkError::Unimplemented => tr("rynk_client.reason_unimplemented", &[]),
                    RynkError::Invalid => tr("rynk_client.reason_invalid", &[]),
                    RynkError::UnknownCmd => tr("rynk_client.reason_unknown_cmd", &[]),
                    RynkError::Locked => tr("rynk_client.reason_locked", &[]),
                    RynkError::Busy => tr("rynk_client.reason_busy", &[]),
                    other => tr(
                        "rynk_client.reason_other",
                        &[("error", &format!("{other:?}"))],
                    ),
                };
                tr(
                    "rynk_client.device",
                    &[("cmd", &cmd(sent)), ("reason", &reason)],
                )
            }
            Self::Malformed { cmd: sent } => tr("rynk_client.malformed", &[("cmd", &cmd(sent))]),
            Self::CmdMismatch { sent, got } => tr(
                "rynk_client.cmd_mismatch",
                &[("sent", &cmd(sent)), ("got", &cmd(got))],
            ),
            Self::TooLarge {
                cmd: sent,
                len,
                max,
            } => tr(
                "rynk_client.too_large",
                &[
                    ("cmd", &cmd(sent)),
                    ("len", &len.to_string()),
                    ("max", &max.to_string()),
                ],
            ),
            Self::VersionMismatch { board, host } => tr(
                if (board.major, board.minor) > (host.major, host.minor) {
                    "rynk_client.board_protocol_newer"
                } else {
                    "rynk_client.board_protocol_older"
                },
                &[("board", &version(board)), ("host", &version(host))],
            ),
            Self::RevisionMismatch {
                board: Some(board),
                host,
            } => tr(
                if board > host {
                    "rynk_client.board_revision_newer"
                } else {
                    "rynk_client.board_revision_older"
                },
                &[("board", &board.to_string()), ("host", &host.to_string())],
            ),
            Self::RevisionMismatch { board: None, host } => {
                tr("rynk_client.no_revision", &[("host", &host.to_string())])
            }
            Self::DefinitionHash { expected, actual } => tr(
                "rynk_client.definition_hash",
                &[
                    ("expected", &format!("{expected:#010x}")),
                    ("actual", &format!("{actual:#010x}")),
                ],
            ),
            Self::WriteFault => tr("rynk_client.write_fault", &[]),
            Self::WritesPending(status) => tr(
                "rynk_client.writes_pending",
                &[
                    ("keys", &status.pending_keys.to_string()),
                    ("chunks", &status.pending_macro_chunks.to_string()),
                ],
            ),
            Self::BulkProtocol { cmd: sent } => tr("rynk_client.bulk", &[("cmd", &cmd(sent))]),
            Self::ReadOnly { cmd: sent } => tr("rynk_client.read_only", &[("cmd", &cmd(sent))]),
        }
    }
}

impl std::error::Error for RynkClientError {}

impl From<io::Error> for RynkClientError {
    fn from(error: io::Error) -> Self {
        Self::Transport(error)
    }
}

/// A frame from the board.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Incoming {
    /// A reply, header included, for `decode_reply`.
    Reply { seq: u8, frame: Vec<u8> },
    /// A topic the board pushed.
    Topic,
}

/// The frame for request `E` with `seq`, refused when its payload passes
/// `max_payload`.
pub(crate) fn encode_request<E: Endpoint>(
    seq: u8,
    request: &E::Request,
    max_payload: usize,
) -> Result<Vec<u8>, RynkClientError> {
    let cmd = E::CMD.raw();
    let len = postcard_len(request).ok_or(RynkClientError::TooLarge {
        cmd,
        len: usize::MAX,
        max: max_payload,
    })?;
    if len > max_payload {
        return Err(RynkClientError::TooLarge {
            cmd,
            len,
            max: max_payload,
        });
    }
    let mut frame = vec![0; max_wire_size(RYNK_HEADER_SIZE + len)];
    let header = RynkHeader { cmd: E::CMD, seq };
    let frame_len =
        encode_frame(&mut frame, header, request).map_err(|_| RynkClientError::TooLarge {
            cmd,
            len,
            max: max_payload,
        })?;
    frame.truncate(frame_len);
    Ok(frame)
}

/// The response in a decoded reply frame (header included) for request `E`.
pub(crate) fn decode_reply<E: Endpoint>(frame: &[u8]) -> Result<E::Response, RynkClientError> {
    let cmd = E::CMD.raw();
    let header = frame
        .first_chunk::<RYNK_HEADER_SIZE>()
        .map(RynkHeader::parse)
        .ok_or(RynkClientError::Malformed { cmd })?;
    if header.cmd != E::CMD {
        return Err(RynkClientError::CmdMismatch {
            sent: cmd,
            got: header.cmd.raw(),
        });
    }
    match postcard::take_from_bytes::<Result<E::Response, RynkError>>(&frame[RYNK_HEADER_SIZE..]) {
        Ok((reply, [])) => reply.map_err(|error| RynkClientError::Device { cmd, error }),
        _ => Err(RynkClientError::Malformed { cmd }),
    }
}

fn postcard_len<T: Serialize>(value: &T) -> Option<usize> {
    postcard::serialize_with_flavor(value, postcard::ser_flavors::Size::default()).ok()
}

/// What a board is, as `connect` reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RynkBoard {
    pub(crate) info: ErgohavenInfo,
    pub(crate) capabilities: DeviceCapabilities,
}

/// A start SEQ that differs from session to session, so a reply left over
/// from an earlier session rarely matches the first request.
fn initial_seq() -> u8 {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    (nanos % 255) as u8 + 1
}

/// FNV-1a, the board definition's hash in `ErgohavenInfo`.
pub(crate) fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

pub(crate) struct RynkClient<T> {
    transport: T,
    rx: Vec<u8>,
    deframer: Deframer,
    next_seq: u8,
    max_payload: usize,
    timeouts: RynkTimeouts,
    read_only: Option<ReadOnlyRynkSession>,
}

impl<T: RynkTransport> RynkClient<T> {
    pub(crate) fn new(transport: T, timeouts: RynkTimeouts) -> Self {
        Self {
            transport,
            rx: vec![0; RX_BUFFER_SIZE],
            deframer: Deframer::new(),
            next_seq: initial_seq(),
            max_payload: HANDSHAKE_MAX_PAYLOAD,
            timeouts,
            read_only: read_only_rynk_session(),
        }
    }

    /// A client inside `session`, as `enforce_read_only_rynk` would make it.
    #[cfg(test)]
    pub(crate) fn test_read_only(
        transport: T,
        timeouts: RynkTimeouts,
        session: ReadOnlyRynkSession,
    ) -> Self {
        Self {
            read_only: Some(session),
            ..Self::new(transport, timeouts)
        }
    }

    /// Checks the protocol version and the ergohaven revision, then reads the
    /// board's identity and capabilities; the capabilities size every later
    /// request. The revision is checked before anything else is decoded, since
    /// another revision may have reshaped the rest.
    pub(crate) fn connect(
        transport: T,
        timeouts: RynkTimeouts,
    ) -> Result<(Self, RynkBoard), RynkClientError> {
        let mut client = Self::new(transport, timeouts);
        client.drain()?;
        let board = client.request::<command::GetVersion>(&())?;
        if board != ProtocolVersion::CURRENT {
            return Err(RynkClientError::VersionMismatch {
                board,
                host: ProtocolVersion::CURRENT,
            });
        }
        let revision = client.probe_revision()?;
        if revision != ERGOHAVEN_RYNK_REVISION {
            return Err(RynkClientError::RevisionMismatch {
                board: Some(revision),
                host: ERGOHAVEN_RYNK_REVISION,
            });
        }
        let info = client.request::<ergohaven::GetErgohavenInfo>(&())?;
        let capabilities = client.request::<command::GetCapabilities>(&())?;
        client.max_payload = capabilities.max_payload_size as usize;
        Ok((client, RynkBoard { info, capabilities }))
    }

    /// The revision `GetErgohavenInfo` leads with, read without the rest.
    fn probe_revision(&mut self) -> Result<u16, RynkClientError> {
        let cmd = ergohaven::GetErgohavenInfo::CMD;
        let seq = self.send::<ergohaven::GetErgohavenInfo>(&())?;
        let frame = self.wait_reply(cmd, seq)?;
        match postcard::take_from_bytes::<Result<u16, RynkError>>(&frame[RYNK_HEADER_SIZE..]) {
            Ok((Ok(revision), _)) => Ok(revision),
            Ok((Err(RynkError::UnknownCmd), _)) => Err(RynkClientError::RevisionMismatch {
                board: None,
                host: ERGOHAVEN_RYNK_REVISION,
            }),
            Ok((Err(error), _)) => Err(RynkClientError::Device {
                cmd: cmd.raw(),
                error,
            }),
            Err(_) => Err(RynkClientError::Malformed { cmd: cmd.raw() }),
        }
    }

    /// One request and its reply.
    pub(crate) fn request<E: Endpoint>(
        &mut self,
        request: &E::Request,
    ) -> Result<E::Response, RynkClientError> {
        let seq = self.send::<E>(request)?;
        let reply = self
            .wait_reply(E::CMD, seq)
            .and_then(|frame| decode_reply::<E>(&frame));
        if let (Err(_), Some(session)) = (&reply, &self.read_only) {
            session.record_failed_read(E::CMD);
        }
        reply
    }

    /// Requests of one endpoint and their replies, in order. Over a transport
    /// that pipelines, a window of them goes out back to back before their
    /// replies are read, which the board sends in order. One the board
    /// answers `Busy`, short of room beside the requests parked behind it,
    /// goes again on its own. Elsewhere they go one at a time, as `request`.
    pub(crate) fn request_all<E: Endpoint>(
        &mut self,
        requests: &[E::Request],
    ) -> Result<Vec<E::Response>, RynkClientError> {
        if !self.transport.pipelines() {
            return requests
                .iter()
                .map(|request| self.request::<E>(request))
                .collect();
        }
        self.admit::<E>()?;
        let replies = self.pipeline::<E>(requests);
        if let (Err(_), Some(session)) = (&replies, &self.read_only) {
            session.record_failed_read(E::CMD);
        }
        replies
    }

    fn pipeline<E: Endpoint>(
        &mut self,
        requests: &[E::Request],
    ) -> Result<Vec<E::Response>, RynkClientError> {
        let mut replies: Vec<Option<E::Response>> = requests.iter().map(|_| None).collect();
        let mut queue: VecDeque<usize> = (0..requests.len()).collect();
        let mut alone = false;
        while !queue.is_empty() {
            let mut window = Vec::new();
            let mut frames = Vec::new();
            let mut bytes = 0;
            while let Some(&index) = queue.front() {
                if !window.is_empty() && (alone || window.len() == PIPELINE_FRAMES) {
                    break;
                }
                let seq = self.take_seq();
                let frame = encode_request::<E>(seq, &requests[index], self.max_payload)?;
                if !window.is_empty() && bytes + frame.len() > PIPELINE_BYTES {
                    break;
                }
                bytes += frame.len();
                frames.push(frame);
                window.push((index, seq));
                queue.pop_front();
            }
            self.take_unasked()?;
            self.transport.send_all(&frames)?;
            let mut busy = Vec::new();
            for &(index, seq) in &window {
                let frame = self.wait_reply(E::CMD, seq)?;
                match decode_reply::<E>(&frame) {
                    Ok(reply) => replies[index] = Some(reply),
                    Err(RynkClientError::Device {
                        error: RynkError::Busy,
                        ..
                    }) if window.len() > 1 => busy.push(index),
                    Err(error) => return Err(error),
                }
            }
            alone = !busy.is_empty();
            for index in busy.into_iter().rev() {
                queue.push_front(index);
            }
        }
        Ok(replies
            .into_iter()
            .map(|reply| reply.expect("every request was answered"))
            .collect())
    }

    /// Sends a request without waiting; its reply is dropped when it comes.
    /// Returns the request's SEQ. Every frame leaves through here or
    /// `request_all`, which is where a read-only session refuses what is not
    /// a read.
    pub(crate) fn send<E: Endpoint>(
        &mut self,
        request: &E::Request,
    ) -> Result<u8, RynkClientError> {
        self.admit::<E>()?;
        let seq = self.take_seq();
        let frame = encode_request::<E>(seq, request, self.max_payload)?;
        self.take_unasked()?;
        self.transport.send(&frame)?;
        Ok(seq)
    }

    fn admit<E: Endpoint>(&self) -> Result<(), RynkClientError> {
        if self
            .read_only
            .as_ref()
            .is_some_and(|session| !session.admit(E::CMD))
        {
            return Err(RynkClientError::ReadOnly { cmd: E::CMD.raw() });
        }
        Ok(())
    }

    fn take_seq(&mut self) -> u8 {
        let seq = self.next_seq;
        self.next_seq = if seq == u8::MAX { 1 } else { seq + 1 };
        seq
    }

    /// Reads away what the board sent since the last reply: topics, or the
    /// reply to a request given up on. Over USB the board takes no frame while
    /// one of its own waits for the host, so a frame longer than a packet
    /// would wait for it until the write timed out and broke the link. A read
    /// that finds nothing stays queued and takes what the board sends next.
    fn take_unasked(&mut self) -> Result<(), RynkClientError> {
        loop {
            while self.next_frame().is_some() {}
            let n = self
                .transport
                .recv(self.deframer.tail(&mut self.rx), Duration::ZERO)?;
            if n == 0 {
                return Ok(());
            }
            self.deframer.commit(n);
        }
    }

    fn next_frame(&mut self) -> Option<Incoming> {
        let len = self.deframer.next(&mut self.rx)?;
        let frame = &self.rx[..len];
        let header = RynkHeader::parse(frame.first_chunk().expect("frames hold a header"));
        Some(if header.cmd.is_topic() {
            Incoming::Topic
        } else {
            Incoming::Reply {
                seq: header.seq,
                frame: frame.to_vec(),
            }
        })
    }

    /// Reads away what an earlier session left in the link.
    fn drain(&mut self) -> Result<(), RynkClientError> {
        let limit = Instant::now() + DRAIN_LIMIT;
        loop {
            let wait = DRAIN_QUIET.min(limit.saturating_duration_since(Instant::now()));
            if wait.is_zero() || self.transport.recv(&mut self.rx, wait)? == 0 {
                break;
            }
        }
        self.deframer = Deframer::new();
        Ok(())
    }

    fn wait_reply(&mut self, cmd: Cmd, seq: u8) -> Result<Vec<u8>, RynkClientError> {
        let start = Instant::now();
        let mut last_byte = None::<Instant>;
        loop {
            while let Some(incoming) = self.next_frame() {
                match incoming {
                    // Nothing on this side listens to topics.
                    Incoming::Topic => {}
                    Incoming::Reply { seq: reply, frame } if reply == seq => return Ok(frame),
                    // Anything else answers a request nobody waits for any more.
                    Incoming::Reply { .. } => {}
                }
            }
            let now = Instant::now();
            let deadline = match last_byte {
                Some(at) if self.deframer.has_pending() => at + self.timeouts.idle,
                _ => start + self.timeouts.reply,
            }
            .min(start + self.timeouts.total);
            let Some(wait) = deadline
                .checked_duration_since(now)
                .filter(|wait| !wait.is_zero())
            else {
                return Err(RynkClientError::Timeout { cmd: cmd.raw() });
            };
            let n = self
                .transport
                .recv(self.deframer.tail(&mut self.rx), wait)?;
            if n > 0 {
                self.deframer.commit(n);
                last_byte = Some(Instant::now());
            }
        }
    }

    /// The board definition (Vial's xz-compressed JSON), checked against the
    /// hash `GetErgohavenInfo` reported.
    pub(crate) fn read_board_definition(
        &mut self,
        expected_hash: u32,
    ) -> Result<Vec<u8>, RynkClientError> {
        let cmd = ergohaven::GetBoardDefinition::CMD.raw();
        let mut definition = Vec::new();
        let mut total = None;
        loop {
            let chunk =
                self.request::<ergohaven::GetBoardDefinition>(&(definition.len() as u32))?;
            let total = *total.get_or_insert(chunk.total_len as usize);
            if chunk.total_len as usize != total || definition.len() + chunk.bytes.len() > total {
                return Err(RynkClientError::BulkProtocol { cmd });
            }
            definition.extend_from_slice(&chunk.bytes);
            if definition.len() == total {
                break;
            }
            if chunk.bytes.is_empty() {
                return Err(RynkClientError::BulkProtocol { cmd });
            }
        }
        let actual = fnv1a(&definition);
        if actual != expected_hash {
            return Err(RynkClientError::DefinitionHash {
                expected: expected_hash,
                actual,
            });
        }
        Ok(definition)
    }

    pub(crate) fn layout_options(&mut self) -> Result<u32, RynkClientError> {
        self.request::<ergohaven::GetLayoutOption>(&())
    }

    pub(crate) fn set_layout_options(&mut self, options: u32) -> Result<(), RynkClientError> {
        self.request::<ergohaven::SetLayoutOption>(&options)
    }

    /// Every setting id the board serves, ascending, paged by cursor as Vial's
    /// query is.
    pub(crate) fn setting_keys(&mut self) -> Result<Vec<u16>, RynkClientError> {
        let cmd = ergohaven::GetSettingKeys::CMD.raw();
        let mut keys: Vec<u16> = Vec::new();
        loop {
            let after = keys.last().copied().unwrap_or(0);
            let page = self.request::<ergohaven::GetSettingKeys>(&after)?.0;
            if page.is_empty() {
                return Ok(keys);
            }
            if page.first().is_some_and(|&first| first <= after) || !page.is_sorted_by(|a, b| a < b)
            {
                return Err(RynkClientError::BulkProtocol { cmd });
            }
            keys.extend(page);
        }
    }

    /// A setting's value in the bytes Vial carries for the same id.
    pub(crate) fn setting(&mut self, id: u16) -> Result<Vec<u8>, RynkClientError> {
        Ok(self.request::<ergohaven::GetSetting>(&id)?.0.to_vec())
    }

    /// The settings `ids` name, in order.
    pub(crate) fn settings(&mut self, ids: &[u16]) -> Result<Vec<Vec<u8>>, RynkClientError> {
        Ok(self
            .request_all::<ergohaven::GetSetting>(ids)?
            .into_iter()
            .map(|value| value.0.to_vec())
            .collect())
    }

    pub(crate) fn set_setting(&mut self, id: u16, value: &[u8]) -> Result<(), RynkClientError> {
        let cmd = ergohaven::SetSetting::CMD.raw();
        let value = value.try_into().map_err(|_| RynkClientError::TooLarge {
            cmd,
            len: value.len(),
            max: ergohaven::SETTING_VALUE_MAX_LEN,
        })?;
        self.request::<ergohaven::SetSetting>(&SetSettingRequest {
            id,
            value: SettingValue(value),
        })
    }

    /// Battery levels of the halves; also asks them for fresh ones.
    pub(crate) fn battery_halves(&mut self) -> Result<BatteryHalves, RynkClientError> {
        self.request::<ergohaven::GetBatteryHalves>(&())
    }

    /// Starts writing staged changes and waits until none are pending,
    /// reporting each status read on the way. `Ok` only once the queue is
    /// empty and nothing failed; a board without staging is done at once.
    pub(crate) fn commit_writes(
        &mut self,
        poll: Duration,
        limit: Duration,
        mut progress: impl FnMut(WriteStatus),
    ) -> Result<(), RynkClientError> {
        let deadline = Instant::now() + limit;
        let mut status = self.request::<ergohaven::CommitWrites>(&())?;
        loop {
            progress(status);
            if status.fault {
                return Err(RynkClientError::WriteFault);
            }
            if status.pending_keys == 0 && status.pending_macro_chunks == 0 {
                return Ok(());
            }
            if Instant::now() + poll > deadline {
                return Err(RynkClientError::WritesPending(status));
            }
            std::thread::sleep(poll);
            status = self.request::<ergohaven::GetWriteStatus>(&())?;
        }
    }

    /// Every key of the first `layers` layers, row by row and layer after
    /// layer, in as few pages as the board sends. A page runs on across rows
    /// and layers to the end of the board's keymap, whatever layer it started
    /// in, so the keymap is read in one pass.
    pub(crate) fn read_keymap(
        &mut self,
        layers: u8,
        rows: u8,
        cols: u8,
    ) -> Result<Vec<KeyAction>, RynkClientError> {
        let layer_len = rows as usize * cols as usize;
        let total = layers as usize * layer_len;
        let mut actions = Vec::with_capacity(total);
        while actions.len() < total {
            let at = actions.len();
            let request = GetKeymapBulkRequest {
                layer: (at / layer_len) as u8,
                start_row: (at % layer_len / cols as usize) as u8,
                start_col: (at % cols as usize) as u8,
            };
            let page = self.request::<command::GetKeymapBulk>(&request)?.actions;
            if page.is_empty() {
                return Err(RynkClientError::BulkProtocol {
                    cmd: command::GetKeymapBulk::CMD.raw(),
                });
            }
            // Past the last layer read, the page holds the board's next ones.
            actions.extend(page.into_iter().take(total - at));
        }
        Ok(actions)
    }

    pub(crate) fn read_combos(&mut self, count: u8) -> Result<Vec<Combo>, RynkClientError> {
        self.read_bulk::<command::GetComboBulk, _>(
            count,
            |start_index| GetComboBulkRequest { start_index },
            |page| page.configs,
        )
    }

    pub(crate) fn read_morses(&mut self, count: u8) -> Result<Vec<Morse>, RynkClientError> {
        self.read_bulk::<command::GetMorseBulk, _>(
            count,
            |start_index| GetMorseBulkRequest { start_index },
            |page| page.configs,
        )
    }

    fn read_bulk<E: Endpoint, I>(
        &mut self,
        count: u8,
        request: impl Fn(u8) -> E::Request,
        items: impl Fn(E::Response) -> Vec<I>,
    ) -> Result<Vec<I>, RynkClientError> {
        let mut all = Vec::with_capacity(count as usize);
        while all.len() < count as usize {
            let page = items(self.request::<E>(&request(all.len() as u8))?);
            if page.is_empty() || all.len() + page.len() > count as usize {
                return Err(RynkClientError::BulkProtocol { cmd: E::CMD.raw() });
            }
            all.extend(page);
        }
        Ok(all)
    }

    /// The first `count` macros.
    pub(crate) fn read_macros(&mut self, count: u8) -> Result<Vec<Vec<MacroOp>>, RynkClientError> {
        let indices: Vec<u8> = (0..count).collect();
        Ok(self
            .request_all::<command::GetMacro>(&indices)?
            .iter()
            .map(|stored| stored.ops().collect())
            .collect())
    }

    pub(crate) fn write_macro(
        &mut self,
        index: u8,
        ops: &[MacroOp],
    ) -> Result<(), RynkClientError> {
        let cmd = command::SetMacro::CMD.raw();
        let macro_ops = Macro::from_slice(ops).map_err(|_| RynkClientError::TooLarge {
            cmd,
            len: super::macros::macro_slot_size(ops),
            max: self.max_payload,
        })?;
        self.request::<command::SetMacro>(&SetMacroRequest { index, macro_ops })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A fake board for tests of the client and of what drives it.

    use std::collections::VecDeque;
    use std::sync::{Arc, Condvar, Mutex, MutexGuard};

    use super::*;

    type Handler = Box<dyn FnMut(RynkHeader, &[u8]) -> Vec<Vec<u8>> + Send>;

    struct State {
        handler: Handler,
        pending: VecDeque<u8>,
        chunk: usize,
        usb: bool,
        pipelines: bool,
        /// Frames per `send_all`, one entry per call.
        batches: Vec<usize>,
        sent: Vec<RynkHeader>,
        dropped: usize,
        unplug_on: Option<Cmd>,
        /// Bumped by an unplug: transports opened before it are dead.
        plugged: u32,
    }

    /// A board in a box: decodes each frame a transport sends and queues the
    /// frames its handler answers with, read back `chunk` bytes at a time.
    /// Clones share the board.
    #[derive(Clone)]
    pub(crate) struct FakeBoard(Arc<(Mutex<State>, Condvar)>);

    impl FakeBoard {
        pub(crate) fn new(
            handler: impl FnMut(RynkHeader, &[u8]) -> Vec<Vec<u8>> + Send + 'static,
        ) -> Self {
            Self(Arc::new((
                Mutex::new(State {
                    handler: Box::new(handler),
                    pending: VecDeque::new(),
                    chunk: usize::MAX,
                    usb: false,
                    pipelines: false,
                    batches: Vec::new(),
                    sent: Vec::new(),
                    dropped: 0,
                    unplug_on: None,
                    plugged: 0,
                }),
                Condvar::new(),
            )))
        }

        /// Replies in 32-byte HID reports, zero-padded like `RynkHidService`.
        pub(crate) fn hid_reports(self) -> Self {
            self.state().chunk = 32;
            self
        }

        /// Takes no frame while a reply waits to be read, as over USB: the
        /// firmware's session blocks on the reply until the host polls for it.
        pub(crate) fn usb(self) -> Self {
            self.state().usb = true;
            self
        }

        /// Takes requests back to back, as over BLE.
        pub(crate) fn pipelining(self) -> Self {
            self.state().pipelines = true;
            self
        }

        /// Frames per `send_all` so far.
        pub(crate) fn batches(&self) -> Vec<usize> {
            self.state().batches.clone()
        }

        /// A transport to the board; dropping it is letting the board go.
        pub(crate) fn transport(&self) -> FakeTransport {
            FakeTransport(self.clone(), self.state().plugged)
        }

        /// Bytes the board sends unasked: topics, or what a session left behind.
        pub(crate) fn push(&self, bytes: &[u8]) {
            self.state().pending.extend(bytes);
            self.0 .1.notify_all();
        }

        /// Headers of every frame received, in order.
        pub(crate) fn sent(&self) -> Vec<RynkHeader> {
            self.state().sent.clone()
        }

        /// Pulls the board out as a frame of `cmd` is sent: the transport that
        /// sends it breaks, and a transport opened afterwards finds the board.
        pub(crate) fn unplug_on(&self, cmd: Cmd) {
            self.state().unplug_on = Some(cmd);
        }

        /// Transports dropped so far.
        pub(crate) fn dropped(&self) -> usize {
            self.state().dropped
        }

        fn state(&self) -> MutexGuard<'_, State> {
            self.0 .0.lock().unwrap_or_else(|error| error.into_inner())
        }
    }

    pub(crate) struct FakeTransport(FakeBoard, u32);

    impl RynkTransport for FakeTransport {
        fn send(&mut self, frame: &[u8]) -> io::Result<()> {
            let mut buf = frame.to_vec();
            let mut deframer = Deframer::new();
            let n = deframer.tail(&mut buf).len();
            deframer.commit(n);
            let len = deframer.next(&mut buf).expect("one whole frame");
            let header = RynkHeader::parse(buf.first_chunk().unwrap());
            let mut state = self.0.state();
            if state.plugged != self.1 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            if state.unplug_on == Some(header.cmd) {
                state.unplug_on = None;
                state.plugged += 1;
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            if state.usb && !state.pending.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the board took no frame",
                ));
            }
            state.sent.push(header);
            for mut reply in (state.handler)(header, &buf[RYNK_HEADER_SIZE..len]) {
                if state.chunk != usize::MAX {
                    reply.resize(reply.len().div_ceil(state.chunk) * state.chunk, 0);
                }
                state.pending.extend(reply);
            }
            drop(state);
            self.0 .0 .1.notify_all();
            Ok(())
        }

        fn pipelines(&self) -> bool {
            self.0.state().pipelines
        }

        fn send_all(&mut self, frames: &[Vec<u8>]) -> io::Result<()> {
            self.0.state().batches.push(frames.len());
            frames.iter().try_for_each(|frame| self.send(frame))
        }

        fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
            let (lock, ready) = &*self.0 .0;
            let state = lock.lock().unwrap_or_else(|error| error.into_inner());
            let (mut state, _) = ready
                .wait_timeout_while(state, timeout, |state| state.pending.is_empty())
                .unwrap_or_else(|error| error.into_inner());
            if state.plugged != self.1 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let n = buf.len().min(state.chunk).min(state.pending.len());
            for (slot, byte) in buf.iter_mut().zip(state.pending.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }
    }

    impl Drop for FakeTransport {
        fn drop(&mut self) {
            self.0.state().dropped += 1;
        }
    }

    pub(crate) fn frame<T: Serialize>(cmd: Cmd, seq: u8, value: &T) -> Vec<u8> {
        let mut buf = vec![0; 8192];
        let len = encode_frame(&mut buf, RynkHeader { cmd, seq }, value).unwrap();
        buf.truncate(len);
        buf
    }

    pub(crate) fn ok<T: Serialize>(header: RynkHeader, value: T) -> Vec<u8> {
        frame(header.cmd, header.seq, &Ok::<T, RynkError>(value))
    }

    pub(crate) fn err(header: RynkHeader, error: RynkError) -> Vec<u8> {
        frame(header.cmd, header.seq, &Err::<(), RynkError>(error))
    }

    pub(crate) fn quick() -> RynkTimeouts {
        RynkTimeouts {
            reply: Duration::from_millis(30),
            idle: Duration::from_millis(30),
            total: Duration::from_millis(100),
        }
    }

    pub(crate) fn capabilities(max_payload_size: u16) -> DeviceCapabilities {
        DeviceCapabilities {
            max_payload_size,
            num_layers: 2,
            num_rows: 3,
            num_cols: 5,
            ..Default::default()
        }
    }

    pub(crate) fn info(revision: u16) -> ErgohavenInfo {
        ErgohavenInfo {
            revision,
            firmware: rmk_types_rynk::protocol::rynk::FirmwareVersion {
                major: 2,
                minor: 3,
                patch: 1,
            },
            keyboard_uid: [0x80, 0x04, 1, 2, 3, 4, 5, 6],
            caps: ergohaven::caps::LAYOUT_OPTIONS,
            definition_hash: fnv1a(DEFINITION),
        }
    }

    /// A definition longer than two `GetBoardDefinition` pages.
    pub(crate) const DEFINITION: &[u8] = &[0x5D; 600];

    /// The handshake replies of a board of the current revision.
    pub(crate) fn handshake(header: RynkHeader, max_payload_size: u16) -> Option<Vec<u8>> {
        match header.cmd {
            Cmd::GetVersion => Some(ok(header, ProtocolVersion::CURRENT)),
            Cmd::GetErgohavenInfo => Some(ok(header, info(ERGOHAVEN_RYNK_REVISION))),
            Cmd::GetCapabilities => Some(ok(header, capabilities(max_payload_size))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::testing::*;
    use super::*;
    use rmk_types_rynk::action::Action;
    use rmk_types_rynk::protocol::rynk::GetKeymapBulkResponse;

    #[test]
    fn connect_checks_the_version_and_takes_the_payload_size() {
        let board = FakeBoard::new(|header, _| vec![handshake(header, 4000).unwrap()]);
        let (client, board) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert_eq!(client.max_payload, 4000);
        assert_eq!(board.capabilities.num_cols, 5);
        assert_eq!(board.info, info(ERGOHAVEN_RYNK_REVISION));

        let old =
            FakeBoard::new(|header, _| vec![ok(header, ProtocolVersion { major: 0, minor: 1 })]);
        assert!(matches!(
            RynkClient::connect(old.transport(), quick()),
            Err(RynkClientError::VersionMismatch {
                board: ProtocolVersion { major: 0, minor: 1 },
                ..
            })
        ));
    }

    #[test]
    fn replies_match_by_seq_through_reports_topics_and_stale_frames() {
        let board = FakeBoard::new(|header, _| {
            if let Some(reply) = handshake(header, 512) {
                return vec![reply];
            }
            let stale = frame(
                header.cmd,
                header.seq.wrapping_sub(1),
                &Ok::<u8, RynkError>(9),
            );
            vec![frame(Cmd::LayerChange, 0, &3u8), stale, ok(header, 2u8)]
        })
        .hid_reports();
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert_eq!(client.request::<command::GetCurrentLayer>(&()).unwrap(), 2);
    }

    #[test]
    fn a_frame_the_board_sent_unasked_does_not_hold_the_next_request() {
        let board = FakeBoard::new(|header, _| {
            vec![handshake(header, 4128).unwrap_or_else(|| ok(header, ()))]
        })
        .usb();
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        // A layer change between two requests, and the reply to a request
        // given up on: neither is read by the time the next frame goes out.
        board.push(&frame(Cmd::LayerChange, 0, &3u8));
        board.push(&frame(Cmd::GetCurrentLayer, 9, &Ok::<u8, RynkError>(1)));
        let text = vec![MacroOp::Char(b'a'); 1023];
        client.write_macro(0, &text).unwrap();
        assert_eq!(
            board.sent().last().map(|header| header.cmd),
            Some(Cmd::SetMacro)
        );
    }

    #[test]
    fn board_errors_and_bad_replies_are_told_apart() {
        let board = FakeBoard::new(|header, _| {
            let reply = match header.cmd.raw() {
                0x0801 => err(header, RynkError::UnknownCmd),
                0x0802 => err(header, RynkError::Locked),
                // A u8 reply with a byte too many.
                0x0805 => frame(header.cmd, header.seq, &Ok::<(u16, u8), RynkError>((7, 1))),
                // Answered under another command.
                0x0806 => frame(Cmd::GetWpm, header.seq, &Ok::<u16, RynkError>(7)),
                _ => return vec![],
            };
            vec![reply]
        });
        let mut client = RynkClient::new(board.transport(), quick());
        assert!(matches!(
            client.request::<command::GetCurrentLayer>(&()),
            Err(RynkClientError::Device {
                error: RynkError::UnknownCmd,
                ..
            })
        ));
        assert!(matches!(
            client.request::<command::GetMatrixState>(&()),
            Err(RynkClientError::Device {
                error: RynkError::Locked,
                ..
            })
        ));
        assert!(matches!(
            client.request::<command::GetWpm>(&()),
            Err(RynkClientError::Malformed { cmd: 0x0805 })
        ));
        assert!(matches!(
            client.request::<command::GetSleepState>(&()),
            Err(RynkClientError::CmdMismatch {
                sent: 0x0806,
                got: 0x0805
            })
        ));
        assert!(matches!(
            client.request::<command::GetLedIndicator>(&()),
            Err(RynkClientError::Timeout { cmd: 0x0807 })
        ));
    }

    #[test]
    fn requests_past_the_board_frame_are_refused_before_sending() {
        let board = FakeBoard::new(|header, _| vec![handshake(header, 64).unwrap()]);
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        let ops = vec![MacroOp::Char(b'a'); 40];
        assert!(matches!(
            client.write_macro(0, &ops),
            Err(RynkClientError::TooLarge {
                cmd: 0x0202,
                len: 82,
                max: 64
            })
        ));
        drop(client);
        assert_eq!(board.sent().len(), 4, "only the handshake went out");
    }

    /// Endpoints declared outside the client, as the ergohaven vendor table and
    /// a private branch declare theirs.
    #[test]
    fn endpoints_from_other_tables_go_through_request() {
        enum SetAgents {}
        impl Endpoint for SetAgents {
            const CMD: Cmd = Cmd::from_raw(0x7EF0);
            type Request = [u8; 3];
            type Response = ();
        }
        let board = FakeBoard::new(|header, payload| {
            assert_eq!(header.cmd.raw(), 0x7EF0);
            assert_eq!(payload, [1, 2, 3]);
            vec![ok(header, ())]
        });
        let mut client = RynkClient::new(board.transport(), quick());
        client.request::<SetAgents>(&[1, 2, 3]).unwrap();
    }

    /// Phenom's keymap, as it pages it: 16 layers of 10x8 keys, 255 keys a
    /// page whatever layer the page starts in.
    #[test]
    fn a_keymap_page_runs_on_across_layers() {
        let keys: Vec<KeyAction> = (0..16 * 80)
            .map(|i| KeyAction::Single(Action::User((i % 251) as u8)))
            .collect();
        let stored = keys.clone();
        let pages = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = pages.clone();
        let board = FakeBoard::new(move |header, payload| {
            if let Some(reply) = handshake(header, 4096) {
                return vec![reply];
            }
            assert_eq!(header.cmd, Cmd::GetKeymapBulk);
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let request: GetKeymapBulkRequest = postcard::from_bytes(payload).unwrap();
            let at = request.layer as usize * 80
                + request.start_row as usize * 8
                + request.start_col as usize;
            let actions = stored[at..stored.len().min(at + 255)].to_vec();
            vec![ok(header, GetKeymapBulkResponse { actions })]
        });
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();

        assert_eq!(client.read_keymap(16, 10, 8).unwrap(), keys);
        assert_eq!(
            pages.load(std::sync::atomic::Ordering::SeqCst),
            6,
            "1280 keys in pages of 255"
        );
    }

    #[test]
    fn keymap_pages_are_read_in_order() {
        // Two 3x5 layers.
        let keys: Vec<KeyAction> = (0..30u8)
            .map(|i| KeyAction::Single(Action::User(i)))
            .collect();
        let stored = keys.clone();
        let board = FakeBoard::new(move |header, payload| {
            if let Some(reply) = handshake(header, 40) {
                return vec![reply];
            }
            match header.cmd {
                // As the firmware pages: on across layers, to the keymap's end.
                Cmd::GetKeymapBulk => {
                    let request: GetKeymapBulkRequest = postcard::from_bytes(payload).unwrap();
                    let at = request.layer as usize * 15
                        + request.start_row as usize * 5
                        + request.start_col as usize;
                    let actions = stored[at..stored.len().min(at + 4)].to_vec();
                    vec![ok(header, GetKeymapBulkResponse { actions })]
                }
                _ => vec![],
            }
        });
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert_eq!(client.read_keymap(2, 3, 5).unwrap(), keys);
        // The page from key 12 runs into layer 1, which this read leaves out.
        assert_eq!(client.read_keymap(1, 3, 5).unwrap(), keys[..15]);
    }

    #[test]
    fn read_only_sessions_send_reads_and_refuse_everything_else() {
        let board = FakeBoard::new(|header, _| {
            vec![match header.cmd {
                Cmd::GetVersion => ok(header, ProtocolVersion::CURRENT),
                Cmd::GetCapabilities => ok(header, capabilities(512)),
                Cmd::GetKeyAction => ok(header, KeyAction::No),
                Cmd::GetMacro => ok(header, Macro::new()),
                _ => err(header, RynkError::Locked),
            }]
        });
        let session = ReadOnlyRynkSession::default();
        let mut client = RynkClient::test_read_only(board.transport(), quick(), session.clone());
        client.request::<command::GetVersion>(&()).unwrap();
        client.max_payload = client
            .request::<command::GetCapabilities>(&())
            .unwrap()
            .max_payload_size as usize;
        let position = rmk_types_rynk::protocol::rynk::KeyPosition {
            layer: 0,
            row: 0,
            col: 0,
        };
        assert_eq!(
            client.request::<command::GetKeyAction>(&position).unwrap(),
            KeyAction::No
        );
        assert_eq!(client.read_macros(1).unwrap(), [Vec::<MacroOp>::new()]);
        assert!(matches!(
            client.request::<command::GetMatrixState>(&()),
            Err(RynkClientError::Device {
                error: RynkError::Locked,
                ..
            })
        ));

        enum SetAgents {}
        impl Endpoint for SetAgents {
            const CMD: Cmd = Cmd::from_raw(0x7EF0);
            type Request = ();
            type Response = ();
        }
        let refused = [
            client.request::<command::SetKeyAction>(
                &rmk_types_rynk::protocol::rynk::SetKeyRequest {
                    position,
                    action: KeyAction::No,
                },
            ),
            client.write_macro(0, &[]),
            client.request::<command::StorageReset>(
                &rmk_types_rynk::protocol::rynk::StorageResetMode::Full,
            ),
            client.request::<command::Reboot>(&()),
            client.request::<command::BootloaderJump>(&()),
            client.request::<command::Lock>(&()),
            client.request::<command::UnlockPoll>(&()).map(|_| ()),
            client.request::<command::SwitchBleProfile>(&1),
            client.request::<SetAgents>(&()),
            client.send::<command::SetDefaultLayer>(&1).map(|_| ()),
        ];
        for result in refused {
            assert!(
                matches!(result, Err(RynkClientError::ReadOnly { .. })),
                "{result:?}"
            );
        }
        drop(client);
        let sent: Vec<u16> = board.sent().iter().map(|header| header.cmd.raw()).collect();
        assert_eq!(sent, [0x0001, 0x0002, 0x0101, 0x0201, 0x0802]);
        assert_eq!(
            session.refused_requests(),
            [0x0102, 0x0202, 0x0005, 0x0003, 0x0004, 0x0008, 0x0007, 0x0704, 0x7EF0, 0x0104]
        );
        assert_eq!(session.failed_reads(), [0x0802]);
    }

    #[test]
    fn connect_refuses_another_revision_before_decoding_the_rest() {
        let next = ERGOHAVEN_RYNK_REVISION + 1;
        // A later revision may have reshaped the info: here it grew a field.
        let later = FakeBoard::new(move |header, _| {
            vec![match header.cmd {
                Cmd::GetErgohavenInfo => ok(header, (next, [0xAAu8; 9], "more")),
                _ => handshake(header, 512).unwrap(),
            }]
        });
        assert!(matches!(
            RynkClient::connect(later.transport(), quick()),
            Err(RynkClientError::RevisionMismatch { board: Some(board), .. }) if board == next
        ));
        // Plain RMK firmware has no ergohaven table.
        let plain = FakeBoard::new(|header, _| {
            vec![match header.cmd {
                Cmd::GetErgohavenInfo => err(header, RynkError::UnknownCmd),
                _ => handshake(header, 512).unwrap(),
            }]
        });
        assert!(matches!(
            RynkClient::connect(plain.transport(), quick()),
            Err(RynkClientError::RevisionMismatch { board: None, .. })
        ));
        drop(plain);
    }

    #[test]
    fn the_board_definition_is_paged_in_and_checked_against_its_hash() {
        let serve = |definition: &'static [u8], total: Option<u32>| {
            move |header: RynkHeader, payload: &[u8]| {
                if let Some(reply) = handshake(header, 512) {
                    return vec![reply];
                }
                assert_eq!(header.cmd, Cmd::GetBoardDefinition);
                let offset: u32 = postcard::from_bytes(payload).unwrap();
                let start = (offset as usize).min(definition.len());
                let end = (start + 244).min(definition.len());
                let chunk = rmk_types_rynk::protocol::rynk::LayoutChunk {
                    total_len: total.unwrap_or(definition.len() as u32),
                    bytes: definition[start..end].try_into().unwrap(),
                };
                vec![ok(header, chunk)]
            }
        };
        let board = FakeBoard::new(serve(DEFINITION, None));
        let (mut client, info) = RynkClient::connect(board.transport(), quick()).unwrap();
        let hash = info.info.definition_hash;
        assert_eq!(client.read_board_definition(hash).unwrap(), DEFINITION);
        assert!(matches!(
            client.read_board_definition(hash ^ 1),
            Err(RynkClientError::DefinitionHash { .. })
        ));
        drop(client);
        let pages = board
            .sent()
            .iter()
            .filter(|h| h.cmd == Cmd::GetBoardDefinition)
            .count();
        assert_eq!(pages, 6, "three 244-byte pages per read");

        // A board that claims more than it sends.
        let short = FakeBoard::new(serve(DEFINITION, Some(700)));
        let (mut client, _) = RynkClient::connect(short.transport(), quick()).unwrap();
        assert!(matches!(
            client.read_board_definition(hash),
            Err(RynkClientError::BulkProtocol { cmd: 0x7E02 })
        ));
    }

    #[test]
    fn layout_options_read_and_write() {
        let stored = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(5));
        let cell = stored.clone();
        let board = FakeBoard::new(move |header, payload| {
            if let Some(reply) = handshake(header, 512) {
                return vec![reply];
            }
            vec![match header.cmd {
                Cmd::GetLayoutOption => ok(header, cell.load(std::sync::atomic::Ordering::Relaxed)),
                Cmd::SetLayoutOption => {
                    cell.store(
                        postcard::from_bytes(payload).unwrap(),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    ok(header, ())
                }
                _ => return vec![],
            }]
        });
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert_eq!(client.layout_options().unwrap(), 5);
        client.set_layout_options(0x1_0002).unwrap();
        assert_eq!(client.layout_options().unwrap(), 0x1_0002);
        assert_eq!(stored.load(std::sync::atomic::Ordering::Relaxed), 0x1_0002);
    }

    #[test]
    fn connect_drains_the_link() {
        let board = FakeBoard::new(|header, _| {
            vec![handshake(header, 512).unwrap_or_else(|| ok(header, 7u8))]
        });
        // Left over from an earlier session: a reply and half a frame.
        let stale = frame(Cmd::GetCurrentLayer, 9, &Ok::<u8, RynkError>(1));
        board.push(&[&stale[..], &stale[..3]].concat());
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert!(!client.deframer.has_pending());
        assert_eq!(client.next_frame(), None);
        assert_eq!(client.request::<command::GetCurrentLayer>(&()).unwrap(), 7);
    }

    #[test]
    fn sessions_start_at_a_seq_in_range() {
        for _ in 0..100 {
            assert!((1..=u8::MAX).contains(&initial_seq()));
        }
    }

    #[test]
    fn settings_page_by_cursor_and_keep_vial_bytes() {
        let ids: Vec<u16> = (1..=150).map(|i| i * 2).collect();
        let values = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let store = values.clone();
        let board = FakeBoard::new(move |header, payload| {
            if let Some(reply) = handshake(header, 512) {
                return vec![reply];
            }
            vec![match header.cmd {
                Cmd::GetSettingKeys => {
                    let after: u16 = postcard::from_bytes(payload).unwrap();
                    let page: Vec<u16> = ids
                        .iter()
                        .copied()
                        .filter(|&id| id > after)
                        .take(64)
                        .collect();
                    ok(
                        header,
                        ergohaven::SettingKeysPage(page.as_slice().try_into().unwrap()),
                    )
                }
                Cmd::SetSetting => {
                    let request: SetSettingRequest = postcard::from_bytes(payload).unwrap();
                    store.lock().unwrap().insert(request.id, request.value);
                    ok(header, ())
                }
                Cmd::GetSetting => {
                    let id: u16 = postcard::from_bytes(payload).unwrap();
                    match store.lock().unwrap().get(&id) {
                        Some(value) => ok(header, value.clone()),
                        None => err(header, RynkError::Invalid),
                    }
                }
                _ => return vec![],
            }]
        });
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        let keys = client.setting_keys().unwrap();
        assert_eq!(keys, (1..=150).map(|i| i * 2).collect::<Vec<u16>>());
        let pages = board
            .sent()
            .iter()
            .filter(|h| h.cmd == Cmd::GetSettingKeys)
            .count();
        assert_eq!(pages, 4, "three pages and the empty one");

        // A layer name: UTF-8 and its NUL, as Vial carries it.
        let name = b"\xd0\x9d\xd0\xb0\xd0\xb2\0";
        client.set_setting(200, name).unwrap();
        assert_eq!(client.setting(200).unwrap(), name);
        assert!(client
            .setting(201)
            .unwrap_err()
            .to_string()
            .contains("Invalid"));
        assert!(matches!(
            client.set_setting(200, &[0; 29]),
            Err(RynkClientError::TooLarge {
                len: 29,
                max: 28,
                ..
            })
        ));
        assert_eq!(values.lock().unwrap().len(), 1);
    }

    #[test]
    fn setting_pages_that_do_not_advance_are_refused() {
        let board = FakeBoard::new(|header, _| {
            vec![handshake(header, 512).unwrap_or_else(|| {
                ok(
                    header,
                    ergohaven::SettingKeysPage([5u16, 3].as_slice().try_into().unwrap()),
                )
            })]
        });
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert!(matches!(
            client.setting_keys(),
            Err(RynkClientError::BulkProtocol { cmd: 0x7E10 })
        ));
    }

    #[test]
    fn commit_waits_for_an_empty_queue_and_reports_a_fault() {
        let statuses = |list: Vec<(u16, u16, bool)>| {
            let list = std::sync::Arc::new(std::sync::Mutex::new(VecDeque::from(list)));
            FakeBoard::new(move |header, _| {
                if let Some(reply) = handshake(header, 512) {
                    return vec![reply];
                }
                assert!(matches!(
                    header.cmd,
                    Cmd::CommitWrites | Cmd::GetWriteStatus
                ));
                let (pending_keys, pending_macro_chunks, fault) =
                    list.lock().unwrap().pop_front().unwrap_or((0, 0, false));
                vec![ok(
                    header,
                    WriteStatus {
                        pending_keys,
                        pending_macro_chunks,
                        fault,
                    },
                )]
            })
        };
        let poll = Duration::from_millis(1);

        let board = statuses(vec![(40, 2, false), (12, 1, false), (0, 0, false)]);
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        let mut seen = Vec::new();
        client
            .commit_writes(poll, Duration::from_secs(5), |status| {
                seen.push(status.pending_keys)
            })
            .unwrap();
        assert_eq!(seen, [40, 12, 0]);
        let commits: Vec<u16> = board.sent()[4..].iter().map(|h| h.cmd.raw()).collect();
        assert_eq!(commits, [0x7E50, 0x7E51, 0x7E51]);

        let board = statuses(vec![(3, 0, false), (0, 0, true)]);
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert!(matches!(
            client.commit_writes(poll, Duration::from_secs(5), |_| {}),
            Err(RynkClientError::WriteFault)
        ));

        let board = statuses(vec![(9, 0, false); 1000]);
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert!(matches!(
            client.commit_writes(poll, Duration::from_millis(20), |_| {}),
            Err(RynkClientError::WritesPending(WriteStatus {
                pending_keys: 9,
                ..
            }))
        ));
    }

    #[test]
    fn every_error_reads_in_both_languages_and_says_what_to_update() {
        let version = |major, minor| ProtocolVersion { major, minor };
        let status = WriteStatus {
            pending_keys: 3,
            pending_macro_chunks: 1,
            fault: false,
        };
        let mut errors = vec![
            RynkClientError::Transport(io::Error::other("gone")),
            RynkClientError::Timeout { cmd: 0x0101 },
            RynkClientError::Malformed { cmd: 0x0101 },
            RynkClientError::CmdMismatch {
                sent: 0x0101,
                got: 0x0102,
            },
            RynkClientError::TooLarge {
                cmd: 0x0101,
                len: 600,
                max: 512,
            },
            RynkClientError::VersionMismatch {
                board: version(2, 0),
                host: version(1, 0),
            },
            RynkClientError::VersionMismatch {
                board: version(1, 0),
                host: version(1, 1),
            },
            RynkClientError::RevisionMismatch {
                board: Some(ERGOHAVEN_RYNK_REVISION + 1),
                host: ERGOHAVEN_RYNK_REVISION,
            },
            RynkClientError::RevisionMismatch {
                board: Some(0),
                host: ERGOHAVEN_RYNK_REVISION,
            },
            RynkClientError::RevisionMismatch {
                board: None,
                host: ERGOHAVEN_RYNK_REVISION,
            },
            RynkClientError::DefinitionHash {
                expected: 1,
                actual: 2,
            },
            RynkClientError::WriteFault,
            RynkClientError::WritesPending(status),
            RynkClientError::BulkProtocol { cmd: 0x0101 },
            RynkClientError::ReadOnly { cmd: 0x0101 },
        ];
        errors.extend(
            [
                RynkError::Malformed,
                RynkError::NotReady,
                RynkError::StorageFault,
                RynkError::Internal,
                RynkError::Unimplemented,
                RynkError::Invalid,
                RynkError::UnknownCmd,
                RynkError::Locked,
                RynkError::Busy,
            ]
            .map(|error| RynkClientError::Device { cmd: 0x0101, error }),
        );
        for error in &errors {
            let english = error.describe(crate::i18n::Language::English);
            let russian = error.describe(crate::i18n::Language::Russian);
            for text in [&english, &russian] {
                assert!(
                    !text.is_empty() && !text.contains('{') && !text.contains("rynk_client."),
                    "{error}: {text}"
                );
            }
            assert_ne!(english, russian, "{error}");
        }
        let update = |error: &RynkClientError| error.describe(crate::i18n::Language::English);
        assert!(update(&errors[5]).ends_with("update Entropy"));
        assert!(update(&errors[6]).ends_with("update the firmware"));
        assert!(update(&errors[7]).ends_with("update Entropy"));
        assert!(update(&errors[8]).ends_with("update the firmware"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_board_bluetooth_does_not_reach_is_named_as_out_of_reach() {
        let away = RynkClientError::Transport(io::Error::new(
            io::ErrorKind::NotConnected,
            super::super::transport::NotConnected {
                board: "K:04 Mini".into(),
            },
        ));
        let lost = RynkClientError::Transport(io::Error::other("gone"));
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            let text = away.describe(language);
            assert_eq!(
                text,
                crate::i18n::tr_catalog_format(
                    language,
                    "rynk_client.not_connected",
                    &[("board", "K:04 Mini")]
                )
            );
            assert!(text.contains("K:04 Mini") && !text.contains('{'), "{text}");
            assert_ne!(text, lost.describe(language));
        }
    }

    /// A board that answers `GetSetting` with the id, little-endian, and
    /// calls `busy` first: a `Busy` there answers that request instead.
    fn settings_board(mut busy: impl FnMut(u16) -> bool + Send + 'static) -> FakeBoard {
        FakeBoard::new(move |header, payload| {
            if let Some(reply) = handshake(header, 4128) {
                return vec![reply];
            }
            let id: u16 = postcard::from_bytes(payload).unwrap();
            if busy(id) {
                return vec![err(header, RynkError::Busy)];
            }
            let value = SettingValue(id.to_le_bytes().as_slice().try_into().unwrap());
            vec![ok(header, value)]
        })
    }

    #[test]
    fn pipelined_reads_go_out_in_windows_and_come_back_in_order() {
        let board = settings_board(|_| false).pipelining();
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        let ids: Vec<u16> = (0..100).collect();

        let values = client.settings(&ids).unwrap();

        let expected: Vec<Vec<u8>> = ids.iter().map(|id| id.to_le_bytes().to_vec()).collect();
        assert_eq!(values, expected);
        let batches = board.batches();
        assert_eq!(batches.iter().sum::<usize>(), ids.len());
        assert!(batches.len() <= 4, "{batches:?}");
        assert!(
            batches
                .iter()
                .all(|&frames| (2..=PIPELINE_FRAMES).contains(&frames)),
            "{batches:?}"
        );
        let settings = board
            .sent()
            .iter()
            .filter(|header| header.cmd == ergohaven::GetSetting::CMD)
            .count();
        assert_eq!(settings, ids.len(), "each id asked once");
    }

    #[test]
    fn a_request_the_board_is_too_busy_for_goes_again_on_its_own() {
        let mut first = true;
        let board = settings_board(move |id| id == 5 && std::mem::take(&mut first)).pipelining();
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        let ids: Vec<u16> = (0..10).collect();

        let values = client.settings(&ids).unwrap();

        assert_eq!(values[5], 5u16.to_le_bytes());
        assert_eq!(values.len(), 10);
        assert_eq!(
            board.batches(),
            [10, 1],
            "the busy one alone, then nothing left"
        );

        // Busy on its own too: the board means it.
        let board = settings_board(|id| id == 7).pipelining();
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();
        assert!(matches!(
            client.settings(&[6, 7, 8]),
            Err(RynkClientError::Device {
                error: RynkError::Busy,
                ..
            })
        ));
    }

    #[test]
    fn without_pipelining_reads_go_one_at_a_time() {
        let board = settings_board(|_| false).usb();
        let (mut client, _) = RynkClient::connect(board.transport(), quick()).unwrap();

        assert_eq!(client.settings(&[3, 4]).unwrap(), [[3, 0], [4, 0]]);
        assert_eq!(
            board.batches(),
            Vec::<usize>::new(),
            "no frames back to back"
        );
    }

    #[test]
    fn read_only_sessions_pipeline_reads_and_refuse_writes_before_sending() {
        let board = settings_board(|_| false).pipelining();
        let session = ReadOnlyRynkSession::default();
        let mut client = RynkClient::test_read_only(board.transport(), quick(), session.clone());
        client.max_payload = 4128;

        assert_eq!(client.settings(&[1, 2]).unwrap(), [[1, 0], [2, 0]]);
        let request = SetSettingRequest {
            id: 1,
            value: SettingValue::default(),
        };
        assert!(matches!(
            client.request_all::<ergohaven::SetSetting>(&[request]),
            Err(RynkClientError::ReadOnly { .. })
        ));
        assert_eq!(board.batches(), [2], "the write never left");
        assert_eq!(
            session.refused_requests(),
            [ergohaven::SetSetting::CMD.raw()]
        );
    }
}
