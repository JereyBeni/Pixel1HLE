//! Compact dashboard + Android QS-style Import ya! tile + settings drawer
//!
//! Modular AppInfo listing (touchHLE-inspired), platform-specific import:
//! - Windows: native `rfd` file dialog → copy into `apps/`
//! - Android: SAF `ACTION_OPEN_DOCUMENT` via JNI + apps/ poll fallback

use crate::platform;
use crate::ui::{ApkEntry, EmulatorState, LogFilterLevel, SharedUi};
use egui::{self, Color32, RichText, Sense, Vec2};
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Validated APK package metadata (sandbox-safe names only).
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub path: PathBuf,
    pub display_name: String,
    pub size_mb: f32,
}

impl AppInfo {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_string();
        if !platform::is_plain_name(&name) {
            warn!(target: "hle::import", "Skipping unsafe path component: {name}");
            return None;
        }
        if !name.to_lowercase().ends_with(".apk") {
            return None;
        }
        let size_mb = std::fs::metadata(path)
            .map(|m| m.len() as f32 / 1_048_576.0)
            .unwrap_or(0.0);
        Some(Self {
            path: path.to_path_buf(),
            display_name: name,
            size_mb,
        })
    }
}

pub struct AppPicker;

impl AppPicker {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        // Poll SAF import completion on Android
        poll_saf_if_pending(shared);

        // ── Status bar (tight) ───────────────────────────────────────────
        egui::TopBottomPanel::top("hle_status")
            .exact_height(22.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(18, 20, 28))
                    .inner_margin(egui::Margin::symmetric(8.0, 2.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    ui.label(
                        RichText::new("Pixel1HLE")
                            .strong()
                            .size(11.0)
                            .color(Color32::from_rgb(80, 220, 120)),
                    );
                    let api = shared.lock().settings.target_api;
                    ui.label(
                        RichText::new(format!("API {api}"))
                            .size(10.0)
                            .color(Color32::from_rgb(130, 140, 155)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let eng = shared.lock().settings.active_engine.clone();
                        ui.label(
                            RichText::new(eng)
                                .size(10.0)
                                .color(Color32::from_rgb(180, 160, 100)),
                        );
                    });
                });
            });

        // ── QS action bar: Import ya! + gear ─────────────────────────────
        egui::TopBottomPanel::top("hle_qs")
            .exact_height(46.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(22, 26, 34))
                    .inner_margin(egui::Margin::symmetric(8.0, 5.0)),
            )
            .show(ctx, |ui| {
                draw_qs_bar(ui, shared);
            });

        // Settings drawer
        if shared.lock().settings.show_settings {
            egui::SidePanel::right("hle_settings")
                .exact_width(200.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(20, 24, 32))
                        .inner_margin(8.0)
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 50, 62))),
                )
                .show(ctx, |ui| {
                    draw_settings_drawer(ui, shared);
                });
        }

        egui::TopBottomPanel::bottom("hle_foot")
            .exact_height(16.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(12, 14, 18))
                    .inner_margin(egui::Margin::symmetric(8.0, 1.0)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("` terminal  ·  Unity / AIR")
                        .size(9.0)
                        .color(Color32::from_rgb(75, 85, 100)),
                );
            });

        // ── Body: library + explorer (stacked, compact) ──────────────────
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(16, 18, 24))
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);

                ui.label(
                    RichText::new("Library")
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(190, 200, 210)),
                );

                let apps = enumerate_apps();
                let lib_h = (ui.available_height() * 0.30).clamp(72.0, 140.0);

                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 24, 32))
                    .rounding(6.0)
                    .inner_margin(5.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(lib_h)
                            .show(ui, |ui| {
                                if apps.is_empty() {
                                    ui.label(
                                        RichText::new("Empty — Import ya! to add APKs")
                                            .size(11.0)
                                            .color(Color32::from_rgb(110, 120, 135)),
                                    );
                                } else {
                                    for app in &apps {
                                        let short = truncate(&app.display_name, 26);
                                        let row = format!(
                                            "▶ {short}  ·  {:.1} MB",
                                            app.size_mb
                                        );
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(row)
                                                        .size(11.0)
                                                        .color(Color32::from_rgb(90, 210, 130)),
                                                )
                                                .fill(Color32::from_rgb(28, 34, 42))
                                                .rounding(4.0)
                                                .min_size(Vec2::new(ui.available_width(), 26.0)),
                                            )
                                            .clicked()
                                        {
                                            launch_apk(
                                                shared,
                                                &app.path.to_string_lossy(),
                                                &app.display_name,
                                            );
                                        }
                                    }
                                }
                            });
                    });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Explorer")
                            .strong()
                            .size(12.0)
                            .color(Color32::from_rgb(190, 200, 210)),
                    );
                    if ui
                        .add(
                            egui::Button::new(RichText::new("↻").size(11.0).color(Color32::GRAY))
                                .fill(Color32::from_rgb(32, 38, 48))
                                .min_size(Vec2::new(26.0, 20.0)),
                        )
                        .clicked()
                    {
                        let _ = platform::ensure_sandbox_dirs();
                        shared.lock().explorer.refresh();
                        sync_library(shared);
                    }
                });

                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 24, 32))
                    .rounding(6.0)
                    .inner_margin(5.0)
                    .show(ui, |ui| {
                        let picked = {
                            let mut g = shared.lock();
                            g.explorer.ui(ui)
                        };
                        if let Some(path) = picked {
                            match import_and_register(shared, &path) {
                                Ok((dest, name)) => launch_apk(shared, &dest, &name),
                                Err(e) => {
                                    warn!(target: "hle::import", "{e}");
                                    log::error!(target: "hle::import", "Import failed: {e}");
                                }
                            }
                        }
                    });
            });
    }
}

