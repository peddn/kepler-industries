//! Karte "Ziel" der Seitenleiste: mit ◄ ► ein anderes Schiff, einen Planeten
//! oder Mond als Ziel wählen, die Werte dazu sehen (Annäherung, Startfenster)
//! und das Andocken ein- und ausschalten.

use crate::hud::{self, format_span, km};
use crate::rendezvous::{Rendezvous, TARGET_COLOR, TargetInfo, TransferInfo, distance_text};
use crate::ui::{Notes, card, color32, stepper, wide_button};
use bevy_egui::egui;
use sim::World;

/// Ein Ziel in der Karte "Ziel".
#[derive(Clone, Copy, PartialEq)]
enum Target {
    None,
    Vessel(usize),
    Body(usize),
}

/// Das Ziel eines Schiffs.
fn target_of(world: &World, vessel: usize) -> Target {
    let v = &world.vessels[vessel];
    match (v.target, v.target_body) {
        (Some(t), _) => Target::Vessel(t),
        (None, Some(b)) => Target::Body(b),
        (None, None) => Target::None,
    }
}

/// Nächstes Ziel beim Blättern: kein Ziel, dann die anderen Schiffe, dann
/// Planeten und Monde (alle Körper mit einer Bahn, also ohne die Sonne).
fn cycle_target(world: &World, vessel: usize, step: i32) -> Target {
    let vessels = (0..world.vessels.len())
        .filter(|&k| k != vessel)
        .map(Target::Vessel);
    let bodies = (0..world.bodies.len())
        .filter(|&b| world.bodies[b].orbit.is_some())
        .map(Target::Body);
    let choices: Vec<Target> = std::iter::once(Target::None)
        .chain(vessels)
        .chain(bodies)
        .collect();
    let now = target_of(world, vessel);
    let current = choices.iter().position(|&c| c == now).unwrap_or(0);
    choices[(current as i32 + step).rem_euclid(choices.len() as i32) as usize]
}

/// Werte zu einem Himmelskörper als Ziel: Abstand, Startfenster, Annäherung.
fn transfer_result(world: &World, info: &TransferInfo) -> String {
    let row = |label: &str, value: String| format!("{label:<14}{value}");
    let day = hud::day(world);
    let center = |b| sim::body::body_state(&world.bodies, b, world.time).position;
    let (own_body, own) = world.vessel_state(info.vessel);
    let distance = (center(own_body) + own.position).distance(center(info.body));
    let mut lines = vec![row("Abstand", distance_text(distance))];
    match &info.window {
        Some(w) => {
            lines.push(row(
                "Phasenwinkel",
                format!(
                    "{:.1}° (nötig {:.1}°)",
                    w.phase.to_degrees(),
                    w.needed.to_degrees()
                ),
            ));
            // Auf ein Grad genau ist "jetzt".
            let off = (w.phase - w.needed).to_degrees().rem_euclid(360.0);
            let when = if off.min(360.0 - off) < 1.0 {
                "jetzt".into()
            } else {
                format!("in {}", format_span(w.wait, day))
            };
            lines.push(row("Startfenster", when));
            if let Some(origin) = w.origin {
                lines.push(row("", format!("von {} aus", world.bodies[origin].name)));
            }
        }
        None => lines.push(row("Startfenster", "–".into())),
    }
    match &info.approach {
        Some(a) => {
            let when = format!("in {}", format_span((a.time - world.time).max(0.0), day));
            if a.encounter {
                lines.push(row("Begegnung", when));
                lines.push(row(
                    "  Periapsis",
                    km(a.distance - world.bodies[info.body].radius),
                ));
            } else {
                lines.push(row("Annäherung", when));
                lines.push(row("  auf", distance_text(a.distance)));
            }
        }
        None => lines.push(row("Annäherung", "–".into())),
    }
    lines.join("\n")
}

