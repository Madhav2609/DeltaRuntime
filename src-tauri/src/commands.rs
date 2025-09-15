use tauri::State;
use std::sync::Mutex;
use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::settings::{Settings, ValidationResult};
use crate::path_utils::{get_drive_letter, is_ntfs_volume, get_free_space, format_size};
use tracing::info;

/// Application state for settings
pub type SettingsState = Mutex<Option<Settings>>;

/// Response for drive validation
#[derive(Debug, Serialize, Deserialize)]
pub struct DriveInfo {
    pub drive_letter: Option<char>,
    pub is_ntfs: bool,
    pub free_space_bytes: u64,
    pub free_space_formatted: String,
    pub is_valid: bool,
    pub error_message: Option<String>,
}

/// Response for path validation
#[derive(Debug, Serialize, Deserialize)]
pub struct PathValidation {
    pub is_valid: bool,
    pub exists: bool,
    pub is_directory: bool,
    pub has_gta_exe: bool,
    pub error_message: Option<String>,
}

/// Response for settings validation
#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsValidation {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl From<ValidationResult> for SettingsValidation {
    fn from(result: ValidationResult) -> Self {
        Self {
            is_valid: result.is_valid(),
            errors: result.errors,
            warnings: result.warnings,
        }
    }
}

/// Load existing settings if available
#[tauri::command]
pub async fn load_settings(state: State<'_, SettingsState>) -> Result<Option<Settings>, String> {
    info!("Loading settings...");
    
    // Try to load existing settings
    if let Some(existing) = Settings::try_load_existing() {
        let mut settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
        *settings_guard = Some(existing.clone());
        info!("Existing settings loaded");
        return Ok(Some(existing));
    }
    
    info!("No existing settings found");
    Ok(None)
}

/// Check if wizard needs to be shown
#[tauri::command]
pub async fn needs_wizard(state: State<'_, SettingsState>) -> Result<bool, String> {
    let settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    
    match &*settings_guard {
        Some(settings) => Ok(settings.needs_wizard()),
        None => Ok(true), // No settings means we need the wizard
    }
}

/// Validate a GTA:SA base path
#[tauri::command]
pub async fn validate_gta_base_path(path: String) -> Result<PathValidation, String> {
    info!("Validating GTA base path: {}", path);
    
    let path_buf = PathBuf::from(&path);
    let exists = path_buf.exists();
    let is_directory = path_buf.is_dir();
    let has_gta_exe = path_buf.join("gta_sa.exe").exists();
    
    let is_valid = exists && is_directory && has_gta_exe;
    let error_message = if !exists {
        Some("Path does not exist".to_string())
    } else if !is_directory {
        Some("Path is not a directory".to_string())
    } else if !has_gta_exe {
        Some("gta_sa.exe not found in this directory".to_string())
    } else {
        None
    };
    
    Ok(PathValidation {
        is_valid,
        exists,
        is_directory,
        has_gta_exe,
        error_message,
    })
}

/// Get drive information for a path
#[tauri::command]
pub async fn get_drive_info(path: String) -> Result<DriveInfo, String> {
    info!("Getting drive info for path: {}", path);
    
    let path_buf = PathBuf::from(&path);
    
    let drive_letter = get_drive_letter(&path_buf)
        .map_err(|e| format!("Failed to get drive letter: {}", e))?;
    
    let is_ntfs = if drive_letter.is_some() {
        is_ntfs_volume(&path_buf)
            .map_err(|e| format!("Failed to check NTFS: {}", e))?
    } else {
        false
    };
    
    let free_space_bytes = get_free_space(&path_buf)
        .unwrap_or(0);
    
    let free_space_formatted = format_size(free_space_bytes);
    
    let is_valid = drive_letter.is_some() && is_ntfs && free_space_bytes > 1024 * 1024 * 100; // At least 100MB
    let error_message = if drive_letter.is_none() {
        Some("Could not determine drive letter".to_string())
    } else if !is_ntfs {
        Some("Drive is not NTFS (required for hardlinks)".to_string())
    } else if free_space_bytes < 1024 * 1024 * 100 {
        Some("Insufficient free space (at least 100MB required)".to_string())
    } else {
        None
    };
    
    Ok(DriveInfo {
        drive_letter,
        is_ntfs,
        free_space_bytes,
        free_space_formatted,
        is_valid,
        error_message,
    })
}