// ─── Quick Settings tile bar ────────────────────────────────────────────────

fn draw_qs_bar(ui: &mut egui::Ui, shared: &SharedUi) {
    let bar_h = 34.0_f32;
    let avail = ui.available_width();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        // Import ya! QS tile
        let tile_w = (avail - 42.0).max(100.0);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(tile_w, bar_h), Sense::click());
        let fill = if resp.hovered() {
            Color32::from_rgb(38, 88, 52)
        } else {
            Color32::from_rgb(30, 68, 46)
        };
        ui.painter().rect_filled(rect, 10.0, fill);
        ui.painter().rect_stroke(
            rect,
            10.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 155, 100)),
        );
        ui.painter().text(
            egui::pos2(rect.left() + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "⬇",
            egui::FontId::proportional(15.0),
            Color32::from_rgb(120, 230, 150),
        );
        ui.painter().text(
            egui::pos2(rect.left() + 32.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Import ya!",
            egui::FontId::proportional(13.0),
            Color32::from_rgb(220, 240, 225),
        );
        #[cfg(target_os = "android")]
        let badge = "SAF";
        #[cfg(not(target_os = "android"))]
        let badge = "APK";
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            badge,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(100, 175, 120),
        );

        if resp.clicked() {
            trigger_import(shared);
        }

        // Gear
        let (grect, gresp) = ui.allocate_exact_size(Vec2::splat(bar_h), Sense::click());
        let open = shared.lock().settings.show_settings;
        let gfill = if gresp.hovered() || open {
            Color32::from_rgb(48, 56, 70)
        } else {
            Color32::from_rgb(34, 40, 52)
        };
        ui.painter().rect_filled(grect, 10.0, gfill);
        ui.painter().rect_stroke(
            grect,
            10.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 80, 100)),
        );
        ui.painter().text(
            grect.center(),
            egui::Align2::CENTER_CENTER,
            "⚙",
            egui::FontId::proportional(16.0),
            Color32::from_rgb(180, 190, 210),
        );
        if gresp.clicked() {
            let mut g = shared.lock();
            g.settings.show_settings = !g.settings.show_settings;
        }
    });
}

