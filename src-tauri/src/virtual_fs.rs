use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use tracing::{debug, info};

/// Represents a file or directory in the virtual file system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualNode {
    /// Name of the file/directory
    pub name: String,
    /// Full virtual path from game root
    pub path: String,
    /// Whether this is a directory
    pub is_directory: bool,
    /// File size in bytes (None for directories)
    pub size: Option<u64>,
    /// Source of this file/directory
    pub source: VirtualNodeSource,
    /// Whether this node is writable (overlay files are writable, base files are read-only)
    pub writable: bool,
    /// Children (for directories only)
    pub children: Option<Vec<VirtualNode>>,
    /// File modification time
    pub modified: Option<String>,
}

/// Source of a virtual node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VirtualNodeSource {
    /// File/directory from the base game installation
    Base,
    /// File/directory from the workspace overlay
    Workspace,
    /// File/directory that exists in both (workspace overrides base)
    WorkspaceOverride,
    /// Tombstone - base file is hidden/deleted
    Tombstone,
}

/// Represents a tombstone file that marks a base file as deleted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tombstone {
    /// Path of the deleted file/directory relative to game root
    pub path: String,
    /// When the deletion was created
    pub created: chrono::DateTime<chrono::Utc>,
    /// Whether this tombstone deletes a directory (recursive)
    pub is_directory: bool,
}

/// Virtual file system that overlays workspace on top of base game installation
pub struct VirtualFileSystem {
    /// Path to the base game installation
    base_path: PathBuf,
    /// Path to the workspace overlay
    workspace_path: PathBuf,
    /// Path to tombstones file
    tombstones_path: PathBuf,
    /// Cached tombstones
    tombstones: HashMap<String, Tombstone>,
}

impl VirtualFileSystem {
    /// Create a new virtual file system
    pub fn new(base_path: PathBuf, workspace_path: PathBuf) -> Self {
        let tombstones_path = workspace_path.join(".deltaruntime_tombstones.json");
        Self {
            base_path,
            workspace_path,
            tombstones_path,
            tombstones: HashMap::new(),
        }
    }

    /// Initialize the virtual file system (load tombstones)
    pub fn initialize(&mut self) -> Result<()> {
        self.load_tombstones()?;
        Ok(())
    }

    /// Load tombstones from disk
    fn load_tombstones(&mut self) -> Result<()> {
        if !self.tombstones_path.exists() {
            debug!("No tombstones file found, starting with empty tombstones");
            return Ok(());
        }

        let content = fs::read_to_string(&self.tombstones_path)
            .with_context(|| format!("Failed to read tombstones file: {}", self.tombstones_path.display()))?;

        let tombstones: Vec<Tombstone> = serde_json::from_str(&content)
            .with_context(|| "Failed to parse tombstones file")?;

        self.tombstones.clear();
        for tombstone in tombstones {
            self.tombstones.insert(tombstone.path.clone(), tombstone);
        }

        debug!("Loaded {} tombstones", self.tombstones.len());
        Ok(())
    }

    /// Save tombstones to disk
    fn save_tombstones(&self) -> Result<()> {
        let tombstones: Vec<&Tombstone> = self.tombstones.values().collect();
        let content = serde_json::to_string_pretty(&tombstones)
            .context("Failed to serialize tombstones")?;

        fs::write(&self.tombstones_path, content)
            .with_context(|| format!("Failed to write tombstones file: {}", self.tombstones_path.display()))?;

        debug!("Saved {} tombstones", self.tombstones.len());
        Ok(())
    }

    /// Check if a path is tombstoned (deleted)
    fn is_tombstoned(&self, virtual_path: &str) -> bool {
        // Check if this exact path is tombstoned
        if self.tombstones.contains_key(virtual_path) {
            return true;
        }

        // Check if any parent directory is tombstoned
        let path = Path::new(virtual_path);
        for ancestor in path.ancestors() {
            if let Some(ancestor_str) = ancestor.to_str() {
                if ancestor_str != virtual_path && self.tombstones.contains_key(ancestor_str) {
                    return true;
                }
            }
        }

        false
    }

