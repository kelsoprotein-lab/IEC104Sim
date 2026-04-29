use crate::data_point::{DataPoint, DataPointMap, DataPointValue};
use crate::log_collector::LogCollector;
use crate::log_entry::{Direction, FrameLabel, LogEntry};
use crate::types::AsduTypeId;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A control command response received from the slave.
#[derive(Debug, Clone)]
pub struct ControlResponse {
    pub ioa: u32,
    pub asdu_type: u8,
    pub cot: u8,
    pub positive: bool,
}

/// Result of a control command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResult {
    pub steps: Vec<ControlStep>,
    pub duration_ms: u64,
}

/// A single step in a control command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlStep {
    pub action: String,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// TLS Configuration
// ---------------------------------------------------------------------------

/// Strategy for choosing the TLS protocol version on the client side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsVersionPolicy {
    /// Negotiate automatically (min = TLS 1.2, no max cap).
    #[default]
    Auto,
    /// Pin to TLS 1.2 (min = max = TLS 1.2).
    Tls12Only,
    /// Pin to TLS 1.3 (min = max = TLS 1.3).
    Tls13Only,
}

/// TLS configuration for a master connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,
    /// Path to CA certificate file (PEM format) for server verification
    #[serde(default)]
    pub ca_file: String,
    /// Path to client certificate file (PEM format) for mutual TLS
    #[serde(default)]
    pub cert_file: String,
    /// Path to client private key file (PEM format)
    #[serde(default)]
    pub key_file: String,
    /// Path to client PKCS#12 bundle for mutual TLS (preferred on macOS)
    #[serde(default)]
    pub pkcs12_file: String,
    /// Password for the PKCS#12 bundle
    #[serde(default)]
    pub pkcs12_password: String,
    /// Accept invalid/self-signed certificates (for testing)
    #[serde(default)]
    pub accept_invalid_certs: bool,
    /// TLS version policy. Defaults to `Auto` (min=1.2, no max cap).
    #[serde(default)]
    pub version: TlsVersionPolicy,
}

// ---------------------------------------------------------------------------
// Stream Abstraction
// ---------------------------------------------------------------------------

/// A stream that can be either plain TCP or TLS-wrapped.
enum MasterStream {
    Plain(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
}

impl MasterStream {
    #[allow(dead_code)]
    fn try_clone(&self) -> std::io::Result<Self> {
        match self {
            MasterStream::Plain(s) => Ok(MasterStream::Plain(s.try_clone()?)),
            MasterStream::Tls(_) => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "TLS stream cannot be cloned",
                ))
            }
        }
    }
}

impl Read for MasterStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            MasterStream::Plain(s) => s.read(buf),
            MasterStream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for MasterStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            MasterStream::Plain(s) => s.write(buf),
            MasterStream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            MasterStream::Plain(s) => s.flush(),
            MasterStream::Tls(s) => s.flush(),
        }
    }
}

// Implement Read/Write for &MasterStream (needed for shared access via RwLock)
impl Read for &MasterStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            MasterStream::Plain(s) => (&*s).read(buf),
            MasterStream::Tls(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Cannot read from shared TLS ref; use mutable access",
            )),
        }
    }
}

impl Write for &MasterStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            MasterStream::Plain(s) => (&*s).write(buf),
            MasterStream::Tls(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Cannot write to shared TLS ref; use mutable access",
            )),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            MasterStream::Plain(s) => (&*s).flush(),
            MasterStream::Tls(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Cannot flush shared TLS ref",
            )),
        }
    }
}

// We need Send + Sync for Arc<RwLock<..>>
// native_tls::TlsStream<TcpStream> is Send but not Sync by default.
// Since we guard with RwLock and only access mutably, this is safe.
unsafe impl Sync for MasterStream {}

// ---------------------------------------------------------------------------
// Master State & Config
// ---------------------------------------------------------------------------

/// Running state of a master connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Configuration for a master connection.
///
/// Protocol parameters t0/t1/t2/t3/k/w follow IEC 60870-5-104 §5.2 defaults
/// (t0=30s, t1=15s, t2=10s, t3=20s, k=12, w=8). `default_qoi` / `default_qcc`
/// are applied when the caller doesn't override them on `send_interrogation`
/// / `send_counter_read`. `interrogate_period_s` and
/// `counter_interrogate_period_s` drive the optional auto-poll loops; 0
/// disables them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterConfig {
    pub target_address: String,
    pub port: u16,
    pub common_address: u16,
    /// Legacy: TCP connect timeout in ms. Kept for backward compat with
    /// older persisted configs; superseded by `t0` (in seconds) when both
    /// are present.
    pub timeout_ms: u64,
    /// TLS configuration (optional)
    #[serde(default)]
    pub tls: TlsConfig,
    /// t0: connection establishment timeout (seconds).
    #[serde(default = "default_t0")]
    pub t0: u32,
    /// t1: timeout waiting for ACK of sent I-frame or TESTFR_CON (seconds).
    #[serde(default = "default_t1")]
    pub t1: u32,
    /// t2: timeout for sending an S-frame ACK after receiving I-frames (seconds).
    /// Spec requires t2 < t1.
    #[serde(default = "default_t2")]
    pub t2: u32,
    /// t3: idle timeout before sending TESTFR_ACT (seconds).
    #[serde(default = "default_t3")]
    pub t3: u32,
    /// k: max number of unacknowledged outgoing I-frames.
    #[serde(default = "default_k")]
    pub k: u16,
    /// w: max number of received I-frames before forcing an S-frame ACK.
    /// Spec recommends w ≤ 2/3 · k.
    #[serde(default = "default_w")]
    pub w: u16,
    /// Default QOI (Qualifier of Interrogation) for general interrogation.
    /// 20 = global station interrogation.
    #[serde(default = "default_qoi_value")]
    pub default_qoi: u8,
    /// Default QCC (Qualifier of Counter Interrogation). 5 = total + no freeze.
    #[serde(default = "default_qcc_value")]
    pub default_qcc: u8,
    /// Period for auto general interrogation in seconds. 0 disables.
    #[serde(default)]
    pub interrogate_period_s: u32,
    /// Period for auto counter interrogation in seconds. 0 disables.
    #[serde(default)]
    pub counter_interrogate_period_s: u32,
}

fn default_t0() -> u32 { 30 }
fn default_t1() -> u32 { 15 }
fn default_t2() -> u32 { 10 }
fn default_t3() -> u32 { 20 }
fn default_k() -> u16 { 12 }
fn default_w() -> u16 { 8 }
fn default_qoi_value() -> u8 { 20 }
fn default_qcc_value() -> u8 { 5 }

impl Default for MasterConfig {
    fn default() -> Self {
        Self {
            target_address: "127.0.0.1".to_string(),
            port: 2404,
            common_address: 1,
            timeout_ms: 3000,
            tls: TlsConfig::default(),
            t0: default_t0(),
            t1: default_t1(),
            t2: default_t2(),
            t3: default_t3(),
            k: default_k(),
            w: default_w(),
            default_qoi: default_qoi_value(),
            default_qcc: default_qcc_value(),
            interrogate_period_s: 0,
            counter_interrogate_period_s: 0,
        }
    }
}

/// Received data storage.
///
/// One IEC 104 master TCP connection can talk to multiple stations
/// (each identified by its Common Address). The same IOA can exist on
/// different stations with completely different meaning, so we keep a
/// separate `DataPointMap` per CA — keying everything by IOA alone would
/// silently overwrite collisions. A connection-wide monotonic
/// `seq_counter` lets the frontend ask "what changed since X?" across
/// every CA in one query.
pub type SharedReceivedData = Arc<RwLock<MasterReceivedData>>;

#[derive(Debug, Default)]
pub struct MasterReceivedData {
    by_ca: std::collections::HashMap<u16, DataPointMap>,
    seq_counter: u64,
}

impl MasterReceivedData {
    pub fn new() -> Self { Self::default() }

    /// Insert/update a data point under the given CA, stamping it with
    /// the connection-wide seq.
    pub fn insert(&mut self, ca: u16, mut point: DataPoint) {
        self.seq_counter += 1;
        point.update_seq = self.seq_counter;
        let map = self.by_ca.entry(ca).or_default();
        // Bypass DataPointMap::insert (which would overwrite update_seq with
        // its own per-map counter); we want the connection-wide stamp.
        map.points.insert((point.ioa, point.asdu_type), point);
    }

    pub fn current_seq(&self) -> u64 { self.seq_counter }

    pub fn total_len(&self) -> usize {
        self.by_ca.values().map(|m| m.len()).sum()
    }

    /// Sorted list of CAs that have at least one point.
    pub fn cas(&self) -> Vec<u16> {
        let mut v: Vec<u16> = self.by_ca.keys().copied().collect();
        v.sort();
        v
    }

    /// Read access to a single CA's map (for backwards-compat tests).
    pub fn ca_map(&self, ca: u16) -> Option<&DataPointMap> { self.by_ca.get(&ca) }

    /// All points across every CA, sorted by (CA, IOA).
    pub fn all_sorted(&self) -> Vec<(u16, &DataPoint)> {
        let mut out = Vec::with_capacity(self.total_len());
        for (&ca, map) in &self.by_ca {
            for p in map.points.values() {
                out.push((ca, p));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.ioa.cmp(&b.1.ioa)));
        out
    }

