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
const WIDTH: f32 = 300.0;

/// Liste der Schiffe; angedockte stehen bei ihrem Verbund.
fn ships(ui: &mut egui::Ui, world: &World, selected: &mut Selected) {
    card(ui, "Schiffe", |ui| {
        if world.vessels.is_empty() {
            ui.weak("Noch keine. Bestellen: rechts in der Karte \"Startbasis\".");
        }
        for i in (0..world.vessels.len()).filter(|&i| world.vessels[i].docked_to.is_none()) {
            let chosen = selected.vessel.0 == Some(i);
            if ui
                .selectable_label(chosen, hud::ship_name(world, i))
                .clicked()
            {
                selected.select_vessel(i);
            }
            // Ohne die Spalten der früheren Liste: Die Leiste ist schmal.
            ui.weak(format!(
                "  {}",
                hud::ship_status(world, i).replace("  ", " ")
            ));
        }
    });
}

/// Angaben zu dem, was gewählt ist.
fn info(ui: &mut egui::Ui, world: &World, selected: &Selected) {
    card(ui, "Info", |ui| {
        let mut any = false;
        let mut section = |ui: &mut egui::Ui, title: &str, lines: Vec<String>| {
            if any {
                ui.separator();
            }
            any = true;
            ui.strong(title);
            ui.label(lines.join("\n"));
        };
        if let Some(i) = selected.vessel.0 {
            section(ui, &hud::ship_name(world, i), hud::info_lines(world, i));
        }
        if let Some(k) = selected.zone.0 {
            let mut lines = hud::zone_lines(world, k);
            let title = lines.remove(0);
            section(ui, &title, lines);
        }
        if let Some(b) = selected.body.0 {
            section(ui, &world.bodies[b].name, hud::body_details(world, b));
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
        .frame(panels.side_frame(root.style()))
        .show(root, |ui| {
            panels.opacity_slider(ui, "left_opacity");
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
