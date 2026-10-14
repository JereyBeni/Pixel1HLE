//! Classic Android-style APK drawer (vintage 4.1–7.1 look)

use crate::ui::{ApkEntry, EmulatorState, SharedUi};
use egui::{self, Color32, RichText, Sense, Vec2};

pub struct AppPicker;

impl AppPicker {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        let mut guard = shared.lock();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(18, 18, 24)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
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
                    ui.add_space(30.0);
                });

                if guard.available_apks.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(
                            RichText::new("No APKs found")
                                .size(20.0)
                                .color(Color32::from_rgb(180, 180, 200)),
                        );
                        ui.label(
                            RichText::new("Drop .apk files next to the binary\nor place them in ./apks/")
                                .size(14.0)
                                .color(Color32::from_rgb(120, 120, 140)),
                        );
                        ui.add_space(20.0);
                        if ui.button(RichText::new("  Refresh  ").size(16.0)).clicked() {
                            // parent will re-scan
                        }
                    });
                    return;
                }

                // Grid of APK cards (vintage drawer style)
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
                            egui::Stroke::new(1.0, Color32::from_rgb(60, 70, 90)),
                        );

                        // Icon placeholder (green Android-ish circle)
                        let icon_center = egui::pos2(rect.left() + 36.0, rect.center().y);
                        ui.painter().circle_filled(icon_center, 22.0, Color32::from_rgb(60, 180, 90));
                        ui.painter().text(
                            icon_center,
                            egui::Align2::CENTER_CENTER,
                            "▶",
                            egui::FontId::proportional(18.0),
                            Color32::WHITE,
                        );

                        // Text
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
