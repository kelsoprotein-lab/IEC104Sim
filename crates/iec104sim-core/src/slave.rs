use crate::data_point::{DataPoint, DataPointMap, DataPointValue, InformationObjectDef};
use crate::log_collector::LogCollector;
use crate::log_entry::{Direction, FrameLabel, LogEntry};
use crate::types::{AsduTypeId, DataCategory};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener as AsyncTcpListener, TcpStream as AsyncTcpStream};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// TLS Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlaveTlsConfig {
    pub enabled: bool,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub ca_file: String,
    #[serde(default)]
    pub require_client_cert: bool,
    /// Optional PKCS#12 (.p12/.pfx) identity file. When set, cert_file and
    /// key_file are ignored for identity loading. Required on macOS when using
    /// ECDSA keys (native-tls / Security framework limitation).
    #[serde(default)]
    pub pkcs12_file: String,
    /// Password for the PKCS#12 file (may be empty string).
    #[serde(default)]
    pub pkcs12_password: String,
}

// ---------------------------------------------------------------------------
// Cyclic / Spontaneous Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CyclicConfig {
    pub enabled: bool,
    pub interval_ms: u32,
}

impl Default for CyclicConfig {
    fn default() -> Self {
        Self { enabled: false, interval_ms: 2000 }
    }
}

// ---------------------------------------------------------------------------
// Stream Abstraction (for blocking TLS path)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
enum SlaveStream {
    Plain(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
}

impl std::io::Read for SlaveStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            SlaveStream::Plain(s) => s.read(buf),
            SlaveStream::Tls(s) => s.read(buf),
        }
    }
}

impl std::io::Write for SlaveStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            SlaveStream::Plain(s) => s.write(buf),
            SlaveStream::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            SlaveStream::Plain(s) => s.flush(),
            SlaveStream::Tls(s) => s.flush(),
        }
    }
}

// ---------------------------------------------------------------------------
// Station
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub common_address: u16,
    pub name: String,
    pub data_points: DataPointMap,
    pub object_defs: Vec<InformationObjectDef>,
    #[serde(default)]
    pub cyclic_config: CyclicConfig,
}

impl Station {
    pub fn new(common_address: u16, name: impl Into<String>) -> Self {
        Self {
            common_address,
            name: name.into(),
            data_points: DataPointMap::new(),
            object_defs: Vec::new(),
            cyclic_config: CyclicConfig::default(),
        }
    }

    pub fn with_default_points(common_address: u16, name: impl Into<String>, count_per_category: u32) -> Self {
        let mut station = Self::new(common_address, name);
        // 16 个 ASDU 类型（8 物理分类 × NA + TB），全部共享同一段 IOA 1..=N。
        // (ioa, asdu_type) 是数据点的复合主键，不同类型可以挂在同一 IOA 下。
        let asdu_types: [(AsduTypeId, DataCategory); 16] = [
            (AsduTypeId::MSpNa1, DataCategory::SinglePoint),
            (AsduTypeId::MSpTb1, DataCategory::SinglePoint),
            (AsduTypeId::MDpNa1, DataCategory::DoublePoint),
            (AsduTypeId::MDpTb1, DataCategory::DoublePoint),
            (AsduTypeId::MStNa1, DataCategory::StepPosition),
            (AsduTypeId::MStTb1, DataCategory::StepPosition),
            (AsduTypeId::MBoNa1, DataCategory::Bitstring),
            (AsduTypeId::MBoTb1, DataCategory::Bitstring),
            (AsduTypeId::MMeNa1, DataCategory::NormalizedMeasured),
            (AsduTypeId::MMeTd1, DataCategory::NormalizedMeasured),
            (AsduTypeId::MMeNb1, DataCategory::ScaledMeasured),
            (AsduTypeId::MMeTe1, DataCategory::ScaledMeasured),
            (AsduTypeId::MMeNc1, DataCategory::FloatMeasured),
            (AsduTypeId::MMeTf1, DataCategory::FloatMeasured),
            (AsduTypeId::MItNa1, DataCategory::IntegratedTotals),
            (AsduTypeId::MItTb1, DataCategory::IntegratedTotals),
        ];
        for (asdu_type, category) in &asdu_types {
            for i in 0..count_per_category {
                let ioa = 1 + i;
                station.data_points.insert(DataPoint::new(ioa, *asdu_type));
                station.object_defs.push(InformationObjectDef {
                    ioa,
                    asdu_type: *asdu_type,
                    category: *category,
                    name: String::new(),
                    comment: String::new(),
                });
            }
        }
        station
    }

    pub fn with_random_points(common_address: u16, name: impl Into<String>, count_per_category: u32) -> Self {
        use rand::Rng;
        let mut station = Self::with_default_points(common_address, name, count_per_category);
        let mut rng = rand::thread_rng();
        for point in station.data_points.points.values_mut() {
            point.value = match point.asdu_type.category() {
                DataCategory::SinglePoint => DataPointValue::SinglePoint { value: rng.gen() },
                DataCategory::DoublePoint => DataPointValue::DoublePoint { value: rng.gen_range(1..=2) },
                DataCategory::NormalizedMeasured => DataPointValue::Normalized { value: rng.gen_range(-1.0..1.0) },
                DataCategory::ScaledMeasured => DataPointValue::Scaled { value: rng.gen_range(-1000..1000) },
                DataCategory::FloatMeasured => DataPointValue::ShortFloat { value: rng.gen_range(-100.0..100.0) },
                DataCategory::IntegratedTotals => DataPointValue::IntegratedTotal { value: rng.gen_range(0..10000), carry: false, sequence: 0 },
                _ => DataPointValue::default_for(point.asdu_type),
            };
        }
        station
    }

    pub fn add_point(&mut self, def: InformationObjectDef) -> Result<(), SlaveError> {
        if !self.data_points.contains(def.ioa, def.asdu_type) {
            self.data_points.insert(DataPoint::new(def.ioa, def.asdu_type));
        }
        // Update or add metadata
        if let Some(existing_def) = self.object_defs.iter_mut().find(|d| d.ioa == def.ioa && d.asdu_type == def.asdu_type) {
            *existing_def = def;
        } else {
            self.object_defs.push(def);
        }
        Ok(())
    }

