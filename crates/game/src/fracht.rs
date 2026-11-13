//! Karte "Fracht & Basis" der Seitenleiste (siehe `sim::base` und
//! `sim::goods`).
//!
//! Sie erscheint für Schiffe mit Frachtraum, für Basisschiffe und für jedes
//! Schiff, das im Umkreis einer Basis steht. Sie zeigt Fracht, Baustelle und
//! Lager. Steht das Schiff an einer Basis, hat jedes Gut, das sich dort
//! bewegen lässt, eine Zeile mit − (abladen) und + (laden) in Schritten von
//! `LOT`. Die Schaltflächen darunter sind, was keine Lagerbewegung ist: mit
//! dem Basisschiff gründen und tanken.

use crate::ui::{Notes, card, stepper, wide_button};
use crate::zahl::De;
use bevy_egui::egui;
use sim::base::SITE_RADIUS;
use sim::goods::LOT;
use sim::{BaseError, BaseKind, Good, Segment, World};

/// Was eine Schaltfläche tut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    FoundMine,
    FoundStation,
    Refuel,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::FoundMine => "Mine gründen",
            Action::FoundStation => "Tankstelle gründen",
            Action::Refuel => "Tanken",
        }
    }
}

/// Tonnen mit höchstens zwei Nachkommastellen (Preise, Lagerstand der
/// Startbasis): "1 t", "0,5 t", "0,25 t".
pub fn tons_fine(kg: f64) -> String {
    let number = format!("{:.2}", De(kg / 1_000.0));
    let number = number.trim_end_matches('0').trim_end_matches(',');
    format!("{number} t")
}

/// Tonnen mit einer Nachkommastelle.
pub fn tons(kg: f64) -> String {
    format!("{:.1} t", De(kg / 1_000.0))
}

/// Steht das Schiff heil am Boden?
fn landed(world: &World, vessel: usize) -> bool {
    matches!(
        world.vessels[vessel].trajectory.current(),
        Segment::Surface { wreck: false, .. }
    )
}

/// Kann das Schiff gerade etwas tun: heil am Boden, nicht angedockt, kein
/// Fundament?
fn ready(world: &World, vessel: usize) -> bool {
    let v = &world.vessels[vessel];
    landed(world, vessel) && v.docked_to.is_none() && v.foundation_of.is_none()
}

