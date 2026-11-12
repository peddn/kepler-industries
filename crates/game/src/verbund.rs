//! Karte "Verbund" in der Seitenleiste: Abdocken eines angedockten Schiffs.
//!
//! Sie erscheint, wenn ein Verbund gewählt ist. `◄ ►` wählt das Teil (bei
//! mehreren), `− +` den Treibstoff, den es mitnimmt (Umschalt: fein),
//! vorgeschlagen ist eine Aufteilung nach Tankgröße. Darunter steht, was beide
//! danach haben. "Abdocken" führt es jetzt aus (`World::undock`).

use crate::hud::km;
use crate::ui::{Notes, card, stepper, wide_button};
use bevy::prelude::*;
use bevy_egui::egui;
use sim::docking::SEPARATION_SPEED;
use sim::{World, kepler};

/// Schritte für den Treibstoff in kg (mit Umschalt: fein).
const STEP: f64 = 10.0;
const FINE_STEP: f64 = 1.0;

/// Was in der Karte eingestellt ist (Bedienung, nicht im Spielstand).
#[derive(Resource, Default)]
pub struct UndockChoice {
    /// Hauptschiff, für das die Wahl gilt.
    host: Option<usize>,
    part: Option<usize>,
    /// Mitgenommener Treibstoff in kg; `None`: der Vorschlag.
    fuel: Option<f64>,
}

/// Gewähltes Teil und Treibstoff, passend zum gewählten Verbund.
fn current(world: &World, choice: &mut UndockChoice, host: usize, parts: &[usize]) -> (usize, f64) {
    if choice.host != Some(host) || !choice.part.is_some_and(|p| parts.contains(&p)) {
        *choice = UndockChoice {
            host: Some(host),
            part: Some(parts[0]),
            fuel: None,
        };
    }
    let part = choice.part.expect("gesetzt");
    let range = world.undock_fuel(part).expect("angedockt");
    let fuel = choice
        .fuel
        .map_or(range.suggested, |f| f.clamp(range.min, range.max));
    (part, fuel)
}

pub fn show(
    ui: &mut egui::Ui,
    world: &mut World,
    host: usize,
    choice: &mut UndockChoice,
    notes: &mut Notes,
) {
    let parts = world.parts(host);
    if parts.is_empty() {
        return;
    }
    card(ui, "Verbund", |ui| {
        let (part, fuel) = current(world, choice, host, &parts);
        let step = stepper(ui, ["◄", "►"], world.vessels[part].name.as_str().into());
        if step != 0 {
            let k = parts.iter().position(|&p| p == part).unwrap_or(0) as i32;
            choice.part = Some(parts[(k + step).rem_euclid(parts.len() as i32) as usize]);
            choice.fuel = None;
        }
        let (part, fuel_now) = current(world, choice, host, &parts);
        let fuel = if step == 0 { fuel } else { fuel_now };
        let range = world.undock_fuel(part).expect("angedockt");

        ui.label("Treibstoff mitnehmen");
        let text = format!(
            "{fuel:.0} kg ({:.0}–{:.0})",
            range.min.floor(),
            range.max.floor()
        );
        let sign = stepper(ui, ["−", "+"], text.into());
        if sign != 0 {
            let fine = ui.input(|input| input.modifiers.shift);
            let step = if fine { FINE_STEP } else { STEP };
            choice.fuel = Some((fuel + sign as f64 * step).clamp(range.min, range.max));
        }

        let (part_dv, host_dv) = world.undock_delta_v(part, fuel);
        // Ein Stoß gegen die Flugrichtung lässt das Schiff pro Umlauf etwa
        // 3 · Umlaufzeit · Stoß vorauslaufen.
        let (body, state) = world.vessel_state(host);
        let drift = kepler::period(world.bodies[body].gm, &state)
            .map(|period| 3.0 * period * SEPARATION_SPEED);
        ui.label(
            [
                format!("{} danach:", world.vessels[part].name),
                format!("  {fuel:.0} kg, Δv {part_dv:.0} m/s"),
                format!("{} danach:", world.vessels[host].name),
                format!("  {:.0} kg, Δv {host_dv:.0} m/s", range.pool - fuel),
                format!("Trennstoß {SEPARATION_SPEED:.0} m/s gegen die Flugrichtung,"),
                drift.map_or(String::new(), |d| {
                    format!("danach etwa {} mehr Abstand pro Umlauf.", km(d))
                }),
            ]
            .join("\n"),
        );
        if wide_button(ui, "Abdocken") && notes.check(world.undock(part, fuel)).is_some() {
            *choice = UndockChoice::default();
        }
    });
}
