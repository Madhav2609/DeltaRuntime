use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Result, Context};
use std::fs;
use crate::path_utils::{get_drive_letter, is_ntfs_volume, get_free_space, format_size};
use tracing::{info, warn};

/// Application settings schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Schema version for migration support
    pub schema: u32,
    
    /// Path to the clean game base installation
    pub base_path: PathBuf,
    
    /// Root directory for all runtime data
    pub data_root: PathBuf,
    
    /// Overlay mode (currently only "hardlink" supported)
    pub overlay_mode: String,
    
    /// Settings for the first-run wizard
    #[serde(default)]
    pub wizard: WizardSettings,
    
    /// Application preferences
    #[serde(default)]
    pub preferences: UserPreferences,
}

/// First-run wizard settings and state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WizardSettings {
    /// Whether the wizard has been completed
    pub completed: bool,
    
    /// Timestamp of when the wizard was completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Version of the wizard that was completed
    pub wizard_version: Option<String>,
}

/// User preferences and application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Whether to show debug information in the UI
    pub show_debug_info: bool,
    
    /// Number of days to keep log files
    pub log_retention_days: u64,
    
    /// Whether to automatically check for updates
    pub auto_check_updates: bool,
    
    /// Maximum number of runtime builds to keep
    pub max_runtime_builds: u32,
    
    /// Whether to show file operation progress
    pub show_progress: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            show_debug_info: cfg!(debug_assertions),
            log_retention_days: 30,
            auto_check_updates: true,
            max_runtime_builds: 5,
            show_progress: true,
        }
    }
}

impl Settings {
    /// Current schema version
    pub const CURRENT_SCHEMA: u32 = 1;
    
    /// Default settings file name
    pub const SETTINGS_FILE: &'static str = "settings.json";

    /// Create new default settings
    pub fn new() -> Self {
        Self {
            schema: Self::CURRENT_SCHEMA,
            base_path: PathBuf::new(),
            data_root: PathBuf::new(),
            overlay_mode: "hardlink".to_string(),
            wizard: WizardSettings::default(),
            preferences: UserPreferences::default(),
        }
    }

    /// Create settings for the first-run wizard
    pub fn for_wizard(base_path: PathBuf, data_root: PathBuf) -> Self {
        let mut settings = Self::new();
        settings.base_path = base_path;
        settings.data_root = data_root;
        settings
    }

    /// Load settings from file
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!("Loading settings from: {}", path.display());
        
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read settings file: {}", path.display()))?;
        
        let mut settings: Settings = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse settings file: {}", path.display()))?;
        
        // Migrate settings if needed
        settings = settings.migrate()?;
        
        info!("Settings loaded successfully");
        info!("Base path: {}", settings.base_path.display());
        info!("Data root: {}", settings.data_root.display());
        
