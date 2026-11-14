//! Karte "Startbasis" der Seitenleiste: erscheint, wenn ein Schiff gewählt
//! ist, das an der Startbasis steht, oder ihr Zielgebiet (Klick auf seine
//! Raute; das geht nur, wenn dort kein Schiff steht, denn Schiffe haben beim
//! Klicken Vorrang). Sie zeigt das Lager (Zorit und Bauteile), tauscht Zorit
//! gegen Bauteile, bestellt Schiffe nach den Entwürfen der Welt und baut das gewählte Schiff auf einen anderen Entwurf
//! mit demselben Chassis um (siehe `sim::base` und `sim::design`).

use crate::fracht::tons_fine;
use crate::node_ui::{SelectedZone, Selection};
use crate::ui::{Notes, card, wide_button};
use bevy_egui::egui;
use sim::base::PARTS_PRICE;
use sim::{BaseKind, Good, World};

/// So viele Bauteile tauscht ein Klick ein (kg).
const PARTS_LOT: f64 = 1_000.0;

/// Das Lager der Startbasis, ein Gut je Zeile: Zorit und Bauteile immer (mit
/// dem einen bezahlt man, das andere tauscht man hier ein), alles Weitere,
/// wenn es da ist.
pub fn stock(world: &World) -> Vec<String> {
    let fixed = [Good::Zorit, Good::Parts];
    let more = world
        .depot()
        .into_iter()
        .flat_map(|depot| world.bases[depot].stock.iter())
        .map(|(good, _)| good)
        .filter(|good| !fixed.contains(good));
    fixed
        .into_iter()
        .chain(more)
        .map(|good| {
            let name = good.name(&world.bodies);
            format!("{name} {}", tons_fine(world.in_depot(good)))
        })
        .collect()
}

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

/// Preisunterschied eines Umbaus als Text: „+0,4 t", „−0,4 t" oder
/// „kostenlos".
fn difference(price: f64) -> String {
    if price > 0.0 {
        format!("+{}", tons_fine(price))
    } else if price < 0.0 {
        format!("−{}", tons_fine(-price))
    } else {
        "kostenlos".into()
    }
}

/// Umbau des Schiffs `vessel`, wenn es nach einem Entwurf gebaut ist und an
/// der Startbasis steht: eine Schaltfläche je anderem Entwurf mit demselben
/// Chassis, mit dem Preisunterschied.
fn refit(ui: &mut egui::Ui, world: &mut World, vessel: usize, notes: &mut Notes) {
    let v = &world.vessels[vessel];
    let Some(current) = v.design.clone().filter(|_| world.at_depot(vessel)) else {
        return;
    };
    ui.separator();
    ui.label(format!("Umbau von {}:", v.name));
    let fitting: Vec<usize> = (0..world.designs.len())
        .filter(|&k| world.designs[k].chassis == current.chassis && world.designs[k] != current)
        .collect();
    if fitting.is_empty() {
        ui.weak("Kein anderer Entwurf mit diesem Chassis (Fenster „Entwürfe\").");
    }
    for design in fitting {
        let d = &world.designs[design];
        let price = d.price() - current.price();
        let label = format!("auf {} ({})", d.name, difference(price));
        if wide_button(ui, label)
            && let Some(paid) = notes.check(world.refit(vessel, design))
        {
            let (ship, name) = (&world.vessels[vessel].name, &world.designs[design].name);
            notes.say(format!("{ship} umgebaut auf {name} ({})", difference(paid)));
        }
    }
}

/// Zeichnet die Karte, mit dem Umbau für das gewählte Schiff `vessel`;
/// liefert ein eben bestelltes Schiff (es steht im Protokoll und wird
/// gewählt).
pub fn show(
    ui: &mut egui::Ui,
    world: &mut World,
    vessel: Option<usize>,
    notes: &mut Notes,
) -> Option<usize> {
    card(ui, "Startbasis", |ui| {
        ui.label(format!("Lager:\n  {}", stock(world).join("\n  ")));
        // Tauschen ist ein eigener Schritt: Die Bauteile kommen ins Lager,
        // geladen wird in der Karte „Fracht & Basis".
        ui.label("Zorit gegen Bauteile tauschen:");
        let label = format!(
            "{} Bauteile ({} Zorit)",
            tons_fine(PARTS_LOT),
            tons_fine(PARTS_LOT * PARTS_PRICE)
        );
        if wide_button(ui, label)
            && let Some(bought) = notes.check(world.buy_parts(PARTS_LOT))
        {
            notes.say(format!(
                "{} Bauteile eingetauscht, im Lager {}",
                tons_fine(bought),
                tons_fine(world.in_depot(Good::Parts))
            ));
        }
        ui.label("Preise in Zorit:");
        let mut ordered = None;
        for design in 0..world.designs.len() {
            let d = &world.designs[design];
            let label = format!("{} bestellen ({})", d.name, tons_fine(d.price()));
            if wide_button(ui, label) {
                ordered = notes.check(world.order_ship(design)).or(ordered);
            }
        }
        if let Some(vessel) = vessel {
            refit(ui, world, vessel, notes);
        }
        ordered
    })
}
