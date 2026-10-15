//! Desktop GUI entry – same core as Android NativeActivity

use log::info;
use pixel1_translator::logging;
use pixel1_translator::Pixel1Translator;
use std::path::PathBuf;

fn main() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = std::env::set_current_dir(dir);
        }
    }

    let log_dir = PathBuf::from("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let _log_state = logging::init(&log_dir);

    info!(target: "hle", "=== Pixel1HLE GUI boot (desktop) ===");
    info!(target: "hle", "cwd = {:?}", std::env::current_dir());

    let profile = find_profile();
    let rt = match Pixel1Translator::new(profile.to_str().unwrap_or("config/pixel1-sailfish.yaml")) {
        Ok(rt) => rt,
        Err(e) => {
            let msg = format!("FATAL boot error: {e}\n");
            let _ = std::fs::write(log_dir.join("boot_fatal.txt"), &msg);
            eprintln!("{msg}");
            #[cfg(windows)]
            show_windows_error(&msg);
            std::process::exit(1);
        }
    };

    if let Err(e) = rt.run_ui() {
        let msg = format!("UI loop error: {e}\n");
        let _ = std::fs::write(log_dir.join("ui_fatal.txt"), &msg);
        log::error!(target: "hle", "{msg}");
        #[cfg(windows)]
        show_windows_error(&msg);
        std::process::exit(1);
    }
}

fn find_profile() -> PathBuf {
    for c in [
        PathBuf::from("config/pixel1-sailfish.yaml"),
        PathBuf::from("pixel1-sailfish.yaml"),
    ] {
        if c.is_file() {
            return c;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("config/pixel1-sailfish.yaml");
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from("config/pixel1-sailfish.yaml")
}

#[cfg(windows)]
fn show_windows_error(msg: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            hWnd: *mut core::ffi::c_void,
            lpText: *const u16,
            lpCaption: *const u16,
            uType: u32,
        ) -> i32;
    }
    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }
    let text = wide(msg);
    let caption = wide("Pixel1HLE – error");
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            0x10,
        );
    }
}
