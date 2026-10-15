//! Pixel1HLE – GUI entry point (no CLI required)

use log::info;
use pixel1_translator::logging;
use pixel1_translator::Pixel1Translator;
use std::path::PathBuf;

fn main() {
    // Logs dir next to the executable / cwd
    let log_dir = PathBuf::from("logs");
    let _log_state = logging::HleLogger::init(&log_dir);

    info!(target: "hle", "=== Pixel1HLE GUI boot ===");
    info!(target: "hle", "Crash reports → logs/crash_*.txt");
    info!(target: "hle", "Full log     → logs/pixel1hle.log");
    info!(target: "hle", "Press ` (backtick) to toggle the in-app terminal");

    let rt = match Pixel1Translator::new("config/pixel1-sailfish.yaml") {
        Ok(rt) => rt,
        Err(e) => {
            // Also write a fatal file so GUI-less failures leave a trace
            let _ = std::fs::create_dir_all(&log_dir);
            let msg = format!("FATAL boot error: {e}\n");
            let _ = std::fs::write(log_dir.join("boot_fatal.txt"), &msg);
            eprintln!("{msg}");
            // MessageBox-style: on Windows we could use a dialog; for now keep stderr + file
            std::process::exit(1);
        }
    };

    info!(target: "hle", "Launching GUI (AppPicker → Boot → Loading → Game)");
    if let Err(e) = rt.run_ui() {
        let msg = format!("UI loop error: {e}\n");
        let _ = std::fs::write(log_dir.join("ui_fatal.txt"), &msg);
        log::error!(target: "hle", "{msg}");
        std::process::exit(1);
    }
}
