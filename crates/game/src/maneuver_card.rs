//! Karte "Manöver" der Seitenleiste: die Kette der Nodes des Schiffs (ein
//! Klick wählt einen) und die Werte des gewählten Nodes. Gesetzt, verschoben
//! und geändert werden Nodes in der Karte (`node_ui`).

use crate::hud;
use crate::ui::card;
use bevy_egui::egui;
use sim::{Segment, World};

/// Zeichnet die Karte; `chosen` ist der gewählte Node. Liefert den
/// Zeitpunkt eines angeklickten Nodes (der dann gewählt wird).
pub fn show(ui: &mut egui::Ui, world: &World, i: usize, chosen: Option<usize>) -> Option<f64> {
    let trajectory = &world.vessels[i].trajectory;
    let nodes = trajectory.nodes();
    // Nodes gehen im Flug und am Boden mit geplantem Start (danach).
    let can_plan =
        trajectory.launch().is_some() || !matches!(trajectory.current(), Segment::Surface { .. });
    if nodes.is_empty() && !can_plan {
        return None;
    }
    card(ui, "Manöver", |ui| {
        if nodes.is_empty() {
            ui.weak("Linksklick auf die Bahn setzt einen Node.");
            return None;
        }
        // Im Spiel ist vorerst jede Kette scharf; nicht scharf nur über `--node`.
        if !trajectory.armed() {
            ui.weak("Nicht scharf: rückt bei Brennbeginn einen Umlauf weiter.");
        }
        let mut clicked = None;
        for (k, line) in hud::chain_lines(world, i, chosen).into_iter().enumerate() {
            if ui.selectable_label(chosen == Some(k), line).clicked() {
                clicked = Some(nodes[k].node.time);
            }
        }
        if let Some(k) = chosen
            && let Ok(plan) = nodes[k].plan
        {
            ui.separator();
            ui.label(hud::node_details(world, i, k, &plan).join("\n"));
        }
        clicked
    })
}
