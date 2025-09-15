use std::path::{Path, PathBuf};
use blake3::{Hash, Hasher};
use std::fs;
use std::io::{self, Read};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    cache_dir: PathBuf,
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

    /// Get the blob directory path following the layout: cache/blobs/blake3/aa/hash
    fn get_blob_path(&self, hash: &Hash) -> PathBuf {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}