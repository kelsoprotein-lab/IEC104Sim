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
            // Server commands
            commands::create_server,
            commands::start_server,
            commands::stop_server,
            commands::delete_server,
            commands::list_servers,
            // Station commands
            commands::add_station,
            commands::remove_station,
            commands::list_stations,
            // Data point commands
            commands::add_data_point,
            commands::batch_add_data_points,
            commands::remove_data_point,
            commands::update_data_point,
            commands::list_data_points,
            // Log commands
            commands::get_communication_logs,
            commands::clear_communication_logs,
            commands::export_logs_csv,
            // Simulation commands
            commands::random_mutate_data_points,
            commands::set_cyclic_config,
            // State persistence commands
            commands::export_app_state,
            commands::import_app_state,
            commands::clear_app_state,
            // Tool commands
            commands::parse_hex,
            commands::parse_apci,
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
