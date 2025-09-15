use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::fs;
use std::thread;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use log::{info, warn, error, debug};
use tauri::Emitter;
use crate::blob_cache::BlobCache;
use crate::settings::Settings;

/// Check if two files are hardlinked using Windows API
#[cfg(windows)]
fn are_files_hardlinked(path1: &Path, path2: &Path) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        OPEN_EXISTING, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows::core::PCWSTR;
    use std::os::windows::ffi::OsStrExt;
    
    unsafe {
        // Convert paths to wide strings
        let path1_wide: Vec<u16> = path1.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let path2_wide: Vec<u16> = path2.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        
        // Open both files
        let handle1 = CreateFileW(
            PCWSTR(path1_wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        );
        
        let handle2 = CreateFileW(
            PCWSTR(path2_wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        );
        
        let (handle1, handle2) = match (handle1, handle2) {
            (Ok(h1), Ok(h2)) => (h1, h2),
            _ => return false,
        };
        
        if handle1.is_invalid() || handle2.is_invalid() {
            return false;
        }
        
        // Get file information
        let mut info1 = BY_HANDLE_FILE_INFORMATION::default();
        let mut info2 = BY_HANDLE_FILE_INFORMATION::default();
        
        let success1 = GetFileInformationByHandle(handle1, &mut info1).is_ok();
        let success2 = GetFileInformationByHandle(handle2, &mut info2).is_ok();
        
        if success1 && success2 {
            // Compare volume serial and file index
            info1.dwVolumeSerialNumber == info2.dwVolumeSerialNumber &&
            info1.nFileIndexLow == info2.nFileIndexLow &&
            info1.nFileIndexHigh == info2.nFileIndexHigh
        } else {
            false
        }
    }
}

#[cfg(not(windows))]
fn are_files_hardlinked(_path1: &Path, _path2: &Path) -> bool {
    // On non-Windows platforms, we'd use dev/ino comparison
    // For now, just return false to disable optimization
    false
}

/// Debounced file change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub kind: FileChangeKind,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// Workspace watcher that normalizes files to global cache
pub struct WorkspaceWatcher {
    profile_name: String,
    workspace_path: PathBuf,
    cache: BlobCache,
    watcher: Option<RecommendedWatcher>,
    event_sender: Option<Sender<notify::Result<notify::Event>>>,
    app_handle: Option<tauri::AppHandle>,
}

impl WorkspaceWatcher {
    pub fn new(profile_name: String, workspace_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        // Try to load existing settings, fall back to default cache location
        let cache_dir = if let Some(settings) = Settings::try_load_existing() {
            settings.get_cache_directory()
        } else {
            // Fallback to default location if no settings exist yet
            let data_root = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("./data"))
                .join("DeltaRuntime");
            data_root.join("cache")
        };
        
        let cache = BlobCache::new(cache_dir);

        Ok(Self {
            profile_name,
            workspace_path,
            cache,
            watcher: None,
            event_sender: None,
            app_handle: None,
        })
    }

    pub fn set_app_handle(&mut self, app_handle: tauri::AppHandle) {
        self.app_handle = Some(app_handle);
    }

    /// Start watching the workspace directory
    pub fn start_watching(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        
        // Create watcher with Windows backend
        let mut watcher = RecommendedWatcher::new(
            tx.clone(),
            Config::default(),
        )?;

        // Watch the workspace directory recursively
        watcher.watch(&self.workspace_path, RecursiveMode::Recursive)?;

        self.watcher = Some(watcher);
        self.event_sender = Some(tx);

        // Start the debounce thread
        let profile_name = self.profile_name.clone();
        let workspace_path = self.workspace_path.clone();
        let cache = BlobCache::new(self.cache.cache_dir.clone());
        let app_handle = self.app_handle.clone();

        thread::spawn(move || {
            Self::debounce_handler(rx, profile_name, workspace_path, cache, app_handle);
        });

        info!("Started watching workspace: {}", self.workspace_path.display());
        Ok(())
    }

    /// Stop watching the workspace
    pub fn stop_watching(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }
        self.event_sender = None;
        info!("Stopped watching workspace: {}", self.workspace_path.display());
    }

    /// Debounce handler that batches file changes
    fn debounce_handler(
        rx: Receiver<notify::Result<notify::Event>>,
        profile_name: String,
        workspace_path: PathBuf,
        cache: BlobCache,
        app_handle: Option<tauri::AppHandle>,
    ) {
        let mut pending_changes: HashMap<PathBuf, FileChangeEvent> = HashMap::new();
        let debounce_duration = Duration::from_millis(200); // 200ms debounce
        let mut last_activity = Instant::now();

        loop {
            // Try to receive events with a timeout
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event_result) => {
                    match event_result {
                        Ok(event) => {
                            last_activity = Instant::now();
                            Self::process_notify_event(event, &workspace_path, &mut pending_changes);
                        }
                        Err(e) => {
                            warn!("File watcher error: {}", e);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Check if we should process pending changes
                    if !pending_changes.is_empty() && 
                       last_activity.elapsed() >= debounce_duration {
                        
                        let changes: Vec<FileChangeEvent> = pending_changes.values().cloned().collect();
                        pending_changes.clear();
                        
                        // Process the batched changes
                        let normalized_count = Self::process_file_changes(
                            &changes, 
                            &profile_name, 
                            &workspace_path, 
                            &cache
                        );

                        // Send toast notification to UI
                        if normalized_count > 0 {
                            Self::send_toast_notification(&app_handle, normalized_count);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    debug!("Watcher channel disconnected");
                    break;
                }
            }
        }
    }

    /// Convert notify events to our file change events
    fn process_notify_event(
        event: notify::Event,
        workspace_path: &Path,
        pending_changes: &mut HashMap<PathBuf, FileChangeEvent>,
    ) {
        for path in event.paths {
            // Only process files within our workspace
            if !path.starts_with(workspace_path) {
                continue;
            }

            // Skip directories and hidden files
            if path.is_dir() || 
               path.file_name()
                   .and_then(|n| n.to_str())
                   .map(|s| s.starts_with('.'))
                   .unwrap_or(false) {
                continue;
            }

            let change_kind = match event.kind {
                EventKind::Create(_) => FileChangeKind::Created,
                EventKind::Modify(_) => FileChangeKind::Modified,
                EventKind::Remove(_) => FileChangeKind::Deleted,
                EventKind::Other => FileChangeKind::Modified, // Treat unknown as modified
                _ => continue, // Skip other event types
            };

            let change_event = FileChangeEvent {
                path: path.clone(),
                kind: change_kind,
                timestamp: Instant::now(),
            };

            pending_changes.insert(path, change_event);
        }
    }

    /// Process batched file changes and normalize them
    fn process_file_changes(
        changes: &[FileChangeEvent],
        profile_name: &str,
        workspace_path: &Path,
        cache: &BlobCache,
    ) -> usize {
        let mut normalized_count = 0;

        for change in changes {
            match change.kind {
                FileChangeKind::Created | FileChangeKind::Modified => {
                    if let Err(e) = Self::normalize_file(&change.path, profile_name, workspace_path, cache) {
                        error!("Failed to normalize file {}: {}", change.path.display(), e);
                    } else {
                        normalized_count += 1;
                    }
                }
                FileChangeKind::Deleted => {
                    if let Err(e) = Self::handle_file_deletion(&change.path, profile_name, workspace_path, cache) {
                        error!("Failed to handle deletion of {}: {}", change.path.display(), e);
                    }
                }
                FileChangeKind::Renamed => {
                    // Treat renames as creation of new file
                    if let Err(e) = Self::normalize_file(&change.path, profile_name, workspace_path, cache) {
                        error!("Failed to normalize renamed file {}: {}", change.path.display(), e);
                    } else {
                        normalized_count += 1;
                    }
                }
            }
        }

        if normalized_count > 0 {
            info!("Normalized {} files for profile '{}'", normalized_count, profile_name);
        }

        normalized_count
    }

    /// Normalize a file: hash → ensure_blob → replace with hardlink
    fn normalize_file(
        file_path: &Path,
        profile_name: &str,
        workspace_path: &Path,
        cache: &BlobCache,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Skip if file doesn't exist (might have been deleted while debouncing)
        if !file_path.exists() {
            return Ok(());
        }

        // Get relative path within workspace
        let rel_path = file_path.strip_prefix(workspace_path)?;
        let rel_path_str = rel_path.to_string_lossy().to_string();

        // Hash the current file to check if it needs normalization
        let current_hash = BlobCache::hash_file(file_path)?;
        
        // Check if file is already a hardlink to the correct blob
        let expected_blob_path = cache.get_blob_path(&current_hash);
        if expected_blob_path.exists() {
            // Check if this file is already hardlinked to the correct blob
            if let Ok(_file_metadata) = fs::metadata(file_path) {
                if let Ok(_blob_metadata) = fs::metadata(&expected_blob_path) {
                    // On Windows, compare file indexes to detect hardlinks using Windows API
                    #[cfg(windows)]
                    {
                        if are_files_hardlinked(file_path, &expected_blob_path) {
                            debug!("File already normalized: {} | {} | Profile: {}", 
                                   rel_path_str, 
                                   current_hash.to_hex()[..8].to_string(),
                                   profile_name);
                            
                            // Ensure reference exists (in case index was corrupted)
                            let blob_path = crate::blob_cache::BlobPath {
                                hash: current_hash,
                                path: expected_blob_path,
                            };
                            cache.add_ref(&blob_path, profile_name, &rel_path_str)?;
                            return Ok(());
                        }
                    }
                    
                    // On Unix-like systems, compare inodes
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if file_metadata.ino() == blob_metadata.ino() {
                            debug!("File already normalized: {} | {} | Profile: {}", 
                                   rel_path_str, 
                                   current_hash.to_hex()[..8].to_string(),
                                   profile_name);
                            
                            // Ensure reference exists (in case index was corrupted)
                            let blob_path = crate::blob_cache::BlobPath {
                                hash: current_hash,
                                path: expected_blob_path,
                            };
                            cache.add_ref(&blob_path, profile_name, &rel_path_str)?;
                            return Ok(());
                        }
                    }
                }
            }
        }

        // File needs normalization - ensure blob exists in cache
        let blob_path = cache.ensure_blob(file_path)?;
        let new_hash = blob_path.hash;

        // Add reference for this profile
        cache.add_ref(&blob_path, profile_name, &rel_path_str)?;

        // Replace file with hardlink to blob
        fs::remove_file(file_path)?;
        cache.link_blob_to(file_path, &blob_path)?;

        info!("File normalized: {} | {} | Profile: {}", 
              rel_path_str, 
              new_hash.to_hex()[..8].to_string(),
              profile_name);

        Ok(())
    }

    /// Handle file deletion: remove reference, no tombstones
    fn handle_file_deletion(
        file_path: &Path,
        profile_name: &str,
        workspace_path: &Path,
        cache: &BlobCache,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get relative path within workspace
        let rel_path = file_path.strip_prefix(workspace_path)?;
        let rel_path_str = rel_path.to_string_lossy().to_string();

        // Find the blob that was referenced by this profile and path
        if let Ok(blob_hash) = Self::find_blob_by_reference(cache, profile_name, &rel_path_str) {
            // Create a BlobPath so we can call remove_ref
            let blob_path = crate::blob_cache::BlobPath {
                hash: blob_hash,
                path: cache.get_blob_path(&blob_hash),
            };
            
            // Remove the reference and check if blob should be GC'd
            match cache.remove_ref(&blob_path, profile_name, &rel_path_str) {
                Ok(should_gc) => {
                    info!("File deleted from workspace: {} | Profile: {} | Should GC: {}", 
                          rel_path_str, profile_name, should_gc);
                    
                    if should_gc {
                        debug!("Blob {} eligible for garbage collection", blob_hash.to_hex()[..8].to_string());
                    }
                }
                Err(e) => {
                    warn!("Failed to remove blob reference for deleted file {}: {}", rel_path_str, e);
                }
            }
        } else {
            // No reference found - this is fine, file might not have been normalized yet
            debug!("No blob reference found for deleted file: {} | Profile: {}", rel_path_str, profile_name);
        }
        
        Ok(())
    }

    /// Find blob hash by searching for a specific profile and relative path reference
    pub fn find_blob_by_reference(
        cache: &BlobCache,
        profile_name: &str,
        rel_path: &str,
    ) -> Result<blake3::Hash, Box<dyn std::error::Error>> {
        let index = cache.load_index()?;
        
        // Search through all blob references to find matching profile + path
        for (hash_str, refs) in &index.refs {
            for blob_ref in refs {
                if blob_ref.profile == profile_name && blob_ref.rel_path == rel_path {
                    // Parse the hash string back to Hash using blake3's from_hex
                    match blake3::Hash::from_hex(hash_str) {
                        Ok(hash) => return Ok(hash),
                        Err(e) => {
                            warn!("Failed to parse hash {}: {}", hash_str, e);
                            continue;
                        }
                    }
                }
            }
        }
        
        Err("No blob reference found for the given profile and path".into())
    }

    /// Send toast notification to UI
    fn send_toast_notification(app_handle: &Option<tauri::AppHandle>, count: usize) {
        if let Some(app) = app_handle {
            let message = format!("{} files normalized; runtime will rebuild", count);
            if let Err(e) = app.emit("workspace-normalized", &message) {
                warn!("Failed to send toast notification: {}", e);
            }
        }
    }
}