    pub fn remove_point(&mut self, ioa: u32, asdu_type: AsduTypeId) -> Result<(), SlaveError> {
        if !self.data_points.contains(ioa, asdu_type) { return Err(SlaveError::IoaNotFound(ioa)); }
        self.data_points.remove(ioa, asdu_type);
        self.object_defs.retain(|d| !(d.ioa == ioa && d.asdu_type == asdu_type));
        Ok(())
    }

    /// Batch-add data points with consecutive IOAs starting from `start_ioa`.
    /// Optimized: avoids O(n) linear search in object_defs per point.
    pub fn batch_add_points(
        &mut self,
        start_ioa: u32,
        count: u32,
        asdu_type: AsduTypeId,
        name_prefix: &str,
    ) -> Result<u32, SlaveError> {
        use std::collections::HashSet;
        let category = asdu_type.category();
        // Pre-build set of existing (ioa, type) for O(1) lookup
        let existing: HashSet<(u32, AsduTypeId)> = self.object_defs.iter()
            .map(|d| (d.ioa, d.asdu_type))
            .collect();
        for i in 0..count {
            let ioa = start_ioa + i;
            if !self.data_points.contains(ioa, asdu_type) {
                self.data_points.insert(DataPoint::new(ioa, asdu_type));
            }
            let name = if name_prefix.is_empty() {
                String::new()
            } else {
                format!("{}_{}", name_prefix, ioa)
            };
            if !existing.contains(&(ioa, asdu_type)) {
                self.object_defs.push(InformationObjectDef {
                    ioa, asdu_type, category, name, comment: String::new(),
                });
            }
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Server State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerState { Stopped, Running }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaveTransportConfig {
    pub bind_address: String,
    pub port: u16,
    #[serde(default)]
    pub tls: SlaveTlsConfig,
}

impl Default for SlaveTransportConfig {
    fn default() -> Self {
        Self { bind_address: "0.0.0.0".to_string(), port: 2404, tls: SlaveTlsConfig::default() }
    }
}

// ---------------------------------------------------------------------------
// Connection State — shared between read task and cyclic task
// ---------------------------------------------------------------------------

/// Per-connection write queue. The async write task drains this queue.
struct ConnectionWrite {
    /// Mutex-protected byte queue. Write task drains this.
    queue: Arc<tokio::sync::Mutex<Vec<u8>>>,
    /// Sequence numbers (N(S)<<1 and N(R)<<1, 16-bit).
    ssn: u16,
    rsn: u16,
    /// Last sent value string per IOA.
    last_sent: HashMap<u32, String>,
    /// Logger.
    #[allow(dead_code)]
    log_collector: Option<Arc<LogCollector>>,
}

type SharedConnections = Arc<RwLock<HashMap<SocketAddr, ConnectionWrite>>>;

// ---------------------------------------------------------------------------
// SlaveServer
// ---------------------------------------------------------------------------

pub type SharedStations = Arc<RwLock<HashMap<u16, Station>>>;

pub struct SlaveServer {
    pub transport: SlaveTransportConfig,
    pub stations: SharedStations,
    pub log_collector: Option<Arc<LogCollector>>,
    state: ServerState,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
    cyclic_handle: Option<tokio::task::JoinHandle<()>>,
    connections: SharedConnections,
}

impl SlaveServer {
    pub fn new(transport: SlaveTransportConfig) -> Self {
        Self {
            transport,
            stations: Arc::new(RwLock::new(HashMap::new())),
            log_collector: None,
            state: ServerState::Stopped,
            shutdown_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            server_handle: None,
            cyclic_handle: None,
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_log_collector(mut self, collector: Arc<LogCollector>) -> Self {
        self.log_collector = Some(collector);
        self
    }

    pub fn state(&self) -> ServerState { self.state }

    pub async fn add_station(&self, station: Station) -> Result<(), SlaveError> {
        let mut stations = self.stations.write().await;
        if stations.contains_key(&station.common_address) {
            return Err(SlaveError::DuplicateStation(station.common_address));
        }
        stations.insert(station.common_address, station);
        Ok(())
    }

    pub async fn remove_station(&self, ca: u16) -> Result<Station, SlaveError> {
        let mut stations = self.stations.write().await;
        stations.remove(&ca).ok_or(SlaveError::StationNotFound(ca))
    }

    pub async fn set_cyclic_config(&self, common_address: u16, config: CyclicConfig) -> Result<(), SlaveError> {
        let mut stations = self.stations.write().await;
        let station = stations.get_mut(&common_address).ok_or(SlaveError::StationNotFound(common_address))?;
        station.cyclic_config = config;
        Ok(())
    }

    /// Queue spontaneous I-frames (COT=3) for the given (IOA, type) pairs to all connected clients.
    pub async fn queue_spontaneous(&self, common_address: u16, changed: &[(u32, AsduTypeId)]) {
        if changed.is_empty() { return; }
        let stations = self.stations.read().await;
        let station = match stations.get(&common_address) {
            Some(s) => s,
            None => return,
        };
        let ca_bytes = station.common_address.to_le_bytes();
        let mut conns = self.connections.write().await;
        let mut total_sent = 0usize;
        for (_addr, conn) in conns.iter_mut() {
            let mut batch = Vec::new();
            for &(ioa, asdu_type) in changed {
                let point = match station.data_points.get(ioa, asdu_type) {
                    Some(p) => p,
                    None => continue,
                };
                let ioa_bytes = point.ioa.to_le_bytes();
                batch.extend(encode_point_frame(&point.value, 3, &ca_bytes, &ioa_bytes[..3], &mut conn.ssn, &mut conn.rsn));
                conn.last_sent.insert(ioa, point.value.display());
            }
            if !batch.is_empty() {
                total_sent += 1;
                conn.queue.lock().await.extend(batch);
            }
        }
        if total_sent > 0 {
            if let Some(ref lc) = self.log_collector {
                let detail = if changed.len() == 1 {
                    let (ioa, asdu_type) = changed[0];
                    format!("突发上送 (COT=3) IOA={} {} CA={} → {} 个客户端", ioa, asdu_type.name(), common_address, total_sent)
                } else {
                    format!("突发上送 (COT=3) {} 个 IOA CA={} → {} 个客户端", changed.len(), common_address, total_sent)
                };
                let label = changed
                    .first()
                    .map(|(_, t)| FrameLabel::IFrame(t.name().to_string()))
                    .unwrap_or_else(|| FrameLabel::IFrame(String::new()));
                lc.try_add(LogEntry::new(Direction::Tx, label, detail));
            }
        }
    }

    pub async fn start(&mut self) -> Result<(), SlaveError> {
        if self.state == ServerState::Running { return Err(SlaveError::AlreadyRunning); }

        let addr_str = format!("{}:{}", self.transport.bind_address, self.transport.port);
        let listener = AsyncTcpListener::bind(&addr_str)
            .await
            .map_err(|e| SlaveError::BindError(format!("Failed to bind {}: {}", addr_str, e)))?;

        let tls_acceptor: Option<Arc<native_tls::TlsAcceptor>> = if self.transport.tls.enabled {
            let cfg = &self.transport.tls;
            let identity = if !cfg.pkcs12_file.is_empty() {
                let p12 = std::fs::read(&cfg.pkcs12_file)
                    .map_err(|e| SlaveError::TlsError(format!("读取 PKCS12 {}: {}", cfg.pkcs12_file, e)))?;
                native_tls::Identity::from_pkcs12(&p12, &cfg.pkcs12_password)
                    .map_err(|e| SlaveError::TlsError(format!("加载 PKCS12 身份: {}", e)))?
            } else {
                let cert = std::fs::read(&cfg.cert_file)
                    .map_err(|e| SlaveError::TlsError(format!("读取证书 {}: {}", cfg.cert_file, e)))?;
                let key = std::fs::read(&cfg.key_file)
                    .map_err(|e| SlaveError::TlsError(format!("读取密钥 {}: {}", cfg.key_file, e)))?;
                native_tls::Identity::from_pkcs8(&cert, &key)
                    .map_err(|e| SlaveError::TlsError(format!("加载身份: {}", e)))?
            };
            let mut builder = native_tls::TlsAcceptor::builder(identity);
            builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
            Some(Arc::new(builder.build().map_err(|e| SlaveError::TlsError(format!("创建接受器: {}", e)))?))
        } else { None };

        let shutdown_flag = self.shutdown_flag.clone();
        shutdown_flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let stations = self.stations.clone();
        let log_collector = self.log_collector.clone();
        let is_tls = self.transport.tls.enabled;

        // Shared connections map.
        self.connections = Arc::new(RwLock::new(HashMap::new()));
        let connections = self.connections.clone();
        let cyclic_connections = connections.clone();

        // Start cyclic background task.
        let cyclic_stations = self.stations.clone();
        let cyclic_flag = self.shutdown_flag.clone();
        let cyclic_log = self.log_collector.clone();
        let cyclic_handle = tokio::spawn(async move {
            // Use interval_ms from the first enabled station, default to 2000ms
            let get_interval_ms = || async {
                let stations = cyclic_stations.read().await;
                stations.values()
                    .find(|s| s.cyclic_config.enabled)
                    .map(|s| s.cyclic_config.interval_ms)
                    .unwrap_or(2000)
            };
            let mut interval_ms = get_interval_ms().await;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms as u64));
            loop {
                interval.tick().await;
                if cyclic_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }

                // Check if interval changed
                let new_interval_ms = get_interval_ms().await;
                if new_interval_ms != interval_ms {
                    interval_ms = new_interval_ms;
                    interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms as u64));
                }

                let stations_read = cyclic_stations.read().await;
                let addrs_to_remove: Vec<SocketAddr> = {
                    let mut conns = cyclic_connections.write().await;
                    let to_remove = Vec::new();
                    for (_addr, conn) in conns.iter_mut() {
                        for station in stations_read.values() {
                            if !station.cyclic_config.enabled { continue; }
                            for point in station.data_points.all_sorted() {
                                let value_str = point.value.display();
                                if let Some(last) = conn.last_sent.get(&point.ioa) {
                                    if last == &value_str { continue; }
                                }
                                let ca_bytes = station.common_address.to_le_bytes();
                                let ioa_bytes = point.ioa.to_le_bytes();
                                let asdu = encode_point_frame(&point.value, 3, &ca_bytes, &ioa_bytes[..3], &mut conn.ssn, &mut conn.rsn);
                                conn.queue.lock().await.extend(asdu);
                                conn.last_sent.insert(point.ioa, value_str);
                            }
                        }
                    }
                    to_remove
                };
                drop(stations_read);
                if !addrs_to_remove.is_empty() {
                    let mut conns = cyclic_connections.write().await;
                    for addr in addrs_to_remove {
                        conns.remove(&addr);
                        if let Some(ref lc) = cyclic_log {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::ConnectionEvent,
                                format!("连接关闭 (cyclic): {}", addr),
                            ));
                        }
                    }
                }
            }
        });
        self.cyclic_handle = Some(cyclic_handle);

        let handle = tokio::spawn(async move {
            loop {
                if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        let peer_str = format!("{}", peer_addr);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Rx, FrameLabel::ConnectionEvent,
                                format!("客户端连接: {}{}", peer_str, if is_tls { " (TLS)" } else { "" }),
                            ));
                        }
                        let stations = stations.clone();
                        let lc = log_collector.clone();
                        let flag = shutdown_flag.clone();
                        let tls_acceptor = tls_acceptor.clone();
                        let connections = connections.clone();

                        if tls_acceptor.is_some() {
                            // TLS: blocking I/O via spawn_blocking.
                            // Create a shared queue so queue_spontaneous() can enqueue frames
                            // that the blocking loop drains to the TLS stream.
                            let tls_queue: SharedQueue = Arc::new(tokio::sync::Mutex::new(Vec::new()));
                            connections.write().await.insert(peer_addr, ConnectionWrite {
                                queue: Arc::clone(&tls_queue),
                                ssn: 0, rsn: 0,
                                last_sent: HashMap::new(),
                                log_collector: lc.clone(),
                            });
                            let tls_connections = connections.clone();
                            tokio::task::spawn_blocking(move || {
                                let tcp_stream = stream.into_std().expect("into_std");
                                // into_std() preserves tokio's non-blocking mode; switch to
                                // blocking so native-tls can perform synchronous handshake I/O.
                                tcp_stream.set_nonblocking(false).expect("set_nonblocking(false)");
                                let acceptor = tls_acceptor.as_ref().unwrap();
                                let mut tls_stream = match acceptor.accept(tcp_stream) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        if let Some(ref lc) = lc {
                                            lc.try_add(LogEntry::new(
                                                Direction::Rx, FrameLabel::ConnectionEvent,
                                                format!("TLS 握手失败: {} - {}", peer_str, e),
                                            ));
                                        }
                                        // Clean up connection entry on failure
                                        let rt = tokio::runtime::Handle::try_current();
                                        if let Ok(h) = rt { h.block_on(async { tls_connections.write().await.remove(&peer_addr); }); }
                                        return;
                                    }
                                };
                                // Set read timeout so the loop can periodically drain the write queue.
                                let _ = tls_stream.get_ref().set_read_timeout(Some(std::time::Duration::from_millis(100)));
                                if let Some(ref lc) = lc {
                                    lc.try_add(LogEntry::new(
                                        Direction::Rx, FrameLabel::ConnectionEvent,
                                        format!("TLS 握手成功: {}", peer_str),
                                    ));
                                }
                                handle_client_blocking(&mut tls_stream, stations, lc, flag, tls_queue, tls_connections, peer_addr);
                            });
                        } else {
                            // Plain TCP: async with queue-based cyclic writes.
                            // Split into read/write halves so we can use the write half in a
                            // dedicated write task and pass read half to the read loop.
                            let (rh, wh) = tokio::io::split(stream);

                            let queue: SharedQueue = Arc::new(tokio::sync::Mutex::new(Vec::new()));
                            let queue_for_writer = Arc::clone(&queue);
                            let queue_for_reader = Arc::clone(&queue);
                            let lc_for_reader = lc.clone();
                            let stations_for_reader = stations.clone();
                            let addr_for_read = peer_addr;

                            // Register connection for cyclic task.
                            connections.write().await.insert(peer_addr, ConnectionWrite {
                                queue,
                                ssn: 0, rsn: 0,
                                last_sent: HashMap::new(),
                                log_collector: lc.clone(),
                            });

                            // Spawn async write drain task (owns WriteHalf).
                            let flag_for_writer = flag.clone();
                            let conn_for_writer = Arc::clone(&connections);
                            tokio::spawn(async move {
                                let mut wh = wh;
                                loop {
                                    if flag_for_writer.load(std::sync::atomic::Ordering::SeqCst) { break; }
                                    // Atomically drain pending bytes under lock, then write outside lock
                                    let snapshot = {
                                        let mut bytes = queue_for_writer.lock().await;
                                        if bytes.is_empty() { Vec::new() } else { bytes.drain(..).collect::<Vec<u8>>() }
                                    };
                                    if !snapshot.is_empty() {
                                        match wh.write_all(&snapshot).await {
                                            Ok(()) => {}
                                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                            }
                                            Err(_) => {
                                                conn_for_writer.write().await.remove(&addr_for_read);
                                                return;
                                            }
                                        }
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            });

                            // Spawn read task (owns ReadHalf + queue for enqueueing responses).
                            tokio::spawn(async move {
                                handle_client_read_loop(rh, stations_for_reader, lc_for_reader, flag, connections, queue_for_reader, addr_for_read).await;
                            });
                        }
                    }
                    Err(_) => { tokio::time::sleep(std::time::Duration::from_millis(50)).await; }
                }
            }
        });

        self.server_handle = Some(handle);
        self.state = ServerState::Running;
        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::new(
                Direction::Tx, FrameLabel::ConnectionEvent,
                format!("服务器启动: {}{}", addr_str, if is_tls { " (TLS)" } else { "" }),
            ));
        }
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), SlaveError> {
        if self.state == ServerState::Stopped { return Err(SlaveError::NotRunning); }
        self.shutdown_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        // Connect briefly to unblock listener.accept()
        let addr = format!("{}:{}", self.transport.bind_address, self.transport.port);
        let _ = tokio::net::TcpStream::connect(&addr).await;
        if let Some(h) = self.server_handle.take() { let _ = h.await; }
        if let Some(h) = self.cyclic_handle.take() { let _ = h.await; }
        self.state = ServerState::Stopped;
        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::new(
                Direction::Tx, FrameLabel::ConnectionEvent,
                "服务器停止".to_string(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared Queue type alias
// ---------------------------------------------------------------------------
type SharedQueue = Arc<tokio::sync::Mutex<Vec<u8>>>;

// ---------------------------------------------------------------------------
// Async Client Read Loop
// ---------------------------------------------------------------------------

async fn handle_client_read_loop(
    mut stream: tokio::io::ReadHalf<AsyncTcpStream>,
    stations: SharedStations,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    connections: SharedConnections,
    queue: SharedQueue,
    peer_addr: SocketAddr,
) {
    let mut buf = [0u8; 8192];
    let mut reassembly_buf: Vec<u8> = Vec::with_capacity(65536);
    let mut ssn: u16 = 0;
    let mut rsn: u16 = 0;

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let n = match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(_) => break,
        };

        reassembly_buf.extend_from_slice(&buf[..n]);

        // Extract and process complete frames from the reassembly buffer
        while reassembly_buf.len() >= 2 {
            if reassembly_buf[0] != 0x68 {
                reassembly_buf.remove(0);
                continue;
            }
            let frame_len = reassembly_buf[1] as usize + 2;
            if reassembly_buf.len() < frame_len { break; }
            let data: Vec<u8> = reassembly_buf.drain(..frame_len).collect();
            let n = data.len();

        if let Some(ref lc) = log_collector {
            if let Ok(frame) = crate::frame::parse_apci(&data) {
                let summary = crate::frame::format_frame_summary(&frame);
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Rx, FrameLabel::IFrame(summary.clone()),
                    summary, data.to_vec(),
                ));
            }
        }

        if data.len() >= 6 && data[0] == 0x68 {
            let ctrl1 = data[2];

            if ctrl1 & 0x03 == 0x03 {
                match ctrl1 {
                    0x07 => {
                        let resp = [0x68, 0x04, 0x0B, 0x00, 0x00, 0x00];
                        queue.lock().await.extend_from_slice(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStartCon, "STARTDT CON", resp.to_vec()));
                        }
                    }
                    0x13 => {
                        let resp = [0x68, 0x04, 0x23, 0x00, 0x00, 0x00];
                        queue.lock().await.extend_from_slice(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStopCon, "STOPDT CON", resp.to_vec()));
                        }
                    }
                    0x43 => {
                        let resp = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
                        queue.lock().await.extend_from_slice(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UTestCon, "TESTFR CON", resp.to_vec()));
                        }
                    }
                    _ => {}
                }
            } else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
                let asdu_type = data[6];
                let cause = data[8];
                let ca = u16::from_le_bytes([data[10], data[11]]);

                match asdu_type {
                    100 => {
                        let mut ack = data[..n].to_vec(); ack[8] = 7;
                        queue.lock().await.extend_from_slice(&ack);
                        let stations_read = stations.read().await;
                        if let Some(station) = stations_read.get(&ca) {
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::GeneralInterrogation,
                                    format!("GI 激活确认 CA={}", ca),
                                ));
                            }
                            // Queue GI response frames.
                            let ca_bytes = station.common_address.to_le_bytes();
                            for point in station.data_points.all_sorted() {
                                let ioa_bytes = point.ioa.to_le_bytes();
                                let asdu = encode_point_frame(&point.value, 20, &ca_bytes, &ioa_bytes[..3], &mut ssn, &mut rsn);
                                queue.lock().await.extend_from_slice(&asdu);
                            }
                        }
                        drop(stations_read);
                        let mut term = data[..n].to_vec(); term[8] = 10;
                        queue.lock().await.extend_from_slice(&term);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::GeneralInterrogation,
                                format!("GI 激活终止 CA={}", ca),
                            ));
                        }
                    }
                    101 => {
                        // Counter Interrogation (C_CI_NA_1, Type 101)
                        let mut ack = data[..n].to_vec(); ack[8] = 7;
                        queue.lock().await.extend_from_slice(&ack);
                        let stations_read = stations.read().await;
                        if let Some(station) = stations_read.get(&ca) {
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::CounterInterrogation,
                                    format!("累计量召唤 激活确认 CA={}", ca),
                                ));
                            }
                            let ca_bytes = station.common_address.to_le_bytes();
                            for point in station.data_points.all_sorted() {
                                let ioa_bytes = point.ioa.to_le_bytes();
                                let asdu = match &point.value {
                                    DataPointValue::IntegratedTotal { value, carry, sequence } => {
                                        let b = value.to_le_bytes();
                                        let mut bcr = *sequence & 0x1F;
                                        if *carry { bcr |= 0x20; }
                                        build_i_frame(15, 37, &ca_bytes, &ioa_bytes[..3], &[b[0], b[1], b[2], b[3], bcr], &mut ssn, &mut rsn)
                                    }
                                    _ => continue,
                                };
                                queue.lock().await.extend_from_slice(&asdu);
                            }
                        }
                        drop(stations_read);
                        let mut term = data[..n].to_vec(); term[8] = 10;
                        queue.lock().await.extend_from_slice(&term);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::CounterInterrogation,
                                format!("累计量召唤 激活终止 CA={}", ca),
                            ));
                        }
                    }
                    103 => {
                        let mut ack = data[..n].to_vec(); ack[8] = 7;
                        queue.lock().await.extend_from_slice(&ack);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::ClockSync,
                                format!("时钟同步确认 CA={}", ca),
                            ));
                        }
                    }
                    45 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sco = data[15]; let value = sco & 0x01 != 0; let is_select = sco & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::SinglePoint) {
                                        dp.value = DataPointValue::SinglePoint { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                queue.lock().await.extend_from_slice(&term);
                                // Send spontaneous update (COT=3) after control execution
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::SinglePoint) {
                                        let ca_b = ca.to_le_bytes();
                                        let ioa_b = ioa.to_le_bytes();
                                        let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SingleCommand,
                                    format!("单点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    46 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let dco = data[15]; let value = dco & 0x03; let is_select = dco & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::DoublePoint) {
                                        dp.value = DataPointValue::DoublePoint { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::DoublePoint) {
                                        let ca_b = ca.to_le_bytes();
                                        let ioa_b = ioa.to_le_bytes();
                                        let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::DoubleCommand,
                                    format!("双点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    47 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let rco = data[15]; let step_val = rco & 0x03; let is_select = rco & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::StepPosition) {
                                        if let DataPointValue::StepPosition { ref mut value, .. } = dp.value {
                                            match step_val { 1 => { if *value > -64 { *value -= 1; } } 2 => { if *value < 63 { *value += 1; } } _ => {} }
                                            dp.timestamp = Some(chrono::Utc::now());
                                        }
                                    }
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::StepPosition) {
                                        let ca_b = ca.to_le_bytes();
                                        let ioa_b = ioa.to_le_bytes();
                                        let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                let dir = match step_val { 1 => "降", 2 => "升", _ => "?" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::StepCommand,
                                    format!("步调节命令确认 IOA={} {} {} CA={}", ioa, dir, mode, ca),
                                ));
                            }
                        }
                    }
                    48 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let nva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            let value = nva as f32 / 32767.0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::NormalizedMeasured) {
                                        dp.value = DataPointValue::Normalized { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::NormalizedMeasured) {
                                        let ca_b = ca.to_le_bytes();
                                        let ioa_b = ioa.to_le_bytes();
                                        let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointNormalized,
                                    format!("归一化设定值确认 IOA={} val={:.4} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    49 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::ScaledMeasured) {
                                        dp.value = DataPointValue::Scaled { value: sva };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::ScaledMeasured) {
                                        let ca_b = ca.to_le_bytes();
                                        let ioa_b = ioa.to_le_bytes();
                                        let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointScaled,
                                    format!("标度化设定值确认 IOA={} val={} {} CA={}", ioa, sva, mode, ca),
                                ));
                            }
                        }
                    }
                    50 => {
                        if data.len() >= 20 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let value = f32::from_le_bytes([data[15], data[16], data[17], data[18]]);
                            let qos = data[19]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::FloatMeasured) {
                                        dp.value = DataPointValue::ShortFloat { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::FloatMeasured) {
                                        let ca_b = ca.to_le_bytes();
                                        let ioa_b = ioa.to_le_bytes();
                                        let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointFloat,
                                    format!("浮点设定值确认 IOA={} val={:.3} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    _ => {
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Rx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                format!("未知 ASDU 类型={} CA={} COT={}", asdu_type, ca, cause),
                            ));
                        }
                    }
                }
            }
        }
        } // end while reassembly_buf
    }

    connections.write().await.remove(&peer_addr);
    if let Some(ref lc) = log_collector {
        lc.try_add(LogEntry::new(
            Direction::Tx, FrameLabel::ConnectionEvent,
            format!("连接关闭: {}", peer_addr),
        ));
    }
}

