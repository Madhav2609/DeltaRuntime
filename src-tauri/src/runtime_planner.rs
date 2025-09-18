use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use tracing::{info, debug, warn};

use crate::virtual_fs::{VirtualFileSystem, VirtualNodeSource};
use crate::blob_cache::BlobCache;
use crate::settings::Settings;
use crate::profiles::ProfileManager;

/// Source of a file in the runtime plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuntimeSource {
    /// File comes from the base game installation
    Base,
    /// File comes from a blob in the cache (identified by hash)
    Blob(String), // Hash as hex string for JSON serialization
}

/// A single entry in the runtime plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePlanEntry {
    /// Relative path from game root
    pub rel_path: String,
    /// Source of this file
    pub source: RuntimeSource,
    /// File size in bytes
    pub size: u64,
    /// Whether this file exists in the base installation
    pub has_base: bool,
    /// Whether this file is overridden by workspace
    pub is_override: bool,
}

/// Complete runtime plan for a profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePlan {
    /// Profile name this plan was created for
    pub profile_name: String,
    /// Timestamp when plan was generated
    pub generated_at: String,
    /// Total number of files in the plan
    pub total_files: usize,
    /// Total size of all files in bytes
    pub total_size: u64,
    /// Number of files from base installation
    pub base_files: usize,
    /// Number of files from blob cache (overrides/new files)
    pub blob_files: usize,
    /// The actual plan entries
    pub entries: Vec<RuntimePlanEntry>,
}

/// Runtime plan computer and manager
pub struct RuntimePlanner {
    settings: Settings,
    blob_cache: BlobCache,
}

impl RuntimePlanner {
    /// Create a new runtime planner
    pub fn new(settings: Settings) -> Self {
        let cache_dir = settings.data_root.join("cache");
        let blob_cache = BlobCache::new(cache_dir);
        
        Self {
            settings,
            blob_cache,
        }
    }

    /// Compute a runtime plan for a given profile
    pub fn compute_plan(&self, profile_name: &str) -> Result<RuntimePlan> {
        info!("Computing runtime plan for profile: {}", profile_name);

        // Get profile manager and validate profile exists
        let profiles_root = self.settings.data_root.join("profiles");
        let profile_manager = ProfileManager::new(profiles_root);
        let profile = profile_manager.get_profile(profile_name)?
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", profile_name))?;

        // Create virtual file system
        let vfs = VirtualFileSystem::new(
            self.settings.base_path.clone(),
            profile.workspace_dir.clone(),
        );

        // Get the complete virtual file tree
        let root_node = vfs.get_virtual_tree(None)
            .context("Failed to get virtual file tree")?;

        let mut entries = Vec::new();
        let mut total_size = 0u64;
        let mut base_files = 0;
        let mut blob_files = 0;

        // Recursively traverse the virtual tree and build plan entries
        self.traverse_and_plan(&root_node, "", &mut entries, &mut total_size, &mut base_files, &mut blob_files, profile_name)?;

        let plan = RuntimePlan {
            profile_name: profile_name.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            total_files: entries.len(),
            total_size,
            base_files,
            blob_files,
            entries,
        };

        info!(
            "Runtime plan computed: {} files ({} base, {} blob), {} bytes total",
            plan.total_files,
            plan.base_files, 
            plan.blob_files,
            plan.total_size
        );

        Ok(plan)
    }

    /// Recursively traverse virtual tree and create plan entries
    fn traverse_and_plan(
        &self,
        node: &crate::virtual_fs::VirtualNode,
        current_path: &str,
        entries: &mut Vec<RuntimePlanEntry>,
        total_size: &mut u64,
        base_files: &mut usize,
        blob_files: &mut usize,
        profile_name: &str,
    ) -> Result<()> {
        if node.is_directory {
            // For directories, traverse children
            if let Some(children) = &node.children {
                debug!("Directory {} has {} children", node.name, children.len());
                for child in children {
                    debug!("Traversing directory child: node.name='{}', current_path='{}', child.name='{}'", 
                           child.name, current_path, child.name);
                    
                    // Build the path for this directory level
                    // For root node ("Game Root"), children start with empty path
                    // For other directories, children get the directory name as their path
                    let child_path = if current_path.is_empty() && node.name == "Game Root" {
                        String::new() // Root node children start with empty path
                    } else if current_path.is_empty() {
                        node.name.clone() // First level directories (models, anim, etc.)
                    } else {
                        format!("{}/{}", current_path, node.name) // Nested directories
                    };
                    
                    self.traverse_and_plan(child, &child_path, entries, total_size, base_files, blob_files, profile_name)?;
                }
            } else {
                debug!("Directory {} has no children", node.name);
            }
        } else {
            // For files, create a plan entry
            let rel_path = if current_path.is_empty() {
                node.name.clone()
            } else {
                format!("{}/{}", current_path, node.name)
            };
            
            debug!("Processing file: node.name='{}', current_path='{}', rel_path='{}'", 
                   node.name, current_path, rel_path);

            let size = node.size.unwrap_or(0);
            *total_size += size;

            let (source, has_base, is_override) = match node.source {
                VirtualNodeSource::Base => {
                    *base_files += 1;
                    (RuntimeSource::Base, true, false)
                }
                VirtualNodeSource::Workspace => {
                    *blob_files += 1;
                    // For workspace-only files, look up the blob hash from index
                    let hash = self.get_blob_hash_for_file(profile_name, &rel_path)?;
                    (RuntimeSource::Blob(hash), false, false)
                }
                VirtualNodeSource::Override => {
                    *blob_files += 1;
                    // For override files, look up the blob hash from index
                    let hash = self.get_blob_hash_for_file(profile_name, &rel_path)?;
                    (RuntimeSource::Blob(hash), true, true)
                }
            };

            entries.push(RuntimePlanEntry {
                rel_path,
                source,
                size,
                has_base,
                is_override,
            });
        }

        Ok(())
    }