    /// Points whose seq is strictly greater than the given watermark,
    /// across every CA, sorted by (CA, IOA).
    pub fn changed_since(&self, since: u64) -> Vec<(u16, &DataPoint)> {
        let mut out = Vec::new();
        for (&ca, map) in &self.by_ca {
            for p in map.points.values() {
                if p.update_seq > since {
                    out.push((ca, p));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.ioa.cmp(&b.1.ioa)));
        out
    }
}

/// Full IEC 60870-5-104 protocol state (SSN/RSN + timers + windowing).
///
/// All fields are accessed under a single `std::sync::Mutex` so the blocking
/// receiver thread and async senders can share without crossing async/sync
/// boundaries inside the lock.
///
/// 15-bit SSN/RSN wrap at 32768; helpers `seq_lt` / `seq_inc` handle that.
#[derive(Debug)]
pub struct ProtocolState {
    /// Send Sequence Number — next SSN to use when sending an I-frame.
    pub ssn: u16,
    /// Receive Sequence Number — next SSN expected from peer.
    pub rsn: u16,
    /// Outgoing I-frames awaiting ACK: (their SSN, t1 deadline).
    pub pending_acks: std::collections::VecDeque<(u16, std::time::Instant)>,
    /// Number of received I-frames since we last sent an S/I frame.
    pub unacked_received: u16,
    /// Last time we received any frame (resets t3 idle timer).
    pub last_rx: std::time::Instant,
    /// Deadline for sending an S-frame ACK (armed on first I-frame after
    /// our last ACK; cleared when we send any S/I frame). None = idle.
    pub pending_ack_deadline: Option<std::time::Instant>,
    /// If we've sent TESTFR_ACT, the deadline by which we expect TESTFR_CON.
    /// None = no test in flight.
    pub test_pending_deadline: Option<std::time::Instant>,
    /// Cached protocol parameters from MasterConfig.
    pub t1: std::time::Duration,
    pub t2: std::time::Duration,
    pub t3: std::time::Duration,
    pub k: u16,
    pub w: u16,
}

impl ProtocolState {
    pub fn new(t1: std::time::Duration, t2: std::time::Duration, t3: std::time::Duration, k: u16, w: u16) -> Self {
        Self {
            ssn: 0,
            rsn: 0,
            pending_acks: std::collections::VecDeque::new(),
            unacked_received: 0,
            last_rx: std::time::Instant::now(),
            pending_ack_deadline: None,
            test_pending_deadline: None,
            t1, t2, t3, k, w,
        }
    }
}

/// Strict-less-than for 15-bit sequence numbers (0..32768) with wraparound.
/// Returns true if `a` is "before" `b` in modulo-2^15 arithmetic.
fn seq_lt(a: u16, b: u16) -> bool {
    let diff = b.wrapping_sub(a) & 0x7FFF;
    diff != 0 && diff < 0x4000
}

/// Increment a 15-bit sequence number with wraparound.
fn seq_inc(n: u16) -> u16 {
    (n + 1) & 0x7FFF
}

/// An IEC 104 master connection.
pub struct MasterConnection {
    pub config: MasterConfig,
    pub received_data: SharedReceivedData,
    pub log_collector: Option<Arc<LogCollector>>,
    /// Current master state. `watch::Sender::borrow()` gives the latest value
    /// synchronously, and `subscribe()` yields a receiver for change notifications.
    state_tx: tokio::sync::watch::Sender<MasterState>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    stream: Arc<RwLock<Option<MasterStream>>>,
    /// Mutex-protected TLS stream for send operations (TLS streams cannot be cloned).
    tls_stream_mutex: Option<Arc<std::sync::Mutex<MasterStream>>>,
    receiver_handle: Option<tokio::task::JoinHandle<()>>,
    /// Periodic auto-poll task (GI / counter interrogation) for this connection.
    periodic_handle: Option<tokio::task::JoinHandle<()>>,
    /// Full protocol state: SSN/RSN, timers, windowing.
    protocol: Arc<std::sync::Mutex<ProtocolState>>,
    /// Wakes senders when peer ACKs free up a k slot.
    ack_notify: Arc<tokio::sync::Notify>,
    /// Serializes the (allocate-SSN, write-frame) critical section so two
    /// concurrent senders can't reorder I-frames on the wire.
    send_lock: Arc<tokio::sync::Mutex<()>>,
    /// Broadcast channel for control command responses (COT=7, COT=10).
    control_tx: tokio::sync::broadcast::Sender<ControlResponse>,
}

impl MasterConnection {
    pub fn new(config: MasterConfig) -> Self {
        let (control_tx, _) = tokio::sync::broadcast::channel(64);
        let (state_tx, _) = tokio::sync::watch::channel(MasterState::Disconnected);
        let protocol = ProtocolState::new(
            std::time::Duration::from_secs(config.t1 as u64),
            std::time::Duration::from_secs(config.t2 as u64),
            std::time::Duration::from_secs(config.t3 as u64),
            config.k,
            config.w,
        );
        Self {
            config,
            received_data: Arc::new(RwLock::new(MasterReceivedData::new())),
            log_collector: None,
            state_tx,
            shutdown_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stream: Arc::new(RwLock::new(None)),
            tls_stream_mutex: None,
            receiver_handle: None,
            periodic_handle: None,
            protocol: Arc::new(std::sync::Mutex::new(protocol)),
            ack_notify: Arc::new(tokio::sync::Notify::new()),
            send_lock: Arc::new(tokio::sync::Mutex::new(())),
            control_tx,
        }
    }

    /// Subscribe to state-change notifications. The receiver's initial
    /// `borrow()` yields the current state without blocking.
    pub fn subscribe_state(&self) -> tokio::sync::watch::Receiver<MasterState> {
        self.state_tx.subscribe()
    }

    pub fn with_log_collector(mut self, collector: Arc<LogCollector>) -> Self {
        self.log_collector = Some(collector);
        self
    }

    pub fn state(&self) -> MasterState {
        *self.state_tx.borrow()
    }

    /// Connect to the remote IEC 104 slave (with optional TLS).
    pub async fn connect(&mut self) -> Result<(), MasterError> {
        if self.state() == MasterState::Connected {
            return Err(MasterError::AlreadyConnected);
        }

        self.state_tx.send_replace(MasterState::Connecting);
        // Reset protocol state on new connection — pulls fresh values from
        // the (possibly mutated) MasterConfig so config tweaks between
        // connect cycles take effect.
        *self.protocol.lock().unwrap() = ProtocolState::new(
            std::time::Duration::from_secs(self.config.t1 as u64),
            std::time::Duration::from_secs(self.config.t2 as u64),
            std::time::Duration::from_secs(self.config.t3 as u64),
            self.config.k,
            self.config.w,
        );
        self.shutdown_flag.store(false, std::sync::atomic::Ordering::SeqCst);

        let addr = format!("{}:{}", self.config.target_address, self.config.port);
        // Prefer t0 (seconds, IEC 104 spec param) over the legacy timeout_ms
        // when t0 differs from the default. This keeps old configs working
        // while letting new configs use the spec field.
        let timeout = if self.config.t0 != default_t0() {
            std::time::Duration::from_secs(self.config.t0 as u64)
        } else {
            std::time::Duration::from_millis(self.config.timeout_ms)
        };

        let tcp_stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| MasterError::ConnectionError(format!("Invalid address: {}", e)))?,
            timeout,
        ).map_err(|e| {
            self.state_tx.send_replace(MasterState::Error);
            MasterError::ConnectionError(format!("Failed to connect to {}: {}", addr, e))
        })?;

        // Short read timeout so the receive loop ticks timers (t1/t2/t3)
        // promptly when no data is flowing.
        tcp_stream.set_read_timeout(Some(std::time::Duration::from_millis(100))).ok();
        tcp_stream.set_nodelay(true).ok();

        // Wrap with TLS if configured
        let master_stream = if self.config.tls.enabled {
            if let Some(ref lc) = self.log_collector {
                lc.try_add(LogEntry::new(
                    Direction::Tx,
                    FrameLabel::ConnectionEvent,
                    format!("TLS 握手中... {}", addr),
                ));
            }

            let tls_stream = self.create_tls_stream(tcp_stream)?;

            if let Some(ref lc) = self.log_collector {
                lc.try_add(LogEntry::new(
                    Direction::Rx,
                    FrameLabel::ConnectionEvent,
                    "TLS 握手成功".to_string(),
                ));
            }

            MasterStream::Tls(tls_stream)
        } else {
            MasterStream::Plain(tcp_stream)
        };

