use crate::state::{AppState, DataPointInfo, ServerInfo, SlaveServerState, StationInfo};
use iec104sim_core::data_point::{DataPointValue, InformationObjectDef};
use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::log_entry::LogEntry;
use iec104sim_core::slave::{SlaveServer, SlaveTransportConfig, Station};
use iec104sim_core::types::AsduTypeId;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------------------
// Event Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerStateEvent {
    pub id: String,
    pub state: String,
}

// ---------------------------------------------------------------------------
// Server Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateServerRequest {
    pub bind_address: Option<String>,
    pub port: u16,
    pub init_mode: Option<String>,
    pub use_tls: Option<bool>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
    pub require_client_cert: Option<bool>,
}

#[tauri::command]
pub async fn create_server(
    state: State<'_, AppState>,
    request: CreateServerRequest,
) -> Result<ServerInfo, String> {
    let id = {
        let mut counter = state.next_server_id.write().await;
        let id = format!("server_{}", *counter);
        *counter += 1;
        id
    };

    let transport = SlaveTransportConfig {
        bind_address: request.bind_address.unwrap_or_else(|| "0.0.0.0".to_string()),
        port: request.port,
        tls: iec104sim_core::slave::SlaveTlsConfig {
            enabled: request.use_tls.unwrap_or(false),
            cert_file: request.cert_file.unwrap_or_default(),
            key_file: request.key_file.unwrap_or_default(),
            ca_file: request.ca_file.unwrap_or_default(),
            require_client_cert: request.require_client_cert.unwrap_or(false),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
        },
    };

    let log_collector = Arc::new(LogCollector::new());
    let server = SlaveServer::new(transport).with_log_collector(log_collector.clone());

    // Auto-create default station (CA=1) with pre-filled data points
    let default_station = match request.init_mode.as_deref() {
        Some("random") => Station::with_random_points(1, "站 1", 10),
        _ => Station::with_default_points(1, "站 1", 10),
    };
    server
        .add_station(default_station)
        .await
        .map_err(|e| format!("failed to add default station: {}", e))?;

    let info = ServerInfo {
        id: id.clone(),
        bind_address: server.transport.bind_address.clone(),
        port: server.transport.port,
        state: format!("{:?}", server.state()),
        station_count: 1,
        use_tls: server.transport.tls.enabled,
    };

    state.servers.write().await.insert(
        id,
        SlaveServerState {
            server,
            log_collector,
        },
    );

    Ok(info)
}