    /// Get the blob hash for a file from the index (efficient lookup)
    /// Falls back to computing hash if not found in index
    fn get_blob_hash_for_file(&self, profile_name: &str, rel_path: &str) -> Result<String> {
        debug!("Looking up blob hash for profile='{}', rel_path='{}'", profile_name, rel_path);
        
        // First try to find the hash in the index (most efficient)
        match self.blob_cache.find_blob_hash_for_file(profile_name, rel_path) {
            Ok(Some(hash)) => {
                debug!("Found blob hash in index for {}/{}: {}", profile_name, rel_path, hash);
                Ok(hash)
            }
            Ok(None) => {
                // File not found in index - this shouldn't happen for workspace files
                // that have been processed by the workspace watcher, but we'll handle it
                warn!("File {}/{} not found in blob index, computing hash", profile_name, rel_path);
                
                // Get the actual file path and compute hash
                let profiles_root = self.settings.data_root.join("profiles");
                let profile_manager = ProfileManager::new(profiles_root);
                let profile = profile_manager.get_profile(profile_name)?
                    .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", profile_name))?;
                
                let workspace_file_path = profile.workspace_dir.join(rel_path);
                self.get_file_hash(&workspace_file_path)
            }
            Err(e) => {
                Err(anyhow::anyhow!("Failed to lookup blob hash: {}", e))
            }
        }
    }

    /// Get the hash of a file (either from blob cache or by computing it)
    fn get_file_hash(&self, file_path: &Path) -> Result<String> {
        let hash = BlobCache::hash_file(file_path)
            .with_context(|| format!("Failed to hash file: {}", file_path.display()))?;
        Ok(format!("{}", hash))
    }

    /// Save a runtime plan to disk
    pub fn save_plan(&self, plan: &RuntimePlan) -> Result<PathBuf> {
        let runtimes_dir = self.settings.data_root.join("runtimes");
        fs::create_dir_all(&runtimes_dir)
            .context("Failed to create runtimes directory")?;

        let profile_runtime_dir = runtimes_dir.join(format!("{}-latest", plan.profile_name));
        fs::create_dir_all(&profile_runtime_dir)
            .context("Failed to create profile runtime directory")?;

        let plan_file = profile_runtime_dir.join("runtime_plan.json");
        let plan_json = serde_json::to_string_pretty(plan)
            .context("Failed to serialize runtime plan")?;

        fs::write(&plan_file, plan_json)
            .with_context(|| format!("Failed to write runtime plan to: {}", plan_file.display()))?;

        info!("Runtime plan saved to: {}", plan_file.display());
        Ok(plan_file)
    }

    /// Load a runtime plan from disk
    pub fn load_plan(&self, profile_name: &str) -> Result<Option<RuntimePlan>> {
        let plan_file = self.settings.data_root
            .join("runtimes")
            .join(format!("{}-latest", profile_name))
            .join("runtime_plan.json");

        if !plan_file.exists() {
            return Ok(None);
        }

        let plan_json = fs::read_to_string(&plan_file)
            .with_context(|| format!("Failed to read runtime plan from: {}", plan_file.display()))?;

        let plan: RuntimePlan = serde_json::from_str(&plan_json)
            .context("Failed to deserialize runtime plan")?;

        Ok(Some(plan))
    }

    /// Compare two runtime plans and return the differences
    pub fn diff_plans(&self, old_plan: &RuntimePlan, new_plan: &RuntimePlan) -> RuntimePlanDiff {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();

        // Create maps for easier comparison
        let old_entries: HashMap<String, &RuntimePlanEntry> = old_plan.entries
            .iter()
            .map(|entry| (entry.rel_path.clone(), entry))
            .collect();

        let new_entries: HashMap<String, &RuntimePlanEntry> = new_plan.entries
            .iter()
            .map(|entry| (entry.rel_path.clone(), entry))
            .collect();

        // Find added and changed entries
        for (path, new_entry) in &new_entries {
            match old_entries.get(path) {
                Some(old_entry) => {
                    // Check if entry changed
                    if old_entry.source != new_entry.source || old_entry.size != new_entry.size {
                        changed.push((*new_entry).clone());
                    }
                }
                None => {
                    // Entry was added
                    added.push((*new_entry).clone());
                }
            }
        }

        // Find removed entries
        for (path, old_entry) in &old_entries {
            if !new_entries.contains_key(path) {
                removed.push((*old_entry).clone());
            }
        }

        RuntimePlanDiff {
            added,
            removed,
            changed,
        }
    }
}

/// Represents the differences between two runtime plans
#[derive(Debug, Clone)]
pub struct RuntimePlanDiff {
    pub added: Vec<RuntimePlanEntry>,
    pub removed: Vec<RuntimePlanEntry>,
    pub changed: Vec<RuntimePlanEntry>,
}

impl RuntimePlanDiff {
    /// Check if the diff is empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Get the total number of touched files
    pub fn touched_count(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}