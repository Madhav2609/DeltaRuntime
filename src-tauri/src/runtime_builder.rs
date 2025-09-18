use std::path::{Path, PathBuf};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result, anyhow};
use rayon::prelude::*;
use tracing::{info, warn, error};

use crate::runtime_planner::{RuntimePlan, RuntimePlanEntry, RuntimeSource, RuntimePlanner};
use crate::blob_cache::{BlobCache, BlobPath};
use crate::settings::Settings;
use blake3::Hash;

/// Progress information for runtime building
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgress {
    /// Current phase of the build
    pub phase: BuildPhase,
    /// Current step number
    pub current_step: usize,
    /// Total number of steps
    pub total_steps: usize,
    /// Current file being processed
    pub current_file: Option<String>,
    /// Files processed so far
    pub files_processed: usize,
    /// Total files to process
    pub total_files: usize,
    /// Bytes processed so far
    pub bytes_processed: u64,
    /// Total bytes to process
    pub total_bytes: u64,
    /// Any error message
    pub error: Option<String>,
    /// Whether the build is complete
    pub completed: bool,
}

/// Phases of runtime building
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildPhase {
    /// Validating prerequisites
    Preflight,
    /// Creating temporary runtime directory
    CreateTemp,
    /// Linking base game files
    LinkBase,
    /// Overlaying workspace files
    OverlayWorkspace,
    /// Finalizing runtime
    Finalize,
    /// Build completed successfully
    Complete,
    /// Build failed
    Failed,
}

/// Statistics from a runtime build
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStats {
    /// Total files linked
    pub total_files: usize,
    /// Files from base installation
    pub base_files: usize,
    /// Files from blob cache
    pub blob_files: usize,
    /// Total size in bytes
    pub total_bytes: u64,
    /// Time taken for the build in milliseconds
    pub build_time_ms: u64,
    /// Average files per second
    pub files_per_second: f64,
    /// Average MB per second
    pub mb_per_second: f64,
}

/// Result of a runtime build operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    /// Whether the build succeeded
    pub success: bool,
    /// Path to the created runtime directory
    pub runtime_path: Option<PathBuf>,
    /// Build statistics
    pub stats: Option<BuildStats>,
    /// Error message if build failed
    pub error: Option<String>,
}

/// Callback function type for progress updates
pub type ProgressCallback = Arc<dyn Fn(BuildProgress) + Send + Sync>;

/// Runtime builder that creates hardlink-based game runtimes
pub struct RuntimeBuilder {
    settings: Settings,
    blob_cache: BlobCache,
    planner: RuntimePlanner,
}

impl RuntimeBuilder {
    /// Create a new runtime builder
    pub fn new(settings: Settings) -> Self {
        let cache_dir = settings.data_root.join("cache");
        let blob_cache = BlobCache::new(&cache_dir);
        let planner = RuntimePlanner::new(settings.clone());

        Self {
            settings,
            blob_cache,
            planner,
        }
    }