fn draw_settings_drawer(ui: &mut egui::Ui, shared: &SharedUi) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Settings")
                .strong()
                .size(13.0)
                .color(Color32::from_rgb(200, 210, 220)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("✕").size(11.0).color(Color32::WHITE))
                        .fill(Color32::from_rgb(55, 40, 40))
                        .min_size(Vec2::new(22.0, 20.0)),
                )
                .clicked()
            {
                shared.lock().settings.show_settings = false;
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    ui.label(RichText::new("Target API").size(10.0).color(Color32::GRAY));
    {
        let mut g = shared.lock();
        let api = &mut g.settings.target_api;
        ui.horizontal(|ui| {
            for (label, val) in [("4.1", 16), ("4.4", 19), ("5.0", 21), ("7.1", 25)] {
                let on = *api == val;
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).size(10.0).color(Color32::WHITE))
                            .fill(if on {
                                Color32::from_rgb(50, 120, 80)
                            } else {
                                Color32::from_rgb(32, 38, 48)
                            })
                            .min_size(Vec2::new(40.0, 22.0)),
                    )
                    .clicked()
                {
                    *api = val;
                    info!(target: "hle::settings", "Target API → {val}");
                }
            }
        });
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Log filter").size(10.0).color(Color32::GRAY));
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
                let on = *lvl == val;
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).size(10.0).color(Color32::WHITE))
                            .fill(if on {
                                Color32::from_rgb(55, 85, 140)
                            } else {
                                Color32::from_rgb(32, 38, 48)
                            })
                            .min_size(Vec2::new(32.0, 22.0)),
                    )
                    .clicked()
                {
                    *lvl = val;
                }
            }
        });
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Engine").size(10.0).color(Color32::GRAY));
    {
        let mut g = shared.lock();
        let eng = &mut g.settings.active_engine;
        for name in ["auto", "unity", "adobe_air", "main"] {
            let on = eng.as_str() == name;
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(name)
                            .size(10.0)
                            .color(Color32::from_rgb(220, 210, 180)),
                    )
                    .fill(if on {
                        Color32::from_rgb(90, 70, 40)
                    } else {
                        Color32::from_rgb(32, 38, 48)
                    })
                    .min_size(Vec2::new(ui.available_width(), 22.0)),
                )
                .clicked()
            {
                *eng = name.into();
            }
        }
    }

    ui.add_space(8.0);
    ui.label(RichText::new("GLES thread guard").size(10.0).color(Color32::GRAY));
    {
        let mut g = shared.lock();
        let on = g.settings.gles_thread_guard;
        let label = if on { "ON – serialized GL" } else { "OFF – free thread" };
        if ui
            .add(
                egui::Button::new(RichText::new(label).size(10.0).color(Color32::WHITE))
                    .fill(if on {
                        Color32::from_rgb(40, 100, 90)
                    } else {
                        Color32::from_rgb(70, 40, 40)
                    })
                    .min_size(Vec2::new(ui.available_width(), 24.0)),
            )
            .clicked()
        {
            g.settings.gles_thread_guard = !on;
            info!(
                target: "hle::settings",
                "GLES thread guard → {}",
                g.settings.gles_thread_guard
            );
        }
    }

    ui.add_space(10.0);
    ui.separator();
    ui.label(
        RichText::new("sailfish · 1080×1920\napps/ sandbox")
            .size(9.0)
            .color(Color32::from_rgb(85, 95, 110)),
    );
}

// ─── Import plumbing ────────────────────────────────────────────────────────

