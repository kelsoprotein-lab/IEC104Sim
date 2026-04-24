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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterConfig {
    pub target_address: String,
    pub port: u16,
    pub common_address: u16,
    pub timeout_ms: u64,
    /// TLS configuration (optional)
    #[serde(default)]
    pub tls: TlsConfig,
}

impl Default for MasterConfig {
    fn default() -> Self {
        Self {
            target_address: "127.0.0.1".to_string(),
            port: 2404,
            common_address: 1,
            timeout_ms: 3000,
            tls: TlsConfig::default(),
        }
    }
}

/// Received data storage.
pub type SharedReceivedData = Arc<RwLock<DataPointMap>>;

/// Shared sequence number counters for IEC 104 protocol.
#[derive(Debug)]
pub struct SeqNumbers {
    /// Send Sequence Number (incremented for each I-frame sent)
    pub ssn: u16,
    /// Receive Sequence Number (incremented for each I-frame received)
    pub rsn: u16,
}

impl SeqNumbers {
    pub fn new() -> Self {
        Self { ssn: 0, rsn: 0 }
    }
}

impl Default for SeqNumbers {
    fn default() -> Self {
        Self::new()
    }
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
    /// Shared sequence numbers for SSN/RSN tracking.
    seq: Arc<std::sync::Mutex<SeqNumbers>>,
    /// Broadcast channel for control command responses (COT=7, COT=10).
    control_tx: tokio::sync::broadcast::Sender<ControlResponse>,
}

impl MasterConnection {
    pub fn new(config: MasterConfig) -> Self {
        let (control_tx, _) = tokio::sync::broadcast::channel(64);
        let (state_tx, _) = tokio::sync::watch::channel(MasterState::Disconnected);
        Self {
            config,
            received_data: Arc::new(RwLock::new(DataPointMap::new())),
            log_collector: None,
            state_tx,
            shutdown_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stream: Arc::new(RwLock::new(None)),
            tls_stream_mutex: None,
            receiver_handle: None,
            seq: Arc::new(std::sync::Mutex::new(SeqNumbers::new())),
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
        // Reset sequence numbers on new connection
        *self.seq.lock().unwrap() = SeqNumbers::new();

        let addr = format!("{}:{}", self.config.target_address, self.config.port);
        let timeout = std::time::Duration::from_millis(self.config.timeout_ms);

        let tcp_stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| MasterError::ConnectionError(format!("Invalid address: {}", e)))?,
            timeout,
        ).map_err(|e| {
            self.state_tx.send_replace(MasterState::Error);
            MasterError::ConnectionError(format!("Failed to connect to {}: {}", addr, e))
        })?;

        tcp_stream.set_read_timeout(Some(std::time::Duration::from_millis(500))).ok();
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

            self.state_tx.send_replace(MasterState::Connected);

            // Start receiver thread with mutex-based stream access
            self.shutdown_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            let shutdown_flag = self.shutdown_flag.clone();
            let received_data = self.received_data.clone();
            let log_collector = self.log_collector.clone();
            let state_tx = self.state_tx.clone();
            let stream_for_receiver = stream_mutex.clone();
            let seq = self.seq.clone();
            let control_tx = self.control_tx.clone();

