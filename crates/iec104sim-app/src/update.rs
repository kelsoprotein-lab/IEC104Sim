use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::UpdaterExt;

const STORE_FILE: &str = "update_state.json";
const KEY_LAST_CHECK: &str = "last_check_at";
const KEY_SNOOZED_VER: &str = "snoozed_version";
const KEY_SNOOZED_UNTIL: &str = "snoozed_until";
const THROTTLE_HOURS: i64 = 6;
const SNOOZE_HOURS: i64 = 24;

#[derive(Serialize, Clone)]
pub struct UpdateMeta {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<String>,
}

fn read_str(app: &AppHandle, key: &str) -> Option<String> {
    let store = app.store(STORE_FILE).ok()?;
    store.get(key).and_then(|v| v.as_str().map(String::from))
}

fn write_str(app: &AppHandle, key: &str, value: &str) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(key, serde_json::Value::String(value.to_string()));
        let _ = store.save();
    }
}

fn parse_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateMeta>, String> {
    let now = Utc::now();
    let last = parse_ts(read_str(&app, KEY_LAST_CHECK));
    if !should_check(last, now, Duration::hours(THROTTLE_HOURS)) {
        return Ok(None);
    }
    write_str(&app, KEY_LAST_CHECK, &now.to_rfc3339());

    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = match updater.check().await {
        Ok(u) => u,
        Err(e) => {
            log::warn!("update check failed: {e}");
            return Ok(None);
        }
    };
    let Some(update) = update else { return Ok(None) };

    let snoozed_v = read_str(&app, KEY_SNOOZED_VER);
    let snoozed_u = parse_ts(read_str(&app, KEY_SNOOZED_UNTIL));
    if is_snoozed(snoozed_v.as_deref(), snoozed_u, &update.version, now) {
        return Ok(None);
    }

    Ok(Some(UpdateMeta {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        pub_date: update.date.map(|d| d.to_string()),
    }))
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update available".to_string())?;

    let mut downloaded: u64 = 0;
    let app_clone = app.clone();
    update
        .download_and_install(
            move |chunk_len, content_len| {
                downloaded += chunk_len as u64;
                if let Some(total) = content_len {
                    let pct = (downloaded as f64 / total as f64 * 100.0).round() as u32;
                    let _ = app_clone.emit("update-progress", pct);
                }
            },
            || {
                log::info!("update downloaded, installing");
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    app.restart();
}

#[tauri::command]
pub fn snooze_update(app: AppHandle, version: String) -> Result<(), String> {
    let until = Utc::now() + Duration::hours(SNOOZE_HOURS);
    write_str(&app, KEY_SNOOZED_VER, &version);
    write_str(&app, KEY_SNOOZED_UNTIL, &until.to_rfc3339());
    Ok(())
}

pub fn should_check(
    last_check: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    throttle: Duration,
) -> bool {
    match last_check {
        None => true,
        Some(last) => now - last >= throttle,
    }
}

pub fn is_snoozed(
    snoozed_version: Option<&str>,
    snoozed_until: Option<DateTime<Utc>>,
    remote_version: &str,
    now: DateTime<Utc>,
) -> bool {
    match (snoozed_version, snoozed_until) {
        (Some(v), Some(until)) => v == remote_version && now < until,
        _ => false,
    }
}
