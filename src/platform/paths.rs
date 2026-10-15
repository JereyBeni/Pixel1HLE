//! Sandbox & apps directory layout (touchHLE-inspired)

use std::path::{Path, PathBuf};

/// Whether `name` is a single ordinary path component (blocks `..`, `/`, `\\`).
pub fn is_plain_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && matches!(
            (components.next(), components.next()),
            (Some(std::path::Component::Normal(_)), None)
        )
}

/// Base directory for emulator user data.
pub fn sandbox_base() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // App-specific external/internal storage
        for candidate in [
            "/data/data/com.pixel.hle/files",
            "/storage/emulated/0/Android/data/com.pixel.hle/files",
            "/sdcard/Android/data/com.pixel.hle/files",
        ] {
            let p = PathBuf::from(candidate);
            if p.exists() || std::fs::create_dir_all(&p).is_ok() {
                return p;
            }
        }
        PathBuf::from("/data/data/com.pixel.hle/files")
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.to_path_buf();
            }
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Folder that holds imported / local `.apk` packages.
pub fn apps_dir() -> PathBuf {
    sandbox_base().join("apps")
}

pub fn ensure_sandbox_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(apps_dir())?;
    std::fs::create_dir_all(sandbox_base().join("logs"))?;
    Ok(())
}

/// Safe destination path inside apps/ for an APK file name.
pub fn apps_dest_for(file_name: &str) -> Option<PathBuf> {
    if !is_plain_name(file_name) {
        return None;
    }
    if !file_name.to_lowercase().ends_with(".apk") {
        return None;
    }
    Some(apps_dir().join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names() {
        assert!(is_plain_name("game.apk"));
        assert!(is_plain_name("com.example.apk"));
        assert!(!is_plain_name(""));
        assert!(!is_plain_name(".."));
        assert!(!is_plain_name("a/b.apk"));
        assert!(!is_plain_name("a\\b.apk"));
    }
}
