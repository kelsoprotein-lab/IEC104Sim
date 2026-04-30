mod commands;
mod state;
pub mod update;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // Connection commands
            commands::create_connection,
            commands::connect_master,
            commands::disconnect_master,
            commands::delete_connection,
            commands::list_connections,
            // IEC 104 commands
            commands::send_interrogation,
            commands::send_clock_sync,
            commands::send_counter_read,
            commands::send_control_command,
            // Data commands
            commands::get_received_data,
            commands::get_received_data_since,
            // Log commands
            commands::get_communication_logs,
            commands::clear_communication_logs,
            commands::export_logs_csv,
            commands::set_logging_enabled,
            // Tool commands
            commands::parse_hex,
            commands::parse_frame_full,
            // Update commands
            update::check_for_update,
            update::install_update,
            update::snooze_update,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
