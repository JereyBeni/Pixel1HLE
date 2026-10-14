use log::info;
use pixel1_translator::Pixel1Translator;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== pixel1-translator HLE + GLES boot ===");

    let rt = match Pixel1Translator::new("config/pixel1-sailfish.yaml") {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("FATAL: {e}");
            std::process::exit(1);
        }
    };

    // Classic Binder cascade
    match rt.example_cascade_get_window_service() {
        Ok(Some(handle)) => info!("WINDOW_SERVICE handle = {handle}"),
        Ok(None) => info!("WINDOW_SERVICE → null (denied)"),
        Err(e) => eprintln!("Binder error: {e}"),
    }

    // Feature probes
    let has_camera = rt.hw.has_feature("android.hardware.camera");
    info!("hasSystemFeature(camera) = {has_camera}");
    let has_gles = rt.hw.has_feature("android.hardware.opengles.aep");
    info!("hasSystemFeature(opengles) = {has_gles}");

    info!("Battery level = {}% (charging={})",
        rt.hw.battery.get_level(),
        rt.hw.battery.is_charging()
    );

    // ─────────────────────────────────────────────────────────
    // Unity graphics pipeline bootstrap test
    // This is the exact sequence libunity.so performs after
    // it has loaded and resolved libEGL / libGLESv2.
    // ─────────────────────────────────────────────────────────
    info!("─── Unity GLES cascade ───");
    rt.demo_unity_graphics();

    info!("HLE runtime ready – GLES bridge live");
}
