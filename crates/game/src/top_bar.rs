//! Leiste oben, zwischen den Seitenleisten: Spielzeit, Zeitraffer und
//! Kamera, ganz rechts der Schalter für das Hilfe-Fenster (`F1`). Sie wird
//! nach den Seitenleisten gezeichnet und beginnt deshalb genau an deren
//! Kante.

use crate::camera::CameraRig;
use crate::hud::{self, format_time};
use crate::side::{Edge, Side};
use crate::{TimeWarp, focus};
use bevy::prelude::*;
use bevy_egui::egui;
use sim::World;

/// Welche Bereiche zu sehen sind und wie (Bedienung, nicht im Spielstand).
#[derive(Resource)]
pub struct Panels {
    /// Liste der Schiffe in der linken Leiste.
    pub ships: bool,
    /// Angaben zur Auswahl in der linken Leiste.
    pub info: bool,
    /// Fenster mit der Bedienhilfe.
    pub help: bool,
    pub left: Side,
    pub right: Side,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            ships: true,
            info: true,
            help: false,
            left: Side::new(Edge::Left),
            right: Side::new(Edge::Right),
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