// ---------------------------------------------------------------------------
// Blocking Client Handler (for TLS)
// ---------------------------------------------------------------------------

fn handle_client_blocking(
    stream: &mut native_tls::TlsStream<TcpStream>,
    stations: SharedStations,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    write_queue: SharedQueue,
    connections: SharedConnections,
    peer_addr: SocketAddr,
) {
    use std::io::{Read, Write};
    let mut buf = [0u8; 512];
    let mut ssn: u16 = 0;
    let mut rsn: u16 = 0;

    // Cache the runtime handle once — this function always runs inside spawn_blocking.
    let rt = tokio::runtime::Handle::current();

    // Drain the shared write queue to the TLS stream.
    let drain_queue = |stream: &mut native_tls::TlsStream<TcpStream>, queue: &SharedQueue, rt: &tokio::runtime::Handle| {
        let pending = rt.block_on(async {
            let mut q = queue.lock().await;
            if q.is_empty() { Vec::new() } else { q.drain(..).collect::<Vec<u8>>() }
        });
        if !pending.is_empty() {
            let _ = stream.write_all(&pending);
        }
    };

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout hit — drain queue and continue waiting for data.
                drain_queue(stream, &write_queue, &rt);
                continue;
            }
            Err(_) => break,
        };

        let data = &buf[..n];

        if let Some(ref lc) = log_collector {
            if let Ok(frame) = crate::frame::parse_apci(data) {
                let summary = crate::frame::format_frame_summary(&frame);
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Rx, FrameLabel::IFrame(summary.clone()),
                    summary, data.to_vec(),
                ));
            }
        }

        if data.len() >= 6 && data[0] == 0x68 {
            let ctrl1 = data[2];

            if ctrl1 & 0x03 == 0x03 {
                match ctrl1 {
                    0x07 => {
                        let resp = [0x68, 0x04, 0x0B, 0x00, 0x00, 0x00];
                        let _ = stream.write_all(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStartCon, "STARTDT CON", resp.to_vec()));
                        }
                    }
                    0x13 => {
                        let resp = [0x68, 0x04, 0x23, 0x00, 0x00, 0x00];
                        let _ = stream.write_all(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStopCon, "STOPDT CON", resp.to_vec()));
                        }
                    }
                    0x43 => {
                        let resp = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
                        let _ = stream.write_all(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UTestCon, "TESTFR CON", resp.to_vec()));
                        }
                    }
                    _ => {}
                }
            } else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
                let asdu_type = data[6];
                let cause = data[8];
                let ca = u16::from_le_bytes([data[10], data[11]]);

                match asdu_type {
                    100 => {
                        let mut ack = data[..n].to_vec(); ack[8] = 7;
                        let _ = stream.write_all(&ack);
                        let rt = tokio::runtime::Handle::try_current();
                        if let Ok(handle) = rt {
                            let stations = stations.clone();
                            let lc = log_collector.clone();
                            handle.block_on(async {
                                let stations_read = stations.read().await;
                                if let Some(station) = stations_read.get(&ca) {
                                    if let Some(ref lc) = lc {
                                        lc.try_add(LogEntry::new(
                                            Direction::Tx, FrameLabel::GeneralInterrogation,
                                            format!("GI 激活确认 CA={}", ca),
                                        ));
                                    }
                                    send_gi_response_blocking(stream, station);
                                }
                                drop(stations_read);
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Some(ref lc) = lc {
                                    lc.try_add(LogEntry::new(
                                        Direction::Tx, FrameLabel::GeneralInterrogation,
                                        format!("GI 激活终止 CA={}", ca),
                                    ));
                                }
                            });
                        }
                    }
                    101 => {
                        // Counter Interrogation (C_CI_NA_1, Type 101)
                        let mut ack = data[..n].to_vec(); ack[8] = 7;
                        let _ = stream.write_all(&ack);
                        let rt = tokio::runtime::Handle::try_current();
                        if let Ok(handle) = rt {
                            let stations = stations.clone();
                            let lc = log_collector.clone();
                            handle.block_on(async {
                                let stations_read = stations.read().await;
                                if let Some(station) = stations_read.get(&ca) {
                                    if let Some(ref lc) = lc {
                                        lc.try_add(LogEntry::new(
                                            Direction::Tx, FrameLabel::CounterInterrogation,
                                            format!("累计量召唤 激活确认 CA={}", ca),
                                        ));
                                    }
                                    // Counter interrogation: send only IntegratedTotals points
                                    let ca_bytes = station.common_address.to_le_bytes();
                                    let mut ssn: u16 = 0;
                                    let mut rsn: u16 = 0;
                                    for point in station.data_points.all_sorted() {
                                        let ioa_bytes = point.ioa.to_le_bytes();
                                        let asdu = match &point.value {
                                            DataPointValue::IntegratedTotal { value, carry, sequence } => {
                                                let b = value.to_le_bytes();
                                                let mut bcr = *sequence & 0x1F;
                                                if *carry { bcr |= 0x20; }
                                                build_i_frame(15, 37, &ca_bytes, &ioa_bytes[..3], &[b[0], b[1], b[2], b[3], bcr], &mut ssn, &mut rsn)
                                            }
                                            _ => continue,
                                        };
                                        let _ = stream.write_all(&asdu);
                                    }
                                }
                                drop(stations_read);
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Some(ref lc) = lc {
                                    lc.try_add(LogEntry::new(
                                        Direction::Tx, FrameLabel::CounterInterrogation,
                                        format!("累计量召唤 激活终止 CA={}", ca),
                                    ));
                                }
                            });
                        }
                    }
                    103 => {
                        let mut ack = data[..n].to_vec(); ack[8] = 7;
                        let _ = stream.write_all(&ack);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::ClockSync,
                                format!("时钟同步确认 CA={}", ca),
                            ));
                        }
                    }
                    45 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sco = data[15]; let value = sco & 0x01 != 0; let is_select = sco & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::SinglePoint) {
                                                dp.value = DataPointValue::SinglePoint { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                        }
                                    });
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::SinglePoint) {
                                                let ca_b = ca.to_le_bytes();
                                                let ioa_b = ioa.to_le_bytes();
                                                let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SingleCommand,
                                    format!("单点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    46 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let dco = data[15]; let value = dco & 0x03; let is_select = dco & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::DoublePoint) {
                                                dp.value = DataPointValue::DoublePoint { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                        }
                                    });
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::DoublePoint) {
                                                let ca_b = ca.to_le_bytes();
                                                let ioa_b = ioa.to_le_bytes();
                                                let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::DoubleCommand,
                                    format!("双点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    47 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let rco = data[15]; let step_val = rco & 0x03; let is_select = rco & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::StepPosition) {
                                                if let DataPointValue::StepPosition { ref mut value, .. } = dp.value {
                                                    match step_val { 1 => { if *value > -64 { *value -= 1; } } 2 => { if *value < 63 { *value += 1; } } _ => {} }
                                                    dp.timestamp = Some(chrono::Utc::now());
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::StepPosition) {
                                                let ca_b = ca.to_le_bytes();
                                                let ioa_b = ioa.to_le_bytes();
                                                let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                let dir = match step_val { 1 => "降", 2 => "升", _ => "?" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::StepCommand,
                                    format!("步调节命令确认 IOA={} {} {} CA={}", ioa, dir, mode, ca),
                                ));
                            }
                        }
                    }
                    48 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let nva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            let value = nva as f32 / 32767.0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::NormalizedMeasured) {
                                                dp.value = DataPointValue::Normalized { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                        }
                                    });
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::NormalizedMeasured) {
                                                let ca_b = ca.to_le_bytes();
                                                let ioa_b = ioa.to_le_bytes();
                                                let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointNormalized,
                                    format!("归一化设定值确认 IOA={} val={:.4} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    49 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::ScaledMeasured) {
                                                dp.value = DataPointValue::Scaled { value: sva };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                        }
                                    });
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::ScaledMeasured) {
                                                let ca_b = ca.to_le_bytes();
                                                let ioa_b = ioa.to_le_bytes();
                                                let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointScaled,
                                    format!("标度化设定值确认 IOA={} val={} {} CA={}", ioa, sva, mode, ca),
                                ));
                            }
                        }
                    }
                    50 => {
                        if data.len() >= 20 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let value = f32::from_le_bytes([data[15], data[16], data[17], data[18]]);
                            let qos = data[19]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::FloatMeasured) {
                                                dp.value = DataPointValue::ShortFloat { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                        }
                                    });
                                }
                            }
                            let mut ack = data[..n].to_vec(); ack[8] = 7;
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let mut term = data[..n].to_vec(); term[8] = 10;
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::FloatMeasured) {
                                                let ca_b = ca.to_le_bytes();
                                                let ioa_b = ioa.to_le_bytes();
                                                let spont = encode_point_frame(&point.value, 3, &ca_b, &ioa_b[..3], &mut ssn, &mut rsn);
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointFloat,
                                    format!("浮点设定值确认 IOA={} val={:.3} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    _ => {
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Rx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                format!("未知 ASDU 类型={} CA={} COT={}", asdu_type, ca, cause),
                            ));
                        }
                    }
                }
            }
        }
    }
    // Clean up the connection entry when the client disconnects.
    rt.block_on(async { connections.write().await.remove(&peer_addr); });
}