    /// Get virtual file system tree starting from root or a specific path
    pub fn get_virtual_tree(&self, virtual_path: Option<&str>) -> Result<VirtualNode> {
        let root_path = virtual_path.unwrap_or("");
        self.build_virtual_node(root_path, true)
    }

    /// Build a virtual node by merging base and workspace
    fn build_virtual_node(&self, virtual_path: &str, include_children: bool) -> Result<VirtualNode> {
        let base_full_path = self.base_path.join(virtual_path);
        let workspace_full_path = self.workspace_path.join(virtual_path);

        let base_exists = base_full_path.exists();
        let workspace_exists = workspace_full_path.exists();
        let is_tombstoned = self.is_tombstoned(virtual_path);

        // If tombstoned and only base exists, this node is deleted
        if is_tombstoned && !workspace_exists {
            return Err(anyhow::anyhow!("Path is tombstoned: {}", virtual_path));
        }

        // Determine the source and primary path to use
        let (source, primary_path, writable) = if workspace_exists && base_exists {
            (VirtualNodeSource::WorkspaceOverride, &workspace_full_path, true)
        } else if workspace_exists {
            (VirtualNodeSource::Workspace, &workspace_full_path, true)
        } else if base_exists && !is_tombstoned {
            (VirtualNodeSource::Base, &base_full_path, false)
        } else {
            return Err(anyhow::anyhow!("Path does not exist: {}", virtual_path));
        };

        let metadata = fs::metadata(primary_path)
            .with_context(|| format!("Failed to get metadata for: {}", primary_path.display()))?;

        let is_directory = metadata.is_dir();
        let size = if is_directory { None } else { Some(metadata.len()) };
        let modified = metadata.modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| {
                chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_default()
            });

        let name = if virtual_path.is_empty() {
            "Game Root".to_string()
        } else {
            Path::new(virtual_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(virtual_path)
                .to_string()
        };

        let children = if is_directory && include_children {
            Some(self.build_virtual_children(virtual_path)?)
        } else {
            None
        };

        Ok(VirtualNode {
            name,
            path: virtual_path.to_string(),
            is_directory,
            size,
            source,
            writable,
            children,
            modified,
        })
    }

    /// Build children for a virtual directory
    fn build_virtual_children(&self, virtual_path: &str) -> Result<Vec<VirtualNode>> {
        let mut children = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        let base_dir = self.base_path.join(virtual_path);
        let workspace_dir = self.workspace_path.join(virtual_path);

        // First, add all workspace files (they take priority)
        if workspace_dir.exists() && workspace_dir.is_dir() {
            for entry in fs::read_dir(&workspace_dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                
                // Skip tombstones file
                if name == ".deltaruntime_tombstones.json" {
                    continue;
                }

                let child_virtual_path = if virtual_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", virtual_path, name)
                };

                if !self.is_tombstoned(&child_virtual_path) {
                    if let Ok(child) = self.build_virtual_node(&child_virtual_path, false) {
                        children.push(child);
                        seen_names.insert(name);
                    }
                }
            }
        }

