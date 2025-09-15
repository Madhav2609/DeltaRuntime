use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use anyhow::{Context, Result};
use tracing::{info, warn, debug};

/// Profile metadata stored in the profile directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    /// Profile name
    pub name: String,
    /// When the profile was created
    pub created_at: DateTime<Utc>,
    /// When the profile was last used/accessed
    pub last_used: DateTime<Utc>,
    /// Optional description
    pub description: Option<String>,
    /// Schema version for future migrations
    pub schema_version: u32,
}

impl ProfileMetadata {
    /// Create new profile metadata
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            name,
            created_at: now,
            last_used: now,
            description: None,
            schema_version: 1,
        }
    }

    /// Update the last used timestamp
    pub fn touch(&mut self) {
        self.last_used = Utc::now();
    }
}

/// A profile represents a self-contained environment with workspace and saves
#[derive(Debug, Clone)]
pub struct Profile {
    /// Profile metadata
    pub metadata: ProfileMetadata,
    /// Path to the profile directory
    pub profile_dir: PathBuf,
    /// Path to the workspace overlay directory
    pub workspace_dir: PathBuf,
    /// Path to the saves directory
    pub saves_dir: PathBuf,
}

impl Profile {
    /// Create a new profile in the given profiles root directory
    pub fn create(profiles_root: &Path, name: String) -> Result<Self> {
        info!("Creating new profile: {}", name);

        // Validate profile name
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }
        
        if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            return Err(anyhow::anyhow!("Profile name contains invalid characters"));
        }

        let profile_dir = profiles_root.join(&name);
        
        // Check if profile already exists
        if profile_dir.exists() {
            return Err(anyhow::anyhow!("Profile '{}' already exists", name));
        }

        // Create profile directory structure
        let workspace_dir = profile_dir.join("workspace");
        let saves_dir = profile_dir.join("saves");
        
        fs::create_dir_all(&workspace_dir)
            .with_context(|| format!("Failed to create workspace directory: {}", workspace_dir.display()))?;
        
        fs::create_dir_all(&saves_dir)
            .with_context(|| format!("Failed to create saves directory: {}", saves_dir.display()))?;

        // Create and save metadata
        let metadata = ProfileMetadata::new(name.clone());
        let metadata_path = profile_dir.join("profile.json");
        
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .context("Failed to serialize profile metadata")?;
        
        fs::write(&metadata_path, metadata_json)
            .with_context(|| format!("Failed to write profile metadata: {}", metadata_path.display()))?;

        debug!("Profile '{}' created successfully at: {}", name, profile_dir.display());

        Ok(Profile {
            metadata,
            profile_dir,
            workspace_dir,
            saves_dir,
        })
    }

    /// Load an existing profile from a profile directory
    pub fn load(profile_dir: &Path) -> Result<Self> {
        let metadata_path = profile_dir.join("profile.json");
        
        if !metadata_path.exists() {
            return Err(anyhow::anyhow!("Profile metadata not found: {}", metadata_path.display()));
        }

        let metadata_content = fs::read_to_string(&metadata_path)
            .with_context(|| format!("Failed to read profile metadata: {}", metadata_path.display()))?;
        
        let metadata: ProfileMetadata = serde_json::from_str(&metadata_content)
            .with_context(|| format!("Failed to parse profile metadata: {}", metadata_path.display()))?;

        let workspace_dir = profile_dir.join("workspace");
        let saves_dir = profile_dir.join("saves");

        // Ensure directories exist (for profiles created before this structure)
        if !workspace_dir.exists() {
            fs::create_dir_all(&workspace_dir)
                .with_context(|| format!("Failed to create workspace directory: {}", workspace_dir.display()))?;
        }
        
        if !saves_dir.exists() {
            fs::create_dir_all(&saves_dir)
                .with_context(|| format!("Failed to create saves directory: {}", saves_dir.display()))?;
        }

        Ok(Profile {
            metadata,
            profile_dir: profile_dir.to_path_buf(),
            workspace_dir,
            saves_dir,
        })
    }

    /// Save profile metadata to disk
    pub fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.profile_dir.join("profile.json");
        
        let metadata_json = serde_json::to_string_pretty(&self.metadata)
            .context("Failed to serialize profile metadata")?;
        
        fs::write(&metadata_path, metadata_json)
            .with_context(|| format!("Failed to write profile metadata: {}", metadata_path.display()))?;

        Ok(())
    }

    /// Rename this profile
    pub fn rename(&mut self, new_name: String, profiles_root: &Path) -> Result<()> {
        info!("Renaming profile '{}' to '{}'", self.metadata.name, new_name);

        // Validate new name
        if new_name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }
        
        if new_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            return Err(anyhow::anyhow!("Profile name contains invalid characters"));
        }

        let new_profile_dir = profiles_root.join(&new_name);
        
        // Check if target already exists
        if new_profile_dir.exists() {
            return Err(anyhow::anyhow!("Profile '{}' already exists", new_name));
        }

        // Rename directory
        fs::rename(&self.profile_dir, &new_profile_dir)
            .with_context(|| format!("Failed to rename profile directory from {} to {}", 
                self.profile_dir.display(), new_profile_dir.display()))?;

        // Update metadata
        self.metadata.name = new_name;
        self.metadata.touch();
        
        // Update paths
        self.profile_dir = new_profile_dir;
        self.workspace_dir = self.profile_dir.join("workspace");
        self.saves_dir = self.profile_dir.join("saves");

        // Save updated metadata
        self.save_metadata()
            .context("Failed to save updated profile metadata after rename")?;

        debug!("Profile renamed successfully");
        Ok(())
    }

    /// Delete this profile completely
    pub fn delete(self) -> Result<()> {
        info!("Deleting profile: {}", self.metadata.name);
        
        fs::remove_dir_all(&self.profile_dir)
            .with_context(|| format!("Failed to delete profile directory: {}", self.profile_dir.display()))?;

        debug!("Profile '{}' deleted successfully", self.metadata.name);
        Ok(())
    }

    /// Touch the profile (update last used time)
    pub fn touch(&mut self) -> Result<()> {
        self.metadata.touch();
        self.save_metadata()
    }
}

