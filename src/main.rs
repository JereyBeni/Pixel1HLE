use log::info;
use pixel1_translator::Pixel1Translator;
use std::path::Path;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== pixel1-translator HLE + GLES + Dynamic Linker ===");

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
        Ok(None) => info!("WINDOW_SERVICE → null"),
        Err(e) => eprintln!("Binder error: {e}"),
    }

    info!("hasSystemFeature(camera) = {}", rt.hw.has_feature("android.hardware.camera"));
    info!("hasSystemFeature(opengles) = {}", rt.hw.has_feature("android.hardware.opengles.aep"));
    info!("Battery = {}% charging={}", rt.hw.battery.get_level(), rt.hw.battery.is_charging());

    // ── GLES bootstrap demo (always available) ───────────────────────────
    info!("─── Unity GLES cascade (no APK) ───");
    rt.demo_unity_graphics();

    // ── Full APK path (only if a real APK is present) ─────────────────────
    // Drop any Android APK that contains lib/arm64-v8a/libunity.so next to
    // the binary and pass it here.
    let demo_apk = Path::new("demo.apk");
    if demo_apk.exists() {
        info!("─── Full APK execution cascade ───");
        match rt.load_and_run_apk(demo_apk, "libunity.so") {
            Ok(lib) => {
                info!("Engine loaded successfully @ 0x{:x}", lib.guest_base);
                if lib.jni_onload.is_some() {
                    info!("JNI_OnLoad was called");
                }
            }
            Err(e) => {
                // Non-fatal – we still have the GLES demo working
                log::error!(target: "hle::linker", "APK load failed (expected if no real APK): {e}");
            }
        }
    } else {
        info!("No demo.apk found – skipping full linker path (drop an APK to test)");
    }

    info!("HLE runtime ready – dynamic linker + GLES bridge live");
}
