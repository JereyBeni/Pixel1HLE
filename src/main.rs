use log::info;
use pixel1_translator::Pixel1Translator;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== Pixel1HLE – sailfish UI lifecycle ===");

    let rt = match Pixel1Translator::new("config/pixel1-sailfish.yaml") {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("FATAL: {e}");
            std::process::exit(1);
        }
    };

    info!("WINDOW_SERVICE probe…");
    let _ = rt.example_cascade_get_window_service();

    info!("Launching interactive UI (AppPicker → Boot → Loading → Game)");
    if let Err(e) = rt.run_ui() {
        eprintln!("UI loop error: {e}");
        std::process::exit(1);
    }
}
