use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

/// Windows long-path prefix for paths exceeding MAX_PATH (260 characters)
const LONG_PATH_PREFIX: &str = r"\\?\";

/// Maximum path length on Windows without the long-path prefix
const MAX_PATH_LENGTH: usize = 260;

/// Converts a path to a Windows long-path format if necessary
/// 
/// This function adds the `\\?\` prefix to paths that are longer than 260 characters
/// or when force_long_path is true. This allows Windows to handle paths up to 
/// approximately 32,767 characters.
///
/// # Arguments
/// * `path` - The path to convert
/// * `force_long_path` - If true, always add the prefix regardless of length
///
/// # Returns
/// A `PathBuf` with the long-path prefix if needed
pub fn to_long_path<P: AsRef<Path>>(path: P, force_long_path: bool) -> Result<PathBuf> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();
    
    // If already has long-path prefix, return as-is
    if path_str.starts_with(LONG_PATH_PREFIX) {
        return Ok(path.to_path_buf());
    }
    
    // Check if we need to add the prefix
    let needs_prefix = force_long_path || 
                      path_str.len() >= MAX_PATH_LENGTH ||
                      path_str.chars().count() >= MAX_PATH_LENGTH;
    
    if needs_prefix {
        // Convert to absolute path first
        let absolute_path = path.canonicalize()
            .or_else(|_| {
                // If canonicalize fails (path doesn't exist), try to make it absolute manually
                if path.is_absolute() {
                    Ok(path.to_path_buf())
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(path))
                        .context("Failed to get current directory")
                }
            })?;
        
        let long_path = format!("{}{}", LONG_PATH_PREFIX, absolute_path.display());
        Ok(PathBuf::from(long_path))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Removes the Windows long-path prefix if present
///
/// # Arguments
/// * `path` - The path that may have the long-path prefix
///
/// # Returns
/// A `PathBuf` without the long-path prefix
pub fn from_long_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path_str = path.as_ref().to_string_lossy();
    if path_str.starts_with(LONG_PATH_PREFIX) {
        PathBuf::from(&path_str[LONG_PATH_PREFIX.len()..])
    } else {
        path.as_ref().to_path_buf()
    }
}

/// Checks if a path has the Windows long-path prefix
///
/// # Arguments
/// * `path` - The path to check
///
/// # Returns
/// `true` if the path has the long-path prefix, `false` otherwise
pub fn is_long_path<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().to_string_lossy().starts_with(LONG_PATH_PREFIX)
}

/// Safely opens a file with long-path support
///
/// This function ensures that file operations work with paths longer than MAX_PATH
/// by automatically converting to long-path format when necessary.
///
/// # Arguments
/// * `path` - The path to the file
///
/// # Returns
/// A `std::fs::File` handle
pub fn open_file_long_path<P: AsRef<Path>>(path: P) -> Result<std::fs::File> {
    let long_path = to_long_path(&path, false)?;
    std::fs::File::open(&long_path)
        .with_context(|| format!("Failed to open file: {}", path.as_ref().display()))
}

/// Safely creates a file with long-path support
///
/// # Arguments
/// * `path` - The path to the file to create
///
/// # Returns
/// A `std::fs::File` handle for the created file
pub fn create_file_long_path<P: AsRef<Path>>(path: P) -> Result<std::fs::File> {
    let long_path = to_long_path(&path, false)?;
    
    // Ensure parent directory exists
    if let Some(parent) = long_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory for: {}", path.as_ref().display()))?;
    }
    
    std::fs::File::create(&long_path)
        .with_context(|| format!("Failed to create file: {}", path.as_ref().display()))
}

/// Creates a directory with long-path support
///
/// # Arguments
/// * `path` - The path to the directory to create
///
/// # Returns
/// `Ok(())` if successful
pub fn create_dir_all_long_path<P: AsRef<Path>>(path: P) -> Result<()> {
    let long_path = to_long_path(&path, false)?;
    std::fs::create_dir_all(&long_path)
        .with_context(|| format!("Failed to create directory: {}", path.as_ref().display()))
}

/// Removes a file with long-path support
///
/// # Arguments
/// * `path` - The path to the file to remove
///
/// # Returns
/// `Ok(())` if successful
pub fn remove_file_long_path<P: AsRef<Path>>(path: P) -> Result<()> {
    let long_path = to_long_path(&path, false)?;
    std::fs::remove_file(&long_path)
        .with_context(|| format!("Failed to remove file: {}", path.as_ref().display()))
}

/// Removes a directory with long-path support
///
/// # Arguments
/// * `path` - The path to the directory to remove
///
/// # Returns
/// `Ok(())` if successful
pub fn remove_dir_all_long_path<P: AsRef<Path>>(path: P) -> Result<()> {
    let long_path = to_long_path(&path, false)?;
    std::fs::remove_dir_all(&long_path)
        .with_context(|| format!("Failed to remove directory: {}", path.as_ref().display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_to_long_path_short_path() {
        let short_path = PathBuf::from(r"C:\short\path");
        let result = to_long_path(&short_path, false).unwrap();
        // Should not add prefix for short paths
        assert!(!result.to_string_lossy().starts_with(LONG_PATH_PREFIX));
    }

    #[test]
    fn test_to_long_path_force() {
        let short_path = PathBuf::from(r"C:\short\path");
        let result = to_long_path(&short_path, true).unwrap();
        // Should add prefix when forced
        assert!(result.to_string_lossy().starts_with(LONG_PATH_PREFIX));
    }

    #[test]
    fn test_from_long_path() {
        let long_path = PathBuf::from(r"\\?\C:\some\long\path");
        let result = from_long_path(&long_path);
        assert_eq!(result, PathBuf::from(r"C:\some\long\path"));
    }

    #[test]
    fn test_is_long_path() {
        let long_path = PathBuf::from(r"\\?\C:\some\long\path");
        let normal_path = PathBuf::from(r"C:\some\normal\path");
        
        assert!(is_long_path(&long_path));
        assert!(!is_long_path(&normal_path));
    }

    #[test]
    fn test_already_long_path() {
        let already_long = PathBuf::from(r"\\?\C:\already\long\path");
        let result = to_long_path(&already_long, false).unwrap();
        assert_eq!(result, already_long);
    }
}