        Ok(settings)
    }

    /// Save settings to file
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        info!("Saving settings to: {}", path.display());
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create settings directory: {}", parent.display()))?;
        }
        
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize settings")?;
        
        fs::write(path, content)
            .with_context(|| format!("Failed to write settings file: {}", path.display()))?;
        
        info!("Settings saved successfully");
        Ok(())
    }

    /// Load settings from the data root directory
    pub fn load_from_data_root<P: AsRef<std::path::Path>>(data_root: P) -> Result<Self> {
        let settings_path = data_root.as_ref().join(Self::SETTINGS_FILE);
        Self::load(settings_path)
    }

    /// Save settings to the data root directory
    pub fn save_to_data_root(&self) -> Result<()> {
        let settings_path = self.data_root.join(Self::SETTINGS_FILE);
        self.save(settings_path)
    }

    /// Try to find and load existing settings
    pub fn try_load_existing() -> Option<Self> {
        // Try common locations for existing settings
        let possible_locations = vec![
            std::env::current_dir().ok()?.join("DeltaRuntime").join(Self::SETTINGS_FILE),
            PathBuf::from("C:\\DeltaRuntime").join(Self::SETTINGS_FILE),
            PathBuf::from("D:\\DeltaRuntime").join(Self::SETTINGS_FILE),
            PathBuf::from("E:\\DeltaRuntime").join(Self::SETTINGS_FILE),
        ];

        for location in possible_locations {
            if location.exists() {
                match Self::load(&location) {
                    Ok(settings) => {
                        info!("Found existing settings at: {}", location.display());
                        return Some(settings);
                    }
                    Err(e) => {
                        warn!("Failed to load settings from {}: {}", location.display(), e);
                    }
                }
            }
        }

        None
    }

    /// Migrate settings from older schema versions
    fn migrate(mut self) -> Result<Self> {
        if self.schema < Self::CURRENT_SCHEMA {
            info!("Migrating settings from schema {} to {}", self.schema, Self::CURRENT_SCHEMA);
            
            // Add migration logic here as schema evolves
            match self.schema {
                // Future migrations would go here
                _ => {}
            }
            
            self.schema = Self::CURRENT_SCHEMA;
            info!("Settings migration completed");
        }
        
        Ok(self)
    }

    /// Validate that the settings are consistent and paths exist
    pub fn validate(&self) -> Result<ValidationResult> {
        let mut result = ValidationResult::new();

        // Validate base path
        if !self.base_path.exists() {
            result.add_error(format!("Base game path does not exist: {}", self.base_path.display()));
        } else if !self.base_path.is_dir() {
            result.add_error(format!("Base path is not a directory: {}", self.base_path.display()));
        } else {
            // Check for game executable (GTA:SA as example)
            let gta_exe = self.base_path.join("gta_sa.exe");
            if !gta_exe.exists() {
                result.add_warning(format!("Game executable not found at: {}", gta_exe.display()));
            }
        }

        // Validate data root
        if !self.data_root.exists() {
            result.add_warning(format!("Data root does not exist: {}", self.data_root.display()));
        } else if !self.data_root.is_dir() {
            result.add_error(format!("Data root is not a directory: {}", self.data_root.display()));
        }

        // Check if base and data root are on the same NTFS volume
        match (get_drive_letter(&self.base_path), get_drive_letter(&self.data_root)) {
            (Ok(Some(base_drive)), Ok(Some(data_drive))) => {
                if base_drive != data_drive {
                    result.add_error(format!("Base path and data root must be on the same drive for hardlinks. Base: {}, Data: {}", base_drive, data_drive));
                }
                
                // Check if it's NTFS
                if let Ok(is_ntfs) = is_ntfs_volume(&self.base_path) {
                    if !is_ntfs {
                        result.add_error(format!("Drive {} is not NTFS. Hardlinks require NTFS.", base_drive));
                    }
                } else {
                    result.add_warning("Could not determine file system type".to_string());
                }
            }
            _ => {
                result.add_warning("Could not determine drive letters for validation".to_string());
            }
        }

        // Check free space
        if let Ok(free_space) = get_free_space(&self.data_root) {
            if free_space < 1024 * 1024 * 1024 {  // Less than 1GB
                result.add_warning(format!("Low disk space: {} available", format_size(free_space)));
            }
        }

        Ok(result)
    }

    /// Get the expected directory structure under data_root
    pub fn get_data_structure(&self) -> Vec<PathBuf> {
        vec![
            self.data_root.join("cache"),
            self.data_root.join("cache").join("blobs"),
            self.data_root.join("profiles"),
            self.data_root.join("runtimes"),
            self.data_root.join("logs"),
            self.data_root.join("tmp"),
        ]
    }

    /// Create the directory structure under data_root
    pub fn create_data_structure(&self) -> Result<()> {
        info!("Creating data directory structure at: {}", self.data_root.display());
        
        for dir in self.get_data_structure() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
            info!("Created directory: {}", dir.display());
        }

        // Create index.json in cache directory
        let cache_index = self.data_root.join("cache").join("index.json");
        if !cache_index.exists() {
            let empty_index = serde_json::json!({
                "version": 1,
                "blobs": {}
            });
            fs::write(&cache_index, serde_json::to_string_pretty(&empty_index)?)
                .with_context(|| format!("Failed to create cache index: {}", cache_index.display()))?;
            info!("Created cache index: {}", cache_index.display());
        }

        info!("Data directory structure created successfully");
        Ok(())
    }

    /// Mark the wizard as completed
    pub fn complete_wizard(&mut self) {
        self.wizard.completed = true;
        self.wizard.completed_at = Some(chrono::Utc::now());
        self.wizard.wizard_version = Some(env!("CARGO_PKG_VERSION").to_string());
        info!("Wizard marked as completed");
    }

    /// Check if the wizard needs to be shown
    pub fn needs_wizard(&self) -> bool {
        !self.wizard.completed || 
        self.base_path.as_os_str().is_empty() || 
        self.data_root.as_os_str().is_empty()
    }
}

/// Result of settings validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_settings_new() {
        let settings = Settings::new();
        assert_eq!(settings.schema, Settings::CURRENT_SCHEMA);
        assert_eq!(settings.overlay_mode, "hardlink");
        assert!(!settings.wizard.completed);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::new();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings.schema, deserialized.schema);
    }

    #[test]
    fn test_settings_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let settings_path = temp_dir.path().join("settings.json");
        
        let mut settings = Settings::new();
        settings.base_path = PathBuf::from("C:\\Games\\GTA San Andreas");
        settings.data_root = PathBuf::from("C:\\DeltaRuntime");
        
        // Save and load
        settings.save(&settings_path).unwrap();
        let loaded = Settings::load(&settings_path).unwrap();
        
        assert_eq!(loaded.base_path, settings.base_path);
        assert_eq!(loaded.data_root, settings.data_root);
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new();
        assert!(result.is_valid());
        
        result.add_warning("Test warning".to_string());
        assert!(result.is_valid());
        assert!(result.has_warnings());
        
        result.add_error("Test error".to_string());
        assert!(!result.is_valid());
    }
}