/// Was für das Schiff gerade geht.
fn actions(world: &World, vessel: usize) -> Vec<Action> {
    let v = &world.vessels[vessel];
    if !ready(world, vessel) {
        return Vec::new();
    }
    match world.base_of(vessel).map(|k| &world.bases[k]) {
        None if v.params.base_kit => vec![Action::FoundMine, Action::FoundStation],
        Some(base) if base.complete() && base.kind != BaseKind::Mine => {
            let depot = base.kind == BaseKind::Depot;
            let room = world.vessel_fuel(vessel) < v.params.fuel_capacity;
            // An der Tankstelle immer, damit man sieht, dass es dort geht.
            if room || !depot {
                vec![Action::Refuel]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Die Güter, die sich hier zwischen Schiff und Basis bewegen lassen: an der
/// Startbasis alles, was im Lager oder an Bord ist, an der Mine Zorit, an
/// einer Baustelle die Bauteile an Bord.
fn movable(world: &World, vessel: usize) -> Vec<Good> {
    let v = &world.vessels[vessel];
    let Some(k) = world.base_of(vessel).filter(|_| ready(world, vessel)) else {
        return Vec::new();
    };
    let base = &world.bases[k];
    let aboard = |good: Good| v.cargo.get(good) > 0.0;
    [Good::Parts, Good::Zorit]
        .into_iter()
        .filter(|&good| match base.kind {
            _ if !base.complete() => good == Good::Parts && aboard(good),
            BaseKind::Depot => aboard(good) || world.stock(k, good) > 0.0,
            BaseKind::Mine => good == Good::Zorit && v.params.cargo_capacity > 0.0,
            BaseKind::FuelStation => false,
        })
        .collect()
}

/// Die Zeilen der Karte; leer: keine Karte.
fn lines(world: &World, vessel: usize) -> Vec<String> {
    let v = &world.vessels[vessel];
    let mut lines = Vec::new();
    if v.params.cargo_capacity > 0.0 {
        lines.push(format!(
            "Fracht: {} von {}",
            tons(v.cargo.total()),
            tons(v.params.cargo_capacity)
        ));
        // Was eine eigene Zeile zum Laden hat, steht dort.
        let rows = movable(world, vessel);
        for (good, kg) in v.cargo.iter().filter(|(good, _)| !rows.contains(good)) {
            lines.push(format!("  {} {}", good.name(), tons(kg)));
        }
    }
    if let Some(base) = v.foundation_of {
        lines.push(format!("Fundament von {}", world.bases[base].name));
    } else if v.params.base_kit {
        lines.push(
            "Basisschiff: wird am Landeplatz zum Fundament einer Mine oder Tankstelle.".into(),
        );
        if !landed(world, vessel) {
            lines.push(format!(
                "Dazu landen (nicht auf {}).",
                world.bodies[world.home].name
            ));
        }
    }
    if !landed(world, vessel) {
        return lines;
    }
    let tank = || {
        format!(
            "Tank: {:.0} von {:.0} kg",
            De(world.vessel_fuel(vessel).floor()),
            De(world.params(world.host(vessel)).fuel_capacity)
        )
    };
    match world.base_of(vessel).map(|k| (k, &world.bases[k])) {
        Some((_, base)) if !base.complete() => lines.push(format!(
            "{} (Baustelle):\n  {} von {} Bauteile",
            base.name,
            tons(base.delivered),
            tons(base.required)
        )),
        Some((k, base)) => {
            let (rate, capacity) = world.production(k);
            let stock = world.base_stock(k).floor();
            let per_hour = rate * 3_600.0;
            match base.kind {
                // Das Lager der Startbasis zeigt ihre eigene Karte.
                BaseKind::Depot => {
                    lines.push(tank());
                }
                BaseKind::FuelStation => {
                    lines.push(format!(
                        "{}:\n  Treibstoff {} von {}\n  (+{per_hour:.0} kg pro Stunde)",
                        base.name,
                        tons(stock),
                        tons(capacity),
                        per_hour = De(per_hour)
                    ));
                    if v.foundation_of.is_none() {
                        lines.push(tank());
                    }
                }
                BaseKind::Mine => lines.push(format!(
                    "{}:\n  Zorit {} von {}\n  (+{per_hour:.0} kg pro Stunde)",
                    base.name,
                    tons(stock),
                    tons(capacity),
                    per_hour = De(per_hour)
                )),
            }
        }
        None if v.cargo.total() > 0.0 => lines.push(format!(
            "Keine Basis im Umkreis ({:.0} km).",
            De(SITE_RADIUS / 1_000.0)
        )),
        None => {}
    }
    lines
}

impl Action {
    /// Führt die Aktion aus; die Meldung dazu (leer: steht schon im
    /// Protokoll).
    fn run(self, world: &mut World, vessel: usize) -> Result<String, BaseError> {
        let name = world.vessels[vessel].name.clone();
        Ok(match self {
            Action::FoundMine => {
                world.found_base(vessel, BaseKind::Mine)?;
                String::new()
            }
            Action::FoundStation => {
                world.found_base(vessel, BaseKind::FuelStation)?;
                String::new()
            }
            Action::Refuel => format!("{name}: {:.0} kg getankt", De(world.refuel(vessel)?)),
        })
    }
}

pub fn show(ui: &mut egui::Ui, world: &mut World, vessel: usize, notes: &mut Notes) {
    let lines = lines(world, vessel);
    if lines.is_empty() {
        return;
    }
    card(ui, "Fracht & Basis", |ui| {
        ui.label(lines.join("\n"));
        let goods = movable(world, vessel);
        if !goods.is_empty() {
            ui.weak(format!("− abladen, + laden (je {})", tons(LOT)));
        }
        for good in goods {
            let aboard = world.vessels[vessel].cargo.get(good);
            let label = format!("{} {}", good.name(), tons(aboard));
            let moved = match stepper(ui, ["−", "+"], egui::RichText::new(label)) {
                1 => world.load(vessel, good, LOT).map(|kg| (kg, "geladen")),
                -1 => world.unload(vessel, good, LOT).map(|kg| (kg, "abgeladen")),
                _ => continue,
            };
            if let Some((kg, done)) = notes.check(moved) {
                let ship = &world.vessels[vessel].name;
                notes.say(format!("{ship}: {} {} {done}", tons(kg), good.name()));
            }
        }
        for action in actions(world, vessel) {
            if wide_button(ui, action.label())
                && let Some(text) = notes.check(action.run(world, vessel))
                && !text.is_empty()
            {
                notes.say(text);
            }
        }
    });
}
