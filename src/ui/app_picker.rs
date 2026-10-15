//! Android-style home screen launcher (portrait), inspired by touchHLE-style grids.
//!
//! Pages of app icons, status bar, dock (+ Import / Settings), page dots.

use crate::ui::{ApkEntry, EmulatorState, SharedUi};
use egui::{self, Color32, Pos2, Rect, RichText, Sense, Vec2};
use std::path::Path;

const COLS: usize = 4;
const ROWS: usize = 5;
const ICONS_PER_PAGE: usize = COLS * ROWS;

pub struct AppPicker;

impl AppPicker {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        // Snapshot without holding lock across UI
        let (apks, page) = {
            let g = shared.lock();
            (g.available_apks.clone(), g.picker_page)
        };

        let pages = if apks.is_empty() {
            1
        } else {
            (apks.len() + ICONS_PER_PAGE - 1) / ICONS_PER_PAGE
        };
        let page = page.min(pages.saturating_sub(1));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(12, 14, 22)))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let w = full.width();
                let h = full.height();

                // Soft gradient-ish wallpaper (two rects)
                ui.painter().rect_filled(
                    full,
                    0.0,
                    Color32::from_rgb(18, 22, 36),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(Pos2::new(full.left(), full.top() + h * 0.55), Vec2::new(w, h * 0.45)),
                    0.0,
                    Color32::from_rgb(14, 28, 24),
                );

                // ── Status bar ──────────────────────────────────────────
                let status_h = (h * 0.035).clamp(22.0, 36.0);
                ui.painter().text(
                    Pos2::new(full.left() + 16.0, full.top() + status_h * 0.5),
                    egui::Align2::LEFT_CENTER,
                    "Pixel 1",
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(200, 210, 220),
                );
                ui.painter().text(
                    Pos2::new(full.right() - 16.0, full.top() + status_h * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    "100%  ⚡",
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(80, 220, 120),
                );

                // ── Title ───────────────────────────────────────────────
                let title_y = full.top() + status_h + 18.0;
                ui.painter().text(
                    Pos2::new(full.center().x, title_y),
                    egui::Align2::CENTER_TOP,
                    "Pixel1HLE",
                    egui::FontId::proportional(22.0),
                    Color32::from_rgb(90, 220, 130),
                );
                ui.painter().text(
                    Pos2::new(full.center().x, title_y + 26.0),
                    egui::Align2::CENTER_TOP,
                    "sailfish  ·  Android 7.1",
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(130, 140, 160),
                );

                // ── Icon grid ───────────────────────────────────────────
                let grid_top = title_y + 56.0;
                let dock_h = (h * 0.14).clamp(80.0, 120.0);
                let dots_h = 28.0;
                let grid_bottom = full.bottom() - dock_h - dots_h - 8.0;
                let grid_h = (grid_bottom - grid_top).max(120.0);
                let grid_w = w * 0.92;
                let grid_left = full.center().x - grid_w * 0.5;

                let cell_w = grid_w / COLS as f32;
                let cell_h = grid_h / ROWS as f32;
                let icon_size = (cell_w.min(cell_h) * 0.62).clamp(48.0, 88.0);

                let start = page * ICONS_PER_PAGE;
                let end = (start + ICONS_PER_PAGE).min(apks.len());

                let mut clicked: Option<usize> = None;

                if apks.is_empty() {
                    ui.painter().text(
                        Pos2::new(full.center().x, grid_top + grid_h * 0.4),
                        egui::Align2::CENTER_CENTER,
                        "No APKs yet",
                        egui::FontId::proportional(20.0),
                        Color32::from_rgb(160, 170, 190),
                    );
                    ui.painter().text(
                        Pos2::new(full.center().x, grid_top + grid_h * 0.4 + 28.0),
                        egui::Align2::CENTER_CENTER,
                        "Tap + in the dock to Import ya!",
                        egui::FontId::proportional(14.0),
                        Color32::from_rgb(110, 120, 140),
                    );
                } else {
                    for (slot, idx) in (start..end).enumerate() {
                        let entry = &apks[idx];
                        let col = slot % COLS;
                        let row = slot / COLS;
                        let cx = grid_left + (col as f32 + 0.5) * cell_w;
                        let cy = grid_top + (row as f32 + 0.35) * cell_h;

                        let icon_rect = Rect::from_center_size(
                            Pos2::new(cx, cy),
                            Vec2::splat(icon_size),
                        );
                        let sense_rect = Rect::from_center_size(
                            Pos2::new(cx, cy + 10.0),
                            Vec2::new(cell_w * 0.9, cell_h * 0.9),
                        );

                        let id = ui.id().with("apk_icon").with(idx);
                        let resp = ui.interact(sense_rect, id, Sense::click());

                        let bg = if resp.hovered() {
                            Color32::from_rgb(50, 70, 55)
                        } else {
                            Color32::from_rgb(36, 48, 42)
                        };
                        ui.painter().rect_filled(icon_rect, 16.0, bg);
                        ui.painter().rect_stroke(
                            icon_rect,
                            16.0,
                            egui::Stroke::new(1.5_f32, Color32::from_rgb(70, 160, 100)),
                        );

                        // Letter badge from app name
                        let letter = entry
                            .name
                            .chars()
                            .next()
                            .unwrap_or('A')
                            .to_uppercase()
                            .to_string();
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            letter,
                            egui::FontId::proportional(icon_size * 0.42),
                            Color32::from_rgb(120, 230, 150),
                        );

                        // Label under icon
                        let label = if entry.name.len() > 12 {
                            format!("{}…", &entry.name[..11])
                        } else {
                            entry.name.clone()
                        };
                        ui.painter().text(
                            Pos2::new(cx, cy + icon_size * 0.55 + 6.0),
                            egui::Align2::CENTER_TOP,
                            label,
                            egui::FontId::proportional(11.0),
                            Color32::from_rgb(210, 215, 225),
                        );

                        if resp.clicked() {
                            clicked = Some(idx);
                        }
                    }
                }

                // ── Page dots ───────────────────────────────────────────
                let dots_y = grid_bottom + 10.0;
                let mut new_page = page;
                if pages > 1 {
                    let total_w = pages as f32 * 14.0;
                    let start_x = full.center().x - total_w * 0.5;
                    for p in 0..pages {
                        let dx = start_x + p as f32 * 14.0;
                        let r = if p == page { 5.0 } else { 3.5 };
                        let c = if p == page {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(90, 100, 120)
                        };
                        let dot_rect =
                            Rect::from_center_size(Pos2::new(dx, dots_y), Vec2::splat(r * 2.5));
                        let id = ui.id().with("dot").with(p);
                        let resp = ui.interact(dot_rect, id, Sense::click());
                        ui.painter()
                            .circle_filled(Pos2::new(dx, dots_y), r, c);
                        if resp.clicked() {
                            new_page = p;
                        }
                    }

                    // Swipe-ish: left/right zones
                    let left_zone = Rect::from_min_max(
                        Pos2::new(full.left(), grid_top),
                        Pos2::new(full.left() + 40.0, grid_bottom),
                    );
                    let right_zone = Rect::from_min_max(
                        Pos2::new(full.right() - 40.0, grid_top),
                        Pos2::new(full.right(), grid_bottom),
                    );
                    if ui
                        .interact(left_zone, ui.id().with("page_left"), Sense::click())
                        .clicked()
                        && page > 0
                    {
                        new_page = page - 1;
                    }
                    if ui
                        .interact(right_zone, ui.id().with("page_right"), Sense::click())
                        .clicked()
                        && page + 1 < pages
                    {
                        new_page = page + 1;
                    }
                }

                // ── Dock ────────────────────────────────────────────────
                let dock_top = full.bottom() - dock_h;
                let dock_rect = Rect::from_min_max(
                    Pos2::new(full.left() + w * 0.06, dock_top),
                    Pos2::new(full.right() - w * 0.06, full.bottom() - 12.0),
                );
                ui.painter().rect_filled(
                    dock_rect,
                    22.0,
                    Color32::from_rgba_unmultiplied(30, 36, 48, 220),
                );

                let dock_cy = dock_rect.center().y;
                let import_clicked = dock_button(
                    ui,
                    Pos2::new(dock_rect.left() + dock_rect.width() * 0.28, dock_cy),
                    "+",
                    "Import",
                    Color32::from_rgb(80, 220, 120),
                );
                let refresh_clicked = dock_button(
                    ui,
                    Pos2::new(dock_rect.left() + dock_rect.width() * 0.72, dock_cy),
                    "↻",
                    "Refresh",
                    Color32::from_rgb(100, 160, 220),
                );

                // Apply interactions with lock
                if clicked.is_some() || import_clicked || refresh_clicked || new_page != page {
                    let mut g = shared.lock();
                    g.picker_page = new_page;

                    if let Some(idx) = clicked {
                        if let Some(entry) = g.available_apks.get(idx).cloned() {
                            log::info!(target: "hle::display", "Launch {}", entry.name);
                            g.selected_apk = Some(entry.path.clone());
                            g.state = EmulatorState::BootAnimation {
                                apk_name: entry.name,
                            };
                            g.loading_progress = 0.0;
                            g.loading_status = "Preparing…".into();
                        }
                    }

                    if import_clicked {
                        drop(g);
                        do_import(shared);
                        return;
                    }

                    if refresh_clicked {
                        g.available_apks = rescan();
                        log::info!(target: "hle::display", "Refresh: {} APKs", g.available_apks.len());
                    }
                }
            });
    }
}