    /// Build a runtime for the specified profile
    pub fn build_runtime(
        &self,
        profile_name: &str,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<BuildResult> {
        let start_time = SystemTime::now();
        let callback = progress_callback.unwrap_or_else(|| Arc::new(|_| {}));

        info!("Starting runtime build for profile: {}", profile_name);

        // Phase 1: Preflight checks
        callback(BuildProgress {
            phase: BuildPhase::Preflight,
            current_step: 1,
            total_steps: 5,
            current_file: None,
            files_processed: 0,
            total_files: 0,
            bytes_processed: 0,
            total_bytes: 0,
            error: None,
            completed: false,
        });

        if let Err(e) = self.preflight_checks() {
            let error_msg = format!("Preflight checks failed: {}", e);
            error!("{}", error_msg);
            callback(BuildProgress {
                phase: BuildPhase::Failed,
                current_step: 1,
                total_steps: 5,
                current_file: None,
                files_processed: 0,
                total_files: 0,
                bytes_processed: 0,
                total_bytes: 0,
                error: Some(error_msg.clone()),
                completed: true,
            });
            return Ok(BuildResult {
                success: false,
                runtime_path: None,
                stats: None,
                error: Some(error_msg),
            });
        }

        // Compute or load runtime plan
        let plan = match self.planner.compute_plan(profile_name) {
            Ok(plan) => plan,
            Err(e) => {
                let error_msg = format!("Failed to compute runtime plan: {}", e);
                error!("{}", error_msg);
                callback(BuildProgress {
                    phase: BuildPhase::Failed,
                    current_step: 1,
                    total_steps: 5,
                    current_file: None,
                    files_processed: 0,
                    total_files: 0,
                    bytes_processed: 0,
                    total_bytes: 0,
                    error: Some(error_msg.clone()),
                    completed: true,
                });
                return Ok(BuildResult {
                    success: false,
                    runtime_path: None,
                    stats: None,
                    error: Some(error_msg),
                });
            }
        };

        // Phase 2: Create temporary runtime directory
        callback(BuildProgress {
            phase: BuildPhase::CreateTemp,
            current_step: 2,
            total_steps: 5,
            current_file: None,
            files_processed: 0,
            total_files: plan.total_files,
            bytes_processed: 0,
            total_bytes: plan.total_size,
            error: None,
            completed: false,
        });

        let temp_runtime_dir = self.create_temp_runtime_dir(profile_name)?;
        info!("Created temporary runtime directory: {}", temp_runtime_dir.display());

        // Build counters for progress tracking
        let files_processed = Arc::new(AtomicUsize::new(0));
        let bytes_processed = Arc::new(AtomicUsize::new(0));

        // Phase 3: Link base game files
        callback(BuildProgress {
            phase: BuildPhase::LinkBase,
            current_step: 3,
            total_steps: 5,
            current_file: None,
            files_processed: 0,
            total_files: plan.total_files,
            bytes_processed: 0,
            total_bytes: plan.total_size,
            error: None,
            completed: false,
        });

        let base_entries: Vec<_> = plan.entries.iter()
            .filter(|entry| matches!(entry.source, RuntimeSource::Base))
            .collect();

        self.link_base_files(&base_entries, &temp_runtime_dir, &files_processed, &bytes_processed, &callback, &plan)?;

        // Phase 4: Overlay workspace files
        callback(BuildProgress {
            phase: BuildPhase::OverlayWorkspace,
            current_step: 4,
            total_steps: 5,
            current_file: None,
            files_processed: files_processed.load(Ordering::Relaxed),
            total_files: plan.total_files,
            bytes_processed: bytes_processed.load(Ordering::Relaxed) as u64,
            total_bytes: plan.total_size,
            error: None,
            completed: false,
        });

        let blob_entries: Vec<_> = plan.entries.iter()
            .filter(|entry| matches!(entry.source, RuntimeSource::Blob(_)))
            .collect();

        self.overlay_workspace_files(&blob_entries, &temp_runtime_dir, &files_processed, &bytes_processed, &callback, &plan)?;

        // Phase 5: Finalize runtime
        callback(BuildProgress {
            phase: BuildPhase::Finalize,
            current_step: 5,
            total_steps: 5,
            current_file: None,
            files_processed: files_processed.load(Ordering::Relaxed),
            total_files: plan.total_files,
            bytes_processed: bytes_processed.load(Ordering::Relaxed) as u64,
            total_bytes: plan.total_size,
            error: None,
            completed: false,
        });

        let final_runtime_dir = self.finalize_runtime(profile_name, temp_runtime_dir)?;
        
        // Save the runtime plan to the final directory
        self.planner.save_plan(&plan)?;

        let build_time = start_time.elapsed().unwrap_or_default();
        let build_time_ms = build_time.as_millis() as u64;
        
        let stats = BuildStats {
            total_files: plan.total_files,
            base_files: plan.base_files,
            blob_files: plan.blob_files,
            total_bytes: plan.total_size,
            build_time_ms,
            files_per_second: if build_time_ms > 0 {
                (plan.total_files as f64 * 1000.0) / build_time_ms as f64
            } else {
                0.0
            },
            mb_per_second: if build_time_ms > 0 {
                (plan.total_size as f64 / 1024.0 / 1024.0 * 1000.0) / build_time_ms as f64
            } else {
                0.0
            },
        };

        info!(
            "Runtime build completed: {} files in {}ms ({:.1} files/sec, {:.1} MB/sec)",
            stats.total_files,
            stats.build_time_ms,
            stats.files_per_second,
            stats.mb_per_second
        );

        // Final progress update
        callback(BuildProgress {
            phase: BuildPhase::Complete,
            current_step: 5,
            total_steps: 5,
            current_file: None,
            files_processed: plan.total_files,
            total_files: plan.total_files,
            bytes_processed: plan.total_size,
            total_bytes: plan.total_size,
            error: None,
            completed: true,
        });

        Ok(BuildResult {
            success: true,
            runtime_path: Some(final_runtime_dir),
            stats: Some(stats),
            error: None,
        })
    }

    /// Perform preflight checks before building
    fn preflight_checks(&self) -> Result<()> {
        info!("Performing preflight checks");

        // Check that base path exists
        if !self.settings.base_path.exists() {
            return Err(anyhow!("Base game path does not exist: {}", self.settings.base_path.display()));
        }

        // Check that cache directory exists
        let cache_dir = self.settings.data_root.join("cache");
        if !cache_dir.exists() {
            return Err(anyhow!("Cache directory does not exist: {}", cache_dir.display()));
        }

        // Note: Volume compatibility is ensured during settings validation
        // All paths (base, cache, profiles) are guaranteed to be on the same NTFS volume

        info!("Preflight checks passed");
        Ok(())
    }

    /// Create a temporary runtime directory
    fn create_temp_runtime_dir(&self, profile_name: &str) -> Result<PathBuf> {
        let runtimes_dir = self.settings.data_root.join("runtimes");
        fs::create_dir_all(&runtimes_dir)
            .context("Failed to create runtimes directory")?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let temp_dir = runtimes_dir.join(format!("{}-{}-tmp", profile_name, timestamp));
        fs::create_dir_all(&temp_dir)
            .with_context(|| format!("Failed to create temporary runtime directory: {}", temp_dir.display()))?;

        Ok(temp_dir)
    }

    /// Link base game files to the runtime directory
    fn link_base_files(
        &self,
        entries: &[&RuntimePlanEntry],
        runtime_dir: &Path,
        files_processed: &Arc<AtomicUsize>,
        bytes_processed: &Arc<AtomicUsize>,
        callback: &ProgressCallback,
        plan: &RuntimePlan,
    ) -> Result<()> {
        info!("Linking {} base game files", entries.len());

        entries.par_iter().try_for_each(|entry| -> Result<()> {
            let source_path = self.settings.base_path.join(&entry.rel_path);
            let dest_path = runtime_dir.join(&entry.rel_path);

            // Create parent directory if it doesn't exist
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            // Create hardlink using standard library
            std::fs::hard_link(&source_path, &dest_path)
                .with_context(|| format!("Failed to create hardlink: {} -> {}", source_path.display(), dest_path.display()))?;

            // Update progress counters
            let processed = files_processed.fetch_add(1, Ordering::Relaxed) + 1;
            bytes_processed.fetch_add(entry.size as usize, Ordering::Relaxed);

            // Send progress update every 100 files
            if processed % 100 == 0 {
                callback(BuildProgress {
                    phase: BuildPhase::LinkBase,
                    current_step: 3,
                    total_steps: 5,
                    current_file: Some(entry.rel_path.clone()),
                    files_processed: processed,
                    total_files: plan.total_files,
                    bytes_processed: bytes_processed.load(Ordering::Relaxed) as u64,
                    total_bytes: plan.total_size,
                    error: None,
                    completed: false,
                });
            }

            Ok(())
        })?;

        info!("Base file linking completed");
        Ok(())
    }

    /// Overlay workspace files from blob cache to the runtime directory
    fn overlay_workspace_files(
        &self,
        entries: &[&RuntimePlanEntry],
        runtime_dir: &Path,
        files_processed: &Arc<AtomicUsize>,
        bytes_processed: &Arc<AtomicUsize>,
        callback: &ProgressCallback,
        plan: &RuntimePlan,
    ) -> Result<()> {
        info!("Overlaying {} workspace files", entries.len());

        entries.par_iter().try_for_each(|entry| -> Result<()> {
            if let RuntimeSource::Blob(hash_str) = &entry.source {
                let blob_path = self.blob_cache.get_blob_path_from_hash(hash_str)?;
                let dest_path = runtime_dir.join(&entry.rel_path);

                // Create parent directory if it doesn't exist
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
                }

                // If this is an override, remove the base file first
                if entry.is_override && dest_path.exists() {
                    fs::remove_file(&dest_path)
                        .with_context(|| format!("Failed to remove base file for override: {}", dest_path.display()))?;
                }

                // Create hardlink from blob cache to runtime using the existing BlobCache method
                let blob_path = BlobPath {
                    hash: Hash::from_hex(hash_str)
                        .map_err(|e| anyhow::anyhow!("Invalid hash: {}", e))?,
                    path: blob_path,
                };
                self.blob_cache.link_blob_to(&dest_path, &blob_path)
                    .with_context(|| format!("Failed to create hardlink from blob: {} -> {}", blob_path.path.display(), dest_path.display()))?;

                // Update progress counters
                let processed = files_processed.fetch_add(1, Ordering::Relaxed) + 1;
                bytes_processed.fetch_add(entry.size as usize, Ordering::Relaxed);

                // Send progress update every 50 files
                if processed % 50 == 0 {
                    callback(BuildProgress {
                        phase: BuildPhase::OverlayWorkspace,
                        current_step: 4,
                        total_steps: 5,
                        current_file: Some(entry.rel_path.clone()),
                        files_processed: processed,
                        total_files: plan.total_files,
                        bytes_processed: bytes_processed.load(Ordering::Relaxed) as u64,
                        total_bytes: plan.total_size,
                        error: None,
                        completed: false,
                    });
                }
            }

            Ok(())
        })?;

        info!("Workspace overlay completed");
        Ok(())
    }

    /// Finalize the runtime by atomically renaming from temp to final
    fn finalize_runtime(&self, profile_name: &str, temp_dir: PathBuf) -> Result<PathBuf> {
        let runtimes_dir = self.settings.data_root.join("runtimes");
        let final_dir = runtimes_dir.join(format!("{}-latest", profile_name));

        // Remove existing runtime if it exists
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir)
                .with_context(|| format!("Failed to remove existing runtime: {}", final_dir.display()))?;
        }

        // Atomic rename
        fs::rename(&temp_dir, &final_dir)
            .with_context(|| format!("Failed to rename runtime directory: {} -> {}", temp_dir.display(), final_dir.display()))?;

        info!("Runtime finalized at: {}", final_dir.display());
        Ok(final_dir)
    }

    /// Clean up old temporary runtime directories
    pub fn cleanup_temp_runtimes(&self) -> Result<()> {
        let runtimes_dir = self.settings.data_root.join("runtimes");
        if !runtimes_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&runtimes_dir)
            .context("Failed to read runtimes directory")?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();
            
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("-tmp") {
                        info!("Cleaning up temporary runtime: {}", path.display());
                        if let Err(e) = fs::remove_dir_all(&path) {
                            warn!("Failed to remove temporary runtime {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}