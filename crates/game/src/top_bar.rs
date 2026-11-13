//! Leiste oben, zwischen den Seitenleisten: Spielzeit, Zeitraffer und
//! Kamera, ganz rechts die Schalter für die Fenster Entwürfe, Wirtschaft und Hilfe
//! (`F1`). Sie wird
//! nach den Seitenleisten gezeichnet und beginnt deshalb genau an deren
//! Kante.

use crate::camera::CameraRig;
use crate::fracht::tons_fine;
use crate::hud::{self, format_time};
use crate::side::{Edge, Side};
use crate::zahl::De;
use crate::{TimeWarp, focus, wirtschaft};
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
    /// Fenster mit dem Überblick über Basen, Fracht, Aufgaben und Technik,
    /// und sein Reiter.
    pub economy: bool,
    pub economy_tab: wirtschaft::Tab,
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
            economy_tab: wirtschaft::Tab::default(),
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
            // Die Daten gehören dem Spieler, nicht einem Ort, und stehen
            // deshalb hier; ein Klick öffnet die Übersicht dazu.
            ui.toggle_value(
                &mut panels.economy,
                format!("Daten {:.0} GB", De(world.data)),
            )
            .on_hover_text(format!(
                "Daten aus erfüllten Aufgaben. Klick: Übersicht, Aufgaben und Technik.\n\
                     Im Lager der Startbasis: Zorit {}",
                tons_fine(world.zorit)
            ));
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
        .default_pos(crate::ui::window_pos(0))
        .default_width(640.0)
        .show(root.ctx(), |ui| {
            for line in hud::HELP {
                ui.label(line);
            }
        });
}
