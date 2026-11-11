//! Portable, source-backed request and completion policy for the Windows barrier.
#![allow(dead_code)]
pub(super) const FLAG: &str = "--entropy-backup-volume-helper-v1";
pub(super) const BUDGET_MS: u64 = 120_000;
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Request {
    pub nonce: u32,
    pub deadline: u64,
    pub serial: u32,
    pub file_id: u64,
    pub volume: Vec<u16>,
    pub path: Vec<u16>,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Dispatch {
    Ordinary,
    Invalid,
    Helper(Request),
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn hex(a: &[u16], len: usize) -> Option<u64> {
    if a.len() != len {
        return None;
    }
    a.iter().try_fold(0u64, |n, &c| {
        let d = match c {
            48..=57 => c - 48,
            97..=102 => c - 87,
            _ => return None,
        };
        n.checked_mul(16)?.checked_add(u64::from(d))
    })
}
pub(super) fn valid_nonce(n: u32) -> bool {
    // Reserved application customer-success domain: excludes ordinary exit
    // errors, Rust panic 101, STILL_ACTIVE and NTSTATUS exception severities.
    (0x2000_0000..=0x3fff_ffff).contains(&n)
}
/// Desired access used by production pins. Bit 1 is FILE_READ_DATA for files
/// and FILE_LIST_DIRECTORY for directories; metadata-only opens do not pin.
pub(super) fn pin_access() -> u32 {
    0x80 | 1
}
pub(super) fn volume_valid(a: &[u16]) -> bool {
    let prefix = wide("\\\\?\\Volume{");
    a.len() == 49
        && a.starts_with(&prefix)
        && a[47] == 125
        && a[48] == 92
        && a[11..47].iter().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                *c == 45
            } else {
                matches!(*c,48..=57|65..=70|97..=102)
            }
        })
}
/// Strict lexical drive path: no UNC, device namespace, ADS, dot walk or DOS trimming.
/// Extended drive prefixes are accepted, but the OS handles still decide actual identity.
pub(super) fn drive_path(a: &[u16]) -> Option<Vec<u16>> {
    let a = if a.starts_with(&wide("\\\\?\\")) {
        &a[4..]
    } else {
        a
    };
    if a.len() < 4
        || !matches!(a[0],65..=90|97..=122)
        || a[1] != 58
        || a[2] != 92
        || a.contains(&0)
        || a.contains(&47)
    {
        return None;
    }
    for part in a[3..].split(|c| *c == 92) {
        if part.is_empty()
            || part == [46]
            || part == [46, 46]
            || matches!(part.last(), Some(32 | 46))
            || part
                .iter()
                .any(|c| matches!(*c, 0..=31 | 34 | 42 | 58 | 60 | 62 | 63 | 124))
        {
            return None;
        }
    }
    Some(a.to_vec())
}
pub(super) fn parse(args: &[Vec<u16>], now: u64) -> Dispatch {
    let flag = wide(FLAG);
    if !args
        .iter()
        .any(|a| a.starts_with(&wide("--entropy-backup-volume-helper")))
    {
        return Dispatch::Ordinary;
    }
    let read = || -> Option<Request> {
        if args.len() != 7 || args[0] != flag {
            return None;
        }
        let nonce = hex(&args[1], 8)? as u32;
        let deadline = hex(&args[2], 16)?;
        if !valid_nonce(nonce) || deadline <= now || deadline.checked_sub(now)? > BUDGET_MS {
            return None;
        }
        if !volume_valid(&args[5]) {
            return None;
        }
        Some(Request {
            nonce,
            deadline,
            serial: hex(&args[3], 8)? as u32,
            file_id: hex(&args[4], 16)?,
            volume: args[5].clone(),
            path: drive_path(&args[6])?,
        })
    };
    read().map(Dispatch::Helper).unwrap_or(Dispatch::Invalid)
}
/// Standard Windows argv quoting, preserving UTF-16 (including unpaired surrogates).
pub(super) fn quote(arg: &[u16]) -> Result<Vec<u16>, &'static str> {
    if arg.contains(&0) {
        return Err("NUL in Windows argument");
    }
    let mut out = vec![34];
    let mut slashes = 0;
    for &c in arg {
        if c == 92 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            92,
            if c == 34 { slashes * 2 + 1 } else { slashes },
        ));
        out.push(c);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(92, slashes * 2));
    out.push(34);
    Ok(out)
}
pub(super) fn completed(nonce: u32, signaled: bool, exit: u32, expired: bool) -> bool {
    valid_nonce(nonce) && signaled && !expired && exit == nonce
}
/// Ownership handed to the importing caller after helper completion.
/// The worker owns OS pins and waits for the paired receiver to disconnect.
#[derive(Debug)]
pub(super) struct CompletionLease {
    _release: std::sync::mpsc::Sender<()>,
}
pub(super) fn completion_lease() -> (CompletionLease, std::sync::mpsc::Receiver<()>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    (CompletionLease { _release: sender }, receiver)
}