            let handle = tokio::task::spawn_blocking(move || {
                receive_loop_mutex(stream_for_receiver, received_data, log_collector, shutdown_flag, state_tx, seq, control_tx);
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
            self.state_tx.send_replace(MasterState::Connected);

            self.shutdown_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            let shutdown_flag = self.shutdown_flag.clone();
            let received_data = self.received_data.clone();
            let log_collector = self.log_collector.clone();
            let state_tx = self.state_tx.clone();
            let seq = self.seq.clone();
            let control_tx = self.control_tx.clone();

            let handle = tokio::task::spawn_blocking(move || {
                receive_loop(stream_clone, received_data, log_collector, shutdown_flag, state_tx, seq, control_tx);
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

        Ok(())
    }

    /// Create a TLS stream from a TCP stream using the configured certificates.
    fn create_tls_stream(&self, tcp_stream: TcpStream) -> Result<native_tls::TlsStream<TcpStream>, MasterError> {
        let mut builder = native_tls::TlsConnector::builder();

        // Set minimum TLS version to 1.2 (IEC 62351 requirement)
        builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));

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

        if let Some(handle) = self.receiver_handle.take() {
            let _ = handle.await;
        }

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

    /// Send General Interrogation command.
    pub async fn send_interrogation(&self, ca: u16) -> Result<(), MasterError> {
        let frame = build_gi_command(ca);
        self.send_frame(&frame, "GI", FrameLabel::GeneralInterrogation, ca).await
    }

    /// Send Clock Synchronization command.
    pub async fn send_clock_sync(&self, ca: u16) -> Result<(), MasterError> {
        let frame = build_clock_sync_command(ca);
        self.send_frame(&frame, "时钟同步", FrameLabel::ClockSync, ca).await
    }

    /// Send Counter Interrogation command.
    pub async fn send_counter_read(&self, ca: u16) -> Result<(), MasterError> {
        let frame = build_counter_read_command(ca);
        self.send_frame(&frame, "累计量召唤", FrameLabel::CounterRead, ca).await
    }

    /// Send Single Command.
    pub async fn send_single_command(&self, ioa: u32, value: bool, select: bool, ca: u16) -> Result<(), MasterError> {
        let frame = build_single_command(ca, ioa, value, select);
        let detail = format!("单点命令 IOA={} val={} sel={}", ioa, value, select);
        self.send_frame(&frame, &detail, FrameLabel::SingleCommand, ca).await
    }

    /// Send Double Command.
    pub async fn send_double_command(&self, ioa: u32, value: u8, select: bool, ca: u16) -> Result<(), MasterError> {
        let frame = build_double_command(ca, ioa, value, select);
        let detail = format!("双点命令 IOA={} val={} sel={}", ioa, value, select);
        self.send_frame(&frame, &detail, FrameLabel::DoubleCommand, ca).await
    }

    /// Send Step Command.
    pub async fn send_step_command(&self, ioa: u32, value: u8, select: bool, ca: u16) -> Result<(), MasterError> {
        let frame = build_step_command(ca, ioa, value, select);
        let detail = format!("步调节命令 IOA={} val={} sel={}", ioa, value, select);
        self.send_frame(&frame, &detail, FrameLabel::StepCommand, ca).await
    }

    /// Send Set-point (normalized) command.
    pub async fn send_setpoint_normalized(&self, ioa: u32, value: f32, select: bool, ca: u16) -> Result<(), MasterError> {
        let frame = build_setpoint_normalized(ca, ioa, value, select);
        let detail = format!("归一化设定值 IOA={} val={:.4} sel={}", ioa, value, select);
        self.send_frame(&frame, &detail, FrameLabel::SetpointNormalized, ca).await
    }

    /// Send Set-point (scaled) command.
    pub async fn send_setpoint_scaled(&self, ioa: u32, value: i16, select: bool, ca: u16) -> Result<(), MasterError> {
        let frame = build_setpoint_scaled(ca, ioa, value, select);
        let detail = format!("标度化设定值 IOA={} val={} sel={}", ioa, value, select);
        self.send_frame(&frame, &detail, FrameLabel::SetpointScaled, ca).await
    }

    /// Send Set-point (short float) command.
    pub async fn send_setpoint_float(&self, ioa: u32, value: f32, select: bool, ca: u16) -> Result<(), MasterError> {
        let frame = build_setpoint_float_command(ca, ioa, value, select);
        let detail = format!("浮点设定值 IOA={} val={:.3} sel={}", ioa, value, select);
        self.send_frame(&frame, &detail, FrameLabel::SetpointFloat, ca).await
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
        use std::time::Instant;
        let start = Instant::now();
        let mut steps = Vec::new();
        let mut rx = self.control_tx.subscribe();

        // Step 1: Send Select frame
        self.send_frame(&select_frame, &format!("{} (Select)", detail_prefix), label.clone(), ca).await?;
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
        self.send_frame(&execute_frame, &format!("{} (Execute)", detail_prefix), label, ca).await?;
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
        // Patch the frame with current SSN/RSN
        let mut frame = frame.to_vec();
        {
            let mut seq = self.seq.lock().unwrap();
            let ssn_bytes = (seq.ssn << 1).to_le_bytes();
            let rsn_bytes = (seq.rsn << 1).to_le_bytes();
            frame[2] = ssn_bytes[0];
            frame[3] = ssn_bytes[1];
            frame[4] = rsn_bytes[0];
            frame[5] = rsn_bytes[1];
            seq.ssn = seq.ssn.wrapping_add(1);
        }

        if let Some(ref mutex) = self.tls_stream_mutex {
            let mut stream = mutex.lock()
                .map_err(|e| MasterError::SendError(format!("mutex lock failed: {}", e)))?;
            stream.write_all(&frame)
                .map_err(|e| MasterError::SendError(format!("{}: {}", detail, e)))?;
        } else {
            let stream_guard = self.stream.read().await;
            let stream = stream_guard.as_ref()
                .ok_or(MasterError::NotConnected)?;
            match stream {
                MasterStream::Plain(s) => {
                    (&*s).write_all(&frame)
                        .map_err(|e| MasterError::SendError(format!("{}: {}", detail, e)))?;
                }
                MasterStream::Tls(_) => unreachable!("TLS stream should use tls_stream_mutex"),
            }
        }

        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::with_raw_bytes(
                Direction::Tx,
                label,
                format!("{} CA={}", detail, ca),
                frame.to_vec(),
            ));
        }

        Ok(())
    }
}