fn trigger_import(shared: &SharedUi) {
    let _ = platform::ensure_sandbox_dirs();

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Android Package", &["apk"])
            .set_title("Import ya! – choose an APK")
            .pick_file()
        {
            match import_and_register(shared, &path) {
                Ok((dest, name)) => {
                    info!(target: "hle::import", "Windows import OK: {name}");
                    launch_apk(shared, &dest, &name);
                }
                Err(e) => warn!(target: "hle::import", "{e}"),
            }
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
    {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Android Package", &["apk"])
            .set_title("Import ya! – choose an APK")
            .pick_file()
        {
            match import_and_register(shared, &path) {
                Ok((dest, name)) => launch_apk(shared, &dest, &name),
                Err(e) => warn!(target: "hle::import", "{e}"),
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        match platform::android_saf::launch_apk_document_picker() {
            Ok(()) => {
                let mut g = shared.lock();
                g.pending_saf = true;
                g.saf_started_ms = now_ms();
                g.loading_status = "SAF open — pick an APK…".into();
                info!(target: "hle::saf", "Waiting for package in {}", platform::apps_dir().display());
            }
            Err(e) => {
                warn!(target: "hle::saf", "{e}");
                // Fallback: rescan known public dirs into library
                sync_library(shared);
            }
        }
    }
}

fn poll_saf_if_pending(shared: &SharedUi) {
    #[cfg(target_os = "android")]
    {
        let pending = shared.lock().pending_saf;
        if !pending {
            return;
        }
        let started = shared.lock().saf_started_ms;
        if now_ms().saturating_sub(started) < 500 {
            return;
        }
        // Look for newest apk in apps/
        if let Some(app) = newest_app_in_sandbox() {
            let mut g = shared.lock();
            g.pending_saf = false;
            drop(g);
            info!(target: "hle::saf", "Detected imported {}", app.display_name);
            launch_apk(shared, &app.path.to_string_lossy(), &app.display_name);
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = shared;
    }
}

fn import_and_register(shared: &SharedUi, src: &Path) -> Result<(String, String), String> {
    let info = AppInfo::from_path(src).ok_or_else(|| "invalid APK path".to_string())?;

    #[cfg(target_os = "android")]
    let dest = {
        platform::android_saf::import_apk_to_sandbox(src)
            .map_err(|e| e.to_string())?
    };
    #[cfg(not(target_os = "android"))]
    let dest = {
        let _ = platform::ensure_sandbox_dirs();
        match platform::paths::apps_dest_for(&info.display_name) {
            Some(d) => {
                if src != d.as_path() {
                    std::fs::copy(src, &d).map_err(|e| e.to_string())?;
                }
                d
            }
            None => src.to_path_buf(),
        }
    };

    let dest_str = dest.to_string_lossy().to_string();
    let name = info.display_name.clone();
    {
        let mut g = shared.lock();
        if !g.available_apks.iter().any(|e| e.path == dest_str) {
            g.available_apks.push(ApkEntry {
                path: dest_str.clone(),
                name: name.clone(),
                size_mb: info.size_mb,
            });
        }
    }
    Ok((dest_str, name))
}

fn launch_apk(shared: &SharedUi, path: &str, name: &str) {
    info!(target: "hle::display", "Launch {name}");
    let mut g = shared.lock();
    g.selected_apk = Some(path.to_string());
    g.state = EmulatorState::BootAnimation {
        apk_name: name.to_string(),
    };
    g.loading_progress = 0.0;
    g.loading_status = "Preparing…".into();
    g.settings.show_settings = false;
    g.pending_saf = false;
}

fn enumerate_apps() -> Vec<AppInfo> {
    let _ = platform::ensure_sandbox_dirs();
    let mut apps = Vec::new();
    let mut roots = vec![platform::apps_dir()];
    #[cfg(target_os = "android")]
    {
        roots.push(PathBuf::from("/sdcard/Download"));
        roots.push(PathBuf::from("/storage/emulated/0/Download"));
    }
    for dir in roots {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if let Some(info) = AppInfo::from_path(&e.path()) {
                    if !apps.iter().any(|a| a.path == info.path) {
                        apps.push(info);
                    }
                }
            }
        }
    }
    apps.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    apps
}

fn newest_app_in_sandbox() -> Option<AppInfo> {
    let mut best: Option<(AppInfo, SystemTime)> = None;
    if let Ok(rd) = std::fs::read_dir(platform::apps_dir()) {
        for e in rd.flatten() {
            if let Some(info) = AppInfo::from_path(&e.path()) {
                let modified = e.metadata().and_then(|m| m.modified()).ok();
                if let Some(m) = modified {
                    match &best {
                        None => best = Some((info, m)),
                        Some((_, t)) if m > *t => best = Some((info, m)),
                        _ => {}
                    }
                }
            }
        }
    }
    best.map(|(i, _)| i)
}

fn sync_library(shared: &SharedUi) {
    let apps = enumerate_apps();
    let mut g = shared.lock();
    g.available_apks = apps
        .into_iter()
        .map(|a| ApkEntry {
            path: a.path.to_string_lossy().into(),
            name: a.display_name,
            size_mb: a.size_mb,
        })
        .collect();
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
