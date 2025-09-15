use std::path::{Path, PathBuf};
use blake3::{Hash, Hasher};
use std::fs;
use std::io::{self, Read};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Represents a blob path in the cache
#[derive(Debug, Clone)]
pub struct BlobPath {
    pub hash: Hash,
    pub path: PathBuf,
}

/// Structure for tracking blob references in index.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobReference {
    pub profile: String,
    pub rel_path: String,
}

/// Index structure for blob reference tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlobIndex {
    pub refs: HashMap<String, Vec<BlobReference>>, // hash -> list of references
}

/// Content-addressed blob cache manager
pub struct BlobCache {
    pub cache_dir: PathBuf,
}

impl BlobCache {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Self {
        Self {
            cache_dir: cache_dir.as_ref().to_path_buf(),
        }
    }

    /// Hash a file using BLAKE3
    pub fn hash_file<P: AsRef<Path>>(file_path: P) -> io::Result<Hash> {
        let mut file = fs::File::open(file_path)?;
        let mut hasher = Hasher::new();
        let mut buffer = [0; 8192]; // 8KB buffer for reading

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hasher.finalize())
    }

    /// Ensure a file is stored in the blob cache, returning the blob path
    /// If the blob already exists, returns existing path without copying
    /// If absent, copies the file into the blob storage
    pub fn ensure_blob<P: AsRef<Path>>(&self, file_path: P) -> io::Result<BlobPath> {
        let file_path = file_path.as_ref();
        
        // Hash the file
        let hash = Self::hash_file(file_path)?;
        let blob_path = self.get_blob_path(&hash);
        
        // If blob already exists, return it
        if blob_path.exists() {
            return Ok(BlobPath {
                hash,
                path: blob_path,
            });
        }
        
        // Create directory structure if it doesn't exist
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Copy file to blob storage
        fs::copy(file_path, &blob_path)?;
        
        Ok(BlobPath {
            hash,
            path: blob_path,
        })
    }

    /// Create a hardlink from a blob to a destination with atomic temp → rename operation
    /// This ensures the destination either gets the complete file or nothing
    /// CRITICAL: This method ONLY creates hardlinks - never copies. If hardlink fails, operation fails.
    pub fn link_blob_to<P: AsRef<Path>>(&self, dst: P, blob: &BlobPath) -> io::Result<()> {
        let dst = dst.as_ref();
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Generate a temporary filename in the same directory as the destination
        let temp_name = format!(".tmp_{}", Uuid::new_v4());
        let temp_path = dst.parent().unwrap_or(Path::new(".")).join(temp_name);
        
        // ONLY create hardlink - no fallback to copy
        // This enforces the zero-overhead workspace principle
        fs::hard_link(&blob.path, &temp_path)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to create hardlink from '{}' to '{}': {}. Ensure cache and workspace are on the same NTFS volume.", 
                            blob.path.display(), temp_path.display(), e)
                )
            })?;
        
        // Hardlink successful, atomically rename to final destination
        fs::rename(&temp_path, dst)?;
        Ok(())
    }

    /// Get the blob directory path following the layout: cache/blobs/blake3/aa/hash
    pub fn get_blob_path(&self, hash: &Hash) -> PathBuf {
        let hash_str = hash.to_hex().to_string();
        let prefix = &hash_str[0..2]; // First 2 characters for directory sharding
        
        self.cache_dir
            .join("blobs")
            .join("blake3")
            .join(prefix)
            .join(&hash_str)
    }

    /// Get the index.json path
    fn get_index_path(&self) -> PathBuf {
        self.cache_dir.join("blobs").join("index.json")
    }

    /// Load blob index from disk
    pub fn load_index(&self) -> io::Result<BlobIndex> {
        let index_path = self.get_index_path();
        
        if !index_path.exists() {
            return Ok(BlobIndex::default());
        }
        
        let content = fs::read_to_string(index_path)?;
        let index: BlobIndex = serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        Ok(index)
    }

    /// Save blob index to disk
    fn save_index(&self, index: &BlobIndex) -> io::Result<()> {
        let index_path = self.get_index_path();
        
        // Create directory if it doesn't exist
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(index)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        fs::write(index_path, content)?;
        Ok(())
    }

    /// Add a reference to a blob
    pub fn add_ref(&self, blob: &BlobPath, profile: &str, rel_path: &str) -> io::Result<()> {
        let mut index = self.load_index()?;
        let hash_str = blob.hash.to_hex().to_string();
        
        let refs = index.refs.entry(hash_str).or_insert_with(Vec::new);
        
        // Check if reference already exists
        let new_ref = BlobReference {
            profile: profile.to_string(),
            rel_path: rel_path.to_string(),
        };
        
        if !refs.iter().any(|r| r.profile == new_ref.profile && r.rel_path == new_ref.rel_path) {
            refs.push(new_ref);
            self.save_index(&index)?;
        }
        
        Ok(())
    }

    /// Remove a reference from a blob
    /// Returns true if the blob has no more references and can be garbage collected
    pub fn remove_ref(&self, blob: &BlobPath, profile: &str, rel_path: &str) -> io::Result<bool> {
        let mut index = self.load_index()?;
        let hash_str = blob.hash.to_hex().to_string();
        
        let should_remove_blob = if let Some(refs) = index.refs.get_mut(&hash_str) {
            // Remove the specific reference
            refs.retain(|r| !(r.profile == profile && r.rel_path == rel_path));
            
            // If no references left, remove the entire entry and return true for GC
            if refs.is_empty() {
                index.refs.remove(&hash_str);
                true
            } else {
                false
            }
        } else {
            // No references found, blob can be removed
            true
        };
        
        self.save_index(&index)?;
        Ok(should_remove_blob)
    }

    /// Get all references for a blob
    pub fn get_refs(&self, blob: &BlobPath) -> io::Result<Vec<BlobReference>> {
        let index = self.load_index()?;
        let hash_str = blob.hash.to_hex().to_string();
        
        Ok(index.refs.get(&hash_str).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_blob_path_layout() {
        let temp_dir = TempDir::new().unwrap();
        let cache = BlobCache::new(temp_dir.path());
        
        // Create a test hash
        let hash = blake3::hash(b"test content");
        let blob_path = cache.get_blob_path(&hash);
        
        let hash_str = hash.to_hex().to_string();
        let expected_path = temp_dir.path()
            .join("blobs")
            .join("blake3")
            .join(&hash_str[0..2])
            .join(&hash_str);
        
        assert_eq!(blob_path, expected_path);
    }

    #[test]
    fn test_hash_file() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        
        // Create a test file
        let content = b"Hello, World!";
        let mut file = File::create(&test_file).unwrap();
        file.write_all(content).unwrap();
        
        // Hash the file
        let hash = BlobCache::hash_file(&test_file).unwrap();
        
        // Compare with direct hash of content
        let expected_hash = blake3::hash(content);
        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn test_ensure_blob() {
        let temp_dir = TempDir::new().unwrap();
        let cache = BlobCache::new(temp_dir.path());
        let test_file = temp_dir.path().join("test.txt");
        
        // Create a test file
        let content = b"Test content for blob";
        let mut file = File::create(&test_file).unwrap();
        file.write_all(content).unwrap();
        
        // Ensure blob
        let blob_path = cache.ensure_blob(&test_file).unwrap();
        
        // Verify blob file exists
        assert!(blob_path.path.exists());
        
        // Verify content matches
        let blob_content = fs::read(&blob_path.path).unwrap();
        assert_eq!(blob_content, content);
        
        // Ensure calling again returns the same path without copying
        let blob_path2 = cache.ensure_blob(&test_file).unwrap();
        assert_eq!(blob_path.path, blob_path2.path);
        assert_eq!(blob_path.hash, blob_path2.hash);
    }

    #[test]
    fn test_link_blob_to() {
        let temp_dir = TempDir::new().unwrap();
        let cache = BlobCache::new(temp_dir.path());
        let test_file = temp_dir.path().join("test.txt");
        let dest_file = temp_dir.path().join("dest.txt");
        
        // Create a test file
        let content = b"Content for linking test";
        let mut file = File::create(&test_file).unwrap();
        file.write_all(content).unwrap();
        
        // Ensure blob
        let blob_path = cache.ensure_blob(&test_file).unwrap();
        
        // Link blob to destination
        cache.link_blob_to(&dest_file, &blob_path).unwrap();
        
        // Verify destination file exists and has correct content
        assert!(dest_file.exists());
        let dest_content = fs::read(&dest_file).unwrap();
        assert_eq!(dest_content, content);
        
        // Verify files are linked (same inode on Unix-like systems) or at least identical
        let original_metadata = fs::metadata(&blob_path.path).unwrap();
        let dest_metadata = fs::metadata(&dest_file).unwrap();
        assert_eq!(original_metadata.len(), dest_metadata.len());
    }

    #[test]
    fn test_index_operations() {
        let temp_dir = TempDir::new().unwrap();
        let cache = BlobCache::new(temp_dir.path());
        
        // Test loading empty index
        let index = cache.load_index().unwrap();
        assert!(index.refs.is_empty());
        
        // Create test index
        let mut test_index = BlobIndex::default();
        test_index.refs.insert(
            "test_hash".to_string(),
            vec![
                BlobReference {
                    profile: "profile1".to_string(),
                    rel_path: "data/test.txt".to_string(),
                },
                BlobReference {
                    profile: "profile2".to_string(),
                    rel_path: "mods/test.txt".to_string(),
                },
            ]
        );
        
        // Save and reload
        cache.save_index(&test_index).unwrap();
        let loaded_index = cache.load_index().unwrap();
        
        assert_eq!(loaded_index.refs.len(), 1);
        assert_eq!(loaded_index.refs["test_hash"].len(), 2);
        assert_eq!(loaded_index.refs["test_hash"][0].profile, "profile1");
        assert_eq!(loaded_index.refs["test_hash"][0].rel_path, "data/test.txt");
    }

    #[test]
    fn test_reference_management() {
        let temp_dir = TempDir::new().unwrap();
        let cache = BlobCache::new(temp_dir.path());
        let test_file = temp_dir.path().join("test.txt");
        
        // Create a test file and blob
        let content = b"Reference management test";
        let mut file = File::create(&test_file).unwrap();
        file.write_all(content).unwrap();
        
        let blob_path = cache.ensure_blob(&test_file).unwrap();
        
        // Add references
        cache.add_ref(&blob_path, "profile1", "data/test.txt").unwrap();
        cache.add_ref(&blob_path, "profile2", "mods/test.txt").unwrap();
        
        // Get references
        let refs = cache.get_refs(&blob_path).unwrap();
        assert_eq!(refs.len(), 2);
        
        // Test duplicate reference (should not add)
        cache.add_ref(&blob_path, "profile1", "data/test.txt").unwrap();
        let refs = cache.get_refs(&blob_path).unwrap();
        assert_eq!(refs.len(), 2); // Still 2, not 3
        
        // Remove one reference
        let should_gc = cache.remove_ref(&blob_path, "profile1", "data/test.txt").unwrap();
        assert!(!should_gc); // Still has references
        
        let refs = cache.get_refs(&blob_path).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].profile, "profile2");
        
        // Remove last reference
        let should_gc = cache.remove_ref(&blob_path, "profile2", "mods/test.txt").unwrap();
        assert!(should_gc); // No more references, should GC
        
        let refs = cache.get_refs(&blob_path).unwrap();
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_milestone3_requirements() {
        let temp_dir = TempDir::new().unwrap();
        let cache = BlobCache::new(temp_dir.path());
        
        // Test 1: Global cache - identical files produce same blob
        let file1 = temp_dir.path().join("source1.txt");
        let file2 = temp_dir.path().join("source2.txt");
        let identical_content = b"This is identical content across files";
        
        fs::write(&file1, identical_content).unwrap();
        fs::write(&file2, identical_content).unwrap();
        
        let blob1 = cache.ensure_blob(&file1).unwrap();
        let blob2 = cache.ensure_blob(&file2).unwrap();
        
        // Same hash for identical content (global deduplication)
        assert_eq!(blob1.hash, blob2.hash);
        // Same physical path (only one copy stored)
        assert_eq!(blob1.path, blob2.path);
        
        // Test 2: Different versions produce different blobs
        let handling_v1 = temp_dir.path().join("handling_v1.cfg");
        let handling_v2 = temp_dir.path().join("handling_v2.cfg");
        
        fs::write(&handling_v1, b"version 1 of handling.cfg").unwrap();
        fs::write(&handling_v2, b"version 2 of handling.cfg").unwrap();
        
        let blob_v1 = cache.ensure_blob(&handling_v1).unwrap();
        let blob_v2 = cache.ensure_blob(&handling_v2).unwrap();
        
        // Different hashes for different content
        assert_ne!(blob_v1.hash, blob_v2.hash);
        // Different physical paths
        assert_ne!(blob_v1.path, blob_v2.path);
        
        // Test 3: Multiple profiles pointing to same blob
        cache.add_ref(&blob1, "profile1", "data/shared.txt").unwrap();
        cache.add_ref(&blob1, "profile2", "mods/shared.txt").unwrap();
        cache.add_ref(&blob1, "profile3", "assets/shared.txt").unwrap();
        
        let refs = cache.get_refs(&blob1).unwrap();
        assert_eq!(refs.len(), 3);
        
        // Test 4: Hardlink creation (atomic temp → rename)
        let workspace_file = temp_dir.path().join("workspace_copy.txt");
        cache.link_blob_to(&workspace_file, &blob1).unwrap();
        
        // Verify content is identical
        let workspace_content = fs::read(&workspace_file).unwrap();
        assert_eq!(workspace_content, identical_content);
        
        // Test 5: Lifecycle management - blob persists while referenced
        let should_gc = cache.remove_ref(&blob1, "profile1", "data/shared.txt").unwrap();
        assert!(!should_gc); // Still has 2 references
        
        let should_gc = cache.remove_ref(&blob1, "profile2", "mods/shared.txt").unwrap();
        assert!(!should_gc); // Still has 1 reference
        
        let should_gc = cache.remove_ref(&blob1, "profile3", "assets/shared.txt").unwrap();
        assert!(should_gc); // No references left, can be GC'd
        
        // Test 6: Multiple profiles can reference different versions of same filename
        cache.add_ref(&blob_v1, "mod_profile_a", "data/handling.cfg").unwrap();
        cache.add_ref(&blob_v2, "mod_profile_b", "data/handling.cfg").unwrap();
        
        let v1_refs = cache.get_refs(&blob_v1).unwrap();
        let v2_refs = cache.get_refs(&blob_v2).unwrap();
        
        assert_eq!(v1_refs.len(), 1);
        assert_eq!(v2_refs.len(), 1);
        assert_eq!(v1_refs[0].profile, "mod_profile_a");
        assert_eq!(v2_refs[0].profile, "mod_profile_b");
        // Same relative path, different blobs
        assert_eq!(v1_refs[0].rel_path, "data/handling.cfg");
        assert_eq!(v2_refs[0].rel_path, "data/handling.cfg");
    }
}