fn send_gi_response_blocking(
    stream: &mut native_tls::TlsStream<TcpStream>,
    station: &Station,
) {
    use std::io::Write;
    let ca_bytes = station.common_address.to_le_bytes();
    let mut ssn: u16 = 0;
    let mut rsn: u16 = 0;
    for point in station.data_points.all_sorted() {
        let ioa_bytes = point.ioa.to_le_bytes();
        let asdu = encode_point_frame(&point.value, 20, &ca_bytes, &ioa_bytes[..3], &mut ssn, &mut rsn);
        let _ = stream.write_all(&asdu);
    }
}

// ---------------------------------------------------------------------------
// I-Frame Builder
// ---------------------------------------------------------------------------

fn build_i_frame(
    asdu_type: u8, cause: u8, ca: &[u8], ioa: &[u8], value_bytes: &[u8],
    ssn: &mut u16, rsn: &mut u16,
) -> Vec<u8> {
    let asdu_len = 6 + ioa.len() + value_bytes.len();
    let total_len = 4 + asdu_len;
    let mut frame = Vec::with_capacity(2 + total_len);
    frame.push(0x68);
    frame.push(total_len as u8);
    // 4 APCI control bytes for I-frame:
    // Bytes 2-3: N(S) << 1, 16-bit little-endian (bit 0 = 0 indicates I-frame)
    // Bytes 4-5: N(R) << 1, 16-bit little-endian
    frame.push((*ssn & 0xFF) as u8);
    frame.push(((*ssn >> 8) & 0xFF) as u8);
    frame.push((*rsn & 0xFF) as u8);
    frame.push(((*rsn >> 8) & 0xFF) as u8);
    *ssn = ssn.wrapping_add(2);
    // N(R) is not auto-incremented per sent frame; it tracks the peer's N(S).
    // Leaving rsn unchanged here — it should only be updated when receiving I-frames.
    frame.extend_from_slice(&[asdu_type, 0x01, cause, 0x00]);
    frame.extend_from_slice(&ca[..2]);
    frame.extend_from_slice(ioa);
    frame.extend_from_slice(value_bytes);
    frame
}

