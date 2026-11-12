//! Leiste oben: links, so breit wie die Leiste darunter, die Schalter für
//! deren Teile (Schiffe, Info); daneben Spielzeit, Zeitraffer und Kamera;
//! ganz rechts der Schalter für das Hilfe-Fenster (`F1`).

use crate::camera::CameraRig;
use crate::hud::{self, format_time};
use crate::left_bar::WIDTH as LEFT_WIDTH;
use crate::{TimeWarp, focus};
use bevy::prelude::*;
use bevy_egui::egui;
use sim::World;

/// Deckkraft einer Seitenleiste (0 durchsichtig bis 1 deckend): Dahinter
/// läuft die Karte weiter.
pub struct Opacity(f32);

impl Opacity {
    /// Kleinste einstellbare Deckkraft: Die Schrift muss lesbar bleiben.
    const MIN: f32 = 0.2;

    /// Rahmen der Seitenleiste mit dieser Deckkraft.
    pub fn frame(&self, style: &egui::Style) -> egui::Frame {
        let frame = egui::Frame::side_top_panel(style);
        frame.fill(frame.fill.gamma_multiply(self.0))
    }

    /// Regler am unteren Ende der Seitenleiste (ohne eigenen Grund, er
    /// gehört zur Leiste).
    pub fn slider(&mut self, ui: &mut egui::Ui, id: &str) {
        egui::Panel::bottom(id.to_owned())
            .frame(egui::Frame::NONE)
            .resizable(false)
            .show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut self.0, Self::MIN..=1.0)
                        .show_value(false)
                        .text("Deckkraft"),
                );
            });
    }
}

/// Welche Bereiche zu sehen sind und wie (Bedienung, nicht im Spielstand).
#[derive(Resource)]
pub struct Panels {
    /// Liste der Schiffe in der linken Leiste.
    pub ships: bool,
    /// Angaben zur Auswahl in der linken Leiste.
    pub info: bool,
    /// Fenster mit der Bedienhilfe.
    pub help: bool,
    /// Deckkraft der linken und der rechten Seitenleiste.
    pub left: Opacity,
    pub right: Opacity,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            ships: true,
            info: true,
            help: false,
            left: Opacity(0.85),
            right: Opacity(0.85),
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
            // Die Schalter der linken Leiste stehen über ihr, in ihrer Breite.
            ui.allocate_ui_with_layout(
                egui::vec2(
                    LEFT_WIDTH - ui.spacing().item_spacing.x,
                    ui.available_height(),
                ),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.toggle_value(&mut panels.ships, "Schiffe");
                    ui.toggle_value(&mut panels.info, "Info");
                    ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
                },
            );
            ui.separator();
            ui.label(format_time(world.time, hud::day(world)));
            ui.separator();
            let pause = if warp.paused { "  (Pause)" } else { "" };
            ui.label(format!("Zeitraffer {}×{pause}", warp.factor()));
            ui.separator();
            let near = if rig.near { " (Nahansicht)" } else { "" };
            ui.label(format!("Kamera: {}{near}", focus::name(world, rig.target)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut panels.help, "Hilfe");
            });
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
