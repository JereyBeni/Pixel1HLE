//! Platform-specific paths, sandbox, and import (Windows rfd / Android SAF)

pub mod paths;

#[cfg(target_os = "android")]
pub mod android_saf;

pub use paths::{apps_dir, is_plain_name, sandbox_base, ensure_sandbox_dirs};