pub(super) struct Gate(std::sync::atomic::AtomicBool);
pub(super) struct Permit<'a>(&'a Gate);
impl Gate {
    pub const fn new() -> Self {
        Self(std::sync::atomic::AtomicBool::new(false))
    }
    pub fn acquire(&self) -> Option<Permit<'_>> {
        use std::sync::atomic::Ordering;
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Permit(self))
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0 .0.store(false, std::sync::atomic::Ordering::Release);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    #[test]
    fn valid_request_and_strict_numbers_deadline_nonce() {
        let mut a = vec![
            w(FLAG),
            w("21234567"),
            w("000000000001d4c0"),
            w("12345678"),
            w("0123456789abcdef"),
            w("\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\"),
            w("C:\\backup\\保存.entlayout"),
        ];
        assert!(matches!(parse(&a, 1), Dispatch::Helper(_)));
        for (index, bad) in [
            (1, "00000000"),
            (1, "00000103"),
            (1, "+1234567"),
            (2, "ffffffffffffffff"),
            (3, "1234567"),
            (4, "xyz"),
            (5, "\\\\server\\share\\"),
            (6, "C:\\a\\..\\b"),
            (6, "C:\\x:stream"),
            (6, "C:\\link\\."),
        ] {
            let saved = a[index].clone();
            a[index] = w(bad);
            assert_eq!(parse(&a, 1), Dispatch::Invalid, "{index}: {bad}");
            a[index] = saved;
        }
        assert_eq!(parse(&a, 120_000), Dispatch::Invalid);
        a.push(w("extra"));
        assert_eq!(parse(&a, 1), Dispatch::Invalid);
    }
    #[test]
    fn quoting_is_unicode_lossless_and_rejects_nul() {
        assert_eq!(quote(&w("C:\\保存 x\\")).unwrap(), w("\"C:\\保存 x\\\\\""));
        assert_eq!(quote(&w("a\"b")).unwrap(), w("\"a\\\"b\""));
        assert_eq!(quote(&[]).unwrap(), w("\"\""));
        assert_eq!(quote(&[0xd800]).unwrap(), vec![34, 0xd800, 34]);
        assert!(quote(&[0]).is_err());
    }
    #[test]
    fn only_signaled_same_attempt_nonzero_exit_before_deadline_succeeds() {
        assert!(completed(0x21234567, true, 0x21234567, false));
        for (nonce, signal, exit, expired) in [
            (101, true, 101, false),
            (0xc0000005, true, 0xc0000005, false),
            (0x2000002a, false, 0x2000002a, false),
            (0x2000002a, true, 0, false),
            (0x2000002a, true, 0x2000002b, false),
            (0x2000002a, true, 0x2000002a, true),
            (0, true, 0, false),
            (259, true, 259, false),
            (1, true, 1, false),
        ] {
            assert!(!completed(nonce, signal, exit, expired));
        }
    }
    #[test]
    fn timed_out_receiver_does_not_release_outstanding_helper_slot() {
        let gate = Gate::new();
        let permit = gate.acquire().expect("first attempt");
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(1))
            .is_err());
        drop(rx);
        assert!(
            gate.acquire().is_none(),
            "timeout must not authorize another helper"
        );
        assert!(tx.send(()).is_err());
        assert!(
            gate.acquire().is_none(),
            "late completion delivery failure retains worker-owned slot"
        );
        drop(permit);
        let again = gate.acquire().expect("actual worker exit releases slot");
        assert!(gate.acquire().is_none());
        drop(again);
    }
    #[test]
    fn completion_lease_keeps_worker_protection_until_import_releases_it() {
        let gate = std::sync::Arc::new(Gate::new());
        let worker_gate = gate.clone();
        let (lease, release) = completion_lease();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _permit = worker_gate.acquire().unwrap();
            ready_tx.send(()).unwrap();
            let _ = release.recv();
            done_tx.send(()).unwrap();
        });
        ready_rx.recv().unwrap();
        assert!(
            matches!(
                done_rx.recv_timeout(std::time::Duration::from_millis(25)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ),
            "successful delivery released protection before caller's SET"
        );
        assert!(gate.acquire().is_none());
        drop(lease);
        worker.join().unwrap();
        assert!(gate.acquire().is_some());
    }

    #[test]
    fn pin_access_participates_in_windows_share_checks() {
        assert_ne!(
            pin_access() & 1,
            0,
            "metadata-only opens cannot establish sharing pins"
        );
        assert_ne!(pin_access() & 0x80, 0);
    }

    #[test]
    fn malformed_helper_never_falls_through_to_gui() {
        assert_eq!(parse(&[w(FLAG)], 0), Dispatch::Invalid);
        assert_eq!(parse(&[w("ordinary"), w(FLAG)], 0), Dispatch::Invalid);
        assert_eq!(parse(&[w(&format!("{FLAG}=oops"))], 0), Dispatch::Invalid);
        assert_eq!(
            parse(&[w("--entropy-backup-volume-helper-v2")], 0),
            Dispatch::Invalid
        );
        assert_eq!(
            parse(&[w("--entropy-backup-volume-helper")], 0),
            Dispatch::Invalid
        );
        assert_eq!(parse(&[w("--ordinary")], 0), Dispatch::Ordinary);
    }
}