        // Send STARTDT ACT
        let startdt_act = [0x68, 0x04, 0x07, 0x00, 0x00, 0x00];
        // We need mutable access for TLS streams
        {
            match &master_stream {
                MasterStream::Plain(s) => {
                    (&*s).write_all(&startdt_act)
                        .map_err(|e| MasterError::ConnectionError(format!("Failed to send STARTDT: {}", e)))?;
                }
                MasterStream::Tls(_) => {
                    // For TLS, we'll write after storing the stream
                }
            }
        }

        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::with_raw_bytes(
                Direction::Tx,
                FrameLabel::UStartAct,
                format!("STARTDT ACT -> {}{}", addr, if self.config.tls.enabled { " (TLS)" } else { "" }),
                startdt_act.to_vec(),
            ));
        }

        // For TLS streams, we can't clone, so we use a different approach:
        // Store the stream in a mutex and share it between sender and receiver.
        let is_tls = self.config.tls.enabled;

        if is_tls {
            // For TLS: use Arc<Mutex> for shared mutable access
            let stream_mutex = Arc::new(std::sync::Mutex::new(master_stream));

            // Write STARTDT ACT through the mutex
            {
                let mut locked = stream_mutex.lock().unwrap();
                locked.write_all(&startdt_act)
                    .map_err(|e| MasterError::ConnectionError(format!("Failed to send STARTDT: {}", e)))?;
            }

            // Mark protocol state's last_rx now that the link is up — the
            // t3 idle timer counts from here.
            self.protocol.lock().unwrap().last_rx = std::time::Instant::now();
            self.state_tx.send_replace(MasterState::Connected);

            // Start receiver thread with mutex-based stream access
            let shutdown_flag = self.shutdown_flag.clone();
            let received_data = self.received_data.clone();
            let log_collector = self.log_collector.clone();
            let state_tx = self.state_tx.clone();
            let stream_for_receiver = stream_mutex.clone();
            let protocol = self.protocol.clone();
            let ack_notify = self.ack_notify.clone();
            let control_tx = self.control_tx.clone();

            let handle = tokio::task::spawn_blocking(move || {
                receive_loop_mutex(stream_for_receiver, received_data, log_collector, shutdown_flag, state_tx, protocol, ack_notify, control_tx);
            });

            self.receiver_handle = Some(handle);

            // Store the mutex for send/disconnect operations
            *self.stream.write().await = None;
            self.tls_stream_mutex = Some(stream_mutex);
        } else {
            // For plain TCP: clone the stream for the receiver thread
            let stream_clone = match &master_stream {
                MasterStream::Plain(s) => s.try_clone()
                    .map_err(|e| MasterError::ConnectionError(format!("Failed to clone stream: {}", e)))?,
                _ => unreachable!(),
            };

            *self.stream.write().await = Some(master_stream);
            // Mark t3 baseline now that the link is up.
            self.protocol.lock().unwrap().last_rx = std::time::Instant::now();
            self.state_tx.send_replace(MasterState::Connected);

            let shutdown_flag = self.shutdown_flag.clone();
            let received_data = self.received_data.clone();
            let log_collector = self.log_collector.clone();
            let state_tx = self.state_tx.clone();
            let protocol = self.protocol.clone();
            let ack_notify = self.ack_notify.clone();
            let control_tx = self.control_tx.clone();

            let handle = tokio::task::spawn_blocking(move || {
                receive_loop(stream_clone, received_data, log_collector, shutdown_flag, state_tx, protocol, ack_notify, control_tx);
            });

            self.receiver_handle = Some(handle);
        }

        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::new(
                Direction::Rx,
                FrameLabel::ConnectionEvent,
                format!("已连接到 {}{}", addr, if is_tls { " (TLS)" } else { "" }),
            ));
        }

        // Spawn the optional periodic GI / counter interrogation poller.
        self.spawn_periodic_poller();

        Ok(())
    }

    /// Background task: emits GI and/or counter interrogation at the
    /// configured periods. No-op if both periods are 0. Terminates when the
    /// connection state leaves Connected.
    fn spawn_periodic_poller(&mut self) {
        let gi_period = self.config.interrogate_period_s;
        let cn_period = self.config.counter_interrogate_period_s;
        if gi_period == 0 && cn_period == 0 {
            return;
        }

        let ca = self.config.common_address;
        let qoi = self.config.default_qoi;
        let qcc = self.config.default_qcc;
        let send_lock = self.send_lock.clone();
        let protocol = self.protocol.clone();
        let ack_notify = self.ack_notify.clone();
        let stream = self.stream.clone();
        let tls_mutex = self.tls_stream_mutex.clone();
        let log_collector = self.log_collector.clone();
        let state_tx = self.state_tx.clone();
        let mut state_rx = self.state_tx.subscribe();
        let shutdown_flag = self.shutdown_flag.clone();

        let handle = tokio::spawn(async move {
            let mut gi_interval = if gi_period > 0 {
                Some(tokio::time::interval(std::time::Duration::from_secs(gi_period as u64)))
            } else {
                None
            };
            let mut cn_interval = if cn_period > 0 {
                Some(tokio::time::interval(std::time::Duration::from_secs(cn_period as u64)))
            } else {
                None
            };
            // tokio::time::interval fires the first tick immediately; consume it
            // so the initial poll waits one full period after connect.
            if let Some(ref mut iv) = gi_interval { iv.tick().await; }
            if let Some(ref mut iv) = cn_interval { iv.tick().await; }

            loop {
                if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                if !matches!(*state_rx.borrow(), MasterState::Connected) {
                    if state_rx.changed().await.is_err() { break; }
                    continue;
                }
                tokio::select! {
                    _ = async {
                        if let Some(ref mut iv) = gi_interval { iv.tick().await; }
                        else { std::future::pending::<()>().await; }
                    } => {
                        let frame = build_gi_command(ca, qoi);
                        let _ = send_async_frame(
                            &send_lock, &protocol, &ack_notify, &stream, &tls_mutex,
                            &log_collector, &state_tx, frame, "周期性 GI",
                            FrameLabel::GeneralInterrogation, ca, None,
                        ).await;
                    }
                    _ = async {
                        if let Some(ref mut iv) = cn_interval { iv.tick().await; }
                        else { std::future::pending::<()>().await; }
                    } => {
                        let frame = build_counter_read_command(ca, qcc);
                        let _ = send_async_frame(
                            &send_lock, &protocol, &ack_notify, &stream, &tls_mutex,
                            &log_collector, &state_tx, frame, "周期性计数量召唤",
                            FrameLabel::CounterRead, ca, None,
                        ).await;
                    }
                    res = state_rx.changed() => {
                        if res.is_err() { break; }
                    }
                }
            }
        });
        self.periodic_handle = Some(handle);
    }

    /// Create a TLS stream from a TCP stream using the configured certificates.
    fn create_tls_stream(&self, tcp_stream: TcpStream) -> Result<native_tls::TlsStream<TcpStream>, MasterError> {
        let mut builder = native_tls::TlsConnector::builder();

        // Apply configured TLS version policy. For `Tls13Only` we pin both ends
        // explicitly — macOS Security Framework silently downgrades `max=Tlsv13`
        // to 1.2 if `min != Tlsv13` (see native-tls 0.2.18 imp/security_framework.rs).
        match self.config.tls.version {
            TlsVersionPolicy::Auto => {
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
            }
            TlsVersionPolicy::Tls12Only => {
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
                builder.max_protocol_version(Some(native_tls::Protocol::Tlsv12));
            }
            TlsVersionPolicy::Tls13Only => {
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv13));
                builder.max_protocol_version(Some(native_tls::Protocol::Tlsv13));
            }
        }

        // Load CA certificate if provided
        if !self.config.tls.ca_file.is_empty() {
            let ca_pem = std::fs::read(&self.config.tls.ca_file)
                .map_err(|e| MasterError::TlsError(format!("读取 CA 证书失败 {}: {}", self.config.tls.ca_file, e)))?;
            let ca_cert = native_tls::Certificate::from_pem(&ca_pem)
                .map_err(|e| MasterError::TlsError(format!("解析 CA 证书失败: {}", e)))?;
            builder.add_root_certificate(ca_cert);
        }

        // Load client identity for mutual TLS.
        // Prefer PKCS#12 (works on macOS Security framework with ECDSA keys);
        // fall back to PEM cert+key if no PKCS#12 is configured.
        if !self.config.tls.pkcs12_file.is_empty() {
            let p12_bytes = std::fs::read(&self.config.tls.pkcs12_file)
                .map_err(|e| MasterError::TlsError(format!("读取客户端 PKCS#12 失败 {}: {}", self.config.tls.pkcs12_file, e)))?;
            let identity = native_tls::Identity::from_pkcs12(&p12_bytes, &self.config.tls.pkcs12_password)
                .map_err(|e| MasterError::TlsError(format!("加载客户端身份 (PKCS#12) 失败: {}", e)))?;
            builder.identity(identity);
        } else if !self.config.tls.cert_file.is_empty() && !self.config.tls.key_file.is_empty() {
            let cert_pem = std::fs::read(&self.config.tls.cert_file)
                .map_err(|e| MasterError::TlsError(format!("读取客户端证书失败 {}: {}", self.config.tls.cert_file, e)))?;
            let key_pem = std::fs::read(&self.config.tls.key_file)
                .map_err(|e| MasterError::TlsError(format!("读取客户端密钥失败 {}: {}", self.config.tls.key_file, e)))?;

            let identity = native_tls::Identity::from_pkcs8(&cert_pem, &key_pem)
                .map_err(|e| MasterError::TlsError(format!("加载客户端身份失败: {}", e)))?;
            builder.identity(identity);
        }

        // Accept invalid certs (for self-signed testing)
        if self.config.tls.accept_invalid_certs {
            builder.danger_accept_invalid_certs(true);
            builder.danger_accept_invalid_hostnames(true);
        }

        let connector = builder.build()
            .map_err(|e| MasterError::TlsError(format!("创建 TLS 连接器失败: {}", e)))?;

        let domain = &self.config.target_address;
        let tls_stream = connector.connect(domain, tcp_stream)
            .map_err(|e| MasterError::TlsError(format!("TLS 握手失败: {}", e)))?;

        Ok(tls_stream)
    }

    /// Disconnect from the remote slave.
    pub async fn disconnect(&mut self) -> Result<(), MasterError> {
        if self.state() == MasterState::Disconnected {
            return Err(MasterError::NotConnected);
        }

        // Send STOPDT ACT (best effort)
        let stopdt = [0x68, 0x04, 0x13, 0x00, 0x00, 0x00];
        if let Some(ref mutex) = self.tls_stream_mutex {
            // TLS path
            if let Ok(mut stream) = mutex.lock() {
                let _ = stream.write_all(&stopdt);
            }
        } else {
            // Plain TCP path
            let stream_guard = self.stream.read().await;
            if let Some(ref stream) = *stream_guard {
                match stream {
                    MasterStream::Plain(s) => { let _ = (&*s).write_all(&stopdt); }
                    MasterStream::Tls(_) => {}
                }
            }
        }

        self.shutdown_flag.store(true, std::sync::atomic::Ordering::SeqCst);

        // Abort the periodic poller before joining the receiver — it shares
        // the stream lock and would otherwise hold up disconnect.
        if let Some(handle) = self.periodic_handle.take() {
            handle.abort();
        }

        if let Some(handle) = self.receiver_handle.take() {
            // Cap the wait so disconnect() can never hang the Tauri command
            // thread. The receiver loop polls shutdown_flag after each
            // (potentially blocking) read; if the read happens to be stuck
            // (rare TLS edge case where the read timeout doesn't propagate),
            // we abandon the join — the task will exit on the next read
            // timeout and drop its Arc, freeing the underlying socket.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
        }

        // Wake any sender that's still parked on the k-window so it can
        // notice the connection went down and bail out.
        self.ack_notify.notify_waiters();

        *self.stream.write().await = None;
        self.tls_stream_mutex = None;
        self.state_tx.send_replace(MasterState::Disconnected);

        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::new(
                Direction::Tx,
                FrameLabel::ConnectionEvent,
                "已断开连接".to_string(),
            ));
        }

        Ok(())
    }

    /// Send General Interrogation command. `qoi=None` falls back to the
    /// connection's `default_qoi` (typically 20 = global station).
    pub async fn send_interrogation(&self, ca: u16) -> Result<(), MasterError> {
        self.send_interrogation_with_qoi(ca, None).await
    }

    /// Same as `send_interrogation` but with an explicit QOI override.
    pub async fn send_interrogation_with_qoi(&self, ca: u16, qoi: Option<u8>) -> Result<(), MasterError> {
        let qoi = qoi.unwrap_or(self.config.default_qoi);
        let frame = build_gi_command(ca, qoi);
        self.send_frame(&frame, &format!("GI QOI={}", qoi), FrameLabel::GeneralInterrogation, ca).await
    }

    /// Send Clock Synchronization command.
    pub async fn send_clock_sync(&self, ca: u16) -> Result<(), MasterError> {
        let frame = build_clock_sync_command(ca);
        self.send_frame(&frame, "时钟同步", FrameLabel::ClockSync, ca).await
    }

    /// Send Counter Interrogation command. `qcc=None` falls back to the
    /// connection's `default_qcc` (typically 5 = total + no freeze).
    pub async fn send_counter_read(&self, ca: u16) -> Result<(), MasterError> {
        self.send_counter_read_with_qcc(ca, None).await
    }

    /// Same as `send_counter_read` but with an explicit QCC override.
    pub async fn send_counter_read_with_qcc(&self, ca: u16, qcc: Option<u8>) -> Result<(), MasterError> {
        let qcc = qcc.unwrap_or(self.config.default_qcc);
        let frame = build_counter_read_command(ca, qcc);
        self.send_frame(&frame, &format!("累计量召唤 QCC={}", qcc), FrameLabel::CounterRead, ca).await
    }

    /// Send Single Command.
    pub async fn send_single_command(&self, ioa: u32, value: bool, select: bool, ca: u16, qu: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_single_command(ca, ioa, value, select, qu, cot);
        let detail = format!("单点命令 IOA={} val={} sel={} QU={} COT={}", ioa, value, select, qu, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "single_command".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "qu": qu, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::SingleCommand, ca, Some(event)).await
    }

    /// Send Double Command.
    pub async fn send_double_command(&self, ioa: u32, value: u8, select: bool, ca: u16, qu: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_double_command(ca, ioa, value, select, qu, cot);
        let detail = format!("双点命令 IOA={} val={} sel={} QU={} COT={}", ioa, value, select, qu, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "double_command".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "qu": qu, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::DoubleCommand, ca, Some(event)).await
    }

    /// Send Step Command.
    pub async fn send_step_command(&self, ioa: u32, value: u8, select: bool, ca: u16, qu: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_step_command(ca, ioa, value, select, qu, cot);
        let detail = format!("步调节命令 IOA={} val={} sel={} QU={} COT={}", ioa, value, select, qu, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "step_command".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "qu": qu, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::StepCommand, ca, Some(event)).await
    }

    /// Send Set-point (normalized) command.
    pub async fn send_setpoint_normalized(&self, ioa: u32, value: f32, select: bool, ca: u16, ql: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_setpoint_normalized(ca, ioa, value, select, ql, cot);
        let detail = format!("归一化设定值 IOA={} val={:.4} sel={} QL={} COT={}", ioa, value, select, ql, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "setpoint_normalized".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "ql": ql, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::SetpointNormalized, ca, Some(event)).await
    }

    /// Send Set-point (scaled) command.
    pub async fn send_setpoint_scaled(&self, ioa: u32, value: i16, select: bool, ca: u16, ql: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_setpoint_scaled(ca, ioa, value, select, ql, cot);
        let detail = format!("标度化设定值 IOA={} val={} sel={} QL={} COT={}", ioa, value, select, ql, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "setpoint_scaled".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "ql": ql, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::SetpointScaled, ca, Some(event)).await
    }

    /// Send Set-point (short float) command.
    pub async fn send_setpoint_float(&self, ioa: u32, value: f32, select: bool, ca: u16, ql: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_setpoint_float_command(ca, ioa, value, select, ql, cot);
        let detail = format!("浮点设定值 IOA={} val={:.3} sel={} QL={} COT={}", ioa, value, select, ql, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "setpoint_float".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "ql": ql, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::SetpointFloat, ca, Some(event)).await
    }

    /// Send Bitstring 32-bit Command (C_BO_NA_1, type 51).
    pub async fn send_bitstring_command(&self, ioa: u32, value: u32, ca: u16, cot: u8) -> Result<(), MasterError> {
        let frame = build_bitstring_command(ca, ioa, value, cot);
        let detail = format!("位串命令 IOA={} val=0x{:08X} COT={}", ioa, value, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "bitstring_command".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "cot": cot }),
        };
        self.send_frame_with_event(&frame, &detail, FrameLabel::Bitstring, ca, Some(event)).await
    }

    /// Send a user-supplied APDU as-is. Reuses the I-frame SSN/RSN patching when applicable;
    /// U-frames pass through untouched, S-frames receive only RSN patching.
    pub async fn send_raw_apdu(&self, frame: Vec<u8>) -> Result<(), MasterError> {
        self.send_frame(&frame, "原始报文(用户注入)", FrameLabel::RawApdu, 0).await
    }

    /// Subscribe to control responses (for SbO flow).
    pub fn subscribe_control_responses(&self) -> tokio::sync::broadcast::Receiver<ControlResponse> {
        self.control_tx.subscribe()
    }

    /// Execute a control command with automatic Select-before-Execute.
    /// Sends Select, waits for confirmation, then sends Execute.
    pub async fn send_control_with_sbo(
        &self,
        select_frame: Vec<u8>,
        execute_frame: Vec<u8>,
        ioa: u32,
        detail_prefix: &str,
        label: FrameLabel,
        ca: u16,
    ) -> Result<ControlResult, MasterError> {
        self.send_control_with_sbo_event(select_frame, execute_frame, ioa, detail_prefix, label, ca, None).await
    }

    /// Same as `send_control_with_sbo` but attaches a structured detail event
    /// to the resulting log entries (frontend uses it for i18n rendering).
    /// `select` and `phase` ("select"/"execute") are added to the payload so
    /// the frontend can display the SbO step distinctly.
    pub async fn send_control_with_sbo_event(
        &self,
        select_frame: Vec<u8>,
        execute_frame: Vec<u8>,
        ioa: u32,
        detail_prefix: &str,
        label: FrameLabel,
        ca: u16,
        event: Option<crate::log_entry::DetailEvent>,
    ) -> Result<ControlResult, MasterError> {
        use std::time::Instant;
        let start = Instant::now();
        let mut steps = Vec::new();
        let mut rx = self.control_tx.subscribe();

        let phase_event = |phase: &str| -> Option<crate::log_entry::DetailEvent> {
            event.as_ref().map(|e| {
                let mut payload = e.payload.clone();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("phase".to_string(), serde_json::Value::String(phase.to_string()));
                }
                crate::log_entry::DetailEvent { kind: e.kind.clone(), payload }
            })
        };

        // Step 1: Send Select frame
        self.send_frame_with_event(&select_frame, &format!("{} (Select)", detail_prefix), label.clone(), ca, phase_event("select")).await?;
        steps.push(ControlStep {
            action: "select_sent".to_string(),
            timestamp: chrono::Utc::now().format("%H:%M:%S%.3f").to_string(),
        });

        // Step 2: Wait for Select confirmation (COT=7)
        let select_confirmed = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Self::wait_for_response(&mut rx, ioa, 7),
        ).await;

        match select_confirmed {
            Ok(Ok(resp)) => {
                if !resp.positive {
                    return Err(MasterError::SendError("选择被拒绝 (否定确认)".to_string()));
                }
                steps.push(ControlStep {
                    action: "select_confirmed".to_string(),
                    timestamp: chrono::Utc::now().format("%H:%M:%S%.3f").to_string(),
                });
            }
            Ok(Err(e)) => return Err(MasterError::SendError(format!("等待选择确认失败: {}", e))),
            Err(_) => return Err(MasterError::SendError("选择确认超时 (5s)".to_string())),
        }

        // Step 3: Send Execute frame
        self.send_frame_with_event(&execute_frame, &format!("{} (Execute)", detail_prefix), label, ca, phase_event("execute")).await?;
        steps.push(ControlStep {
            action: "execute_sent".to_string(),
            timestamp: chrono::Utc::now().format("%H:%M:%S%.3f").to_string(),
        });

        // Step 4: Wait for Execute confirmation (COT=7)
        let exec_confirmed = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Self::wait_for_response(&mut rx, ioa, 7),
        ).await;

        match exec_confirmed {
            Ok(Ok(resp)) => {
                if !resp.positive {
                    return Err(MasterError::SendError("执行被拒绝 (否定确认)".to_string()));
                }
                steps.push(ControlStep {
                    action: "execute_confirmed".to_string(),
                    timestamp: chrono::Utc::now().format("%H:%M:%S%.3f").to_string(),
                });
            }
            Ok(Err(e)) => return Err(MasterError::SendError(format!("等待执行确认失败: {}", e))),
            Err(_) => return Err(MasterError::SendError("执行确认超时 (5s)".to_string())),
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        Ok(ControlResult { steps, duration_ms })
    }

    /// Wait for a ControlResponse matching the given IOA and COT.
    async fn wait_for_response(
        rx: &mut tokio::sync::broadcast::Receiver<ControlResponse>,
        ioa: u32,
        expected_cot: u8,
    ) -> Result<ControlResponse, String> {
        loop {
            match rx.recv().await {
                Ok(resp) if resp.ioa == ioa && resp.cot == expected_cot => return Ok(resp),
                Ok(_) => continue, // Not our response, keep waiting
                Err(e) => return Err(format!("broadcast recv error: {}", e)),
            }
        }
    }

    async fn send_frame(&self, frame: &[u8], detail: &str, label: FrameLabel, ca: u16) -> Result<(), MasterError> {
        self.send_frame_with_event(frame, detail, label, ca, None).await
    }

    async fn send_frame_with_event(
        &self,
        frame: &[u8],
        detail: &str,
        label: FrameLabel,
        ca: u16,
        event: Option<crate::log_entry::DetailEvent>,
    ) -> Result<(), MasterError> {
        send_async_frame(
            &self.send_lock,
            &self.protocol,
            &self.ack_notify,
            &self.stream,
            &self.tls_stream_mutex,
            &self.log_collector,
            &self.state_tx,
            frame.to_vec(),
            detail,
            label,
            ca,
            event,
        ).await
    }
}

