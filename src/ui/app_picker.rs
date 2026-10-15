//! Compact HyperHLE launcher with Android-style quick-action bar

use crate::ui::{ApkEntry, EmulatorState, LogFilterLevel, SharedUi};
use egui::{self, Color32, RichText, Sense, Vec2};

pub struct AppPicker;

impl AppPicker {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        let mem_est = 64.0_f32;

        // ── Compact top status ───────────────────────────────────────────
        egui::TopBottomPanel::top("hle_status")
            .exact_height(24.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(18, 20, 28))
                    .inner_margin(egui::Margin::symmetric(8.0, 3.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new("Pixel1HLE")
                            .strong()
                            .color(Color32::from_rgb(80, 220, 120))
                            .size(12.0),
                    );
                    ui.label(
                        RichText::new("·")
                            .color(Color32::from_rgb(60, 70, 80))
                            .size(11.0),
                    );
                    ui.label(
                        RichText::new("API 16–25")
                            .color(Color32::from_rgb(130, 140, 155))
                            .size(11.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{mem_est:.0} MB"))
                                .color(Color32::from_rgb(100, 170, 210))
                                .size(11.0),
                        );
                    });
                });
            });

        // ── Android-style quick action bar (Import ya! + gear) ───────────
        egui::TopBottomPanel::top("hle_action_bar")
            .exact_height(48.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(22, 26, 34))
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
            )
            .show(ctx, |ui| {
                draw_import_action_bar(ui, shared);
            });

        // Settings slide-out sheet
        let show_settings = shared.lock().settings.show_settings;
        if show_settings {
            egui::SidePanel::right("hle_settings_sheet")
                .exact_width(220.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(20, 24, 32))
                        .inner_margin(10.0)
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 50, 60))),
                )
                .show(ctx, |ui| {
                    draw_settings_sheet(ui, shared);
                });
        }

        // ── Compact bottom hint ──────────────────────────────────────────
        egui::TopBottomPanel::bottom("hle_hint")
            .exact_height(18.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(12, 14, 18))
                    .inner_margin(egui::Margin::symmetric(8.0, 2.0)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("` terminal  ·  Unity / AIR  ·  arm64")
                        .size(10.0)
                        .color(Color32::from_rgb(80, 90, 105)),
                );
            });

        // ── Main content: stacked library + explorer (no wide dual columns) ─
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(16, 18, 24))
                    .inner_margin(8.0),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);

                // Library section
                ui.label(
                    RichText::new("Library")
                        .strong()
                        .color(Color32::from_rgb(190, 200, 210))
                        .size(13.0),
                );
                ui.add_space(2.0);

                let apks = shared.lock().available_apks.clone();
                let lib_h = (ui.available_height() * 0.32).clamp(80.0, 160.0);

                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 24, 32))
                    .rounding(6.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(lib_h)
                            .show(ui, |ui| {
                                if apks.is_empty() {
                                    ui.label(
                                        RichText::new("No APKs — use Import ya! below")
                                            .size(11.0)
                                            .color(Color32::from_rgb(110, 120, 135)),
                                    );
                                } else {
                                    for entry in &apks {
                                        let short = truncate_name(&entry.name, 28);
                                        let label = format!(
                                            "▶  {short}  ·  {:.1} MB",
                                            entry.size_mb
                                        );
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(label)
                                                        .color(Color32::from_rgb(90, 210, 130))
                                                        .size(12.0),
                                                )
                                                .fill(Color32::from_rgb(28, 34, 42))
                                                .rounding(4.0)
                                                .min_size(Vec2::new(ui.available_width(), 28.0)),
                                            )
                                            .clicked()
                                        {
                                            launch_apk(shared, &entry.path, &entry.name);
                                        }
                                    }
                                }
                            });
                    });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Explorer")
                            .strong()
                            .color(Color32::from_rgb(190, 200, 210))
                            .size(13.0),
                    );
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("↻")
                                    .size(12.0)
                                    .color(Color32::from_rgb(160, 170, 180)),
                            )
                            .fill(Color32::from_rgb(32, 38, 48))
                            .rounding(4.0)
                            .min_size(Vec2::new(28.0, 22.0)),
                        )
                        .clicked()
                    {
                        let list = rescan();
                        let mut g = shared.lock();
                        g.available_apks = list;
                        g.explorer.refresh();
                    }
                });
                ui.add_space(2.0);

                // File explorer fills remaining space
                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 24, 32))
                    .rounding(6.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        let picked = {
                            let mut g = shared.lock();
                            g.explorer.ui(ui)
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

/// Android quick-settings style row: [Import ya!]  [⚙]
fn draw_import_action_bar(ui: &mut egui::Ui, shared: &SharedUi) {
    let bar_h = 36.0_f32;
    let avail = ui.available_width();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        // Main tile – looks like an Android QS tile / toolbar action
        let tile_w = (avail - 48.0).max(120.0);
        let tile_size = Vec2::new(tile_w, bar_h);
        let (tile_rect, tile_resp) =
            ui.allocate_exact_size(tile_size, Sense::click());

        let hovered = tile_resp.hovered();
        let fill = if hovered {
            Color32::from_rgb(40, 90, 55)
        } else {
            Color32::from_rgb(32, 70, 48)
        };
        ui.painter().rect_filled(tile_rect, 10.0, fill);
        ui.painter().rect_stroke(
            tile_rect,
            10.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 160, 100)),
        );

        // Download-style glyph + label
        ui.painter().text(
            egui::pos2(tile_rect.left() + 14.0, tile_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "⬇",
            egui::FontId::proportional(16.0),
            Color32::from_rgb(120, 230, 150),
        );
        ui.painter().text(
            egui::pos2(tile_rect.left() + 36.0, tile_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Import ya!",
            egui::FontId::proportional(14.0),
            Color32::from_rgb(220, 240, 225),
        );
        ui.painter().text(
            egui::pos2(tile_rect.right() - 12.0, tile_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "APK",
            egui::FontId::proportional(10.0),
            Color32::from_rgb(100, 180, 120),
        );

        if tile_resp.clicked() {
            do_system_import(shared);
        }

        // Gear tile – opens settings sheet
        let gear_size = Vec2::new(bar_h, bar_h);
        let (gear_rect, gear_resp) =
            ui.allocate_exact_size(gear_size, Sense::click());
        let gear_open = shared.lock().settings.show_settings;
        let gear_fill = if gear_resp.hovered() || gear_open {
            Color32::from_rgb(50, 58, 72)
        } else {
            Color32::from_rgb(34, 40, 52)
        };
        ui.painter().rect_filled(gear_rect, 10.0, gear_fill);
        ui.painter().rect_stroke(
            gear_rect,
            10.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 80, 100)),
        );
        ui.painter().text(
            gear_rect.center(),
            egui::Align2::CENTER_CENTER,
            "⚙",
            egui::FontId::proportional(18.0),
            Color32::from_rgb(180, 190, 210),
        );

        if gear_resp.clicked() {
            let mut g = shared.lock();
            g.settings.show_settings = !g.settings.show_settings;
        }
    });
}