/// Background receive loop for plain TCP connections.
fn receive_loop(
    mut stream: TcpStream,
    received_data: SharedReceivedData,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    state_tx: tokio::sync::watch::Sender<MasterState>,
    seq: Arc<std::sync::Mutex<SeqNumbers>>,
    control_tx: tokio::sync::broadcast::Sender<ControlResponse>,
) {
    let mut reassembly_buf = Vec::with_capacity(65536);
    let mut read_buf = [0u8; 8192];

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        let n = match stream.read(&mut read_buf) {
            Ok(0) => {
                state_tx.send_replace(MasterState::Disconnected);
                if let Some(ref lc) = log_collector {
                    lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, "连接已关闭"));
                }
                break;
            }
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut
                || e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => {
                state_tx.send_replace(MasterState::Disconnected);
                if let Some(ref lc) = log_collector {
                    lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, format!("读取错误,连接断开: {}", e)));
                }
                break;
            }
        };

        reassembly_buf.extend_from_slice(&read_buf[..n]);

        // Extract complete frames from the reassembly buffer
        while reassembly_buf.len() >= 2 {
            // Find the start byte 0x68
            if reassembly_buf[0] != 0x68 {
                reassembly_buf.remove(0);
                continue;
            }
            let frame_len = reassembly_buf[1] as usize + 2;
            if reassembly_buf.len() < frame_len {
                break; // Wait for more data
            }
            let frame_data: Vec<u8> = reassembly_buf.drain(..frame_len).collect();
            process_received_frame(&frame_data, &received_data, &log_collector, &mut stream, &seq, &control_tx);
        }
    }
}

/// Background receive loop for TLS connections using a shared Mutex.
fn receive_loop_mutex(
    stream: Arc<std::sync::Mutex<MasterStream>>,
    received_data: SharedReceivedData,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    state_tx: tokio::sync::watch::Sender<MasterState>,
    seq: Arc<std::sync::Mutex<SeqNumbers>>,
    control_tx: tokio::sync::broadcast::Sender<ControlResponse>,
) {
    let mut reassembly_buf = Vec::with_capacity(65536);
    let mut read_buf = [0u8; 8192];

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        let n = {
            let mut locked = match stream.lock() {
                Ok(s) => s,
                Err(_) => {
                    state_tx.send_replace(MasterState::Disconnected);
                    break;
                }
            };
            match locked.read(&mut read_buf) {
                Ok(0) => {
                    state_tx.send_replace(MasterState::Disconnected);
                    if let Some(ref lc) = log_collector {
                        lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, "连接已关闭"));
                    }
                    break;
                }
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => {
                    state_tx.send_replace(MasterState::Disconnected);
                    if let Some(ref lc) = log_collector {
                        lc.try_add(LogEntry::new(Direction::Rx, FrameLabel::ConnectionEvent, format!("读取错误,连接断开: {}", e)));
                    }
                    break;
                }
            }
        };

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
            process_received_frame_mutex(&frame_data, &received_data, &log_collector, &stream, &seq, &control_tx);
        }
    }
}