/// Create data root directory structure
#[tauri::command]
pub async fn create_data_structure(base_path: String, data_root: String, state: State<'_, SettingsState>) -> Result<(), String> {
    info!("Creating data structure - Base: {}, Data: {}", base_path, data_root);
    
    let base_path_buf = PathBuf::from(base_path);
    let data_root_buf = PathBuf::from(data_root);
    
    // Create settings
    let mut settings = Settings::for_wizard(base_path_buf, data_root_buf);
    
    // Create directory structure
    settings.create_data_structure()
        .map_err(|e| format!("Failed to create data structure: {}", e))?;
    
    // Mark wizard as completed
    settings.complete_wizard();
    
    // Save settings
    settings.save_to_data_root()
        .map_err(|e| format!("Failed to save settings: {}", e))?;
    
    // Update state
    let mut settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    *settings_guard = Some(settings);
    
    info!("Data structure created and settings saved successfully");
    Ok(())
}

/// Validate current settings
#[tauri::command]
pub async fn validate_settings(state: State<'_, SettingsState>) -> Result<SettingsValidation, String> {
    let settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    
    match &*settings_guard {
        Some(settings) => {
            let validation = settings.validate()
                .map_err(|e| format!("Validation failed: {}", e))?;
            Ok(validation.into())
        }
        None => Ok(SettingsValidation {
            is_valid: false,
            errors: vec!["No settings loaded".to_string()],
            warnings: vec![],
        })
    }
}

/// Get current settings
#[tauri::command]
pub async fn get_settings(state: State<'_, SettingsState>) -> Result<Option<Settings>, String> {
    let settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    Ok(settings_guard.clone())
}

/// Open data root directory in file explorer
#[tauri::command]
pub async fn open_data_root(state: State<'_, SettingsState>) -> Result<(), String> {
    let settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    
    match &*settings_guard {
        Some(settings) => {
            let data_root = &settings.data_root;
            info!("Opening data root in explorer: {}", data_root.display());
            
            #[cfg(windows)]
            {
                std::process::Command::new("explorer")
                    .arg(data_root)
                    .spawn()
                    .map_err(|e| format!("Failed to open explorer: {}", e))?;
            }
            
            #[cfg(not(windows))]
            {
                return Err("Opening file explorer is only supported on Windows".to_string());
            }
            
            Ok(())
        }
        None => Err("No settings loaded".to_string()),
    }
}

/// Open GTA base directory in file explorer
#[tauri::command]
pub async fn open_gta_base(state: State<'_, SettingsState>) -> Result<(), String> {
    let settings_guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    
    match &*settings_guard {
        Some(settings) => {
            let base_path = &settings.base_path;
            info!("Opening GTA base in explorer: {}", base_path.display());
            
            #[cfg(windows)]
            {
                std::process::Command::new("explorer")
                    .arg(base_path)
                    .spawn()
                    .map_err(|e| format!("Failed to open explorer: {}", e))?;
            }
            
            #[cfg(not(windows))]
            {
                return Err("Opening file explorer is only supported on Windows".to_string());
            }
            
            Ok(())
        }
        None => Err("No settings loaded".to_string()),
    }
}

/// Get directory picker dialog (placeholder - will be implemented with tauri-plugin-dialog)
#[tauri::command]
pub async fn pick_directory(title: String) -> Result<Option<String>, String> {
    use std::ptr;
    use windows::Win32::UI::Shell::{SHBrowseForFolderW, SHGetPathFromIDListW, BROWSEINFOW, BIF_RETURNONLYFSDIRS, BIF_NEWDIALOGSTYLE};
    use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
    use windows::Win32::Foundation::{HWND, MAX_PATH};
    use windows::core::PCWSTR;
    
    unsafe {
        // Initialize COM
        let _ = CoInitialize(Some(ptr::null()));
        
        let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        
        let mut bi = BROWSEINFOW {
            hwndOwner: HWND::default(),
            pidlRoot: ptr::null_mut(),
            pszDisplayName: windows::core::PWSTR::null(),
            lpszTitle: PCWSTR(title_wide.as_ptr()),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: None,
            lParam: windows::Win32::Foundation::LPARAM(0),
            iImage: 0,
        };
        
        let pidl = SHBrowseForFolderW(&mut bi);
        
        if pidl.is_null() {
            CoUninitialize();
            return Ok(None);
        }
        
        let mut path: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];
        let result = SHGetPathFromIDListW(pidl, &mut path);
        
        CoUninitialize();
        
        if result.as_bool() {
            let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            let path_string = String::from_utf16(&path[..end])
                .map_err(|e| format!("Failed to convert path: {}", e))?;
            Ok(Some(path_string))
        } else {
            Err("Failed to get path from dialog".to_string())
        }
    }
}