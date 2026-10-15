//! Classic Android spinning/pulsing boot logo (pure egui)

use crate::ui::{EmulatorState, SharedUi};
use egui::{self, Color32, Pos2};
use std::f32::consts::TAU;

pub struct BootAnimation;

impl BootAnimation {
    pub fn ui(ctx: &egui::Context, shared: &SharedUi, time: f64) {
        let guard = shared.lock();
        let apk_name = match &guard.state {
            EmulatorState::BootAnimation { apk_name } => apk_name.clone(),
            _ => return,
        };
        drop(guard);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(0, 0, 0)))
            .show(ctx, |ui| {
                let center = ui.available_rect_before_wrap().center();
                let t = time as f32;

                let pulse = (t * 2.5).sin() * 0.5 + 0.5;
                let radius = 70.0 + pulse * 12.0;
                ui.painter().circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(
                        3.0_f32,
                        Color32::from_rgb(40, 180, 80).gamma_multiply(0.7 + pulse * 0.3),
                    ),
                );

                for i in 0..3 {
                    let angle = t * 1.8 + i as f32 * (TAU / 3.0);
                    let start = angle;
                    let end = angle + 1.1;
                    draw_arc(
                        ui,
                        center,
                        48.0,
                        start,
                        end,
                        Color32::from_rgb(70, 210, 100),
                        4.0,
                    );
                }

                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "▶",
                    egui::FontId::proportional(36.0),
                    Color32::from_rgb(80, 220, 120),
                );

                ui.painter().text(
                    Pos2::new(center.x, center.y + 110.0),
                    egui::Align2::CENTER_CENTER,
                    format!("Starting {apk_name}"),
                    egui::FontId::proportional(16.0),
                    Color32::from_rgb(160, 170, 180),
                );
                ui.painter().text(
                    Pos2::new(center.x, center.y + 135.0),
                    egui::Align2::CENTER_CENTER,
                    "Pixel 1  ·  sailfish",
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(100, 110, 130),
                );
            });
    }
}

fn draw_arc(
    ui: &egui::Ui,
    center: Pos2,
    radius: f32,
    start: f32,
    end: f32,
    color: Color32,
    width: f32,
) {
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let a = start + (end - start) * (i as f32 / steps as f32);
        points.push(Pos2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(width, color)));
}
