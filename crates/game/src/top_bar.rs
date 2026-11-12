//! Leiste oben: links die Schalter, die Bereiche ein- und ausblenden
//! (Schiffe und Info in der Leiste links darunter, Hilfe als Fenster; `F1`
//! schaltet die Hilfe), daneben Spielzeit, Zeitraffer und Kamera.

use crate::camera::CameraRig;
use crate::hud::{self, format_time};
use crate::{TimeWarp, focus};
use bevy::prelude::*;
use bevy_egui::egui;
use sim::World;

/// Welche Bereiche zu sehen sind (Bedienung, nicht im Spielstand).
#[derive(Resource)]
pub struct Panels {
    /// Liste der Schiffe in der linken Leiste.
    pub ships: bool,
    /// Angaben zur Auswahl in der linken Leiste.
    pub info: bool,
    /// Fenster mit der Bedienhilfe.
    pub help: bool,
    /// Deckkraft der Seitenleisten (0 durchsichtig bis 1 deckend): Dahinter
    /// läuft die Karte weiter.
    pub opacity: f32,
}

impl Panels {
    /// Kleinste einstellbare Deckkraft: Die Schrift muss lesbar bleiben.
    const MIN_OPACITY: f32 = 0.2;

    /// Rahmen einer Seitenleiste mit der eingestellten Deckkraft.
    pub fn side_frame(&self, style: &egui::Style) -> egui::Frame {
        let frame = egui::Frame::side_top_panel(style);
        frame.fill(frame.fill.gamma_multiply(self.opacity))
    }

    /// Regler für die Deckkraft, am unteren Ende einer Seitenleiste (ohne
    /// eigenen Grund, er gehört zur Leiste).
    pub fn opacity_slider(&mut self, ui: &mut egui::Ui, id: &str) {
        egui::Panel::bottom(id.to_owned())
            .frame(egui::Frame::NONE)
            .resizable(false)
            .show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut self.opacity, Self::MIN_OPACITY..=1.0)
                        .show_value(false)
                        .text("Deckkraft"),
                );
            });
    }
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            ships: true,
            info: true,
            help: false,
            opacity: 0.85,
        }
    }
}

pub fn show(
    root: &mut egui::Ui,
    world: &World,
    warp: &TimeWarp,
    rig: &CameraRig,
    panels: &mut Panels,
) {
    if root.input(|input| input.key_pressed(egui::Key::F1)) {
        panels.help = !panels.help;
    }
    egui::Panel::top("top").show(root, |ui| {
        ui.horizontal(|ui| {
            ui.toggle_value(&mut panels.ships, "Schiffe");
            ui.toggle_value(&mut panels.info, "Info");
            ui.toggle_value(&mut panels.help, "Hilfe");
            ui.separator();
            ui.label(format_time(world.time, hud::day(world)));
            ui.separator();
            let pause = if warp.paused { "  (Pause)" } else { "" };
            ui.label(format!("Zeitraffer {}×{pause}", warp.factor()));
            ui.separator();
            let near = if rig.near { " (Nahansicht)" } else { "" };
            ui.label(format!("Kamera: {}{near}", focus::name(world, rig.target)));
        });
    });
    egui::Window::new("Hilfe")
        .open(&mut panels.help)
        .resizable(false)
        .collapsible(false)
        .default_pos([320.0, 60.0])
        .default_width(640.0)
        .show(root.ctx(), |ui| {
            for line in hud::HELP {
                ui.label(line);
            }
        });
}
