//! Real-time APK loading progress (fed by the HLE linker)

use crate::ui::{EmulatorState, SharedUi};
use egui::{self, Color32, RichText};

pub struct LoadingScreen;

impl LoadingScreen {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi) {
        let guard = shared.lock();
        let (apk_name, progress, status) = match &guard.state {
            EmulatorState::ApkLoading { apk_name } => {
                (apk_name.clone(), guard.loading_progress, guard.loading_status.clone())
            }
            _ => return,
        };
        drop(guard);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(12, 12, 18)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);

                    ui.label(
                        RichText::new("Loading")
                            .size(28.0)
                            .color(Color32::from_rgb(80, 220, 120))
                            .strong(),
                    );
                    ui.label(
                        RichText::new(&apk_name)
                            .size(18.0)
                            .color(Color32::from_rgb(180, 190, 200)),
                    );

                    ui.add_space(40.0);

                    // Progress bar
                    let bar_width = 420.0_f32.min(ui.available_width() - 80.0);
                    let bar_height = 14.0;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(bar_width, bar_height),
                        egui::Sense::hover(),
                    );

                    // Background track
                    ui.painter().rect_filled(rect, 7.0, Color32::from_rgb(30, 34, 44));

                    // Filled portion
                    let fill_w = (bar_width * progress.clamp(0.0, 1.0)).max(0.0);
                    if fill_w > 0.0 {
                        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, bar_height));
                        ui.painter().rect_filled(fill_rect, 7.0, Color32::from_rgb(50, 200, 100));
                    }

                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("{:.0}%", progress * 100.0))
                            .size(15.0)
                            .color(Color32::from_rgb(140, 150, 170)),
                    );

                    ui.add_space(24.0);
                    ui.label(
                        RichText::new(&status)
                            .size(14.0)
                            .color(Color32::from_rgb(120, 130, 150)),
                    );
                });
            });
    }
}
