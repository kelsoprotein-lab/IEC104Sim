use crate::state::{AppState, ConnectionInfo, IncrementalDataResponse, MasterConnectionState, ReceivedDataPointInfo};
use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::log_entry::LogEntry;
use iec104sim_core::master::{ControlResult, ControlStep, MasterConfig, MasterConnection, TlsConfig, TlsVersionPolicy};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------------------
// Event Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionStateEvent {
    pub id: String,
    pub state: String,
}

// ---------------------------------------------------------------------------
// Connection Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateConnectionRequest {
    pub target_address: String,
    pub port: u16,
    pub common_address: Option<u16>,
    pub timeout_ms: Option<u64>,
    /// TLS configuration
    pub use_tls: Option<bool>,
    pub ca_file: Option<String>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub accept_invalid_certs: Option<bool>,
    /// TLS version policy: "auto" | "tls12_only" | "tls13_only" (default: "auto")
    pub tls_version: Option<String>,
}

#[tauri::command]
pub async fn create_connection(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: CreateConnectionRequest,
) -> Result<ConnectionInfo, String> {
    let id = {
        let mut counter = state.next_connection_id.write().await;
        let id = format!("conn_{}", *counter);
        *counter += 1;
        id
    };

    let config = MasterConfig {
        target_address: request.target_address.clone(),
        port: request.port,
        common_address: request.common_address.unwrap_or(1),
        timeout_ms: request.timeout_ms.unwrap_or(3000),
        tls: TlsConfig {
            enabled: request.use_tls.unwrap_or(false),
            ca_file: request.ca_file.unwrap_or_default(),
            cert_file: request.cert_file.unwrap_or_default(),
            key_file: request.key_file.unwrap_or_default(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: request.accept_invalid_certs.unwrap_or(false),
            version: match request.tls_version.as_deref() {
                Some("tls12_only") => TlsVersionPolicy::Tls12Only,
                Some("tls13_only") => TlsVersionPolicy::Tls13Only,
                _ => TlsVersionPolicy::Auto,
            },
        },
    };

    let log_collector = Arc::new(LogCollector::new());
    let connection = MasterConnection::new(config.clone())
        .with_log_collector(log_collector.clone());

    // Forward state-change notifications from the core connection to the frontend.
    // Exits when the connection is dropped (`delete_connection`) and its `state_tx` closes.
    let mut state_rx = connection.subscribe_state();
    let id_for_task = id.clone();
    let app_handle_for_task = app_handle.clone();
    tokio::spawn(async move {
        while state_rx.changed().await.is_ok() {
            let new_state = *state_rx.borrow_and_update();
            let _ = app_handle_for_task.emit(
                "connection-state",
                ConnectionStateEvent {
                    id: id_for_task.clone(),
                    state: format!("{:?}", new_state),
                },
            );
        }
    });

    let use_tls = config.tls.enabled;
    let info = ConnectionInfo {
        id: id.clone(),
        target_address: config.target_address,
        port: config.port,
        common_address: config.common_address,
        state: format!("{:?}", connection.state()),
        use_tls,
    };

    state.connections.write().await.insert(
        id,
        MasterConnectionState {
            connection,
            log_collector,
        },
    );

    Ok(info)
}

// `connection-state` events are emitted by the watcher spawned in
// `create_connection`, driven by the core's state channel. These commands
// therefore do not need to emit manually.

#[tauri::command]
pub async fn connect_master(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut connections = state.connections.write().await;
    let conn = connections
        .get_mut(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .connect()
        .await
        .map_err(|e| format!("failed to connect: {}", e))
}

#[tauri::command]
pub async fn disconnect_master(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut connections = state.connections.write().await;
    let conn = connections
        .get_mut(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .disconnect()
        .await
        .map_err(|e| format!("failed to disconnect: {}", e))
}

#[tauri::command]
pub async fn delete_connection(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut connections = state.connections.write().await;
    connections
        .remove(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;
    Ok(())
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionInfo>, String> {
    let connections = state.connections.read().await;
    let mut result = Vec::new();

    for (id, conn_state) in connections.iter() {
        result.push(ConnectionInfo {
            id: id.clone(),
            target_address: conn_state.connection.config.target_address.clone(),
            port: conn_state.connection.config.port,
            common_address: conn_state.connection.config.common_address,
            state: format!("{:?}", conn_state.connection.state()),
            use_tls: conn_state.connection.config.tls.enabled,
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// IEC 104 Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn send_interrogation(
    state: State<'_, AppState>,
    id: String,
    common_address: u16,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .send_interrogation(common_address)
        .await
        .map_err(|e| format!("failed to send GI: {}", e))
}

#[tauri::command]
pub async fn send_clock_sync(
    state: State<'_, AppState>,
    id: String,
    common_address: u16,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .send_clock_sync(common_address)
        .await
        .map_err(|e| format!("failed to send clock sync: {}", e))
}

#[tauri::command]
pub async fn send_counter_read(
    state: State<'_, AppState>,
    id: String,
    common_address: u16,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .send_counter_read(common_address)
        .await
        .map_err(|e| format!("failed to send counter read: {}", e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ControlCommandRequest {
    pub connection_id: String,
    pub ioa: u32,
    pub common_address: u16,
    pub command_type: String,
    pub value: String,
    pub select: Option<bool>,
}

#[tauri::command]
pub async fn send_control_command(
    state: State<'_, AppState>,
    request: ControlCommandRequest,
) -> Result<ControlResult, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&request.connection_id)
        .ok_or_else(|| format!("connection {} not found", request.connection_id))?;

    let select = request.select.unwrap_or(false);
    let ca = request.common_address;
    let ioa = request.ioa;

    // Direct execute: send command and return immediately
    if !select {
        let start = std::time::Instant::now();
        match request.command_type.as_str() {
            "single" => {
                let value = parse_bool(&request.value)?;
                conn.connection.send_single_command(ioa, value, false, ca).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "double" => {
                let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
                conn.connection.send_double_command(ioa, value, false, ca).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "step" => {
                let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
                conn.connection.send_step_command(ioa, value, false, ca).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "setpoint_normalized" => {
                let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_normalized(ioa, value, false, ca).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "setpoint_scaled" => {
                let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_scaled(ioa, value, false, ca).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "setpoint_float" => {
                let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_float(ioa, value, false, ca).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            _ => return Err(format!("unknown command type: {}", request.command_type)),
        }
        return Ok(ControlResult {
            steps: vec![ControlStep {
                action: "execute_sent".to_string(),
                timestamp: chrono::Utc::now().format("%H:%M:%S%.3f").to_string(),
            }],
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // SbO mode: delegate to send_control_with_sbo
    // Build select and execute frames via the command-specific logic
    use iec104sim_core::log_entry::FrameLabel;

    match request.command_type.as_str() {
        "single" => {
            let value = parse_bool(&request.value)?;
            let select_frame = build_control_frames_single(ca, ioa, value, true);
            let execute_frame = build_control_frames_single(ca, ioa, value, false);
            conn.connection.send_control_with_sbo(
                select_frame, execute_frame, ioa,
                &format!("单点命令 IOA={} val={}", ioa, value),
                FrameLabel::SingleCommand, ca,
            ).await.map_err(|e| format!("{}", e))
        }
        "double" => {
            let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_double(ca, ioa, value, true);
            let execute_frame = build_control_frames_double(ca, ioa, value, false);
            conn.connection.send_control_with_sbo(
                select_frame, execute_frame, ioa,
                &format!("双点命令 IOA={} val={}", ioa, value),
                FrameLabel::DoubleCommand, ca,
            ).await.map_err(|e| format!("{}", e))
        }
        "step" => {
            let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_step(ca, ioa, value, true);
            let execute_frame = build_control_frames_step(ca, ioa, value, false);
            conn.connection.send_control_with_sbo(
                select_frame, execute_frame, ioa,
                &format!("步调节命令 IOA={} val={}", ioa, value),
                FrameLabel::StepCommand, ca,
            ).await.map_err(|e| format!("{}", e))
        }
        "setpoint_normalized" => {
            let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_norm(ca, ioa, value, true);
            let execute_frame = build_control_frames_setpoint_norm(ca, ioa, value, false);
            conn.connection.send_control_with_sbo(
                select_frame, execute_frame, ioa,
                &format!("归一化设定值 IOA={} val={:.4}", ioa, value),
                FrameLabel::SetpointNormalized, ca,
            ).await.map_err(|e| format!("{}", e))
        }
        "setpoint_scaled" => {
            let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_scaled(ca, ioa, value, true);
            let execute_frame = build_control_frames_setpoint_scaled(ca, ioa, value, false);
            conn.connection.send_control_with_sbo(
                select_frame, execute_frame, ioa,
                &format!("标度化设定值 IOA={} val={}", ioa, value),
                FrameLabel::SetpointScaled, ca,
            ).await.map_err(|e| format!("{}", e))
        }
        "setpoint_float" => {
            let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_float(ca, ioa, value, true);
            let execute_frame = build_control_frames_setpoint_float(ca, ioa, value, false);
            conn.connection.send_control_with_sbo(
                select_frame, execute_frame, ioa,
                &format!("浮点设定值 IOA={} val={:.3}", ioa, value),
                FrameLabel::SetpointFloat, ca,
            ).await.map_err(|e| format!("{}", e))
        }
        _ => Err(format!("unknown command type: {}", request.command_type)),
    }
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s {
        "1" | "true" | "ON" => Ok(true),
        "0" | "false" | "OFF" => Ok(false),
        _ => s.parse::<bool>().map_err(|_| format!("invalid bool: {}", s)),
    }
}

// Frame builders for SbO (need raw frames before SSN/RSN patching)
fn build_control_frames_single(ca: u16, ioa: u32, value: bool, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut sco = if value { 0x01 } else { 0x00 };
    if select { sco |= 0x80; }
    vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00, 45, 0x01, 6, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2], sco]
}

fn build_control_frames_double(ca: u16, ioa: u32, value: u8, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut dco = value & 0x03;
    if select { dco |= 0x80; }
    vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00, 46, 0x01, 6, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2], dco]
}

fn build_control_frames_step(ca: u16, ioa: u32, value: u8, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut rco = value & 0x03;
    if select { rco |= 0x80; }
    vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00, 47, 0x01, 6, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2], rco]
}

fn build_control_frames_setpoint_norm(ca: u16, ioa: u32, value: f32, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva = (value * 32767.0) as i16;
    let nva_bytes = nva.to_le_bytes();
    let qos = if select { 0x80 } else { 0x00 };
    vec![0x68, 0x10, 0x00, 0x00, 0x00, 0x00, 48, 0x01, 6, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
         nva_bytes[0], nva_bytes[1], qos]
}

fn build_control_frames_setpoint_scaled(ca: u16, ioa: u32, value: i16, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let sva_bytes = value.to_le_bytes();
    let qos = if select { 0x80 } else { 0x00 };
    vec![0x68, 0x10, 0x00, 0x00, 0x00, 0x00, 49, 0x01, 6, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
         sva_bytes[0], sva_bytes[1], qos]
}

fn build_control_frames_setpoint_float(ca: u16, ioa: u32, value: f32, select: bool) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let val_bytes = value.to_le_bytes();
    let qos = if select { 0x80 } else { 0x00 };
    vec![0x68, 0x12, 0x00, 0x00, 0x00, 0x00, 50, 0x01, 6, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
         val_bytes[0], val_bytes[1], val_bytes[2], val_bytes[3], qos]
}

// ---------------------------------------------------------------------------
// Data Commands
// ---------------------------------------------------------------------------

fn point_to_info(p: &iec104sim_core::data_point::DataPoint) -> ReceivedDataPointInfo {
    ReceivedDataPointInfo {
        ioa: p.ioa,
        asdu_type: p.asdu_type.name().to_string(),
        category: p.asdu_type.category().name().to_string(),
        value: p.value.display(),
        quality_iv: p.quality.iv,
        timestamp: p.timestamp.map(|t| t.format("%H:%M:%S%.3f").to_string()),
        update_seq: p.update_seq,
    }
}

#[tauri::command]
pub async fn get_received_data(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<ReceivedDataPointInfo>, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    let data = conn.connection.received_data.read().await;
    let result: Vec<ReceivedDataPointInfo> = data
        .all_sorted()
        .iter()
        .map(|p| point_to_info(p))
        .collect();

    Ok(result)
}

#[tauri::command]
pub async fn get_received_data_since(
    state: State<'_, AppState>,
    id: String,
    since_seq: u64,
) -> Result<IncrementalDataResponse, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    let data = conn.connection.received_data.read().await;
    let points: Vec<ReceivedDataPointInfo> = data
        .changed_since(since_seq)
        .iter()
        .map(|p| point_to_info(p))
        .collect();

    Ok(IncrementalDataResponse {
        seq: data.current_seq(),
        total_count: data.len(),
        points,
    })
}

// ---------------------------------------------------------------------------
// Log Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<LogEntry>, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    Ok(conn.log_collector.get_all().await)
}

#[tauri::command]
pub async fn clear_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    conn.log_collector.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn export_logs_csv(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<String, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    Ok(conn.log_collector.export_csv().await)
}
