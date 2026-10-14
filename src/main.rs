use log::info;
use pixel1_translator::Pixel1Translator;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== pixel1-translator HLE boot ===");

    let rt = match Pixel1Translator::new("config/pixel1-sailfish.yaml") {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("FATAL: {e}");
            std::process::exit(1);
        }
    };

    // Demo the cascade
    match rt.example_cascade_get_window_service() {
        Ok(Some(handle)) => info!("WINDOW_SERVICE handle = {handle}"),
        Ok(None) => info!("WINDOW_SERVICE → null (denied)"),
        Err(e) => eprintln!("Binder error: {e}"),
    }

    // Probe a missing feature
    let has_camera = rt.hw.has_feature("android.hardware.camera");
    info!("hasSystemFeature(camera) = {has_camera} (must be false on sailfish)");

    let has_touch = rt.hw.has_feature("android.hardware.touchscreen.multitouch");
    info!("hasSystemFeature(multitouch) = {has_touch}");

    info!("Battery level = {}% (charging={})",
        rt.hw.battery.get_level(),
        rt.hw.battery.is_charging()
    );

    info!("HLE runtime ready – waiting for guest code");
}
