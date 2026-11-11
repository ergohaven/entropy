//! Wire-v3 diagnostics on the existing exclusive HID helper. Each transaction
//! consumes BOTH the explicit reply and VIA's unchanged request echo before the
//! IPC owner can accept any ordinary request (including uncorrelated Vial GET).
use super::*;
use std::time::Instant;

fn trace_request(q: &[u8]) -> bool {
    q.len() == MSG_LEN && q[..2] == [0xd7, 0x62] && matches!(q[2], 0x18 | 0x19)
        && matches!(q[3], 1 | 2) && q[4] == 0 && q[5] != 0
        && q[6] < 132 && (q[3] != 1 || q[6] == 0)
        && q[7..].iter().all(|b| *b == 0)
}

impl HidDevice {
    /// Local-master 0x18 response only. No global Vial correlation relaxation.
    pub(crate) fn sm62_trace_reply(q: &[u8], r: &[u8; MSG_LEN]) -> bool {
        trace_request(q) && r[..4] == q[..4] && r[4] == q[5] && r[5] == q[6]
            && matches!(r[6], 0x11 | 0x12)
            && (if q[3] == 1 { matches!(r[7], 1 | 6 | 8 | 0xfe) }
                else { matches!(r[7], 2 | 3 | 4 | 8 | 0xfe) })
            && (q[3] == 2 && r[7] == 2 || r[8..].iter().all(|b| *b == 0))
    }
}

fn valid_request(q: &[u8]) -> bool {
    trace_request(q) || legacy_request(q)
}
fn legacy_request(q: &[u8]) -> bool {
    q.len() == MSG_LEN
        && q[..3] == [0xd7, 0x62, 3]
        && matches!(q[3], 1 | 2)
        && q[4] <= 1
        && q[5] != 0
        && q[6] < 8
        && (q[3] != 1 || q[6] == 0)
        && q[7..].iter().all(|b| *b == 0)
}

impl HidDevice {
    /// Exact rejection emitted by dualcore_trace.c, not a wire-v3 measurement.
    /// Its role is the responding USB half, NOT the requested target. The older
    /// RPC-only rejection has no nonce/command/page and must remain rejected.
    pub(crate) fn sm62_fps_unsupported(q: &[u8], r: &[u8; MSG_LEN]) -> bool {
        valid_request(q)
            && r[..3] == [0xd7, 0x62, 0x18]
            && r[3] == q[3]
            && r[4] == q[5]
            && r[5] == q[6]
            && matches!(r[6], 0x11 | 0x12 | 0x21 | 0x22)
            && r[7] == 0xfe
            && r[8..].iter().all(|b| *b == 0)
    }
}

pub(super) fn collect_pair(
    q: &[u8],
    deadline: Instant,
    mut read: impl FnMut(Instant) -> Result<[u8; MSG_LEN]>,
) -> Result<[u8; MSG_LEN]> {
    anyhow::ensure!(valid_request(q), "Invalid SM62 FPS request");
    let mut reply = None;
    let mut echo = false;
    // The current firmware emits exactly one explicit reply and one VIA echo.
    // Do NOT return on the first valid reply: the tail would poison the next GET.
    for _ in 0..2 {
        anyhow::ensure!(
            Instant::now() < deadline,
            "SM62 FPS shared deadline expired"
        );
        let r = read(deadline)?;
        anyhow::ensure!(
            Instant::now() < deadline,
            "SM62 FPS shared deadline expired"
        );
        if r.as_slice() == q {
            anyhow::ensure!(!echo, "SM62 FPS duplicate request echo");
            echo = true;
        } else {
            anyhow::ensure!(
                reply.is_none()
                    && (response_matches_command(q, &r) || HidDevice::sm62_fps_unsupported(q, &r)
                        || HidDevice::sm62_trace_reply(q, &r)),
                "SM62 FPS invalid reply: {r:02X?}"
            );
            reply = Some(r);
        }
    }
    anyhow::ensure!(echo, "SM62 FPS missing VIA echo");
    reply.context("SM62 FPS missing explicit reply")
}