fn draw_settings_sheet(ui: &mut egui::Ui, shared: &SharedUi) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Settings")
                .strong()
                .color(Color32::from_rgb(200, 210, 220))
                .size(14.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("✕").size(12.0).color(Color32::WHITE))
                        .fill(Color32::from_rgb(50, 40, 40))
                        .rounding(4.0)
                        .min_size(Vec2::new(24.0, 22.0)),
                )
                .clicked()
            {
                shared.lock().settings.show_settings = false;
            }
        });
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);

    // Target API
    ui.label(
        RichText::new("Target API")
            .size(11.0)
            .color(Color32::from_rgb(130, 140, 155)),
    );
    {
        let mut g = shared.lock();
        let api = &mut g.settings.target_api;
        ui.horizontal(|ui| {
            for (label, val) in [("16", 16), ("19", 19), ("21", 21), ("25", 25)] {
                let selected = *api == val;
                let fill = if selected {
                    Color32::from_rgb(50, 120, 80)
                } else {
                    Color32::from_rgb(32, 38, 48)
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(label)
                                .size(11.0)
                                .color(Color32::WHITE),
                        )
                        .fill(fill)
                        .rounding(4.0)
                        .min_size(Vec2::new(36.0, 24.0)),
                    )
                    .clicked()
                {
                    *api = val;
                }
            }
        });
    }

    ui.add_space(10.0);
    ui.label(
        RichText::new("Log filter")
            .size(11.0)
            .color(Color32::from_rgb(130, 140, 155)),
    );
    {
        let mut g = shared.lock();
        let lvl = &mut g.settings.log_level;
        ui.horizontal(|ui| {
            for (label, val) in [
                ("E", LogFilterLevel::Error),
                ("W", LogFilterLevel::Warn),
                ("I", LogFilterLevel::Info),
                ("D", LogFilterLevel::Debug),
            ] {
                let selected = *lvl == val;
                let fill = if selected {
                    Color32::from_rgb(60, 90, 140)
                } else {
                    Color32::from_rgb(32, 38, 48)
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(label)
                                .size(11.0)
                                .color(Color32::WHITE),
                        )
                        .fill(fill)
                        .rounding(4.0)
                        .min_size(Vec2::new(36.0, 24.0)),
                    )
                    .clicked()
                {
                    *lvl = val;
                }
            }
        });
    }

    ui.add_space(10.0);
    ui.label(
        RichText::new("Active engine")
            .size(11.0)
            .color(Color32::from_rgb(130, 140, 155)),
    );
    {
        let mut g = shared.lock();
        let eng = &mut g.settings.active_engine;
        for name in ["auto", "unity", "adobe_air", "main"] {
            let selected = eng.as_str() == name;
            let fill = if selected {
                Color32::from_rgb(90, 70, 40)
            } else {
                Color32::from_rgb(32, 38, 48)
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(name)
                            .size(11.0)
                            .color(Color32::from_rgb(220, 210, 180)),
                    )
                    .fill(fill)
                    .rounding(4.0)
                    .min_size(Vec2::new(ui.available_width(), 24.0)),
                )
                .clicked()
            {
                *eng = name.to_string();
            }
        }
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(6.0);
    ui.label(
        RichText::new("sailfish · Pixel 1\n1080×1920 · 441 ppi")
            .size(10.0)
            .color(Color32::from_rgb(90, 100, 115)),
    );
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        name.to_string()
    } else {
        format!("{}…", &name[..max.saturating_sub(1)])
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
    g.settings.show_settings = false;
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
