//! Karte "Start" der Seitenleiste: für ein gelandetes Schiff, das starten
//! kann. Start planen oder abbrechen, die Werte des Aufstiegs verstellen
//! (− / +, mit Umschalt fein) und sehen, was daraus folgt.

use crate::hud::{self, format_span, km};
use crate::ui::{Notes, card, stepper, wide_button};
use bevy_egui::egui;
use sim::{Launch, LaunchProfile, PlanError, Segment, World, kepler};

/// Vorgabe für einen neuen Start: so lange bis zum Abheben.
const DEFAULT_COUNTDOWN: f64 = 120.0;

/// Einstellbare Werte des Starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Param {
    StartIn,
    Heading,
    Turn,
    Kick,
    Apoapsis,
}

impl Param {
    const ALL: [Param; 5] = [
        Param::StartIn,
        Param::Heading,
        Param::Turn,
        Param::Kick,
        Param::Apoapsis,
    ];

    fn label(self) -> &'static str {
        match self {
            Param::StartIn => "Start in",
            Param::Heading => "Kurs",
            Param::Turn => "Wendebeginn",
            Param::Kick => "Wendewinkel",
            Param::Apoapsis => "Ziel-Apoapsis",
        }
    }

    /// Schrittweite: normal und fein (mit Umschalt).
    fn step(self, fine: bool) -> f64 {
        match (self, fine) {
            (Param::StartIn, false) => 60.0,
            (Param::StartIn, true) => 10.0,
            (Param::Heading, false) => 5.0,
            (Param::Heading, true) => 1.0,
            (Param::Turn, false) => 500.0,
            (Param::Turn, true) => 100.0,
            (Param::Kick, false) => 5.0,
            (Param::Kick, true) => 1.0,
            (Param::Apoapsis, false) => 10_000.0,
            (Param::Apoapsis, true) => 1_000.0,
        }
    }

    fn value(self, launch: &Launch, now: f64, day: f64) -> String {
        let p = &launch.profile;
        match self {
            Param::StartIn => format_span((launch.time - now).max(0.0), day),
            Param::Heading => format!("{:.0}°", p.heading),
            Param::Turn => km(p.turn_altitude),
            Param::Kick => format!("{:.0}°", p.kick),
            Param::Apoapsis => km(p.target_apoapsis),
        }
    }
}

/// Was nach dem Aufstieg folgt, als Textzeilen.
fn launch_result(world: &World, i: usize) -> String {
    let trajectory = &world.vessels[i].trajectory;
    let Some(plan) = trajectory.launch() else {
        let (body, point) = match trajectory.current() {
            Segment::Surface { body, point, .. } => (*body, *point),
            _ => return String::new(),
        };
        let place = match world.zone_at(body, &point) {
            Some(k) => format!("Gelandet in {}.", world.zones[k].name),
            None => "Außerhalb eines Zielgebiets: mit einem Hop (Start mit niedriger \
                     Ziel-Apoapsis) ins Ziel."
                .into(),
        };
        return format!(
            "{place} Kurs 90° ist Osten. In den Orbit: danach mit einem Node an der \
             Apoapsis zirkularisieren."
        );
    };
    let burn = &plan.burn;
    let params = world.params(i);
    let ascent = trajectory.segments().find(|s| s.is_burn());
    let altitude = ascent.map_or(0.0, |s| {
        s.state_at(&world.bodies, burn.end).position.length() - world.bodies[s.body()].radius
    });
    // Feste Spalten (die Schrift ist dicktengleich).
    let row = |label: &str, value: String| format!("{label:<14}{value}");
    let mut lines = vec![
        row(
            "Brennschluss",
            format!(
                "in {}",
                format_span((burn.end - world.time).max(0.0), hud::day(world))
            ),
        ),
        row("  in Höhe", km(altitude)),
        row(
            "Treibstoff",
            format!("{:.0} kg", burn.end_mass() - params.dry_mass),
        ),
        row(
            "  reicht für",
            format!(
                "{:.0} m/s",
                params.exhaust_velocity * (burn.end_mass() / params.dry_mass).ln()
            ),
        ),
    ];
    let orbit = trajectory
        .segments()
        .find(|s| s.start() >= burn.end && matches!(s, Segment::Kepler { .. }));
    if let Some(Segment::Kepler { state, body, .. }) = orbit {
        let body = &world.bodies[*body];
        let info = kepler::orbit_info(body.gm, state);
        let h = state.position.cross(state.velocity);
        lines.push(row(
            "Bahn danach",
            format!(
                "Ap {}",
                info.apoapsis.map_or("–".into(), |r| km(r - body.radius))
            ),
        ));
        lines.push(row("", format!("Pe {}", km(info.periapsis - body.radius))));
        lines.push(row(
            "Bahnneigung",
            format!("{:.1}°", (h.z / h.length()).acos().to_degrees()),
        ));
    }
    // Kommt die Fähre wieder herunter (Hop), wo und heil?
    if let Some(t) = world.touchdown(i) {
        let label = if t.landing.is_ok() {
            "Aufsetzen"
        } else {
            "Absturz!"
        };
        lines.push(row(
            label,
            match (t.zone, t.nearest) {
                (Some(k), _) => format!("in {}", world.zones[k].name),
                (None, Some((k, d))) => format!("{} von {}", km(d), world.zones[k].name),
                (None, None) => "außerhalb".into(),
            },
        ));
    }
    lines.join("\n")
}

