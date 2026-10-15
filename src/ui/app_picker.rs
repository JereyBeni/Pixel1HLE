//! Classic Android-style APK drawer + "Import ya!" button

use crate::ui::{ApkEntry, EmulatorState, SharedUi};
use egui::{self, Color32, RichText, Sense, Vec2};
use std::path::Path;

pub struct AppPicker;

impl AppPicker {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        let guard = shared.lock();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(18, 18, 24)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(28.0);
                    ui.label(
                        RichText::new("Pixel 1 HLE")
                            .size(32.0)
                            .color(Color32::from_rgb(80, 220, 120))
                            .strong(),
                    );
                    ui.label(
                        RichText::new("sailfish  ·  Android 7.1")
                            .size(14.0)
                            .color(Color32::from_rgb(140, 140, 160)),
                    );
                    ui.add_space(16.0);

                    let import_btn = ui.add_sized(
                        Vec2::new(220.0, 40.0),
                        egui::Button::new(
                            RichText::new("  Import ya!  ")
                                .size(18.0)
                                .color(Color32::from_rgb(20, 20, 28))
                                .strong(),
                        )
                        .fill(Color32::from_rgb(80, 220, 120))
                        .rounding(8.0),
                    );

                    if import_btn.clicked() {
                        drop(guard);
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Android Package", &["apk"])
                            .set_title("Import ya! – choose an APK")
                            .pick_file()
                        {
                            let path_str = path.to_string_lossy().to_string();
                            let name = extract_display_name(&path).unwrap_or_else(|| {
                                path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("imported.apk")
                                    .to_string()
                            });
                            let size_mb = std::fs::metadata(&path)
                                .map(|m| m.len() as f32 / 1_048_576.0)
                                .unwrap_or(0.0);

                            log::info!(target: "hle::display", "Import ya! selected: {} ({})", name, path_str);

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
                        return;
                    }

                    ui.add_space(20.0);
                });

                let mut guard = shared.lock();

                if guard.available_apks.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(
                            RichText::new("No APKs found")
                                .size(20.0)
                                .color(Color32::from_rgb(180, 180, 200)),
                        );
                        ui.label(
                            RichText::new("Use Import ya! or drop .apk files into ./apps/")
                                .size(14.0)
                                .color(Color32::from_rgb(120, 120, 140)),
                        );
                    });
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(10.0);
                    let available = guard.available_apks.clone();
                    for entry in &available {
                        let card_size = Vec2::new(ui.available_width() - 40.0, 72.0);
                        let (rect, response) = ui.allocate_exact_size(card_size, Sense::click());

                        let bg = if response.hovered() {
                            Color32::from_rgb(40, 48, 60)
                        } else {
                            Color32::from_rgb(28, 32, 42)
                        };

                        ui.painter().rect_filled(rect, 10.0, bg);
                        ui.painter().rect_stroke(
                            rect,
                            10.0,
                            egui::Stroke::new(1.0_f32, Color32::from_rgb(60, 70, 90)),
                        );

                        let icon_center = egui::pos2(rect.left() + 36.0, rect.center().y);
                        ui.painter()
                            .circle_filled(icon_center, 22.0, Color32::from_rgb(60, 180, 90));
                        ui.painter().text(
                            icon_center,
                            egui::Align2::CENTER_CENTER,
                            "▶",
                            egui::FontId::proportional(18.0),
                            Color32::WHITE,
                        );

                        ui.painter().text(
                            egui::pos2(rect.left() + 72.0, rect.top() + 18.0),
                            egui::Align2::LEFT_TOP,
                            &entry.name,
                            egui::FontId::proportional(18.0),
                            Color32::from_rgb(220, 220, 230),
                        );
                        ui.painter().text(
                            egui::pos2(rect.left() + 72.0, rect.top() + 42.0),
                            egui::Align2::LEFT_TOP,
                            format!("{:.1} MB  ·  arm64-v8a", entry.size_mb),
                            egui::FontId::proportional(13.0),
                            Color32::from_rgb(130, 140, 160),
                        );

                        if response.clicked() {
                            log::info!(target: "hle::display", "APK selected: {}", entry.name);
                            guard.selected_apk = Some(entry.path.clone());
                            guard.state = EmulatorState::BootAnimation {
                                apk_name: entry.name.clone(),
                            };
                            guard.loading_progress = 0.0;
                            guard.loading_status = "Preparing…".into();
                        }

                        ui.add_space(8.0);
                    }
                });
            });
    }
}

fn extract_display_name(apk_path: &Path) -> Option<String> {
    let file = std::fs::File::open(apk_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let mut manifest = archive.by_name("AndroidManifest.xml").ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut manifest, &mut buf).ok()?;

    let text = String::from_utf8_lossy(&buf);
    for candidate in ["android:label=\"", "android:label=\"@string/"] {
        if let Some(idx) = text.find(candidate) {
            let rest = &text[idx + candidate.len()..];
            let end = rest.find(['"', '<', '\0']).unwrap_or(rest.len().min(64));
            let label = rest[..end].trim();
            if !label.is_empty() && !label.starts_with('@') {
                return Some(label.to_string());
            }
        }
    }

    apk_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}