/// Process a single received IEC 104 frame (plain TCP version).
fn process_received_frame(
    data: &[u8],
    received_data: &SharedReceivedData,
    log_collector: &Option<Arc<LogCollector>>,
    stream: &mut TcpStream,
    seq: &Arc<std::sync::Mutex<SeqNumbers>>,
    control_tx: &tokio::sync::broadcast::Sender<ControlResponse>,
) {
    if data.len() < 6 { return; }
    let ctrl1 = data[2];

    // U-frame (bits 0,1 both set)
    if ctrl1 & 0x03 == 0x03 {
        log_frame(data, log_collector);
        if ctrl1 == 0x43 {
            // TESTFR ACT → reply with TESTFR CON
            let response = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
            let _ = stream.write_all(&response);
        }
    }
    // S-frame (bit 0 = 1, bit 1 = 0) — just an acknowledgment, nothing to do
    else if ctrl1 & 0x01 == 0x01 {
        log_frame(data, log_collector);
    }
    // I-frame (bit 0 = 0)
    else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
        // Increment RSN for each received I-frame
        let rsn = {
            let mut s = seq.lock().unwrap();
            s.rsn = s.rsn.wrapping_add(1);
            s.rsn
        };
        parse_and_store_asdu(data, received_data, log_collector, control_tx);
        // Send S-frame with current RSN to acknowledge
        let rsn_bytes = (rsn << 1).to_le_bytes();
        let s_frame = [0x68, 0x04, 0x01, 0x00, rsn_bytes[0], rsn_bytes[1]];
        let _ = stream.write_all(&s_frame);
    }
}

/// Process a single received IEC 104 frame (TLS/Mutex version).
fn process_received_frame_mutex(
    data: &[u8],
    received_data: &SharedReceivedData,
    log_collector: &Option<Arc<LogCollector>>,
    stream: &Arc<std::sync::Mutex<MasterStream>>,
    seq: &Arc<std::sync::Mutex<SeqNumbers>>,
    control_tx: &tokio::sync::broadcast::Sender<ControlResponse>,
) {
    if data.len() < 6 { return; }
    let ctrl1 = data[2];

    if ctrl1 & 0x03 == 0x03 {
        log_frame(data, log_collector);
        if ctrl1 == 0x43 {
            let response = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
            if let Ok(mut locked) = stream.lock() {
                let _ = locked.write_all(&response);
            }
        }
    } else if ctrl1 & 0x01 == 0x01 {
        log_frame(data, log_collector);
    } else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
        let rsn = {
            let mut s = seq.lock().unwrap();
            s.rsn = s.rsn.wrapping_add(1);
            s.rsn
        };
        parse_and_store_asdu(data, received_data, log_collector, control_tx);
        let rsn_bytes = (rsn << 1).to_le_bytes();
        let s_frame = [0x68, 0x04, 0x01, 0x00, rsn_bytes[0], rsn_bytes[1]];
        if let Ok(mut locked) = stream.lock() {
            let _ = locked.write_all(&s_frame);
        }
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

    // Batch insert — single lock acquisition for all points in this frame
    if !points.is_empty() {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let rd = received_data.clone();
            handle.block_on(async {
                let mut map = rd.write().await;
                for point in points {
                    map.insert(point);
                }
            });
        }
    }
}

// --- Command frame builders ---

fn build_gi_command(ca: u16) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        100, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        0x00, 0x00, 0x00,
        0x14,
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

fn build_counter_read_command(ca: u16) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        101, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        0x00, 0x00, 0x00,
        0x05,
    ]
}