pub(super) fn exchange_local(
    device: &hidapi::HidDevice,
    transport: HidTransport,
    framing: HidWriteFraming,
    q: &[u8],
) -> Result<[u8; MSG_LEN]> {
    anyhow::ensure!(
        !transport.is_bluetooth() && valid_request(q),
        "SM62 FPS requires USB wire-v3 request"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut frame = [0; MSG_LEN + 1];
    frame[1..].copy_from_slice(q);
    let frame = local_hid_write_frame(&mut frame, framing);
    let written = device
        .write(frame)
        .context("SM62 FPS write failed (never retried)")?;
    anyhow::ensure!(
        written == frame.len(),
        "SM62 FPS short write (never retried)"
    );
    collect_pair(q, deadline, |deadline| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        anyhow::ensure!(!remaining.is_zero(), "SM62 FPS timeout");
        let mut buf = [0; MSG_LEN + 1];
        let n = device
            .read_timeout(&mut buf, remaining.as_millis().max(1) as i32)
            .context("SM62 FPS read failed")?;
        anyhow::ensure!(
            n != 0,
            "SM62 FPS timeout waiting for complete reply/echo pair"
        );
        decode_hid_response(&buf, n, framing)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keymap19_pair_is_strict_bounded_and_missing_echo_never_releases_success() {
        let mut q=[0;32];q[..8].copy_from_slice(&[0xd7,0x62,0x19,1,0,23,0,0]);
        let mut good=[0;32];good[..8].copy_from_slice(&[0xd7,0x62,0x19,1,23,0,0x12,1]);
        for pair in [[good,q],[q,good]] {
            let mut packets=std::collections::VecDeque::from(pair);
            assert_eq!(collect_pair(&q,Instant::now()+Duration::from_secs(1),|_|Ok(packets.pop_front().unwrap())).unwrap(),good);
        }
        for i in 0..32 {
            let mut bad=good;bad[i]^=0x80;
            assert!(!HidDevice::sm62_trace_reply(&q,&bad),"byte {i}");
            let mut packets=std::collections::VecDeque::from([bad,q]);
            assert!(collect_pair(&q,Instant::now()+Duration::from_secs(1),|_|Ok(packets.pop_front().unwrap())).is_err(),"byte {i}");
        }
        for pair in [[good,good],[q,q]] {
            let mut packets=std::collections::VecDeque::from(pair);
            assert!(collect_pair(&q,Instant::now()+Duration::from_secs(1),|_|Ok(packets.pop_front().unwrap())).is_err());
        }
        let mut calls=0;
        assert!(collect_pair(&q,Instant::now()+Duration::from_secs(1),|_|{calls+=1;if calls==1{Ok(good)}else{bail!("missing echo")}}).is_err());
        assert_eq!(calls,2);
    }
    #[test]
    #[ignore = "requires generated production firmware fixture; qualification must run explicitly with --ignored"]
    fn keymap_production_c_pairs_preserve_every_reply_and_drain_via_echo() {
        let path = std::path::PathBuf::from(std::env::var("AGENT_BUILD_CACHE").unwrap()).join("dualcore-trace-evidence/keymap-c-pairs.bin");
        let fixture=std::fs::read(path).expect("run qualify_firmware.py first");
        assert_eq!(fixture.len(),133*96);
        let mut words=Vec::new();
        for (i,chunk) in fixture.chunks_exact(96).enumerate() {
            let q: [u8;32]=chunk[..32].try_into().unwrap();
            assert_eq!(q[..3],[0xd7,0x62,0x19]);
            assert_eq!(q[5],23);
            let mut queue=std::collections::VecDeque::from([
                <[u8;32]>::try_from(&chunk[32..64]).unwrap(),
                <[u8;32]>::try_from(&chunk[64..96]).unwrap(), [7;32]
            ]);
            let r=collect_pair(&q,Instant::now()+Duration::from_secs(1),|_|Ok(queue.pop_front().unwrap())).unwrap();
            assert_eq!(queue.pop_front(),Some([7;32])); assert!(queue.is_empty());
            assert_eq!(r[7],if i==0{1}else{2});
            if i>0 { words.extend(r[8..].chunks_exact(4).map(|v|u32::from_le_bytes(v.try_into().unwrap()))); }
        }
        assert_eq!(words.len(),792); assert_eq!(words[7],0); assert_eq!(words[8..11],[2,2,10]);
        assert_eq!(words[18]-words[17],1);
        assert_eq!(words[24], words[5] + 1, "production C separate moving NOW() calls");
        assert_eq!(words[24..104].chunks_exact(8).map(|p|p[5]).sum::<u32>(),4010);
    }
    #[test]
    fn dualcore_trace_18_pair_drains_before_ordinary_get() {
        let mut q = [0; 32];
        q[..8].copy_from_slice(&[0xd7, 0x62, 0x18, 1, 0, 27, 0, 0]);
        let mut r = [0; 32];
        r[..8].copy_from_slice(&[0xd7, 0x62, 0x18, 1, 27, 0, 0x12, 1]);
        for pair in [[r, q], [q, r]] {
            let mut queue = std::collections::VecDeque::from(pair);
            queue.push_back([7; 32]);
            assert_eq!(collect_pair(&q, Instant::now() + Duration::from_secs(1), |_| {
                Ok(queue.pop_front().unwrap())
            }).unwrap(), r);
            assert_eq!(queue.pop_front(), Some([7;32]));
            assert!(queue.is_empty());
        }
    }
    fn q() -> [u8; 32] {
        let mut q = [0; 32];
        q[..8].copy_from_slice(&[0xd7, 0x62, 3, 1, 1, 23, 0, 0]);
        q
    }
    fn r() -> [u8; 32] {
        let mut r = [0; 32];
        r[..8].copy_from_slice(&[0xd7, 0x62, 3, 1, 23, 0, 0x21, 1]);
        r
    }
    #[test]
    fn dualcore_unsupported_pair_is_correlated_drained_and_never_a_measurement() {
        let mut unsupported = [0; 32];
        unsupported[..8].copy_from_slice(&[0xd7, 0x62, 0x18, 1, 23, 0, 0x12, 0xfe]);
        assert!(!response_matches_command(&q(), &unsupported));
        for pair in [[unsupported, q()], [q(), unsupported]] {
            let ordinary = [7; 32];
            let mut queue = std::collections::VecDeque::from(pair);
            queue.push_back(ordinary);
            let result = collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| {
                Ok(queue.pop_front().unwrap())
            });
            assert_eq!(result.unwrap(), unsupported);
            assert_eq!(queue.pop_front(), Some(ordinary));
            assert!(queue.is_empty());
        }
    }
    #[test]
    fn dualcore_unsupported_rejects_wrong_nonce_garbage_and_incomplete_pairs() {
        let mut unsupported = [0; 32];
        unsupported[..8].copy_from_slice(&[0xd7, 0x62, 0x18, 1, 23, 0, 0x12, 0xfe]);
        // Every field, including reserved payload, remains significant.
        for i in 0..32 {
            let mut bad = unsupported;
            bad[i] ^= 0x80;
            assert!(!HidDevice::sm62_fps_unsupported(&q(), &bad), "byte {i}");
            let mut packets = std::collections::VecDeque::from([bad, q()]);
            assert!(collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| {
                Ok(packets.pop_front().unwrap())
            }).is_err(), "byte {i}");
        }
        // Uncorrelated sm62_diag_rpc guard is NOT safe to preserve the owner.
        let mut rpc = [0; 32];
        rpc[..3].copy_from_slice(&[0xd7, 0x62, 0x18]);
        rpc[7] = 0xfe;
        assert!(!HidDevice::sm62_fps_unsupported(&q(), &rpc));
        for pair in [[unsupported, unsupported], [unsupported, [0; 32]], [q(), q()]] {
            let mut packets = std::collections::VecDeque::from(pair);
            assert!(collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| {
                Ok(packets.pop_front().unwrap())
            }).is_err());
        }
        let mut reads = 0;
        assert!(collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| {
            reads += 1;
            if reads == 1 { Ok(unsupported) } else { bail!("missing echo") }
        }).is_err());
        assert_eq!(reads, 2);
        // GET rejection is correlated to its exact page and nonce as well.
        let mut get = q(); get[3] = 2; get[6] = 7;
        unsupported[3] = 2; unsupported[5] = 7;
        assert!(HidDevice::sm62_fps_unsupported(&get, &unsupported));
        get[5] ^= 1;
        assert!(!HidDevice::sm62_fps_unsupported(&get, &unsupported));
    }
    #[test]
    fn complete_pair_consumed_before_next_uncorrelated_get() {
        for packets in [[r(), q()], [q(), r()]] {
            let mut queue = std::collections::VecDeque::from(packets);
            let ordinary = [7; 32];
            queue.push_back(ordinary);
            assert_eq!(
                collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| Ok(queue
                    .pop_front()
                    .unwrap()))
                .unwrap(),
                r()
            );
            assert_eq!(queue.pop_front(), Some(ordinary));
            assert!(queue.is_empty());
        }
    }
    #[test]
    fn rejects_duplicate_missing_echo_wrong_sequence_side_role_and_status() {
        for i in [2, 3, 4, 5, 6, 7] {
            let mut bad = r();
            bad[i] ^= 0x70;
            let mut packets = std::collections::VecDeque::from([bad, q()]);
            assert!(
                collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| Ok(
                    packets.pop_front().unwrap()
                ))
                .is_err()
            );
        }
        for pair in [[r(), r()], [q(), q()]] {
            let mut packets = std::collections::VecDeque::from(pair);
            assert!(
                collect_pair(&q(), Instant::now() + Duration::from_secs(1), |_| Ok(
                    packets.pop_front().unwrap()
                ))
                .is_err()
            );
        }
    }
    #[test]
    fn one_deadline_bounds_all_reads_and_never_rearms() {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut calls = 0;
        assert!(collect_pair(&q(), deadline, |d| {
            assert_eq!(d, deadline);
            calls += 1;
            if calls == 1 {
                Ok(r())
            } else {
                bail!("timeout waiting for echo")
            }
        })
        .is_err());
        assert_eq!(calls, 2);
        assert!(
            collect_pair(&q(), Instant::now() - Duration::from_secs(1), |_| panic!(
                "expired read"
            ))
            .is_err()
        );
    }
}