/// Free-function sender shared by `MasterConnection::send_frame_with_event`
/// and the periodic auto-poller. Handles k-window blocking, SSN allocation,
/// pending-ACK tracking for t1, and stream serialization.
#[allow(clippy::too_many_arguments)]
async fn send_async_frame(
    send_lock: &Arc<tokio::sync::Mutex<()>>,
    protocol: &Arc<std::sync::Mutex<ProtocolState>>,
    ack_notify: &Arc<tokio::sync::Notify>,
    stream: &Arc<RwLock<Option<MasterStream>>>,
    tls_mutex: &Option<Arc<std::sync::Mutex<MasterStream>>>,
    log_collector: &Option<Arc<LogCollector>>,
    state_tx: &tokio::sync::watch::Sender<MasterState>,
    mut frame: Vec<u8>,
    detail: &str,
    label: FrameLabel,
    ca: u16,
    event: Option<crate::log_entry::DetailEvent>,
) -> Result<(), MasterError> {
    if frame.len() < 6 {
        return Err(MasterError::SendError(format!("{}: 帧长度过短", detail)));
    }

    // Take the send-lock for the entire allocate-and-write so two concurrent
    // I-frame senders can't interleave SSN allocation with stream writes.
    let _send_guard = send_lock.lock().await;

    let ctrl1 = frame[2];
    let is_iframe = ctrl1 & 0x01 == 0;
    let is_sframe = ctrl1 & 0x03 == 0x01;

    if is_iframe {
        // Block until pending_acks.len() < k. Re-check on each Notify or
        // every ~100ms; bail out if the connection drops.
        loop {
            if !matches!(*state_tx.borrow(), MasterState::Connected) {
                return Err(MasterError::NotConnected);
            }
            let need_wait = {
                let s = protocol.lock().unwrap();
                s.pending_acks.len() >= s.k as usize
            };
            if !need_wait { break; }
            let notif = ack_notify.notified();
            tokio::pin!(notif);
            let _ = tokio::time::timeout(std::time::Duration::from_millis(200), notif).await;
        }

        let mut s = protocol.lock().unwrap();
        let ssn = s.ssn;
        let rsn = s.rsn;
        let deadline = std::time::Instant::now() + s.t1;
        s.pending_acks.push_back((ssn, deadline));
        s.ssn = seq_inc(s.ssn);
        // I-frame piggybacks our RSN — clears any pending S-frame ACK.
        s.unacked_received = 0;
        s.pending_ack_deadline = None;
        let ssn_bytes = (ssn << 1).to_le_bytes();
        let rsn_bytes = (rsn << 1).to_le_bytes();
        frame[2] = ssn_bytes[0];
        frame[3] = ssn_bytes[1];
        frame[4] = rsn_bytes[0];
        frame[5] = rsn_bytes[1];
    } else if is_sframe {
        let mut s = protocol.lock().unwrap();
        let rsn_bytes = (s.rsn << 1).to_le_bytes();
        frame[4] = rsn_bytes[0];
        frame[5] = rsn_bytes[1];
        s.unacked_received = 0;
        s.pending_ack_deadline = None;
    }
    // U-frame: leave control field untouched.

    if let Some(mutex) = tls_mutex {
        let mut stream_guard = mutex.lock()
            .map_err(|e| MasterError::SendError(format!("mutex lock failed: {}", e)))?;
        let write_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut written = 0;
        while written < frame.len() {
            match stream_guard.write(&frame[written..]) {
                Ok(0) => return Err(MasterError::SendError(format!("{}: write returned 0", detail))),
                Ok(n) => written += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= write_deadline {
                        return Err(MasterError::SendError(format!("{}: write timed out", detail)));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(e) => return Err(MasterError::SendError(format!("{}: {}", detail, e))),
            }
        }
    } else {
        let stream_guard = stream.read().await;
        let s = stream_guard.as_ref()
            .ok_or(MasterError::NotConnected)?;
        match s {
            MasterStream::Plain(s) => {
                (&*s).write_all(&frame)
                    .map_err(|e| MasterError::SendError(format!("{}: {}", detail, e)))?;
            }
            MasterStream::Tls(_) => return Err(MasterError::SendError(format!("{}: TLS stream missing mutex", detail))),
        }
    }

    if let Some(ref lc) = log_collector {
        let mut entry = LogEntry::with_raw_bytes(
            Direction::Tx,
            label,
            format!("{} CA={}", detail, ca),
            frame.to_vec(),
        );
        if let Some(ev) = event {
            entry = entry.with_detail_event(ev.kind, ev.payload);
        }
        lc.try_add(entry);
    }

    Ok(())
}

/// Trait abstraction over "raw write to the wire" so the receive loop can
/// be shared between plain-TCP (cloned `TcpStream`) and TLS (shared
/// `Arc<Mutex<MasterStream>>`) without duplicating the protocol logic.
trait RawWrite {
    fn write_raw(&mut self, frame: &[u8]) -> std::io::Result<()>;
}

impl RawWrite for TcpStream {
    fn write_raw(&mut self, frame: &[u8]) -> std::io::Result<()> {
        self.write_all(frame)
    }
}

struct TlsWriter<'a>(&'a Arc<std::sync::Mutex<MasterStream>>);

