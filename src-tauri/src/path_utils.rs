use std::path::{Path, PathBuf};
use anyhow::{Result, bail};
use crate::long_path::to_long_path;

/// Safely joins paths with long-path support and normalization
///
/// This function joins path components, normalizes the result, and ensures
/// long-path support when necessary.
///
/// # Arguments
/// * `base` - The base path
/// * `path` - The path to join to the base
///
/// # Returns
/// A normalized `PathBuf` with long-path support if needed
pub fn safe_join<P: AsRef<Path>, Q: AsRef<Path>>(base: P, path: Q) -> Result<PathBuf> {
    let base = base.as_ref();
    let path = path.as_ref();
    
    // Prevent path traversal attacks
    if path.is_absolute() {
        bail!("Cannot join absolute path to base: {}", path.display());
    }
    
    // Check for directory traversal attempts
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            // Allow limited parent directory traversal, but validate the result
        }
    }
    
    let joined = base.join(path);
    let normalized = normalize_path(&joined)?;
    
    // Ensure the result is still within or below the base directory
    let base_canonical = base.canonicalize()
        .unwrap_or_else(|_| base.to_path_buf());
    let result_canonical = normalized.canonicalize()
        .unwrap_or_else(|_| normalized.clone());
    
    if !result_canonical.starts_with(&base_canonical) {
        bail!("Path traversal detected: result '{}' is outside base '{}'", 
              result_canonical.display(), base_canonical.display());
    }
    
    to_long_path(&normalized, false)
}

/// Normalizes a path by resolving `.` and `..` components
///
/// This function cleans up path components without requiring the path to exist.
///
/// # Arguments
/// * `path` - The path to normalize
///
/// # Returns
/// A normalized `PathBuf`
pub fn normalize_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    let mut components = Vec::new();
    
    for component in path.components() {
        match component {
            std::path::Component::Prefix(_prefix) => components.push(component),
            std::path::Component::RootDir => components.push(component),
            std::path::Component::CurDir => {
                // Skip current directory components
            }
            std::path::Component::ParentDir => {
                // Pop the last component if it's not a root or prefix
                if let Some(last) = components.last() {
                    match last {
                        std::path::Component::Prefix(_) | 
                        std::path::Component::RootDir => {
                            // Can't go above root
                        }
                        _ => {
                            components.pop();
                        }
                    }
                }
            }
            std::path::Component::Normal(_) => components.push(component),
        }
    }
    
    let mut result = PathBuf::new();
    for component in components {
        result.push(component.as_os_str());
    }
    
    Ok(result)
}

/// Detects if a path is on an NTFS volume
///
/// This function checks if the given path is located on an NTFS file system,
/// which is required for hardlink operations.
///
/// # Arguments
/// * `path` - The path to check
///
/// # Returns
/// `true` if the path is on an NTFS volume, `false` otherwise
pub fn is_ntfs_volume<P: AsRef<Path>>(path: P) -> Result<bool> {
    let path = path.as_ref();
    let drive = get_drive_letter(path)?;
    
    if let Some(drive_letter) = drive {
        // Use Windows API to get file system type
        let drive_root = format!("{}:\\", drive_letter);
        
        #[cfg(windows)]
        {
            use windows::core::PCWSTR;
            use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
            
            let drive_root_wide: Vec<u16> = drive_root.encode_utf16().chain(std::iter::once(0)).collect();
            let mut fs_name = [0u16; 256];
            
            unsafe {
                let result = GetVolumeInformationW(
                    PCWSTR(drive_root_wide.as_ptr()),
                    None, // volume name buffer
                    None, // volume serial number
                    None, // maximum component length
                    None, // file system flags
                    Some(&mut fs_name), // file system name buffer
                );
                
                if result.is_ok() {
                    let fs_name_str = String::from_utf16_lossy(&fs_name);
                    let fs_name_clean = fs_name_str.trim_end_matches('\0');
                    return Ok(fs_name_clean.eq_ignore_ascii_case("NTFS"));
                }
            }
        }
        
        #[cfg(not(windows))]
        {
            // On non-Windows systems, assume false
            return Ok(false);
        }
    }
    
    Ok(false)
}