/// Werte zum Ziel als Textzeilen.
fn target_result(
    world: &World,
    info: Option<&TargetInfo>,
    transfer: Option<&TransferInfo>,
) -> String {
    if let Some(transfer) = transfer {
        return transfer_result(world, transfer);
    }
    let Some(info) = info else {
        return "Kein Ziel. Mit ◄ ► ein Schiff, einen Planeten oder Mond wählen.".into();
    };
    let row = |label: &str, value: String| format!("{label:<14}{value}");
    let (own, target) = (
        world.vessel_state(info.vessel),
        world.vessel_state(info.target),
    );
    let mut lines = Vec::new();
    if own.0 == target.0 {
        lines.push(row(
            "Abstand",
            distance_text(own.1.position.distance(target.1.position)),
        ));
    }
    // Mit geplantem Andocken: dessen Ablauf statt der Annäherung.
    if let Some(docking) = world.vessels[info.vessel].trajectory.docking() {
        match &docking.plan {
            Ok(plan) => {
                let now = world.time;
                if plan.matching.burn.start > now {
                    lines.push(row(
                        "Angleichen",
                        format!(
                            "in {}",
                            format_span(plan.matching.burn.start - now, hud::day(world))
                        ),
                    ));
                    lines.push(row(
                        "  Δv",
                        format!("{:.0} m/s", plan.matching.node.delta_v.length()),
                    ));
                }
                lines.push(row(
                    "Anflug",
                    format!(
                        "{}, {}",
                        distance_text(plan.distance),
                        format_span(plan.contact - plan.matching.burn.end, hud::day(world))
                    ),
                ));
                if let sim::VesselStatus::Approaching { distance } =
                    world.vessel_status(info.vessel)
                {
                    lines.push(row("  noch", distance_text(distance)));
                }
                lines.push(row("  Δv", format!("{:.1} m/s", plan.approach_delta_v)));
                lines.push(row(
                    "Kontakt",
                    format!("in {}", format_span(plan.contact - now, hud::day(world))),
                ));
            }
            Err(e) => lines.push(format!("Andocken geht nicht: {e}")),
        }
        return lines.join("\n");
    }
    match &info.approach {
        Some(a) => {
            lines.push(row(
                "Annäherung",
                format!(
                    "in {}",
                    format_span((a.time - world.time).max(0.0), hud::day(world))
                ),
            ));
            if a.revolutions > 0 {
                lines.push(row(
                    "",
                    match a.revolutions {
                        1 => "nach 1 Umlauf".into(),
                        n => format!("nach {n} Umläufen"),
                    },
                ));
            }
            lines.push(row("  auf", distance_text(a.distance)));
            lines.push(row("  relativ", format!("{:.0} m/s", a.relative_speed)));
        }
        None => lines.push(row("Annäherung", "–".into())),
    }
    if let Some(plane) = &info.plane {
        lines.push(row(
            "Neigung",
            format!("{:.1}°", plane.inclination.to_degrees()),
        ));
        if plane.coplanar {
            lines.push(row("", "gleiche Ebene".into()));
        }
        for (name, time) in [("  AN in", plane.ascending), ("  DN in", plane.descending)] {
            if let Some(t) = time {
                lines.push(row(
                    name,
                    format_span((t - world.time).max(0.0), hud::day(world)),
                ));
            }
        }
    }
    if info.approach.is_none() && info.plane.is_none() {
        lines.push("Erst mit einer Bahn (oder einem geplanten Start).".into());
    }
    lines.join("\n")
}

pub fn show(
    ui: &mut egui::Ui,
    world: &mut World,
    vessel: usize,
    rendezvous: &Rendezvous,
    notes: &mut Notes,
) {
    card(ui, "Ziel", |ui| {
        let colored = |name: &str| egui::RichText::new(name).color(color32(TARGET_COLOR));
        let name = match target_of(world, vessel) {
            Target::Vessel(t) => colored(&world.vessels[t].name),
            Target::Body(b) => colored(&world.bodies[b].name),
            Target::None => egui::RichText::new("kein Ziel").weak(),
        };
        let step = stepper(ui, ["◄", "►"], name);
        if step != 0 {
            match cycle_target(world, vessel, step) {
                Target::Vessel(t) => world.set_target(vessel, Some(t)),
                Target::Body(b) => world.set_target_body(vessel, Some(b)),
                Target::None => {
                    world.set_target(vessel, None);
                    world.set_target_body(vessel, None);
                }
            }
        }
        ui.label(target_result(
            world,
            rendezvous.target.as_ref(),
            rendezvous.transfer.as_ref(),
        ));
        // Andocken nur mit einem Schiff als Ziel, und nicht mehr, wenn es
        // schon läuft.
        let trajectory = &world.vessels[vessel].trajectory;
        if world.vessels[vessel].target.is_some() && !trajectory.docking_underway() {
            let docking = trajectory.docking().is_some();
            let label = if docking {
                "Andocken abbrechen"
            } else {
                "Andocken bei Annäherung"
            };
            if wide_button(ui, label) {
                let result = world.set_docking(vessel, !docking);
                notes.check(result);
            }
        }
    });
}