fn dock_button(ui: &mut egui::Ui, center: Pos2, glyph: &str, label: &str, color: Color32) -> bool {
    let size = 52.0;
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    let id = ui.id().with("dock").with(label);
    let resp = ui.interact(rect, id, Sense::click());
    let bg = if resp.hovered() {
        Color32::from_rgb(50, 60, 70)
    } else {
        Color32::from_rgb(36, 42, 54)
    };
    ui.painter().circle_filled(center, size * 0.45, bg);
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(22.0),
        color,
    );
    ui.painter().text(
        Pos2::new(center.x, center.y + size * 0.48),
        egui::Align2::CENTER_TOP,
        label,
        egui::FontId::proportional(11.0),
        Color32::from_rgb(180, 190, 200),
    );
    resp.clicked()
}

fn do_import(shared: &SharedUi) {
    #[cfg(not(target_os = "android"))]
    {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Android Package", &["apk"])
            .set_title("Import ya! – choose an APK")
            .pick_file()
        {
            register_apk(shared, &path);
        }
    }
    #[cfg(target_os = "android")]
    {
        let mut g = shared.lock();
        g.available_apks = rescan();
        log::info!(target: "hle::display", "Android scan: {} APKs", g.available_apks.len());
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

#[cfg(not(target_os = "android"))]
fn register_apk(shared: &SharedUi, path: &Path) {
    let path_str = path.to_string_lossy().to_string();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("imported.apk")
        .to_string();
    let size_mb = std::fs::metadata(path)
        .map(|m| m.len() as f32 / 1_048_576.0)
        .unwrap_or(0.0);

    log::info!(target: "hle::display", "Import ya!: {} ({})", name, path_str);

    let mut g = shared.lock();
    if !g.available_apks.iter().any(|e| e.path == path_str) {
        g.available_apks.push(ApkEntry {
            path: path_str.clone(),
            name: name.clone(),
            size_mb,
        });
    }
    g.selected_apk = Some(path_str);
    g.state = EmulatorState::BootAnimation { apk_name: name };
    g.loading_progress = 0.0;
    g.loading_status = "Preparing…".into();
}
