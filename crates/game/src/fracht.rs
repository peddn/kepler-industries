//! Karte "Fracht & Basis" der Seitenleiste (siehe `sim::base`).
//!
//! Sie erscheint für Schiffe mit Frachtraum, für Basisschiffe und für jedes
//! Schiff, das im Umkreis einer Basis steht. Sie zeigt Fracht, Baustelle und
//! Lager; die Schaltflächen darunter tun, was dort gerade geht: an der
//! Startbasis laden, tanken und Zorit abladen, mit dem Basisschiff gründen,
//! an der Baustelle abladen, an der Tankstelle tanken, an der Mine laden.

use crate::ui::{Notes, card, wide_button};
use bevy_egui::egui;
use sim::base::{PARTS_PRICE, SITE_RADIUS};
use sim::{BaseError, BaseKind, Segment, World};

/// Was eine Schaltfläche tut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    FoundMine,
    FoundStation,
    Unload,
    Refuel,
    LoadZorit,
    LoadParts,
    DeliverZorit,
}

impl Action {
    /// Beschriftung; beim Laden von Bauteilen mit dem Preis für das, was in
    /// den Frachtraum passt.
    fn label(self, world: &World, vessel: usize) -> String {
        if self == Action::LoadParts {
            let v = &world.vessels[vessel];
            let room = v.params.cargo_capacity - v.cargo.total();
            return format!("Bauteile laden ({} Zorit)", tons_fine(room * PARTS_PRICE));
        }
        match self {
            Action::FoundMine => "Mine gründen",
            Action::FoundStation => "Tankstelle gründen",
            Action::Unload => "Bauteile abladen",
            Action::Refuel => "Tanken",
            Action::LoadZorit => "Zorit laden",
            Action::LoadParts => "Bauteile laden",
            Action::DeliverZorit => "Zorit abladen",
        }
        .into()
    }
}

/// Tonnen mit höchstens zwei Nachkommastellen (Preise, Lagerstand der
/// Startbasis): "1 t", "0,5 t", "0,25 t".
pub fn tons_fine(kg: f64) -> String {
    let number = format!("{:.2}", kg / 1_000.0);
    let number = number.trim_end_matches('0').trim_end_matches('.');
    format!("{} t", number.replace('.', ","))
}

/// Tonnen mit einer Nachkommastelle.
fn tons(kg: f64) -> String {
    format!("{:.1} t", kg / 1_000.0).replace('.', ",")
}

/// Steht das Schiff heil am Boden?
fn landed(world: &World, vessel: usize) -> bool {
    matches!(
        world.vessels[vessel].trajectory.current(),
        Segment::Surface { wreck: false, .. }
    )
}

/// Was für das Schiff gerade geht (höchstens `BUTTONS`).
fn actions(world: &World, vessel: usize) -> Vec<Action> {
    let v = &world.vessels[vessel];
    if !landed(world, vessel) || v.docked_to.is_some() || v.foundation_of.is_some() {
        return Vec::new();
    }
    let base = world.base_of(vessel).map(|k| &world.bases[k]);
    let mut actions = Vec::new();
    match base {
        None if v.params.base_kit => {
            actions.extend([Action::FoundMine, Action::FoundStation]);
        }
        None => {}
        Some(base) if !base.complete() => {
            if v.cargo.parts > 0.0 {
                actions.push(Action::Unload);
            }
        }
        Some(base) => match base.kind {
            BaseKind::Depot => {
                if v.cargo.zorit > 0.0 {
                    actions.push(Action::DeliverZorit);
                }
                if v.cargo.total() < v.params.cargo_capacity {
                    actions.push(Action::LoadParts);
                }
                if world.vessel_fuel(vessel) < v.params.fuel_capacity {
                    actions.push(Action::Refuel);
                }
            }
            BaseKind::FuelStation => actions.push(Action::Refuel),
            BaseKind::Mine => {
                if v.params.cargo_capacity > 0.0 {
                    actions.push(Action::LoadZorit);
                }
            }
        },
    }
    actions
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
        if v.cargo.parts > 0.0 {
            lines.push(format!("  Bauteile {}", tons(v.cargo.parts)));
        }
        if v.cargo.zorit > 0.0 {
            lines.push(format!("  Zorit {}", tons(v.cargo.zorit)));
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
            world.vessel_fuel(vessel).floor(),
            world.params(world.host(vessel)).fuel_capacity
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
                        "{}:\n  Treibstoff {stock:.0} von {capacity:.0} kg\n  (+{per_hour:.0} kg pro Stunde)",
                        base.name
                    ));
                    if v.foundation_of.is_none() {
                        lines.push(tank());
                    }
                }
                BaseKind::Mine => lines.push(format!(
                    "{}:\n  Zorit {stock:.0} von {capacity:.0} kg\n  (+{per_hour:.0} kg pro Stunde)",
                    base.name
                )),
            }
        }
        None if v.cargo.total() > 0.0 => lines.push(format!(
            "Keine Basis im Umkreis ({:.0} km).",
            SITE_RADIUS / 1_000.0
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
            Action::Unload => format!("{name}: {} Bauteile abgeladen", tons(world.unload(vessel)?)),
            Action::Refuel => format!("{name}: {:.0} kg getankt", world.refuel(vessel)?),
            Action::LoadZorit => {
                format!("{name}: {} Zorit geladen", tons(world.load_zorit(vessel)?))
            }
            Action::LoadParts => format!(
                "{name}: {} Bauteile geladen",
                tons(world.load_parts(vessel)?)
            ),
            Action::DeliverZorit => format!(
                "{name}: {} Zorit abgeladen, im Lager {}",
                tons(world.deliver_zorit(vessel)?),
                tons_fine(world.zorit)
            ),
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
        for action in actions(world, vessel) {
            if wide_button(ui, action.label(world, vessel))
                && let Some(text) = notes.check(action.run(world, vessel))
                && !text.is_empty()
            {
                notes.say(text);
            }
        }
    });
}