/// Encode a data point value into an I-frame with the given COT.
fn encode_point_frame(
    value: &DataPointValue, cot: u8, ca: &[u8], ioa: &[u8],
    ssn: &mut u16, rsn: &mut u16,
) -> Vec<u8> {
    match value {
        DataPointValue::SinglePoint { value } => {
            let siq = if *value { 0x01 } else { 0x00 };
            build_i_frame(1, cot, ca, ioa, &[siq], ssn, rsn)
        }
        DataPointValue::DoublePoint { value } => {
            let diq = *value & 0x03;
            build_i_frame(3, cot, ca, ioa, &[diq], ssn, rsn)
        }
        DataPointValue::StepPosition { value, transient } => {
            let vti = ((*value as u8) & 0x7F) | (if *transient { 0x80 } else { 0 });
            build_i_frame(5, cot, ca, ioa, &[vti, 0], ssn, rsn)
        }
        DataPointValue::Bitstring { value } => {
            let b = value.to_le_bytes();
            build_i_frame(7, cot, ca, ioa, &[b[0], b[1], b[2], b[3], 0], ssn, rsn)
        }
        DataPointValue::Normalized { value } => {
            let nva = (*value * 32767.0) as i16; let b = nva.to_le_bytes();
            build_i_frame(9, cot, ca, ioa, &[b[0], b[1], 0u8], ssn, rsn)
        }
        DataPointValue::Scaled { value } => {
            let b = value.to_le_bytes();
            build_i_frame(11, cot, ca, ioa, &[b[0], b[1], 0u8], ssn, rsn)
        }
        DataPointValue::ShortFloat { value } => {
            let b = value.to_le_bytes();
            build_i_frame(13, cot, ca, ioa, &[b[0], b[1], b[2], b[3], 0u8], ssn, rsn)
        }
        DataPointValue::IntegratedTotal { value, carry, sequence } => {
            let b = value.to_le_bytes();
            let mut bcr = *sequence & 0x1F;
            if *carry { bcr |= 0x20; }
            build_i_frame(15, cot, ca, ioa, &[b[0], b[1], b[2], b[3], bcr], ssn, rsn)
        }
    }
}

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SlaveError {
    #[error("IOA {0} already exists")] DuplicateIoa(u32),
    #[error("IOA {0} not found")] IoaNotFound(u32),
    #[error("station CA={0} already exists")] DuplicateStation(u16),
    #[error("station CA={0} not found")] StationNotFound(u16),
    #[error("server is already running")] AlreadyRunning,
    #[error("server is not running")] NotRunning,
    #[error("bind error: {0}")] BindError(String),
    #[error("TLS error: {0}")] TlsError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_creation() {
        let s = Station::new(1, "测试站");
        assert_eq!(s.common_address, 1);
    }

    #[test]
    fn test_station_with_default_points() {
        let s = Station::with_default_points(1, "站1", 10);
        // 16 ASDU 类型 × 10 IOA = 160；所有类型共享 IOA 1..=10
        assert_eq!(s.data_points.len(), 160);
        // IOA=1 上同时挂着所有 16 种 ASDU 类型
        assert!(s.data_points.get(1, AsduTypeId::MSpNa1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MSpTb1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MDpNa1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MMeNc1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MMeTf1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MItTb1).is_some());
        // 边界 IOA=10 也要存在
        assert!(s.data_points.get(10, AsduTypeId::MSpNa1).is_some());
        assert!(s.data_points.get(10, AsduTypeId::MItTb1).is_some());
        // IOA=11 不应该存在（所有类型只到 10）
        assert!(s.data_points.get(11, AsduTypeId::MSpNa1).is_none());
    }

    #[tokio::test]
    async fn test_slave_server_station_management() {
        let server = SlaveServer::new(SlaveTransportConfig::default());
        let station = Station::new(1, "站1");
        server.add_station(station).await.unwrap();
        assert!(server.add_station(Station::new(1, "重复")).await.is_err());
    }

    #[test]
    fn test_add_point_coexist_different_type() {
        let mut station = Station::new(1, "Test");
        let def_sp = InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: "SP".to_string(),
            comment: String::new(),
        };
        station.add_point(def_sp).unwrap();
        assert_eq!(station.data_points.len(), 1);
        assert_eq!(station.data_points.get(100, AsduTypeId::MSpNa1).unwrap().asdu_type, AsduTypeId::MSpNa1);

        // Add float type at same IOA — should coexist
        let def_float = InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MMeNc1,
            category: DataCategory::FloatMeasured,
            name: "Float".to_string(),
            comment: String::new(),
        };
        station.add_point(def_float).unwrap();
        assert_eq!(station.data_points.len(), 2); // both coexist
        assert!(station.data_points.get(100, AsduTypeId::MSpNa1).is_some());
        assert!(station.data_points.get(100, AsduTypeId::MMeNc1).is_some());
        assert_eq!(station.object_defs.len(), 2);
    }

    #[test]
    fn test_batch_add_points() {
        let mut station = Station::new(1, "Test");
        let added = station.batch_add_points(100, 50, AsduTypeId::MSpNa1, "SP").unwrap();
        assert_eq!(added, 50);
        assert_eq!(station.data_points.len(), 50);

        for i in 0..50u32 {
            let ioa = 100 + i;
            let point = station.data_points.get(ioa, AsduTypeId::MSpNa1).unwrap();
            assert_eq!(point.asdu_type, AsduTypeId::MSpNa1);
        }
        assert_eq!(station.object_defs.len(), 50);
        assert_eq!(station.object_defs[0].name, "SP_100");
        assert_eq!(station.object_defs[49].name, "SP_149");

        // Add different type at same IOA range — should coexist
        station.batch_add_points(100, 50, AsduTypeId::MMeNc1, "FL").unwrap();
        assert_eq!(station.data_points.len(), 100); // 50 SP + 50 FL
        for i in 0..50u32 {
            assert!(station.data_points.get(100 + i, AsduTypeId::MSpNa1).is_some());
            assert!(station.data_points.get(100 + i, AsduTypeId::MMeNc1).is_some());
        }
    }
}
