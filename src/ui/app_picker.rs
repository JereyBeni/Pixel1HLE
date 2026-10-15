//! HyperHLE-style technical launcher: status metrics + APK list + file explorer

use crate::ui::{ApkEntry, EmulatorState, SharedUi};
use egui::{self, Color32, RichText, Vec2};
use std::path::Path;

pub struct AppPicker;

impl AppPicker {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        let mem_est = estimate_rss_mb();

        // ── Top status bar (HyperHLE metrics) ────────────────────────────
        egui::TopBottomPanel::top("hle_status")
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(16, 18, 26)).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Pixel1HLE")
                            .strong()
                            .color(Color32::from_rgb(80, 220, 120))
                            .size(14.0),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new("sailfish · API 16–25 · Android 4.1–7.1")
                            .color(Color32::from_rgb(140, 150, 170))
                            .size(12.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("RSS ≈ {mem_est:.0} MB"))
                                .color(Color32::from_rgb(100, 180, 220))
                                .size(12.0),
                        );
                        ui.separator();
                        ui.label(
                            RichText::new("HLE · arm64")
                                .color(Color32::from_rgb(180, 160, 100))
                                .size(12.0),
                        );
                    });
                });
            });

        // ── Bottom console strip (always visible summary) ────────────────
        egui::TopBottomPanel::bottom("hle_hint")
            .exact_height(22.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(10, 12, 16)).inner_margin(4.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "` toggle terminal  ·  Import ya! / explorer for APKs  ·  Unity · Adobe AIR · arm64",
                    )
                    .size(11.0)
                    .color(Color32::from_rgb(90, 100, 120)),
                );
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(14, 16, 22)))
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    // LEFT: library of known APKs
                    cols[0].heading(
                        RichText::new("Library")
                            .color(Color32::from_rgb(200, 210, 220))
                            .size(16.0),
                    );
                    cols[0].separator();

                    let apks = shared.lock().available_apks.clone();
                    if apks.is_empty() {
                        cols[0].label(
                            RichText::new("No APKs in apps/ yet.\nUse the explorer →")
                                .color(Color32::from_rgb(120, 130, 150)),
                        );
                    } else {
                        egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                            for entry in &apks {
                                let label = format!(
                                    "▶  {}  ({:.1} MB)",
                                    entry.name, entry.size_mb
                                );
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(label)
                                                .color(Color32::from_rgb(90, 220, 130))
                                                .size(13.0),
                                        )
                                        .fill(Color32::from_rgb(24, 30, 38))
                                        .min_size(Vec2::new(ui.available_width(), 32.0)),
                                    )
                                    .clicked()
                                {
                                    launch_apk(shared, &entry.path, &entry.name);
                                }
                            }
                        });
                    }

                    cols[0].add_space(8.0);
                    if cols[0]
                        .add(
                            egui::Button::new(
                                RichText::new("↻ Rescan apps/")
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(40, 50, 64)),
                        )
                        .clicked()
                    {
                        let list = rescan();
                        shared.lock().available_apks = list;
                    }

                    // RIGHT: live file explorer
                    cols[1].horizontal(|ui| {
                        ui.heading(
                            RichText::new("File Explorer")
                                .color(Color32::from_rgb(200, 210, 220))
                                .size(16.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Import ya!")
                                            .strong()
                                            .color(Color32::from_rgb(20, 24, 28)),
                                    )
                                    .fill(Color32::from_rgb(80, 220, 120)),
                                )
                                .clicked()
                            {
                                do_system_import(shared);
                            }
                        });
                    });
                    cols[1].separator();

                    // Explorer needs mutable access – lock briefly
                    let picked = {
                        let mut g = shared.lock();
                        g.explorer.ui(&mut cols[1])
                    };
                    if let Some(path) = picked {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("imported.apk")
                            .to_string();
                        let path_str = path.to_string_lossy().to_string();
                        register_and_launch(shared, &path_str, &name);
                    }
                });
            });
    }
}

fn launch_apk(shared: &SharedUi, path: &str, name: &str) {
    log::info!(target: "hle::display", "Launch {name}");
    let mut g = shared.lock();
    g.selected_apk = Some(path.to_string());
    g.state = EmulatorState::BootAnimation {
        apk_name: name.to_string(),
    };
    g.loading_progress = 0.0;
    g.loading_status = "Preparing…".into();
}

fn register_and_launch(shared: &SharedUi, path: &str, name: &str) {
    let size_mb = std::fs::metadata(path)
        .map(|m| m.len() as f32 / 1_048_576.0)
        .unwrap_or(0.0);
    {
        let mut g = shared.lock();
        if !g.available_apks.iter().any(|e| e.path == path) {
            g.available_apks.push(ApkEntry {
                path: path.to_string(),
                name: name.to_string(),
                size_mb,
            });
        }
    }
    launch_apk(shared, path, name);
}

fn do_system_import(shared: &SharedUi) {
    #[cfg(not(target_os = "android"))]
    {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Android Package", &["apk"])
            .set_title("Import ya! – choose an APK")
            .pick_file()
        {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("imported.apk")
                .to_string();
            register_and_launch(shared, &path.to_string_lossy(), &name);
        }
    }
    #[cfg(target_os = "android")]
    {
        let mut g = shared.lock();
        g.available_apks = rescan();
        g.explorer.refresh();
    }
}

fn rescan() -> Vec<ApkEntry> {
    let mut list = Vec::new();
    let mut roots = vec![
        std::path::PathBuf::from("."),
        std::path::PathBuf::from("apps"),
        std::path::PathBuf::from("apks"),
    ];
    #[cfg(target_os = "android")]
    {
        roots.push(std::path::PathBuf::from("/sdcard/Download"));
        roots.push(std::path::PathBuf::from("/storage/emulated/0/Download"));
        roots.push(std::path::PathBuf::from("/data/data/com.pixel.hle/files/apps"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            roots.push(dir.join("apps"));
        }
    }
    for dir in roots {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) == Some("apk") {
                    let size_mb = e
                        .metadata()
                        .map(|m| m.len() as f32 / 1_048_576.0)
                        .unwrap_or(0.0);
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if !list.iter().any(|x: &ApkEntry| x.path == path.to_string_lossy()) {
                        list.push(ApkEntry {
                            path: path.to_string_lossy().into(),
                            name,
                            size_mb,
                        });
                    }
                }
            }
        }
    }
    list
}

fn estimate_rss_mb() -> f32 {
    // Lightweight estimate; platform-specific counters can replace later
    #[cfg(windows)]
    {
        // Approximate via working set is OS-specific; show a stable placeholder
        64.0
    }
    #[cfg(not(windows))]
    {
        64.0
    }
}
