//! Bounded wire-0x19 KEYMAP decoder. This is not a panel FPS instrument.
use super::*;

impl Session {
    pub(super) fn finish_trace(&mut self, q: [u8; 32], r: [u8; 32]) {
        if !crate::hid::HidDevice::sm62_trace_reply(&q, &r) || r[7] != q[3] {
            self.stop("ERROR: KEYMAP trace rejected/malformed/pending; no retry or accepted measurement");
            return;
        }
        if q[3] == 1 {
            self.sides[0] = r[6] & 15;
            self.phase = Phase::Measuring(Instant::now() + Duration::from_secs(6));
            if let Err(e) = self.record("ARM accepted: waiting for first actual KEYMAP, ignoring ORBIT. Press now; no polls for 6s. Normal traffic permitted.") {
                self.stop(&format!("ERROR log write: {e}"));
            }
            return;
        }
        if r[6] & 15 != self.sides[0] {
            self.stop("ERROR: physical side changed");
            return;
        }
        let page = q[6] as usize;
        let words = self.trace.as_mut().unwrap();
        for i in 0..6 {
            words[page * 6 + i] = u32::from_le_bytes(r[8 + i * 4..12 + i * 4].try_into().unwrap());
        }
        if page < 131 {
            self.phase = Phase::Reading(0, q[6] + 1);
            return;
        }
        let raw = format!("Raw frozen words: {words:?}");
        let decoded = keymap_trace_text(words, self.sequence);
        if let Err(e) = self.record(&raw) { self.stop(&format!("ERROR log write: {e}")); return; }
        match decoded {
            Ok(text) => {
                if let Err(e) = self.record(&text) { self.stop(&format!("ERROR log write: {e}")); return; }
                self.stop("SUCCESS: stable KEYMAP single-frame timing saved; not FPS; physical integrity not certified");
            }
            Err(e) => self.stop(&format!("ERROR: {e}; raw pages retained, no accepted measurement")),
        }
    }
}

pub(super) fn keymap_trace_text(w: &[u32; 792], seq: u8) -> Result<String, String> {
    let require = |ok, msg: &str| if ok { Ok(()) } else { Err(msg.to_string()) };
    require(w[..4] == [0x19323644, 0x19, 792, seq as u32], "trace identity/version/length/nonce")?;
    require(w[7] == 0, "timeout/contamination/overflow/frame-count/abort flags")?;
    require(w[8] == 2 && w[9] == 2, "not actual KEYMAP throughout frame")?;
    let count = w[10] as usize;
    require((1..=96).contains(&count), "invalid part count")?;
    require(w[18].wrapping_sub(w[17]) == 1 && w[16] == w[15], "frame delta or packet contamination")?;
    require(w[20..24].iter().all(|n| *n == 0) && w[24 + count * 8..].iter().all(|n| *n == 0), "reserved/unused words")?;
    let since_arm = |t: u32| t.wrapping_sub(w[4]) as u64;
    let begin = since_arm(w[5]);
    let end = since_arm(w[6]);
    require(begin < end && end <= 10_000_000 && since_arm(w[19]) >= end && since_arm(w[19]) <= 10_000_000, "frame/publication timestamp bounds")?;
    let mut previous = begin;
    let mut sums = [0u64; 5]; // parts, viewport, pixdata, flush, gaps
    let mut pixels = 0u64;
    let mut text = format!("KEYMAP single completed frame; not FPS\nactual-scene start={} end={}\nframe_us={} parts={} core0_loops={} key_events={} ordinary_packets={}\n", w[8], w[9], end - begin, count, w[12].wrapping_sub(w[11]), w[14].wrapping_sub(w[13]), w[16].wrapping_sub(w[15]));
    for (i, p) in w[24..24 + count * 8].chunks_exact(8).enumerate() {
        let start = since_arm(p[0]);
        let stop = since_arm(p[1]);
        let x1 = p[2] & 65535; let y1 = p[2] >> 16;
        let x2 = p[3] & 65535; let y2 = p[3] >> 16;
        require(start >= previous && stop >= start && stop <= end, "nonmonotonic/out-of-frame part timestamps")?;
        require(x1 <= x2 && x2 < 240 && y1 <= y2 && y2 < 280, "out-of-panel coordinates")?;
        require(p[7] == ((if i == 0 { 1 } else { 0 }) | (if i + 1 == count { 2 } else { 0 })), "first/last flags")?;
        let dt = stop - start;
        let panel = p[4] as u64 + p[5] as u64 + p[6] as u64;
        require(panel <= dt, "nested panel durations exceed part wall time")?;
        let gap = start - previous;
        sums[0] += dt; sums[1] += p[4] as u64; sums[2] += p[5] as u64; sums[3] += p[6] as u64; sums[4] += gap;
        pixels += (x2 - x1 + 1) as u64 * (y2 - y1 + 1) as u64;
        text.push_str(&format!("part={i} rect={x1},{y1}..{x2},{y2} duration_us={dt} viewport_us={} pixdata_us={} flush_us={} non_panel_wall_us={} gap_us={gap}\n", p[4], p[5], p[6], dt - panel));
        previous = stop;
    }
    text.push_str(&format!("totals: pixel_bytes={} part_wall_us={} viewport_us={} pixdata_us={} flush_us={} interpart_gap_us={} frame_outside_parts_us={}\nNon-panel wall time is NOT pure CPU time; pixdata includes synchronous API wait, not DMA-only time. Coordinates describe actual dirty work, not necessarily a full-screen redraw.\n", pixels * 2, sums[0], sums[1], sums[2], sums[3], sums[4], end - begin - sums[0]));
    Ok(text)
}
