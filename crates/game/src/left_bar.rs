//! Leiste links: reine Information. Die Liste der Schiffe (ein Klick wählt)
//! und die Angaben zur Auswahl: Schiff, Zielgebiet oder Himmelskörper. Beide
//! Teile lassen sich mit den Schaltern in ihrer Kopfzeile ausblenden (zur
//! Leiste selbst siehe `side`).

use crate::hud;
use crate::top_bar::Panels;
use crate::ui::{Selected, card};
use bevy_egui::egui;
use sim::World;

/// Breite der Leiste in Pixeln.
const WIDTH: f32 = 300.0;
/// Schriftgröße der hervorgehobenen Flugdaten.
const VALUE_SIZE: f32 = 16.0;

/// Liste der Schiffe: Name und der Körper, bei dem es ist. Angedockte
/// stehen bei ihrem Verbund.
fn ships(ui: &mut egui::Ui, world: &World, selected: &mut Selected) {
    card(ui, "Schiffe", |ui| {
        if world.ship_count() == 0 {
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
    // Der Inhalt richtet sich nach den Schaltern, wie sie vor diesem Bild
    // standen; ein Klick wirkt im nächsten.
    let (show_ships, show_info) = (panels.ships, panels.info);
    let Panels {
        ships,
        info: infos,
        left,
        ..
    } = panels;
    left.show(
        root,
        "left_bar",
        WIDTH,
        |ui| {
            ui.toggle_value(ships, "Schiffe");
            ui.toggle_value(infos, "Info");
        },
        |ui| {
            if show_ships {
                self::ships(ui, world, selected);
            }
            if show_info {
                info(ui, world, selected);
            }
        },
    );
}