impl<'a> RawWrite for TlsWriter<'a> {
    fn write_raw(&mut self, frame: &[u8]) -> std::io::Result<()> {
        let mut locked = self
            .0
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "stream mutex poisoned"))?;
        locked.write_all(frame)
    }
}

/// Background receive loop for plain TCP connections.
fn receive_loop(
    mut stream: TcpStream,
    received_data: SharedReceivedData,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    state_tx: tokio::sync::watch::Sender<MasterState>,
    protocol: Arc<std::sync::Mutex<ProtocolState>>,
    ack_notify: Arc<tokio::sync::Notify>,
    control_tx: tokio::sync::broadcast::Sender<ControlResponse>,
) {
    let mut reassembly_buf = Vec::with_capacity(65536);
    let mut read_buf = [0u8; 8192];

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        // Tick timers every iteration regardless of read result so t1/t2/t3
        // fire even on a totally idle link.
        if !tick_timers(&protocol, &log_collector, &ack_notify, &mut stream, &state_tx, &shutdown_flag) {
            break;
        }

        match stream.read(&mut read_buf) {
            Ok(0) => {
                state_tx.send_replace(MasterState::Disconnected);
                if let Some(ref lc) = log_collector {
                    lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, "连接已关闭"));
                }
                break;
            }
            Ok(n) => {
                reassembly_buf.extend_from_slice(&read_buf[..n]);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut
                || e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                state_tx.send_replace(MasterState::Disconnected);
                if let Some(ref lc) = log_collector {
                    lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, format!("读取错误,连接断开: {}", e)));
                }
                break;
            }
        }

        while reassembly_buf.len() >= 2 {
            if reassembly_buf[0] != 0x68 {
                reassembly_buf.remove(0);
                continue;
            }
            let frame_len = reassembly_buf[1] as usize + 2;
            if reassembly_buf.len() < frame_len {
                break;
            }
            let frame_data: Vec<u8> = reassembly_buf.drain(..frame_len).collect();
            process_received_frame(&frame_data, &received_data, &log_collector, &mut stream, &protocol, &ack_notify, &control_tx);
        }
    }
}