/// Extracts the drive letter from a Windows path
///
/// # Arguments
/// * `path` - The path to extract the drive letter from
///
/// # Returns
/// The drive letter (e.g., 'C') if found, None otherwise
pub fn get_drive_letter<P: AsRef<Path>>(path: P) -> Result<Option<char>> {
    let path = path.as_ref();
    
    // Convert to absolute path first if relative
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    
    // Get the path string
    let path_str = abs_path.to_string_lossy();
    
    // Handle long-path prefix
    let clean_path = if path_str.starts_with(r"\\?\") {
        &path_str[4..]
    } else {
        &path_str
    };
    
    // Extract drive letter (format should be "C:" or "C:\...")
    if clean_path.len() >= 2 && clean_path.chars().nth(1) == Some(':') {
        let drive_char = clean_path.chars().next().unwrap().to_ascii_uppercase();
        if drive_char.is_ascii_alphabetic() {
            return Ok(Some(drive_char));
        }
    }
    
    Ok(None)
}

/// Checks if two paths are on the same volume
///
/// This is important for hardlink operations, which require both paths
/// to be on the same NTFS volume.
///
/// # Arguments
/// * `path1` - First path
/// * `path2` - Second path
///
/// # Returns
/// `true` if both paths are on the same volume, `false` otherwise
pub fn same_volume<P: AsRef<Path>, Q: AsRef<Path>>(path1: P, path2: Q) -> Result<bool> {
    let drive1 = get_drive_letter(path1)?;
    let drive2 = get_drive_letter(path2)?;
    
    match (drive1, drive2) {
        (Some(d1), Some(d2)) => Ok(d1 == d2),
        _ => Ok(false),
    }
}

/// Gets the available free space on the volume containing the given path
///
/// # Arguments
/// * `path` - Path to check free space for
///
/// # Returns
/// Available free space in bytes
pub fn get_free_space<P: AsRef<Path>>(path: P) -> Result<u64> {
    let path = path.as_ref();
    let drive = get_drive_letter(path)?;
    
    if let Some(drive_letter) = drive {
        let drive_root = format!("{}:\\", drive_letter);
        
        #[cfg(windows)]
        {
            use windows::core::PCWSTR;
            use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
            
            let drive_root_wide: Vec<u16> = drive_root.encode_utf16().chain(std::iter::once(0)).collect();
            let mut free_bytes = 0u64;
            
            unsafe {
                let result = GetDiskFreeSpaceExW(
                    PCWSTR(drive_root_wide.as_ptr()),
                    Some(&mut free_bytes),
                    None, // total bytes
                    None, // total free bytes
                );
                
                if result.is_ok() {
                    return Ok(free_bytes);
                }
            }
        }
        
        #[cfg(not(windows))]
        {
            // On non-Windows systems, use std::fs::metadata approach
            let metadata = std::fs::metadata(path)?;
            // This is a simplified approach - in reality you'd need platform-specific code
            return Ok(u64::MAX); // Placeholder
        }
    }
    
    bail!("Could not determine free space for path: {}", path.display())
}

/// Converts a file size in bytes to a human-readable string
///
/// # Arguments
/// * `bytes` - Size in bytes
///
/// # Returns
/// Human-readable size string (e.g., "1.5 GB")
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_normalize_path() {
        let path = PathBuf::from(r"C:\foo\.\bar\..\baz");
        let normalized = normalize_path(&path).unwrap();
        assert_eq!(normalized, PathBuf::from(r"C:\foo\baz"));
    }

    #[test]
    fn test_safe_join() {
        let base = PathBuf::from(r"C:\base");
        let path = PathBuf::from(r"subdir\file.txt");
        let result = safe_join(&base, &path).unwrap();
        assert!(result.to_string_lossy().contains(r"C:\base\subdir\file.txt"));
    }

    #[test]
    fn test_get_drive_letter() {
        let path = PathBuf::from(r"C:\Windows\System32");
        let drive = get_drive_letter(&path).unwrap();
        assert_eq!(drive, Some('C'));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
    }

    #[test]
    fn test_path_traversal_prevention() {
        let base = PathBuf::from(r"C:\base");
        let malicious_path = PathBuf::from(r"..\..\..\Windows\System32");
        
        // This should fail due to path traversal detection
        let result = safe_join(&base, &malicious_path);
        assert!(result.is_err());
    }
}