#[tauri::command]
pub async fn start_server(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let state_str: String;
    {
        let mut servers = state.servers.write().await;
        let srv = servers
            .get_mut(&id)
            .ok_or_else(|| format!("server {} not found", id))?;

        srv.server
            .start()
            .await
            .map_err(|e| format!("failed to start: {}", e))?;
        state_str = format!("{:?}", srv.server.state());
    }

    app_handle.emit("server-state-changed", ServerStateEvent {
        id, state: state_str,
    }).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn stop_server(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let state_str: String;
    {
        let mut servers = state.servers.write().await;
        let srv = servers
            .get_mut(&id)
            .ok_or_else(|| format!("server {} not found", id))?;

        srv.server
            .stop()
            .await
            .map_err(|e| format!("failed to stop: {}", e))?;
        state_str = format!("{:?}", srv.server.state());
    }

    app_handle.emit("server-state-changed", ServerStateEvent {
        id, state: state_str,
    }).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn delete_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut servers = state.servers.write().await;
    servers
        .remove(&id)
        .ok_or_else(|| format!("server {} not found", id))?;
    Ok(())
}

#[tauri::command]
pub async fn list_servers(
    state: State<'_, AppState>,
) -> Result<Vec<ServerInfo>, String> {
    let servers = state.servers.read().await;
    let mut result = Vec::new();

    for (id, srv_state) in servers.iter() {
        let station_count = srv_state.server.stations.read().await.len();
        result.push(ServerInfo {
            id: id.clone(),
            bind_address: srv_state.server.transport.bind_address.clone(),
            port: srv_state.server.transport.port,
            state: format!("{:?}", srv_state.server.state()),
            station_count,
            use_tls: srv_state.server.transport.tls.enabled,
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Station Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddStationRequest {
    pub server_id: String,
    pub common_address: u16,
    pub name: String,
    pub init_mode: Option<String>,
}

#[tauri::command]
pub async fn add_station(
    state: State<'_, AppState>,
    request: AddStationRequest,
) -> Result<StationInfo, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let station = match request.init_mode.as_deref() {
        Some("random") => Station::with_random_points(request.common_address, request.name.clone(), 10),
        Some("zero") => Station::with_default_points(request.common_address, request.name.clone(), 10),
        _ => Station::new(request.common_address, request.name.clone()),
    };
    let point_count = station.data_points.len();

    srv.server
        .add_station(station)
        .await
        .map_err(|e| format!("failed to add station: {}", e))?;

    Ok(StationInfo {
        common_address: request.common_address,
        name: request.name,
        point_count,
    })
}

#[tauri::command]
pub async fn remove_station(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    srv.server
        .remove_station(common_address)
        .await
        .map_err(|e| format!("failed to remove station: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn list_stations(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<StationInfo>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let stations = srv.server.stations.read().await;
    let result: Vec<StationInfo> = stations
        .values()
        .map(|s| StationInfo {
            common_address: s.common_address,
            name: s.name.clone(),
            point_count: s.data_points.len(),
        })
        .collect();

    Ok(result)
}

// ---------------------------------------------------------------------------
// Data Point Commands
// ---------------------------------------------------------------------------

fn parse_asdu_type(s: &str) -> Result<AsduTypeId, String> {
    match s {
        "m_sp_na_1" | "MSpNa1" => Ok(AsduTypeId::MSpNa1),
        "m_dp_na_1" | "MDpNa1" => Ok(AsduTypeId::MDpNa1),
        "m_st_na_1" | "MStNa1" => Ok(AsduTypeId::MStNa1),
        "m_bo_na_1" | "MBoNa1" => Ok(AsduTypeId::MBoNa1),
        "m_me_na_1" | "MMeNa1" => Ok(AsduTypeId::MMeNa1),
        "m_me_nb_1" | "MMeNb1" => Ok(AsduTypeId::MMeNb1),
        "m_me_nc_1" | "MMeNc1" => Ok(AsduTypeId::MMeNc1),
        "m_it_na_1" | "MItNa1" => Ok(AsduTypeId::MItNa1),
        _ => Err(format!("unknown ASDU type: {}", s)),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddDataPointRequest {
    pub server_id: String,
    pub common_address: u16,
    pub ioa: u32,
    pub asdu_type: String,
    pub name: Option<String>,
    pub comment: Option<String>,
}

#[tauri::command]
pub async fn add_data_point(
    state: State<'_, AppState>,
    request: AddDataPointRequest,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let asdu_type = parse_asdu_type(&request.asdu_type)?;
    let def = InformationObjectDef {
        ioa: request.ioa,
        asdu_type,
        category: asdu_type.category(),
        name: request.name.unwrap_or_default(),
        comment: request.comment.unwrap_or_default(),
    };

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    station.add_point(def)
        .map_err(|e| format!("failed to add point: {}", e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchAddDataPointsRequest {
    pub server_id: String,
    pub common_address: u16,
    pub start_ioa: u32,
    pub count: u32,
    pub asdu_type: String,
    pub name_prefix: Option<String>,
}

#[tauri::command]
pub async fn batch_add_data_points(
    state: State<'_, AppState>,
    request: BatchAddDataPointsRequest,
) -> Result<u32, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let asdu_type = parse_asdu_type(&request.asdu_type)?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    station
        .batch_add_points(
            request.start_ioa,
            request.count,
            asdu_type,
            request.name_prefix.as_deref().unwrap_or(""),
        )
        .map_err(|e| format!("failed to batch add points: {}", e))
}

#[tauri::command]
pub async fn remove_data_point(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let asdu = parse_asdu_type(&asdu_type)?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    station.remove_point(ioa, asdu)
        .map_err(|e| format!("failed to remove point: {}", e))
}

#[tauri::command]
pub async fn update_data_point(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
    value: String,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let asdu = parse_asdu_type(&asdu_type)?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let point = station.data_points.get_mut(ioa, asdu)
        .ok_or_else(|| format!("IOA {} type {} not found", ioa, asdu_type))?;

    // Parse value based on current type
    let new_value = match &point.value {
        DataPointValue::SinglePoint { .. } => {
            let v = value.parse::<bool>().or_else(|_| {
                match value.as_str() {
                    "1" | "true" | "ON" | "on" => Ok(true),
                    "0" | "false" | "OFF" | "off" => Ok(false),
                    _ => Err(format!("invalid bool: {}", value)),
                }
            }).map_err(|e| format!("{}", e))?;
            DataPointValue::SinglePoint { value: v }
        }
        DataPointValue::DoublePoint { .. } => {
            let v = value.parse::<u8>().map_err(|e| format!("{}", e))?;
            DataPointValue::DoublePoint { value: v }
        }
        DataPointValue::Normalized { .. } => {
            let v = value.parse::<f32>().map_err(|e| format!("{}", e))?;
            DataPointValue::Normalized { value: v }
        }
        DataPointValue::Scaled { .. } => {
            let v = value.parse::<i16>().map_err(|e| format!("{}", e))?;
            DataPointValue::Scaled { value: v }
        }
        DataPointValue::ShortFloat { .. } => {
            let v = value.parse::<f32>().map_err(|e| format!("{}", e))?;
            DataPointValue::ShortFloat { value: v }
        }
        DataPointValue::IntegratedTotal { carry, sequence, .. } => {
            let v = value.parse::<i32>().map_err(|e| format!("{}", e))?;
            DataPointValue::IntegratedTotal { value: v, carry: *carry, sequence: *sequence }
        }
        _ => return Err("unsupported value type".to_string()),
    };

    point.value = new_value;
    point.timestamp = Some(chrono::Utc::now());

    drop(stations);
    srv.server.queue_spontaneous(common_address, &[(ioa, asdu)]).await;

    Ok(())
}

#[tauri::command]
pub async fn list_data_points(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    _category: Option<String>,
) -> Result<Vec<DataPointInfo>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let stations = srv.server.stations.read().await;
    let station = stations
        .get(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let points = station.data_points.all_sorted();
    let defs = &station.object_defs;

    // Build O(1) lookup map instead of O(n) linear search per point
    let def_map: std::collections::HashMap<(u32, AsduTypeId), &InformationObjectDef> = defs.iter()
        .map(|d| ((d.ioa, d.asdu_type), d))
        .collect();

    let result: Vec<DataPointInfo> = points
        .iter()
        .map(|p| {
            let def = def_map.get(&(p.ioa, p.asdu_type));
            DataPointInfo {
                ioa: p.ioa,
                asdu_type: p.asdu_type.name().to_string(),
                category: p.asdu_type.category().name().to_string(),
                name: def.map(|d| d.name.clone()).unwrap_or_default(),
                comment: def.map(|d| d.comment.clone()).unwrap_or_default(),
                value: p.value.display(),
                quality_iv: p.quality.iv,
                timestamp: p.timestamp.map(|t| t.format("%H:%M:%S%.3f").to_string()),
            }
        })
        .collect();

    Ok(result)
}

// ---------------------------------------------------------------------------
// Log Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_communication_logs(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<LogEntry>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    Ok(srv.log_collector.get_all().await)
}

#[tauri::command]
pub async fn clear_communication_logs(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    srv.log_collector.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn export_logs_csv(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<String, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    Ok(srv.log_collector.export_csv().await)
}

// ---------------------------------------------------------------------------
// Simulation Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RandomMutateRequest {
    pub server_id: String,
    pub common_address: u16,
}

#[tauri::command]
pub async fn random_mutate_data_points(
    state: State<'_, AppState>,
    request: RandomMutateRequest,
) -> Result<u32, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    let (mutated, changed_ioas) = {
        let mut rng = rand::rng();
        let mut mutated = 0u32;
        let mut changed_ioas: Vec<(u32, AsduTypeId)> = Vec::new();

        let keys: Vec<(u32, AsduTypeId)> = station.data_points.points.keys().copied().collect();
        let count = (keys.len() * 30 / 100).max(3).min(keys.len());

        let mut pick = keys;
        for i in (1..pick.len()).rev() {
            let j = rng.random_range(0..=i);
            pick.swap(i, j);
        }

        for &(ioa, asdu_type) in &pick[..count] {
            if let Some(point) = station.data_points.get_mut(ioa, asdu_type) {
                point.value = match &point.value {
                    DataPointValue::SinglePoint { value } => {
                        DataPointValue::SinglePoint { value: !value }
                    }
                    DataPointValue::DoublePoint { value } => {
                        DataPointValue::DoublePoint { value: if *value == 1 { 2 } else { 1 } }
                    }
                    DataPointValue::Normalized { value } => {
                        let delta: f32 = rng.random_range(-0.1..0.1);
                        DataPointValue::Normalized { value: (*value + delta).clamp(-1.0, 1.0) }
                    }
                    DataPointValue::Scaled { value } => {
                        let delta: i16 = rng.random_range(-100..100);
                        DataPointValue::Scaled { value: value.saturating_add(delta) }
                    }
                    DataPointValue::ShortFloat { value } => {
                        let delta: f32 = rng.random_range(-10.0..10.0);
                        DataPointValue::ShortFloat { value: value + delta }
                    }
                    DataPointValue::IntegratedTotal { value, carry, sequence } => {
                        let delta: i32 = rng.random_range(0..100);
                        DataPointValue::IntegratedTotal {
                            value: value + delta,
                            carry: *carry,
                            sequence: *sequence,
                        }
                    }
                    other => other.clone(),
                };
                point.timestamp = Some(chrono::Utc::now());
                changed_ioas.push((ioa, asdu_type));
                mutated += 1;
            }
        }
        (mutated, changed_ioas)
    }; // rng dropped here

    drop(stations);
    srv.server.queue_spontaneous(request.common_address, &changed_ioas).await;

    Ok(mutated)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CyclicConfigRequest {
    pub server_id: String,
    pub common_address: u16,
    pub enabled: bool,
    pub interval_ms: u32,
}

#[tauri::command]
pub async fn set_cyclic_config(
    state: State<'_, AppState>,
    request: CyclicConfigRequest,
) -> Result<(), String> {
    use iec104sim_core::slave::CyclicConfig;
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;
    srv.server
        .set_cyclic_config(
            request.common_address,
            CyclicConfig { enabled: request.enabled, interval_ms: request.interval_ms },
        )
        .await
        .map_err(|e| format!("{:?}", e))
}

// ---------------------------------------------------------------------------
// State Persistence Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistedServer {
    pub bind_address: String,
    pub port: u16,
    pub stations: Vec<PersistedStation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistedStation {
    pub common_address: u16,
    pub name: String,
    pub object_defs: Vec<InformationObjectDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistedAppState {
    pub version: u32,
    pub servers: Vec<PersistedServer>,
}

#[tauri::command]
pub async fn export_app_state(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let servers = state.servers.read().await;
    let mut persisted_servers = Vec::new();

    for (_id, srv_state) in servers.iter() {
        let stations = srv_state.server.stations.read().await;
        let mut persisted_stations = Vec::new();

        for (_ca, station) in stations.iter() {
            persisted_stations.push(PersistedStation {
                common_address: station.common_address,
                name: station.name.clone(),
                object_defs: station.object_defs.clone(),
            });
        }

        persisted_servers.push(PersistedServer {
            bind_address: srv_state.server.transport.bind_address.clone(),
            port: srv_state.server.transport.port,
            stations: persisted_stations,
        });
    }

    let app_state = PersistedAppState {
        version: 1,
        servers: persisted_servers,
    };

    serde_json::to_string_pretty(&app_state)
        .map_err(|e| format!("failed to serialize: {}", e))
}

#[tauri::command]
pub async fn import_app_state(
    state: State<'_, AppState>,
    input: PersistedAppState,
) -> Result<usize, String> {
    if input.version != 1 {
        return Err(format!("unsupported state version: {}", input.version));
    }

    let mut total_stations = 0;

    for srv_input in input.servers {
        let id = {
            let mut counter = state.next_server_id.write().await;
            let id = format!("server_{}", *counter);
            *counter += 1;
            id
        };

        let transport = SlaveTransportConfig {
            bind_address: srv_input.bind_address,
            port: srv_input.port,
            tls: Default::default(),
        };

        let log_collector = Arc::new(LogCollector::new());
        let server = SlaveServer::new(transport).with_log_collector(log_collector.clone());

        for station_input in srv_input.stations {
            let mut station = Station::new(station_input.common_address, station_input.name);
            for def in station_input.object_defs {
                let _ = station.add_point(def);
            }
            let _ = server.add_station(station).await;
            total_stations += 1;
        }

        state.servers.write().await.insert(
            id,
            SlaveServerState {
                server,
                log_collector,
            },
        );
    }

    Ok(total_stations)
}

#[tauri::command]
pub async fn clear_app_state(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.servers.write().await.clear();
    *state.next_server_id.write().await = 0;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tool Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn parse_hex(data: String) -> Result<Vec<u8>, String> {
    iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn parse_apci(data: String) -> Result<String, String> {
    let bytes = iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))?;
    let frame = iec104sim_core::frame::parse_apci(&bytes)
        .map_err(|e| format!("{}", e))?;
    Ok(iec104sim_core::frame::format_frame_summary(&frame))
}