/// Profile manager for CRUD operations
pub struct ProfileManager {
    profiles_root: PathBuf,
}

impl ProfileManager {
    /// Create a new profile manager
    pub fn new(profiles_root: PathBuf) -> Self {
        Self { profiles_root }
    }

    /// Create a new profile
    pub fn create_profile(&self, name: String) -> Result<Profile> {
        Profile::create(&self.profiles_root, name)
    }

    /// List all profiles
    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        info!("Listing profiles from: {}", self.profiles_root.display());

        if !self.profiles_root.exists() {
            debug!("Profiles directory doesn't exist yet, returning empty list");
            return Ok(Vec::new());
        }

        let mut profiles = Vec::new();
        
        let entries = fs::read_dir(&self.profiles_root)
            .with_context(|| format!("Failed to read profiles directory: {}", self.profiles_root.display()))?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();
            
            if path.is_dir() {
                match Profile::load(&path) {
                    Ok(profile) => {
                        debug!("Loaded profile: {}", profile.metadata.name);
                        profiles.push(profile);
                    }
                    Err(e) => {
                        warn!("Failed to load profile from {}: {}", path.display(), e);
                        // Continue with other profiles
                    }
                }
            }
        }

        // Sort profiles by last used time (most recent first)
        profiles.sort_by(|a, b| b.metadata.last_used.cmp(&a.metadata.last_used));

        debug!("Found {} profiles", profiles.len());
        Ok(profiles)
    }

    /// Get a profile by name
    pub fn get_profile(&self, name: &str) -> Result<Option<Profile>> {
        let profile_dir = self.profiles_root.join(name);
        
        if !profile_dir.exists() {
            return Ok(None);
        }

        match Profile::load(&profile_dir) {
            Ok(profile) => Ok(Some(profile)),
            Err(_) => Ok(None), // Profile directory exists but is invalid
        }
    }

    /// Rename a profile
    pub fn rename_profile(&self, old_name: &str, new_name: String) -> Result<Profile> {
        let mut profile = self.get_profile(old_name)?
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", old_name))?;

        profile.rename(new_name, &self.profiles_root)?;
        Ok(profile)
    }

    /// Delete a profile
    pub fn delete_profile(&self, name: &str) -> Result<()> {
        let profile = self.get_profile(name)?
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", name))?;

        profile.delete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_profile_creation() {
        let temp_dir = TempDir::new().unwrap();
        let profiles_root = temp_dir.path().join("profiles");
        
        let profile = Profile::create(&profiles_root, "test-profile".to_string()).unwrap();
        
        assert_eq!(profile.metadata.name, "test-profile");
        assert!(profile.workspace_dir.exists());
        assert!(profile.saves_dir.exists());
        assert!(profile.profile_dir.join("profile.json").exists());
    }

    #[test]
    fn test_profile_manager() {
        let temp_dir = TempDir::new().unwrap();
        let profiles_root = temp_dir.path().join("profiles");
        let manager = ProfileManager::new(profiles_root);

        // Create profiles
        let profile1 = manager.create_profile("profile1".to_string()).unwrap();
        let profile2 = manager.create_profile("profile2".to_string()).unwrap();
        
        // List profiles
        let profiles = manager.list_profiles().unwrap();
        assert_eq!(profiles.len(), 2);
        
        // Rename profile
        let renamed = manager.rename_profile("profile1", "renamed-profile".to_string()).unwrap();
        assert_eq!(renamed.metadata.name, "renamed-profile");
        
        // Delete profile
        manager.delete_profile("profile2").unwrap();
        let profiles = manager.list_profiles().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].metadata.name, "renamed-profile");
    }

    #[test]
    fn test_invalid_profile_names() {
        let temp_dir = TempDir::new().unwrap();
        let profiles_root = temp_dir.path().join("profiles");
        
        // Empty name
        assert!(Profile::create(&profiles_root, "".to_string()).is_err());
        
        // Invalid characters
        assert!(Profile::create(&profiles_root, "test/profile".to_string()).is_err());
        assert!(Profile::create(&profiles_root, "test\\profile".to_string()).is_err());
        assert!(Profile::create(&profiles_root, "test:profile".to_string()).is_err());
    }
}