fn build_single_command(ca: u16, ioa: u32, value: bool, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut sco = if value { 0x01 } else { 0x00 };
    if select { sco |= 0x80; }
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        45, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        sco,
    ]
}

fn build_double_command(ca: u16, ioa: u32, value: u8, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut dco = value & 0x03;
    if select { dco |= 0x80; }
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        46, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        dco,
    ]
}

fn build_step_command(ca: u16, ioa: u32, value: u8, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut rco = value & 0x03;
    if select { rco |= 0x80; }
    vec![
        0x68, 0x0E,
        0x00, 0x00, 0x00, 0x00,
        47, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        rco,
    ]
}

fn build_setpoint_normalized(ca: u16, ioa: u32, value: f32, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva = (value * 32767.0) as i16;
    let nva_bytes = nva.to_le_bytes();
    let qos = if select { 0x80 } else { 0x00 };
    vec![
        0x68, 0x10,
        0x00, 0x00, 0x00, 0x00,
        48, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        nva_bytes[0], nva_bytes[1],
        qos,
    ]
}

fn build_setpoint_scaled(ca: u16, ioa: u32, value: i16, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let sva_bytes = value.to_le_bytes();
    let qos = if select { 0x80 } else { 0x00 };
    vec![
        0x68, 0x10,
        0x00, 0x00, 0x00, 0x00,
        49, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        sva_bytes[0], sva_bytes[1],
        qos,
    ]
}

fn build_setpoint_float_command(ca: u16, ioa: u32, value: f32, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let val_bytes = value.to_le_bytes();
    let qos = if select { 0x80 } else { 0x00 };
    vec![
        0x68, 0x12,
        0x00, 0x00, 0x00, 0x00,
        50, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        val_bytes[0], val_bytes[1], val_bytes[2], val_bytes[3],
        qos,
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
        let frame = build_gi_command(1);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 100);
        assert_eq!(frame[8], 6);
    }

    #[test]
    fn test_build_single_command() {
        let frame = build_single_command(1, 100, true, false);
        assert_eq!(frame[6], 45);
        assert_eq!(frame[12], 100);
        assert_eq!(frame[15], 0x01);
    }

    #[test]
    fn test_build_step_command() {
        // Lower, Execute
        let frame = build_step_command(1, 600, 1, false);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 47); // Type 47
        assert_eq!(frame[12], 600u32.to_le_bytes()[0]);
        assert_eq!(frame[15], 0x01); // RCO = lower, no select

        // Higher, Select
        let frame = build_step_command(1, 600, 2, true);
        assert_eq!(frame[15], 0x82); // RCO = higher + select bit
    }

    #[test]
    fn test_build_setpoint_normalized() {
        let frame = build_setpoint_normalized(1, 400, 0.5, false);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 48); // Type 48
        let nva = i16::from_le_bytes([frame[15], frame[16]]);
        assert_eq!(nva, (0.5_f32 * 32767.0) as i16);
        assert_eq!(frame[17], 0x00); // QOS = no select

        // With select
        let frame = build_setpoint_normalized(1, 400, -0.5, true);
        assert_eq!(frame[17], 0x80); // QOS = select bit
    }

    #[test]
    fn test_build_setpoint_scaled() {
        let frame = build_setpoint_scaled(1, 500, 1024, false);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 49); // Type 49
        let sva = i16::from_le_bytes([frame[15], frame[16]]);
        assert_eq!(sva, 1024);
        assert_eq!(frame[17], 0x00); // QOS = no select
    }

    #[test]
    fn test_build_setpoint_float_with_select() {
        let frame = build_setpoint_float_command(1, 300, 25.5, true);
        assert_eq!(frame[6], 50); // Type 50
        let val = f32::from_le_bytes([frame[15], frame[16], frame[17], frame[18]]);
        assert!((val - 25.5).abs() < 0.001);
        assert_eq!(frame[19], 0x80); // QOS = select bit

        let frame = build_setpoint_float_command(1, 300, 25.5, false);
        assert_eq!(frame[19], 0x00); // QOS = no select
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