/// Einen Wert des geplanten Starts ändern.
fn adjust(world: &mut World, i: usize, param: Param, delta: f64) -> Result<(), PlanError> {
    let mut launch = world.vessels[i]
        .trajectory
        .launch()
        .ok_or(PlanError::NoNode)?
        .launch;
    let p = &mut launch.profile;
    match param {
        // Die ganze Kette wandert mit.
        Param::StartIn => return world.shift_launch(i, delta),
        Param::Heading => p.heading = (p.heading + delta).rem_euclid(360.0),
        Param::Apoapsis => p.target_apoapsis = (p.target_apoapsis + delta).clamp(1_000.0, 5e6),
        Param::Turn => p.turn_altitude = (p.turn_altitude + delta).max(0.0),
        // Ganz senkrecht wendete es nie, ganz waagerecht stiege es nicht.
        Param::Kick => p.kick = (p.kick + delta).clamp(1.0, 89.0),
    }
    p.turn_altitude = p.turn_altitude.min(0.5 * p.target_apoapsis);
    world.set_launch(i, launch)
}

/// Plant einen Start mit der Voreinstellung für den Körper (mit oder ohne
/// Luft). Vorerst ist jede Kette sofort scharf.
fn plan(world: &mut World, i: usize) -> Result<(), PlanError> {
    let body = &world.bodies[world.vessel_state(i).0];
    let launch = Launch {
        time: world.time + DEFAULT_COUNTDOWN,
        profile: LaunchProfile::for_body(body),
    };
    world.set_launch(i, launch)?;
    world.set_armed(i, true)
}

/// Kann das Schiff starten: heil am Boden und nicht Fundament einer Basis?
fn can_launch(world: &World, i: usize) -> bool {
    world.vessels[i].foundation_of.is_none()
        && matches!(
            world.vessels[i].trajectory.current(),
            Segment::Surface { wreck: false, .. }
        )
}

pub fn show(ui: &mut egui::Ui, world: &mut World, i: usize, notes: &mut Notes) {
    if !can_launch(world, i) {
        return;
    }
    card(ui, &format!("Start: {}", world.vessels[i].name), |ui| {
        let planned = world.vessels[i].trajectory.launch().is_some();
        let label = if planned {
            "Start abbrechen"
        } else {
            "Start planen"
        };
        if wide_button(ui, label) {
            let result = if planned {
                world.clear_launch(i)
            } else {
                plan(world, i)
            };
            notes.check(result);
        }
        if let Some(launch) = world.vessels[i].trajectory.launch().map(|p| p.launch) {
            let fine = ui.input(|input| input.modifiers.shift);
            for param in Param::ALL {
                ui.horizontal(|ui| {
                    ui.label(format!("{:<14}", param.label()));
                    let value = param.value(&launch, world.time, hud::day(world));
                    let step = stepper(ui, ["−", "+"], value.into());
                    if step != 0 {
                        let result = adjust(world, i, param, step as f64 * param.step(fine));
                        notes.check(result);
                    }
                });
            }
        }
        ui.label(launch_result(world, i));
    });
}
