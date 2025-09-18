// Modules
pub mod long_path;
pub mod path_utils;
pub mod logging;
pub mod settings;
pub mod commands;
pub mod profiles;
pub mod virtual_fs;
pub mod blob_cache;
pub mod workspace_watcher;
pub mod runtime_planner;
pub mod runtime_builder;

use commands::SettingsState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // Initialize logging first
  if let Err(e) = logging::init_logging() {
    eprintln!("Failed to initialize logging: {}", e);
  }
  
  // Log startup information
  logging::log_startup_info();
  
  tauri::Builder::default()
    .manage(SettingsState::new(None))
    .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::needs_wizard,
            commands::validate_gta_base_path,
            commands::get_drive_info,
            commands::create_data_structure,
            commands::validate_settings,
            commands::get_settings,
            commands::open_data_root,
            commands::open_gta_base,
            commands::pick_directory,
            commands::create_profile,
            commands::list_profiles,
            commands::rename_profile,
            commands::delete_profile,
            commands::open_profile_workspace,
            commands::get_virtual_file_tree,
            commands::revert_to_original,
            commands::copy_to_workspace,
            commands::delete_workspace_file,
            commands::debug_blob_cache,
            commands::compute_runtime_plan,
            commands::build_runtime,
            commands::get_runtime_plan,
            commands::cleanup_temp_runtimes
        ])
    .setup(|_app| {
      // Setup complete - our logging is already initialized
      tracing::info!("Tauri app setup complete");
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
    
  // Log shutdown information
  logging::log_shutdown_info();
}
