//! End-to-end checks for the IEC 60870-5-104 link-layer state machine
//! added to `MasterConnection`: t1 (I-frame ACK timeout), t2 (delayed
//! S-frame ACK), t3 (idle TESTFR_ACT), w (window-driven S-frame ACK), and
//! k (window-driven sender blocking). The peer in each test is a hand-rolled
//! TCP server so we can drive precisely the byte sequences the protocol
//! state machine is supposed to react to.

use iec104sim_core::master::{MasterConfig, MasterConnection};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tokio::time::sleep;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// After t3 seconds with no traffic, the master must send TESTFR_ACT
/// (U-frame with control=0x43). We drive t3 down to 1s for a fast test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_sends_testfr_act_after_t3_idle() {
    let port = free_port();
    let saw_testfr = Arc::new(AtomicUsize::new(0));
    let saw_testfr_clone = saw_testfr.clone();
    let (close_tx, close_rx) = mpsc::channel::<()>();

    thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(100))).ok();

        let mut buf = [0u8; 256];
        let mut pending = Vec::<u8>::new();
        loop {
            if close_rx.try_recv().is_ok() {
                break;
            }
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => pending.extend_from_slice(&buf[..n]),
                Err(_) => continue,
            }
            // Slurp out complete frames; we only care whether 0x43 (TESTFR ACT) shows up.
            while pending.len() >= 2 {
                if pending[0] != 0x68 {
                    pending.remove(0);
                    continue;
                }
                let len = pending[1] as usize + 2;
                if pending.len() < len {
                    break;
                }
                let frame: Vec<u8> = pending.drain(..len).collect();
                if frame.len() >= 3 && frame[2] == 0x43 {
                    saw_testfr_clone.fetch_add(1, Ordering::SeqCst);
                    // Reply with TESTFR_CON so the master clears its in-flight flag.
                    let _ = stream.write_all(&[0x68, 0x04, 0x83, 0x00, 0x00, 0x00]);
                }
            }
        }
    });

    sleep(Duration::from_millis(80)).await;

    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        // Compress the timers so the test runs in a few seconds.
        t1: 5,
        t2: 2,
        t3: 1,
        ..Default::default()
    });
    master.connect().await.expect("connect");

    // Wait long enough for at least one t3 expiry (1s) plus jitter.
    sleep(Duration::from_millis(2500)).await;

    assert!(
        saw_testfr.load(Ordering::SeqCst) >= 1,
        "expected at least one TESTFR_ACT after t3={}s idle",
        1
    );

    let _ = close_tx.send(());
    let _ = master.disconnect().await;
}

/// Real-world IEC 104 slaves often send I-frames whose N(R) doesn't advance
/// even after they've successfully consumed the master's request. With strict
/// per-frame t1 enforcement that would tear the link down at t1; the master
/// uses peer-silence semantics instead and must keep the link alive as long
/// as the slave keeps sending *anything*.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_keeps_link_alive_when_peer_sends_frames_without_advancing_rsn() {
    let port = free_port();
    let (close_tx, close_rx) = mpsc::channel::<()>();

    // Slave: accept, then every 200 ms send an I-frame whose N(R) is stuck
    // at 0 (never acks the master). The wire-level activity should be
    // enough to keep the watchdog happy.
    thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(50))).ok();
        let mut buf = [0u8; 256];
        let mut ssn: u16 = 0;
        let start = Instant::now();
        loop {
            if close_rx.try_recv().is_ok() {
                break;
            }
            let _ = stream.read(&mut buf);
            if start.elapsed() >= Duration::from_secs(3) {
                break;
            }
            // Forge a small I-frame: type=1 (M_SP_NA_1), one element. SSN
            // increments, RSN stays at 0 (the bug we want to tolerate).
            let ssn_bytes = (ssn << 1).to_le_bytes();
            let frame = [
                0x68, 0x0E,
                ssn_bytes[0], ssn_bytes[1], 0x00, 0x00, // SSN, RSN=0
                0x01, 0x01, 0x03, 0x00,                  // type=1, vsq=1, COT=3
                0x01, 0x00,                              // CA=1
                0x01, 0x00, 0x00,                        // IOA=1
                0x00,                                    // SIQ off
            ];
            let _ = stream.write_all(&frame);
            ssn = (ssn + 1) & 0x7FFF;
            thread::sleep(Duration::from_millis(200));
        }
    });

    sleep(Duration::from_millis(80)).await;

    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        t1: 1,  // tight t1 — strict spec would drop after 1s
        t2: 1,
        t3: 30,
        ..Default::default()
    });
    master.connect().await.expect("connect");
    master
        .send_interrogation(1)
        .await
        .expect("send_interrogation");

    // Wait noticeably longer than t1; if the watchdog were strict the link
    // would already be dead.
    sleep(Duration::from_millis(2500)).await;

    assert!(
        matches!(
            *master.subscribe_state().borrow(),
            iec104sim_core::master::MasterState::Connected
        ),
        "master should stay Connected when the peer is sending frames, even with stale N(R)"
    );

    let _ = close_tx.send(());
    let _ = master.disconnect().await;
}

/// When the peer goes completely silent (no frames whatsoever, including
/// no TESTFR_CON), the master must eventually close the connection. The
/// drop happens at roughly t3 + t1 of total silence: t3 fires TESTFR ACT,
/// then t1 elapses without any reply.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_drops_connection_when_peer_goes_completely_silent() {
    let port = free_port();
    let (close_tx, close_rx) = mpsc::channel::<()>();

    // Slave: accept, read whatever arrives, and never reply.
    thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(200))).ok();
        let mut buf = [0u8; 256];
        loop {
            if close_rx.try_recv().is_ok() {
                break;
            }
            let _ = stream.read(&mut buf);
        }
    });

    sleep(Duration::from_millis(80)).await;

    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        // Compress the watchdog: TESTFR ACT fires at t3=1s, then t1=1s
        // later (so ~2s total) the master drops on no peer activity.
        t1: 1,
        t2: 1,
        t3: 1,
        ..Default::default()
    });
    let mut state_rx = master.subscribe_state();
    master.connect().await.expect("connect");

    master
        .send_interrogation(1)
        .await
        .expect("send_interrogation");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut hit_error = false;
    while Instant::now() < deadline {
        if matches!(
            *state_rx.borrow(),
            iec104sim_core::master::MasterState::Error
                | iec104sim_core::master::MasterState::Disconnected
        ) {
            hit_error = true;
            break;
        }
        let _ = tokio::time::timeout(Duration::from_millis(150), state_rx.changed()).await;
    }
    assert!(hit_error, "master should have closed after t3+t1 of total silence");

    let _ = close_tx.send(());
    let _ = master.disconnect().await;
}
