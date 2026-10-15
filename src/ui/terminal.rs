//! In-app terminal panel (live log stream)

use crate::logging::{self, LogState};
use egui::{self, Color32, RichText, ScrollArea};
use log::Level;

pub struct TerminalPanel;

impl TerminalPanel {
    /// Draw a docked terminal at the bottom. Returns true if focused/consuming keys.
    pub fn ui(ctx: &egui::Context, log_state: &LogState, open: &mut bool) {
        if !*open {
            return;
        }

        egui::TopBottomPanel::bottom("hle_terminal")
            .resizable(true)
            .default_height(180.0)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Terminal")
                            .strong()
                            .color(Color32::from_rgb(80, 220, 120)),
                    );
                    ui.separator();
                    if ui.button("Clear").clicked() {
                        log_state.lock().ring.clear();
                    }
                    if ui.button("Hide (` )").clicked() {
                        *open = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("logs/pixel1hle.log")
                                .small()
                                .color(Color32::from_rgb(120, 130, 150)),
                        );
                    });
                });

                ui.separator();

                let lines = logging::snapshot_lines(log_state);
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &lines {
                            let color = match line.level {
                                Level::Error => Color32::from_rgb(255, 100, 100),
                                Level::Warn => Color32::from_rgb(255, 200, 80),
                                Level::Info => Color32::from_rgb(180, 220, 180),
                                Level::Debug => Color32::from_rgb(140, 160, 200),
                                Level::Trace => Color32::from_rgb(100, 100, 120),
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("[{}]", line.timestamp))
                                        .small()
                                        .color(Color32::from_rgb(90, 100, 120))
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(format!("{:>5}", line.level))
                                        .small()
                                        .color(color)
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(format!("{}: {}", line.target, line.message))
                                        .small()
                                        .color(Color32::from_rgb(200, 210, 220))
                                        .monospace(),
                                );
                            });
                        }
                    });
            });
    }
}
