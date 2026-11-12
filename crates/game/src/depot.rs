//! Karte "Startbasis" der Seitenleiste: erscheint, wenn ein Schiff gewählt
//! ist, das an der Startbasis steht, oder ihr Zielgebiet (Klick auf seine
//! Raute; das geht nur, wenn dort kein Schiff steht, denn Schiffe haben beim
//! Klicken Vorrang). Sie zeigt das Zorit im Lager und bestellt Schiffe nach
//! den Entwürfen der Welt (siehe `sim::base` und `sim::design`).

use crate::fracht::tons_fine;
use crate::node_ui::{SelectedZone, Selection};
use crate::ui::{Notes, card, wide_button};
use bevy_egui::egui;
use sim::base::PARTS_PRICE;
use sim::{BaseKind, World};

/// Ist die Startbasis gewählt: ihr Zielgebiet oder ein Schiff, das dort
/// steht?
pub fn selected(world: &World, zone: &SelectedZone, selection: &Selection) -> bool {
    let by_zone = zone
        .0
        .and_then(|k| world.zones.get(k))
        .and_then(|z| world.base_at(z.body, &z.center));
    let by_vessel = selection.0.and_then(|i| world.base_of(i));
    by_zone
        .or(by_vessel)
        .is_some_and(|base| world.bases[base].kind == BaseKind::Depot)
}

/// Zeichnet die Karte; liefert ein eben bestelltes Schiff (es steht im
/// Protokoll und wird gewählt).
pub fn show(ui: &mut egui::Ui, world: &mut World, notes: &mut Notes) -> Option<usize> {
    card(ui, "Startbasis", |ui| {
        ui.label(format!(
            "Zorit im Lager: {}\nPreise in Zorit:\nBauteile: {} je Tonne",
            tons_fine(world.zorit),
            tons_fine(PARTS_PRICE * 1_000.0)
        ));
        let mut ordered = None;
        for design in 0..world.designs.len() {
            let d = &world.designs[design];
            let label = format!("{} bestellen ({})", d.name, tons_fine(d.price()));
            if wide_button(ui, label) {
                ordered = notes.check(world.order_ship(design)).or(ordered);
            }
        }
        ordered
    })
}