/// Background receive loop for TLS connections using a shared Mutex.
///
/// TLS streams can't be split for concurrent read+write the way `TcpStream`
/// can, so we serialize access via `Arc<Mutex<MasterStream>>`. Holding the
/// lock across a blocking `read()` would block every send for as long as the
/// peer stays silent — and `native_tls` does not reliably propagate the
/// underlying TCP `set_read_timeout` (especially on macOS Security
/// Framework), so the lock could end up held for many seconds.
///
/// The fix: switch the underlying TCP socket to non-blocking after the TLS
/// handshake completes. `read()` then returns `WouldBlock` immediately when
/// no data is available, we release the lock, sleep briefly, and retry. This
/// caps the worst-case `send_frame` latency at roughly the sleep interval
/// (~5 ms) instead of seconds.
fn receive_loop_mutex(
    stream: Arc<std::sync::Mutex<MasterStream>>,
    received_data: SharedReceivedData,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    state_tx: tokio::sync::watch::Sender<MasterState>,
    protocol: Arc<std::sync::Mutex<ProtocolState>>,
    ack_notify: Arc<tokio::sync::Notify>,
    control_tx: tokio::sync::broadcast::Sender<ControlResponse>,
) {
    if let Ok(locked) = stream.lock() {
        if let MasterStream::Tls(tls) = &*locked {
            let _ = tls.get_ref().set_nonblocking(true);
        }
    }

    let mut reassembly_buf = Vec::with_capacity(65536);
    let mut read_buf = [0u8; 8192];

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        // Tick timers; uses TlsWriter so any t2/t3-driven send goes via
        // the same mutex as user sends.
        {
            let mut writer = TlsWriter(&stream);
            if !tick_timers(&protocol, &log_collector, &ack_notify, &mut writer, &state_tx, &shutdown_flag) {
                break;
            }
        }

        let read_result = {
            let mut locked = match stream.lock() {
                Ok(s) => s,
                Err(_) => {
                    state_tx.send_replace(MasterState::Disconnected);
                    break;
                }
            };
            locked.read(&mut read_buf)
        };

        match read_result {
            Ok(0) => {
                state_tx.send_replace(MasterState::Disconnected);
                if let Some(ref lc) = log_collector {
                    lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, "连接已关闭"));
                }
                break;
            }
            Ok(n) => {
                reassembly_buf.extend_from_slice(&read_buf[..n]);
                while reassembly_buf.len() >= 2 {
                    if reassembly_buf[0] != 0x68 {
                        reassembly_buf.remove(0);
                        continue;
                    }
                    let frame_len = reassembly_buf[1] as usize + 2;
                    if reassembly_buf.len() < frame_len {
                        break;
                    }
                    let frame_data: Vec<u8> = reassembly_buf.drain(..frame_len).collect();
                    let mut writer = TlsWriter(&stream);
                    process_received_frame(&frame_data, &received_data, &log_collector, &mut writer, &protocol, &ack_notify, &control_tx);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut => {
                // Release the mutex briefly so a waiting sender can run.
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                state_tx.send_replace(MasterState::Disconnected);
                if let Some(ref lc) = log_collector {
                    lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, format!("读取错误,连接断开: {}", e)));
                }
                break;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TickAction {
    SendSFrame(u16),
    SendTestFr,
    DropT1,
    Idle,
}

/// Run one tick of the t1/t2/t3 timer machinery. Returns false if the
/// connection must die.
///
/// **Liveness semantics (TESTFR-driven):** `pending_acks` is *not* used as a
/// drop trigger — many real-world IEC 104 slaves leave their N(R) stuck for
/// long periods after a GI cycle yet are still perfectly responsive. The
/// spec's strict per-I-frame t1 would tear those links down for no good
/// reason. Liveness is instead handled by the t3 + t1 + TESTFR loop:
///
/// 1. Peer silent for ≥ t3 → master sends TESTFR ACT and arms
///    `test_pending_deadline = now + t1`.
/// 2. Any frame received (TESTFR_CON or anything else) clears the deadline
///    via `process_received_frame` — link is alive.
/// 3. Deadline elapses with no peer activity at all → drop.
///
/// `pending_acks` is still tracked for k-window blocking on the send side,
/// but it does not by itself cause a disconnect.
fn tick_timers<W: RawWrite>(
    protocol: &Arc<std::sync::Mutex<ProtocolState>>,
    log_collector: &Option<Arc<LogCollector>>,
    _ack_notify: &Arc<tokio::sync::Notify>,
    writer: &mut W,
    state_tx: &tokio::sync::watch::Sender<MasterState>,
    shutdown_flag: &Arc<std::sync::atomic::AtomicBool>,
) -> bool {
    let now = std::time::Instant::now();
    let action = {
        let mut s = protocol.lock().unwrap();
        // Drop only when TESTFR ACT was sent and the peer didn't respond
        // with anything within t1.
        let testfr_dead = s
            .test_pending_deadline
            .map(|d| now >= d)
            .unwrap_or(false);
        if testfr_dead {
            TickAction::DropT1
        } else {
            decide_tick(&mut s, now)
        }
    };

    match action {
        TickAction::DropT1 => {
            shutdown_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            state_tx.send_replace(MasterState::Error);
            if let Some(ref lc) = log_collector {
                lc.try_add(LogEntry::new(
                    Direction::Rx,
                    FrameLabel::ConnectionEvent,
                    "t1 超时: TESTFR ACT 后对端在 t1 内仍无任何响应,链路视为已死,连接关闭",
                ));
            }
            false
        }
        TickAction::SendSFrame(rsn) => {
            let rsn_bytes = (rsn << 1).to_le_bytes();
            let s_frame = [0x68, 0x04, 0x01, 0x00, rsn_bytes[0], rsn_bytes[1]];
            let _ = writer.write_raw(&s_frame);
            if let Some(ref lc) = log_collector {
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Tx,
                    FrameLabel::SFrame,
                    format!("S 帧 (t2 触发的 ACK) RSN={}", rsn),
                    s_frame.to_vec(),
                ));
            }
            true
        }
        TickAction::SendTestFr => {
            let f = [0x68, 0x04, 0x43, 0x00, 0x00, 0x00];
            let _ = writer.write_raw(&f);
            if let Some(ref lc) = log_collector {
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Tx,
                    FrameLabel::UTestAct,
                    "TESTFR ACT (t3 触发心跳)",
                    f.to_vec(),
                ));
            }
            true
        }
        TickAction::Idle => true,
    }
}

/// Decide which timer fires next, with the protocol lock already held.
///
/// Drop on TESTFR timeout is handled in `tick_timers`; here we only emit
/// new outgoing frames (S-frame for delayed ACK, TESTFR ACT for idle).
fn decide_tick(s: &mut ProtocolState, now: std::time::Instant) -> TickAction {
    if let Some(deadline) = s.pending_ack_deadline {
        if now >= deadline {
            let rsn = s.rsn;
            s.unacked_received = 0;
            s.pending_ack_deadline = None;
            return TickAction::SendSFrame(rsn);
        }
    }
    if s.test_pending_deadline.is_none() && now.saturating_duration_since(s.last_rx) >= s.t3 {
        s.test_pending_deadline = Some(now + s.t1);
        return TickAction::SendTestFr;
    }
    TickAction::Idle
}

/// Process a single received IEC 104 frame, updating protocol state,
/// emitting S-frame ACKs when w is reached, and handling U-frames.
fn process_received_frame<W: RawWrite>(
    data: &[u8],
    received_data: &SharedReceivedData,
    log_collector: &Option<Arc<LogCollector>>,
    writer: &mut W,
    protocol: &Arc<std::sync::Mutex<ProtocolState>>,
    ack_notify: &Arc<tokio::sync::Notify>,
    control_tx: &tokio::sync::broadcast::Sender<ControlResponse>,
) {
    if data.len() < 6 { return; }
    let ctrl1 = data[2];
    let now = std::time::Instant::now();

    // Any received frame counts as link liveness: refresh the t3 idle
    // baseline and clear any in-flight TESTFR deadline. Even an unrelated
    // I-frame proves the peer is alive, so the watchdog should not fire.
    {
        let mut s = protocol.lock().unwrap();
        s.last_rx = now;
        s.test_pending_deadline = None;
    }

    // U-frame
    if ctrl1 & 0x03 == 0x03 {
        log_frame(data, log_collector);
        if ctrl1 == 0x43 {
            // TESTFR ACT → reply with TESTFR CON
            let response = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
            let _ = writer.write_raw(&response);
        } else if ctrl1 == 0x83 {
            // TESTFR CON — clear the in-flight TESTFR deadline.
            let mut s = protocol.lock().unwrap();
            s.test_pending_deadline = None;
        }
        // Other U-frames (STARTDT_CON 0x0B, STOPDT_CON 0x23) — nothing to track.
    }
    // S-frame
    else if ctrl1 & 0x01 == 0x01 {
        log_frame(data, log_collector);
        if data.len() >= 6 {
            let peer_rsn = u16::from_le_bytes([data[4], data[5]]) >> 1;
            free_acked_pending(protocol, peer_rsn, ack_notify);
        }
    }
    // I-frame
    else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
        // Parse peer's SSN (data[2..4] >> 1) and the piggybacked RSN
        // (data[4..6] >> 1) before consuming the ASDU.
        let peer_ssn = u16::from_le_bytes([data[2], data[3]]) >> 1;
        let peer_rsn = u16::from_le_bytes([data[4], data[5]]) >> 1;
        free_acked_pending(protocol, peer_rsn, ack_notify);

        // Update local rsn, increment unacked_received, and decide if w
        // forces an immediate S-frame ACK.
        let force_ack: Option<u16> = {
            let mut s = protocol.lock().unwrap();
            // V(R) <- N(S) + 1 per IEC 60870-5-104. If the peer's SSN
            // doesn't match V(R) we still advance — log for diagnostics
            // but don't hard-fail (some non-conformant slaves restart).
            s.rsn = seq_inc(peer_ssn);
            s.unacked_received = s.unacked_received.saturating_add(1);
            if s.unacked_received >= s.w {
                let rsn = s.rsn;
                s.unacked_received = 0;
                s.pending_ack_deadline = None;
                Some(rsn)
            } else {
                if s.pending_ack_deadline.is_none() {
                    s.pending_ack_deadline = Some(now + s.t2);
                }
                None
            }
        };

        parse_and_store_asdu(data, received_data, log_collector, control_tx);

        if let Some(rsn) = force_ack {
            let rsn_bytes = (rsn << 1).to_le_bytes();
            let s_frame = [0x68, 0x04, 0x01, 0x00, rsn_bytes[0], rsn_bytes[1]];
            let _ = writer.write_raw(&s_frame);
            if let Some(ref lc) = log_collector {
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Tx,
                    FrameLabel::SFrame,
                    format!("S 帧 (w 阈值触发) RSN={}", rsn),
                    s_frame.to_vec(),
                ));
            }
        }
    }
}

/// Pop pending I-frame entries acknowledged by `peer_rsn` (anything with
/// SSN < peer_rsn modulo 2^15). Notifies senders if any slot was freed.
fn free_acked_pending(
    protocol: &Arc<std::sync::Mutex<ProtocolState>>,
    peer_rsn: u16,
    ack_notify: &Arc<tokio::sync::Notify>,
) {
    let freed = {
        let mut s = protocol.lock().unwrap();
        let mut count = 0;
        while let Some(&(ssn, _)) = s.pending_acks.front() {
            if seq_lt(ssn, peer_rsn) {
                s.pending_acks.pop_front();
                count += 1;
            } else {
                break;
            }
        }
        count
    };
    if freed > 0 {
        ack_notify.notify_waiters();
    }
}

/// Log a received U-frame.
fn log_frame(data: &[u8], log_collector: &Option<Arc<LogCollector>>) {
    if let Some(ref lc) = log_collector {
        if let Ok(frame) = crate::frame::parse_apci(data) {
            let summary = crate::frame::format_frame_summary(&frame);
            lc.try_add(LogEntry::with_raw_bytes(
                Direction::Rx,
                FrameLabel::IFrame(summary.clone()),
                summary,
                data.to_vec(),
            ));
        }
    }
}