        // Then, add base files that aren't overridden or tombstoned
        if base_dir.exists() && base_dir.is_dir() {
            for entry in fs::read_dir(&base_dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();

                if seen_names.contains(&name) {
                    continue; // Already added from workspace
                }

                let child_virtual_path = if virtual_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", virtual_path, name)
                };

                if !self.is_tombstoned(&child_virtual_path) {
                    if let Ok(child) = self.build_virtual_node(&child_virtual_path, false) {
                        children.push(child);
                    }
                }
            }
        }

        // Sort children: directories first, then files, alphabetically
        children.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        Ok(children)
    }

    /// Create a tombstone (mark file/directory as deleted)
    pub fn create_tombstone(&mut self, virtual_path: &str) -> Result<()> {
        info!("Creating tombstone for: {}", virtual_path);

        let base_path = self.base_path.join(virtual_path);
        let is_directory = base_path.is_dir();

        let tombstone = Tombstone {
            path: virtual_path.to_string(),
            created: chrono::Utc::now(),
            is_directory,
        };

        self.tombstones.insert(virtual_path.to_string(), tombstone);
        self.save_tombstones()?;

        Ok(())
    }

    /// Remove a tombstone (restore deleted file/directory)
    pub fn remove_tombstone(&mut self, virtual_path: &str) -> Result<()> {
        info!("Removing tombstone for: {}", virtual_path);

        if self.tombstones.remove(virtual_path).is_some() {
            self.save_tombstones()?;
        }

        Ok(())
    }

    /// Copy a file from base to workspace (make it writable)
    pub fn copy_to_workspace(&self, virtual_path: &str) -> Result<()> {
        let base_file = self.base_path.join(virtual_path);
        let workspace_file = self.workspace_path.join(virtual_path);

        if !base_file.exists() {
            return Err(anyhow::anyhow!("Base file does not exist: {}", virtual_path));
        }

        // Create parent directory in workspace if needed
        if let Some(parent) = workspace_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create workspace directory: {}", parent.display()))?;
        }

        fs::copy(&base_file, &workspace_file)
            .with_context(|| format!("Failed to copy file to workspace: {}", virtual_path))?;

        info!("Copied base file to workspace: {}", virtual_path);
        Ok(())
    }

    /// Delete a file/directory from the virtual file system
    pub fn delete_virtual_path(&mut self, virtual_path: &str) -> Result<()> {
        let workspace_path = self.workspace_path.join(virtual_path);
        let base_path = self.base_path.join(virtual_path);

        // If file exists in workspace, delete it
        if workspace_path.exists() {
            if workspace_path.is_dir() {
                fs::remove_dir_all(&workspace_path)
                    .with_context(|| format!("Failed to delete workspace directory: {}", virtual_path))?;
            } else {
                fs::remove_file(&workspace_path)
                    .with_context(|| format!("Failed to delete workspace file: {}", virtual_path))?;
            }
        }

        // If file exists in base, create tombstone
        if base_path.exists() {
            self.create_tombstone(virtual_path)?;
        }

        info!("Deleted virtual path: {}", virtual_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_virtual_file_system_basic() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("base");
        let workspace_dir = temp_dir.path().join("workspace");

        // Create base structure
        fs::create_dir_all(&base_dir).unwrap();
        fs::create_dir_all(base_dir.join("subdir")).unwrap();
        fs::write(base_dir.join("base_file.txt"), "base content").unwrap();
        fs::write(base_dir.join("subdir/base_sub.txt"), "base sub content").unwrap();

        // Create workspace structure
        fs::create_dir_all(&workspace_dir).unwrap();
        fs::write(workspace_dir.join("workspace_file.txt"), "workspace content").unwrap();
        fs::write(workspace_dir.join("base_file.txt"), "overridden content").unwrap();

        let mut vfs = VirtualFileSystem::new(base_dir, workspace_dir);
        vfs.initialize().unwrap();

        let root = vfs.get_virtual_tree(None).unwrap();
        assert!(root.is_directory);
        assert!(root.children.is_some());

        let children = root.children.unwrap();
        assert!(children.len() >= 3); // base_file.txt, workspace_file.txt, subdir

        // Check that workspace file overrides base file
        let base_file = children.iter().find(|c| c.name == "base_file.txt").unwrap();
        assert_eq!(base_file.source, VirtualNodeSource::WorkspaceOverride);
        assert!(base_file.writable);
    }

    #[test]
    fn test_tombstones() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("base");
        let workspace_dir = temp_dir.path().join("workspace");

        fs::create_dir_all(&base_dir).unwrap();
        fs::create_dir_all(&workspace_dir).unwrap();
        fs::write(base_dir.join("to_delete.txt"), "will be deleted").unwrap();

        let mut vfs = VirtualFileSystem::new(base_dir, workspace_dir);
        vfs.initialize().unwrap();

        // File should exist initially
        let root = vfs.get_virtual_tree(None).unwrap();
        let children = root.children.unwrap();
        assert!(children.iter().any(|c| c.name == "to_delete.txt"));

        // Create tombstone
        vfs.create_tombstone("to_delete.txt").unwrap();

        // File should no longer appear in virtual tree
        let root = vfs.get_virtual_tree(None).unwrap();
        let children = root.children.unwrap();
        assert!(!children.iter().any(|c| c.name == "to_delete.txt"));
    }
}