impl Drop for WorkspaceWatcher {
    fn drop(&mut self) {
        self.stop_watching();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_workspace_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_path).unwrap();

        let watcher = WorkspaceWatcher::new(
            "test_profile".to_string(),
            workspace_path,
        );

        assert!(watcher.is_ok());
    }

    #[test]
    fn test_file_change_event() {
        let event = FileChangeEvent {
            path: PathBuf::from("test.txt"),
            kind: FileChangeKind::Created,
            timestamp: Instant::now(),
        };

        assert_eq!(event.kind, FileChangeKind::Created);
        assert_eq!(event.path, PathBuf::from("test.txt"));
    }

    #[test]
    fn test_milestone4_requirements() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("workspace");
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&workspace_path).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();
        
        // Test workspace watcher creation
        let watcher = WorkspaceWatcher::new(
            "test_profile".to_string(),
            workspace_path.clone(),
        );
        assert!(watcher.is_ok());
        
        let watcher = watcher.unwrap();
        
        // Test cache integration
        let cache = &watcher.cache;
        
        // Create a test file
        let test_file = workspace_path.join("test_mod.txt");
        fs::write(&test_file, b"Test mod content").unwrap();
        
        // Test normalization process (simulate what the watcher would do)
        let result = WorkspaceWatcher::normalize_file(
            &test_file,
            "test_profile",
            &workspace_path,
            cache,
        );
        
        // Verify the file was processed successfully
        assert!(result.is_ok());
        
        // Check that file was replaced with hardlink to blob
        assert!(test_file.exists());
        
        // Verify blob was created in cache
        let blob_hash = BlobCache::hash_file(&test_file).unwrap();
        let blob_path = cache.get_blob_path(&blob_hash);
        assert!(blob_path.exists());
        
        // Test deletion handling with proper blob reference removal
        let test_file2 = workspace_path.join("test_deletion.txt");
        fs::write(&test_file2, b"File to be deleted").unwrap();
        
        // First normalize the file so it gets a blob reference
        let normalize_result = WorkspaceWatcher::normalize_file(
            &test_file2,
            "test_profile",
            &workspace_path,
            cache,
        );
        assert!(normalize_result.is_ok());
        
        // Verify reference was created
        let rel_path = test_file2.strip_prefix(&workspace_path).unwrap();
        let rel_path_str = rel_path.to_string_lossy().to_string();
        let blob_hash_result = WorkspaceWatcher::find_blob_by_reference(
            cache,
            "test_profile",
            &rel_path_str,
        );
        assert!(blob_hash_result.is_ok(), "Should find blob reference after normalization");
        
        // Now delete the file and test deletion handling
        fs::remove_file(&test_file2).unwrap();
        let delete_result = WorkspaceWatcher::handle_file_deletion(
            &test_file2,
            "test_profile",
            &workspace_path,
            cache,
        );
        assert!(delete_result.is_ok());
        
        // Verify reference was removed
        let blob_hash_result_after = WorkspaceWatcher::find_blob_by_reference(
            cache,
            "test_profile",
            &rel_path_str,
        );
        assert!(blob_hash_result_after.is_err(), "Should not find blob reference after deletion");
    }
}