/// Get the data element size (excluding IOA) for a given ASDU type.
/// Returns (value_bytes, has_timestamp_7bytes).
fn asdu_element_size(asdu_type: u8) -> Option<(usize, bool)> {
    match asdu_type {
        1  => Some((1, false)),  // M_SP_NA_1: SIQ
        30 => Some((1, true)),   // M_SP_TB_1: SIQ + CP56Time2a
        3  => Some((1, false)),  // M_DP_NA_1: DIQ
        31 => Some((1, true)),   // M_DP_TB_1: DIQ + CP56Time2a
        5  => Some((2, false)),  // M_ST_NA_1: VTI(1) + QDS(1)
        32 => Some((2, true)),   // M_ST_TB_1: VTI(1) + QDS(1) + CP56Time2a
        7  => Some((5, false)),  // M_BO_NA_1: BSI(4) + QDS(1)
        33 => Some((5, true)),   // M_BO_TB_1: BSI(4) + QDS(1) + CP56Time2a
        9  => Some((3, false)),  // M_ME_NA_1: NVA(2) + QDS(1)
        34 => Some((3, true)),   // M_ME_TD_1: NVA(2) + QDS(1) + CP56Time2a
        11 => Some((3, false)),  // M_ME_NB_1: SVA(2) + QDS(1)
        35 => Some((3, true)),   // M_ME_TE_1: SVA(2) + QDS(1) + CP56Time2a
        13 => Some((5, false)),  // M_ME_NC_1: float(4) + QDS(1)
        36 => Some((5, true)),   // M_ME_TF_1: float(4) + QDS(1) + CP56Time2a
        15 => Some((5, false)),  // M_IT_NA_1: BCR(4+1)
        37 => Some((5, true)),   // M_IT_TB_1: BCR(4+1) + CP56Time2a
        100 => Some((1, false)), // C_IC_NA_1: QOI
        101 => Some((1, false)), // C_CI_NA_1: QCC
        103 => Some((7, false)), // C_CS_NA_1: CP56Time2a
        _ => None,
    }
}

/// Parse ASDU from an I-frame and store data points.
fn parse_and_store_asdu(
    data: &[u8],
    received_data: &SharedReceivedData,
    log_collector: &Option<Arc<LogCollector>>,
    control_tx: &tokio::sync::broadcast::Sender<ControlResponse>,
) {
    if data.len() < 12 { return; }

    let asdu_type = data[6];
    let vsq = data[7];
    let sq = vsq & 0x80 != 0;
    let num_objects = (vsq & 0x7F) as usize;
    let cause = data[8];

    // Handle control command responses (Type 45-50): COT=7 (confirm) or COT=10 (terminate)
    if matches!(asdu_type, 45..=50) {
        let cot = cause & 0x3F;
        let positive = cause & 0x40 == 0; // bit 6 = 0 means positive
        if data.len() >= 15 {
            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
            let type_name = AsduTypeId::from_u8(asdu_type)
                .map(|t| t.name().to_string())
                .unwrap_or_else(|| format!("Type{}", asdu_type));
            let ca = u16::from_le_bytes([data[10], data[11]]);

            if let Some(ref lc) = log_collector {
                let pn_str = if positive { "肯定" } else { "否定" };
                let cot_str = match cot {
                    7 => "激活确认",
                    10 => "激活终止",
                    _ => "未知",
                };
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Rx,
                    FrameLabel::IFrame(type_name),
                    format!("控制响应 IOA={} COT={}({}) P/N={} CA={}", ioa, cot, cot_str, pn_str, ca),
                    data.to_vec(),
                ));
            }

            let _ = control_tx.send(ControlResponse {
                ioa,
                asdu_type,
                cot,
                positive,
            });
        }
        return;
    }
    let ca = u16::from_le_bytes([data[10], data[11]]);

    if let Some(ref lc) = log_collector {
        let type_name = AsduTypeId::from_u8(asdu_type)
            .map(|t| t.name().to_string())
            .unwrap_or_else(|| format!("Type{}", asdu_type));
        lc.try_add(LogEntry::with_raw_bytes(
            Direction::Rx,
            FrameLabel::IFrame(type_name.clone()),
            format!("{} CA={} n={} COT={} SQ={}", type_name, ca, num_objects, cause, sq as u8),
            data.to_vec(),
        ));
    }

    let elem_size = match asdu_element_size(asdu_type) {
        Some((base, has_ts)) => base + if has_ts { 7 } else { 0 },
        None => return, // Unknown type, skip
    };

    let mut obj_offset = 12;
    let mut base_ioa: u32 = 0;
    let asdu_id = AsduTypeId::from_u8(asdu_type).unwrap_or(AsduTypeId::MSpNa1);
    let mut points = Vec::with_capacity(num_objects);

    for i in 0..num_objects {
        if sq {
            if i == 0 {
                if obj_offset + 3 > data.len() { break; }
                base_ioa = u32::from_le_bytes([data[obj_offset], data[obj_offset + 1], data[obj_offset + 2], 0]);
                obj_offset += 3;
            }
        } else {
            if obj_offset + 3 > data.len() { break; }
            base_ioa = u32::from_le_bytes([data[obj_offset], data[obj_offset + 1], data[obj_offset + 2], 0]);
            obj_offset += 3;
        }

        let ioa = if sq { base_ioa + i as u32 } else { base_ioa };

        if obj_offset + elem_size > data.len() { break; }

        let value = match asdu_type {
            1 | 30 => {
                let siq = data[obj_offset];
                DataPointValue::SinglePoint { value: siq & 0x01 != 0 }
            }
            3 | 31 => {
                let diq = data[obj_offset];
                DataPointValue::DoublePoint { value: diq & 0x03 }
            }
            5 | 32 => {
                let vti = data[obj_offset];
                let value = (vti & 0x7F) as i8;
                let transient = vti & 0x80 != 0;
                DataPointValue::StepPosition { value, transient }
            }
            7 | 33 => {
                let bsi = u32::from_le_bytes([
                    data[obj_offset], data[obj_offset + 1],
                    data[obj_offset + 2], data[obj_offset + 3],
                ]);
                DataPointValue::Bitstring { value: bsi }
            }
            9 | 34 => {
                let nva = i16::from_le_bytes([data[obj_offset], data[obj_offset + 1]]);
                DataPointValue::Normalized { value: nva as f32 / 32767.0 }
            }
            11 | 35 => {
                let sva = i16::from_le_bytes([data[obj_offset], data[obj_offset + 1]]);
                DataPointValue::Scaled { value: sva }
            }
            13 | 36 => {
                let fval = f32::from_le_bytes([
                    data[obj_offset], data[obj_offset + 1],
                    data[obj_offset + 2], data[obj_offset + 3],
                ]);
                DataPointValue::ShortFloat { value: fval }
            }
            15 | 37 => {
                let counter = i32::from_le_bytes([
                    data[obj_offset], data[obj_offset + 1],
                    data[obj_offset + 2], data[obj_offset + 3],
                ]);
                let bcr = data[obj_offset + 4];
                let carry = bcr & 0x20 != 0;
                let sequence = bcr & 0x1F;
                DataPointValue::IntegratedTotal { value: counter, carry, sequence }
            }
            _ => break,
        };

        obj_offset += elem_size;
        points.push(DataPoint::with_value(ioa, asdu_id, value));
    }

    // Batch insert — single lock acquisition for all points in this frame.
    // Each point is stored under the CA we extracted from the ASDU header
    // above, so two stations sharing IOAs over the same TCP connection no
    // longer overwrite each other.
    if !points.is_empty() {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let rd = received_data.clone();
            handle.block_on(async {
                let mut maps = rd.write().await;
                for point in points {
                    maps.insert(ca, point);
                }
            });
        }
    }
}

// --- Command frame builders ---

fn build_gi_command(ca: u16, qoi: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        100, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        0x00, 0x00, 0x00,
        qoi,
    ]
}

fn build_clock_sync_command(ca: u16) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let now = chrono::Utc::now();
    let ms = (now.timestamp_subsec_millis() as u16) + ((now.format("%S").to_string().parse::<u16>().unwrap_or(0)) * 1000);
    let min = now.format("%M").to_string().parse::<u8>().unwrap_or(0);
    let hour = now.format("%H").to_string().parse::<u8>().unwrap_or(0);
    let day = now.format("%d").to_string().parse::<u8>().unwrap_or(1);
    let month = now.format("%m").to_string().parse::<u8>().unwrap_or(1);
    let year = (now.format("%Y").to_string().parse::<u16>().unwrap_or(2024) % 100) as u8;
    let ms_bytes = ms.to_le_bytes();

    vec![
        0x68, 0x14,
        0x00, 0x00, 0x00, 0x00,
        103, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        0x00, 0x00, 0x00,
        ms_bytes[0], ms_bytes[1],
        min, hour, day, month, year,
    ]
}

fn build_counter_read_command(ca: u16, qcc: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        101, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        0x00, 0x00, 0x00,
        qcc,
    ]
}

fn build_single_command(ca: u16, ioa: u32, value: bool, select: bool, qu: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut sco = (qu & 0x1F) << 2;
    if value { sco |= 0x01; }
    if select { sco |= 0x80; }
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        45, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        sco,
    ]
}

fn build_double_command(ca: u16, ioa: u32, value: u8, select: bool, qu: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut dco = (value & 0x03) | ((qu & 0x1F) << 2);
    if select { dco |= 0x80; }
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        46, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        dco,
    ]
}

fn build_step_command(ca: u16, ioa: u32, value: u8, select: bool, qu: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut rco = (value & 0x03) | ((qu & 0x1F) << 2);
    if select { rco |= 0x80; }
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        47, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        rco,
    ]
}

