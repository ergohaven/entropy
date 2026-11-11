//! Bounded connection/autolayer timing diagnostics. No HID payloads or identities.
use std::{io::Write, sync::{Mutex, OnceLock, atomic::{AtomicUsize, Ordering}}, time::{Instant, SystemTime, UNIX_EPOCH}};
const LIMIT: usize = 20_000;
static RECORDS: AtomicUsize = AtomicUsize::new(0);
static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
static MONOTONIC_START: OnceLock<Instant> = OnceLock::new();

fn metadata(data: &[u8]) -> String {
    let op = data.first().copied().unwrap_or(0);
    let mut result = format!("opcode={op:02X} bytes={}", data.len());
    if op == 0xFE {
        if let Some(sub) = data.get(1) { result.push_str(&format!(" sub={sub:02X}")); }
        if data.len() >= 4 && matches!(data[1], 0x09..=0x0B) {
            result.push_str(&format!(" qsid={}",u16::from_le_bytes([data[2],data[3]])));
        }
    }
    result
}

pub(crate) fn event(message: &str) {
    #[cfg(test)] { let _ = message; return; }
    #[allow(unreachable_code)]
    {
        let mono_us = MONOTONIC_START.get_or_init(Instant::now).elapsed().as_micros();
        if RECORDS.fetch_add(1, Ordering::Relaxed) >= LIMIT { return; }
        let file = FILE.get_or_init(|| {
            let path = dirs::config_dir()?.join("entropy").join("logs");
            std::fs::create_dir_all(&path).ok()?;
            std::fs::OpenOptions::new().create(true).append(true)
                .open(path.join("entropy-connect-diagnostic.log")).ok().map(Mutex::new)
        });
        let Some(file) = file else { return; };
        if let Ok(mut file) = file.lock() {
            let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
            let _ = writeln!(file,"epoch_ms={ms} mono_us={mono_us} pid={} version={} {message}",std::process::id(),env!("CARGO_PKG_VERSION"));
        }
    }
}

pub(crate) fn request(label: &str, data: &[u8], started: Instant, ok: bool) {
    event(&format!("{label} {} elapsed_us={} ok={ok}",metadata(data),started.elapsed().as_micros()));
}

pub(crate) fn attempt(data: &[u8], attempt: usize, timeout: i32) {
    event(&format!("native.attempt {} attempt={} timeout_ms={timeout}",metadata(data),attempt+1));
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn connect_trace_never_formats_payload_or_identity() {
        let mut packet=[b'X';32];packet[..4].copy_from_slice(&[0xFE,0x0B,230,0]);
        assert_eq!(metadata(&packet),"opcode=FE bytes=32 sub=0B qsid=230");
        assert_eq!(metadata(&[0x0F,b'S',b'E',b'C',b'R',b'E',b'T']),"opcode=0F bytes=7");
        assert_eq!(metadata(&[0xE6,8,0xA5,12,13]),"opcode=E6 bytes=5");
    }
    #[test] fn connect_trace_handles_empty_and_short_commands() {
        assert_eq!(metadata(&[]),"opcode=00 bytes=0");
        assert_eq!(metadata(&[0xFE]),"opcode=FE bytes=1");
    }
}
