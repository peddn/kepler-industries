//! Leiste oben, zwischen den Seitenleisten: Spielzeit, Zeitraffer und
//! Kamera, ganz rechts die Schalter für die Fenster Entwürfe, Wirtschaft und Hilfe
//! (`F1`). Sie wird
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
    /// Fenster mit dem Überblick über Basen und Fracht.
    pub economy: bool,
    /// Fenster mit dem Schiffseditor.
    pub designs: bool,
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
            economy: false,
            designs: false,
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
        // Zuerst die Schalter rechts, dann links, was daneben Platz hat: Wird
        // es eng, kürzt sich die Angabe zur Kamera, statt in die Schalter zu
        // laufen.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.toggle_value(&mut panels.help, "Hilfe");
            ui.toggle_value(&mut panels.economy, "Wirtschaft");
            ui.toggle_value(&mut panels.designs, "Entwürfe");
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(format_time(world.time, hud::day(world)));
                ui.separator();
                // Knapp gehalten: Zwischen den Seitenleisten ist wenig Platz.
                let pause = if warp.paused { " (Pause)" } else { "" };
                ui.label(format!("{}×{pause}", warp.factor()))
                    .on_hover_text("Zeitraffer (Tasten , und .; Leertaste: Pause)");
                ui.separator();
                let near = if rig.near { " (Nahansicht)" } else { "" };
                let camera = format!("Kamera: {}{near}", focus::name(world, rig.target));
                ui.add(egui::Label::new(&camera).truncate())
                    .on_hover_text(camera);
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
