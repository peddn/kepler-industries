//! Leiste links: reine Information. Die Liste der Schiffe (ein Klick wählt)
//! und die Angaben zur Auswahl: Schiff, Zielgebiet oder Himmelskörper. Beide
//! Teile lassen sich in der Leiste oben ausblenden. Unten der Regler für die
//! Deckkraft der Seitenleisten.

use crate::hud;
use crate::top_bar::Panels;
use crate::ui::{Selected, card};
use bevy_egui::egui;
use sim::World;

/// Breite der Leiste in Pixeln.
pub const WIDTH: f32 = 300.0;
/// Schriftgröße der hervorgehobenen Flugdaten.
const VALUE_SIZE: f32 = 16.0;

/// Liste der Schiffe: Name und der Körper, bei dem es ist. Angedockte
/// stehen bei ihrem Verbund.
fn ships(ui: &mut egui::Ui, world: &World, selected: &mut Selected) {
    card(ui, "Schiffe", |ui| {
        if world.vessels.is_empty() {
            ui.weak("Noch keine. Bestellen: rechts in der Karte \"Startbasis\".");
        }
        for i in (0..world.vessels.len()).filter(|&i| world.vessels[i].docked_to.is_none()) {
            ui.horizontal(|ui| {
                let chosen = selected.vessel.0 == Some(i);
                if ui
                    .selectable_label(chosen, hud::ship_name(world, i))
                    .clicked()
                {
                    selected.select_vessel(i);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(hud::ship_body(world, i));
                });
            });
        }
    });
}

/// Zustand und Flugdaten des gewählten Schiffs, die Werte hervorgehoben.
fn vessel_info(ui: &mut egui::Ui, world: &World, i: usize) {
    ui.label(hud::ship_status(world, i).replace("  ", " "));
    egui::Grid::new("flight_data")
        .num_columns(2)
        .show(ui, |ui| {
            for (label, value) in hud::flight_data(world, i) {
                ui.label(label);
                ui.label(egui::RichText::new(value).strong().size(VALUE_SIZE));
                ui.end_row();
            }
        });
    ui.label(hud::info_lines(world, i).join("\n"));
}

/// Angaben zu dem, was gewählt ist.
fn info(ui: &mut egui::Ui, world: &World, selected: &Selected) {
    card(ui, "Info", |ui| {
        let mut any = false;
        let mut title = |ui: &mut egui::Ui, title: &str| {
            if any {
                ui.separator();
            }
            any = true;
            ui.strong(title);
        };
        if let Some(i) = selected.vessel.0 {
            title(ui, &hud::ship_name(world, i));
            vessel_info(ui, world, i);
        }
        if let Some(k) = selected.zone.0 {
            let mut lines = hud::zone_lines(world, k);
            title(ui, &lines.remove(0));
            ui.label(lines.join("\n"));
        }
        if let Some(b) = selected.body.0 {
            title(ui, &world.bodies[b].name);
            ui.label(hud::body_details(world, b).join("\n"));
        }
        if !any {
            ui.weak("Nichts gewählt.");
        }
    });
}

pub fn show(root: &mut egui::Ui, world: &World, selected: &mut Selected, panels: &mut Panels) {
    if !(panels.ships || panels.info) {
        return;
    }
    egui::Panel::left("left_bar")
        .resizable(false)
        .exact_size(WIDTH)
        .frame(panels.left.frame(root.style()))
        .show(root, |ui| {
            panels.left.slider(ui, "left_opacity");
            egui::ScrollArea::vertical().show(ui, |ui| {
                if panels.ships {
                    ships(ui, world, selected);
                }
                if panels.info {
                    info(ui, world, selected);
                }
            });
        });
}
