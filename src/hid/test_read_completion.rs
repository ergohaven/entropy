//! Completion watchdogs for scripted read-only profile recovery, not product latency.
use std::time::{Duration, Instant};

pub(crate) struct TestReadCompletionGuard {
    total_deadline: Instant,
    progress_deadline: Instant,
    idle_timeout: Duration,
    cursor: Option<[u8; 32]>,
}

impl TestReadCompletionGuard {
    // Existing read-only archive fixture policy; never extend this total bound.
    pub(crate) const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

    pub(crate) fn new(idle_timeout: Duration) -> Self {
        Self::at(Instant::now(), idle_timeout)
    }

    fn at(now: Instant, idle_timeout: Duration) -> Self {
        Self {
            total_deadline: now + Self::TOTAL_TIMEOUT,
            progress_deadline: now + idle_timeout,
            idle_timeout,
            cursor: None,
        }
    }

    pub(crate) fn is_live(&mut self, cursor: Option<[u8; 32]>) -> bool {
        self.observe_at(Instant::now(), cursor)
    }

    fn observe_at(&mut self, now: Instant, cursor: Option<[u8; 32]>) -> bool {
        if now >= self.total_deadline || now >= self.progress_deadline {
            return false;
        }
        // This observes issued work, not validated replies or completion. Only
        // the caller's original CRC/archive/owner predicate can accept success.
        // Retrying the same offset with a fresh nonce is not forward work.
        let cursor = cursor.map(|mut packet| {
            packet[4] = 0;
            packet
        });
        if cursor.is_some() && cursor != self.cursor {
            self.cursor = cursor;
            self.progress_deadline = now + self.idle_timeout;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(offset: u16, nonce: u8) -> Option<[u8; 32]> {
        let mut packet = [0; 32];
        packet[..5].copy_from_slice(&[0xef, 9, 6, 0, nonce]);
        packet[5..7].copy_from_slice(&offset.to_le_bytes());
        Some(packet)
    }

    #[test]
    fn advancing_reads_keep_idle_window_without_moving_total_deadline() {
        let now = Instant::now();
        let mut guard = TestReadCompletionGuard::at(now, Duration::from_secs(5));
        for seconds in (0..30).step_by(4) {
            assert!(
                guard.observe_at(
                    now + Duration::from_secs(seconds),
                    read(seconds as u16 * 20, 1)
                ),
                "valid READ progress must not expire an aggregate five-second budget"
            );
        }
        assert!(
            !guard.observe_at(now + TestReadCompletionGuard::TOTAL_TIMEOUT, read(600, 1)),
            "progress must never extend the fixed total bound"
        );
    }

    #[test]
    fn nonce_only_retries_do_not_extend_idle_deadline() {
        let now = Instant::now();
        let mut guard = TestReadCompletionGuard::at(now, Duration::from_secs(5));
        assert!(guard.observe_at(now, read(0, 1)));
        assert!(guard.observe_at(now + Duration::from_secs(4), read(0, 2)));
        assert!(!guard.observe_at(now + Duration::from_secs(5), read(0, 3)));
    }

    #[test]
    fn a_late_cursor_cannot_revive_an_expired_guard() {
        let now = Instant::now();
        let mut guard = TestReadCompletionGuard::at(now, Duration::from_secs(5));
        assert!(guard.observe_at(now, read(0, 1)));
        assert!(!guard.observe_at(now + Duration::from_secs(5), read(20, 2)));
    }
}