fn build_setpoint_normalized(ca: u16, ioa: u32, value: f32, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva = (value * 32767.0) as i16;
    let nva_bytes = nva.to_le_bytes();
    let mut qos = ql & 0x7F;
    if select { qos |= 0x80; }
    vec![
        0x68, 0x10,
        0x00, 0x00, 0x00, 0x00,
        48, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        nva_bytes[0], nva_bytes[1],
        qos,
    ]
}

fn build_setpoint_scaled(ca: u16, ioa: u32, value: i16, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let sva_bytes = value.to_le_bytes();
    let mut qos = ql & 0x7F;
    if select { qos |= 0x80; }
    vec![
        0x68, 0x10,
        0x00, 0x00, 0x00, 0x00,
        49, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        sva_bytes[0], sva_bytes[1],
        qos,
    ]
}

fn build_setpoint_float_command(ca: u16, ioa: u32, value: f32, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let val_bytes = value.to_le_bytes();
    let mut qos = ql & 0x7F;
    if select { qos |= 0x80; }
    vec![
        0x68, 0x12,
        0x00, 0x00, 0x00, 0x00,
        50, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        val_bytes[0], val_bytes[1], val_bytes[2], val_bytes[3],
        qos,
    ]
}

fn build_bitstring_command(ca: u16, ioa: u32, value: u32, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let val_bytes = value.to_le_bytes();
    vec![
        0x68, 0x11,
        0x00, 0x00, 0x00, 0x00,
        51, 0x01, cot, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        val_bytes[0], val_bytes[1], val_bytes[2], val_bytes[3],
    ]
}

#[derive(Debug, thiserror::Error)]
pub enum MasterError {
    #[error("already connected")]
    AlreadyConnected,
    #[error("not connected")]
    NotConnected,
    #[error("connection error: {0}")]
    ConnectionError(String),
    #[error("TLS error: {0}")]
    TlsError(String),
    #[error("send error: {0}")]
    SendError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_config_default() {
        let config = MasterConfig::default();
        assert_eq!(config.port, 2404);
        assert_eq!(config.common_address, 1);
        assert!(!config.tls.enabled);
    }

    #[test]
    fn test_tls_config_default() {
        let tls = TlsConfig::default();
        assert!(!tls.enabled);
        assert!(tls.ca_file.is_empty());
        assert!(tls.cert_file.is_empty());
        assert!(tls.key_file.is_empty());
        assert!(!tls.accept_invalid_certs);
    }

    #[test]
    fn test_tls_version_policy_default_is_auto() {
        let v = TlsVersionPolicy::default();
        assert_eq!(v, TlsVersionPolicy::Auto);
    }

    #[test]
    fn test_tls_config_default_version_is_auto() {
        let cfg = TlsConfig::default();
        assert_eq!(cfg.version, TlsVersionPolicy::Auto);
    }

    #[test]
    fn test_tls_version_policy_serde_snake_case() {
        let auto = serde_json::to_string(&TlsVersionPolicy::Auto).unwrap();
        let v12  = serde_json::to_string(&TlsVersionPolicy::Tls12Only).unwrap();
        let v13  = serde_json::to_string(&TlsVersionPolicy::Tls13Only).unwrap();
        assert_eq!(auto, "\"auto\"");
        assert_eq!(v12, "\"tls12_only\"");
        assert_eq!(v13, "\"tls13_only\"");
    }

    #[test]
    fn test_tls_config_deserialize_without_version_field_defaults_to_auto() {
        let json = r#"{"enabled": true}"#;
        let cfg: TlsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.version, TlsVersionPolicy::Auto);
        assert!(cfg.enabled);
    }

    #[test]
    fn test_build_gi_command() {
        let frame = build_gi_command(1, 0x14);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 100);
        assert_eq!(frame[8], 6);
        assert_eq!(frame[15], 0x14);
    }

    #[test]
    fn test_build_gi_command_custom_qoi() {
        // QOI=21 (group 1 interrogation)
        let frame = build_gi_command(2, 21);
        assert_eq!(frame[15], 21);
        assert_eq!(frame[10], 2u16.to_le_bytes()[0]);
    }

    #[test]
    fn test_build_counter_read_command_custom_qcc() {
        // QCC=0x45 = total + freeze (group 1)
        let frame = build_counter_read_command(1, 0x45);
        assert_eq!(frame[6], 101);
        assert_eq!(frame[15], 0x45);
    }

    #[test]
    fn test_seq_lt_wraparound() {
        // Within window
        assert!(seq_lt(0, 1));
        assert!(seq_lt(100, 200));
        assert!(!seq_lt(1, 0));
        // Equal is not strictly less than
        assert!(!seq_lt(5, 5));
        // Wrap: 32767 -> 0 should be "less than" because diff=1
        assert!(seq_lt(32767, 0));
        assert!(seq_lt(32766, 1));
    }

    #[test]
    fn test_seq_inc_wraparound() {
        assert_eq!(seq_inc(0), 1);
        assert_eq!(seq_inc(32767), 0);
    }

    #[test]
    fn test_master_config_protocol_defaults() {
        let cfg = MasterConfig::default();
        assert_eq!(cfg.t0, 30);
        assert_eq!(cfg.t1, 15);
        assert_eq!(cfg.t2, 10);
        assert_eq!(cfg.t3, 20);
        assert_eq!(cfg.k, 12);
        assert_eq!(cfg.w, 8);
        assert_eq!(cfg.default_qoi, 20);
        assert_eq!(cfg.default_qcc, 5);
        assert_eq!(cfg.interrogate_period_s, 0);
        assert_eq!(cfg.counter_interrogate_period_s, 0);
    }

    #[test]
    fn test_master_config_serde_back_compat() {
        // Old configs without the new protocol fields must still deserialize.
        let json = r#"{"target_address":"127.0.0.1","port":2404,"common_address":1,"timeout_ms":3000}"#;
        let cfg: MasterConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.t1, 15); // pulled from default_t1
        assert_eq!(cfg.k, 12);
        assert_eq!(cfg.default_qoi, 20);
    }

    #[test]
    fn test_build_single_command() {
        let frame = build_single_command(1, 100, true, false, 0, 6);
        assert_eq!(frame[6], 45);
        assert_eq!(frame[12], 100);
        assert_eq!(frame[15], 0x01);
        assert_eq!(frame[8], 6); // COT
    }

    #[test]
    fn test_build_single_command_with_qu_short_pulse() {
        // QU=1 (short pulse), value=ON, SbO select bit on
        let frame = build_single_command(1, 100, true, true, 1, 6);
        // SCO = SE(0x80) | (QU=1 << 2) | SCS(1) = 0x85
        assert_eq!(frame[15], 0x85);
    }

    #[test]
    fn test_build_step_command() {
        // Lower, Execute, QU=0
        let frame = build_step_command(1, 600, 1, false, 0, 6);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 47);
        assert_eq!(frame[12], 600u32.to_le_bytes()[0]);
        assert_eq!(frame[15], 0x01); // RCO = lower

        // Higher, Select, QU=0
        let frame = build_step_command(1, 600, 2, true, 0, 6);
        assert_eq!(frame[15], 0x82); // RCO = higher + select bit
    }

    #[test]
    fn test_build_setpoint_normalized() {
        let frame = build_setpoint_normalized(1, 400, 0.5, false, 0, 6);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 48);
        let nva = i16::from_le_bytes([frame[15], frame[16]]);
        assert_eq!(nva, (0.5_f32 * 32767.0) as i16);
        assert_eq!(frame[17], 0x00); // QOS = no select, QL=0

        // With select
        let frame = build_setpoint_normalized(1, 400, -0.5, true, 0, 6);
        assert_eq!(frame[17], 0x80); // QOS = select bit
    }

    #[test]
    fn test_build_setpoint_normalized_with_ql() {
        // QL=2, no SbO
        let frame = build_setpoint_normalized(1, 400, 0.0, false, 2, 6);
        assert_eq!(frame[17], 0x02);
    }

    #[test]
    fn test_build_setpoint_scaled() {
        let frame = build_setpoint_scaled(1, 500, 1024, false, 0, 6);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 49);
        let sva = i16::from_le_bytes([frame[15], frame[16]]);
        assert_eq!(sva, 1024);
        assert_eq!(frame[17], 0x00);
    }

    #[test]
    fn test_build_setpoint_float_with_select() {
        let frame = build_setpoint_float_command(1, 300, 25.5, true, 0, 6);
        assert_eq!(frame[6], 50);
        let val = f32::from_le_bytes([frame[15], frame[16], frame[17], frame[18]]);
        assert!((val - 25.5).abs() < 0.001);
        assert_eq!(frame[19], 0x80);

        let frame = build_setpoint_float_command(1, 300, 25.5, false, 0, 6);
        assert_eq!(frame[19], 0x00);
    }

    #[test]
    fn test_build_bitstring_command() {
        let frame = build_bitstring_command(1, 700, 0xDEADBEEF, 6);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[1], 0x11);
        assert_eq!(frame[6], 51);
        assert_eq!(frame[8], 6); // COT
        // BSI 4 bytes LE at frame[15..19]
        assert_eq!(frame[15], 0xEF);
        assert_eq!(frame[16], 0xBE);
        assert_eq!(frame[17], 0xAD);
        assert_eq!(frame[18], 0xDE);
    }

    #[test]
    fn test_build_command_cot_override() {
        // COT=8 (deactivation)
        let frame = build_single_command(1, 100, true, false, 0, 8);
        assert_eq!(frame[8], 8);
    }

    #[test]
    fn test_asdu_type_step_command() {
        assert_eq!(AsduTypeId::from_u8(47), Some(AsduTypeId::CRcNa1));
        assert_eq!(AsduTypeId::CRcNa1.name(), "C_RC_NA_1");
        assert_eq!(AsduTypeId::CRcNa1.description(), "步调节命令");
        assert_eq!(AsduTypeId::CRcNa1.category(), crate::types::DataCategory::StepPosition);
    }

    #[test]
    fn test_frame_label_step_command() {
        assert_eq!(FrameLabel::StepCommand.name(), "C_RC");
    }
}
