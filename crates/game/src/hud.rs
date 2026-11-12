//! Texte der Oberfläche: Zeiten, Strecken und Orte als Text, Zustand und
//! Vorschau der Schiffe, Daten von Zielgebieten und Himmelskörpern, Warnungen
//! und die Bedienhilfe. Gezeichnet wird in den Leisten (`ui`, `left_bar`,
//! `top_bar`); nur die Warnungen stehen als Text über der Karte.

use crate::rendezvous::distance_text;
use bevy_egui::egui;
use sim::body::{body_state, orbital_period, sphere_of_influence};
use sim::{
    Crash, Event, LandingBurn, Plan, Segment, SurfacePoint, Touchdown, VesselStatus, World, kepler,
};

const WARNING_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 89, 77);

/// Länge eines Tags in s: eine Umdrehung des Heimatplaneten. Überall gilt
/// diese eine Zeit (wie die Kerbin-Zeit in KSP), auch beim Mond.
pub(crate) fn day(world: &World) -> f64 {
    sim::body::day_length(&world.bodies, world.home)
}

/// Ganze Sekunden eines Tags (für die Anzeige, mindestens 1).
fn day_seconds(day: f64) -> u64 {
    (day.round() as u64).max(1)
}

pub(crate) fn format_time(seconds: f64, day: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let day = day_seconds(day);
    let (days, rest) = (total / day, total % day);
    format!(
        "Tag {days}, {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// " auf dem Mond", wenn das Schiff nicht beim Planeten ist.
fn on_body(world: &World, i: usize) -> String {
    match world.vessel_state(i).0 {
        b if b == world.home => String::new(),
        b => format!(" auf {}", world.bodies[b].name),
    }
}

/// Nächster Wechsel der Einflusssphäre auf der vorausberechneten Bahn.
fn next_sphere_change(world: &World, trajectory: &sim::Trajectory) -> Option<String> {
    let segments: Vec<&Segment> = trajectory.segments().collect();
    segments.iter().enumerate().find_map(|(k, segment)| {
        let t = segment.end();
        if t <= world.time {
            return None;
        }
        let when = format_span(t - world.time, day(world));
        let name = |b: usize| &world.bodies[b].name;
        Some(match segment.end_event()? {
            Event::SoiEntry(moon) => {
                let body = &world.bodies[moon];
                let state = segments.get(k + 1)?.state_at(&world.bodies, t);
                let periapsis = kepler::orbit_info(body.gm, &state).periapsis - body.radius;
                let low = if periapsis < 0.0 {
                    format!("{} (Aufschlag)", km(periapsis))
                } else {
                    km(periapsis)
                };
                format!("Begegnung: {} in {when}, Periapsis {low}", name(moon))
            }
            Event::SoiExit => format!("Verlässt {} in {when}", name(segment.body())),
            _ => return None,
        })
    })
}

/// Angaben zu einem gewählten Mond.
pub fn body_details(world: &World, b: usize) -> Vec<String> {
    let body = &world.bodies[b];
    let gravity = body.gm / (body.radius * body.radius);
    let mut lines = vec![format!(
        "{}: Radius {}, Schwerkraft {gravity:.2} m/s²{}",
        body.name,
        km(body.radius),
        if body.atmosphere.exists() {
            ""
        } else {
            ", keine Luft"
        }
    )];
    if let (Some(orbit), Some(period)) = (&body.orbit, orbital_period(&world.bodies, b)) {
        let parent = &world.bodies[orbit.parent];
        let distance = body_state(&world.bodies, b, world.time)
            .position
            .distance(body_state(&world.bodies, orbit.parent, world.time).position);
        lines.push(format!(
            "Bahn um {} in {}, Umlauf {}",
            parent.name,
            km(distance),
            format_span(period, day(world))
        ));
        if orbit.eccentricity > 0.0 || orbit.inclination != 0.0 {
            lines.push(format!(
                "Abstand {} bis {}, Neigung {:.1}°",
                km(orbit.periapsis()),
                km(orbit.apoapsis()),
                orbit.inclination.to_degrees()
            ));
        }
        lines.push(format!(
            "Einflusssphäre {}",
            km(sphere_of_influence(&world.bodies, b))
        ));
        // Gebunden: Er dreht sich so schnell, wie er umläuft.
        let locked = (body.rotation_period / period - 1.0).abs() < 1e-9;
        lines.push(format!(
            "Drehung {}{}",
            format_span(body.rotation_period, day(world)),
            if locked { " (gebunden)" } else { "" }
        ));
    }
    lines
}

/// Zeitspanne, z. B. "00:12:34" oder "2 d 03:04:05" (Tage siehe `day`).
pub(crate) fn format_span(seconds: f64, day: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let day = day_seconds(day);
    let (days, rest) = (total / day, total % day);
    let clock = format!(
        "{:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    );
    if days > 0 {
        format!("{days} d {clock}")
    } else {
        clock
    }
}

/// Strecke in km, ab einer Million km in Mio. km.
pub(crate) fn km(meters: f64) -> String {
    if meters.abs() >= 1.0e9 {
        format!("{:.2} Mio. km", meters / 1.0e9)
    } else {
        format!("{:.1} km", meters / 1000.0)
    }
}

pub(crate) fn place(point: &SurfacePoint) -> String {
    let lat = point.latitude.to_degrees();
    let lon = point.longitude.to_degrees();
    format!(
        "{:.1}° {}, {:.1}° {}",
        lat.abs(),
        if lat >= 0.0 { "N" } else { "S" },
        lon.abs(),
        if lon >= 0.0 { "O" } else { "W" }
    )
}

/// Lage und Vorschau des gewählten Schiffs: Zeitraffer-Stopp, nächster
/// Wechsel der Einflusssphäre, vorausgesagte Landung. (Treibstoff zeigt der
/// Δv-Balken, Start, Ziel und Manöver ihre Karten, Gefahren `warnings`.)
pub fn info_lines(world: &World, i: usize) -> Vec<String> {
    let vessel = &world.vessels[i];
    let mut lines = vec![format!(
        "Zeitraffer-Stopp bei Brennbeginn: {} (H)",
        if vessel.halt_warp_at_burn {
            "an"
        } else {
            "aus"
        }
    )];
    lines.extend(next_sphere_change(world, &vessel.trajectory));
    if world.touchdown(i).is_some_and(|t| t.landing.is_ok()) {
        lines.extend(touchdown_lines(world, i));
    }
    lines
}

/// Was dem gewählten Schiff droht oder nicht geht: Absturz, ungültige Nodes,
/// Nodes, für die der Treibstoff nicht reicht, Andocken unmöglich.
pub fn warnings(world: &World, i: usize) -> Vec<String> {
    let trajectory = &world.vessels[i].trajectory;
    let mut lines = Vec::new();
    if world.touchdown(i).is_some_and(|t| t.landing.is_err()) {
        lines.extend(touchdown_lines(world, i));
    }
    for (k, node) in trajectory.nodes().iter().enumerate() {
        match node.plan {
            Ok(plan) if plan.burn.delta_v() < plan.node.delta_v.length() - 0.05 => {
                lines.push(format!(
                    "Node {}: Treibstoff reicht nur für {:.0} m/s!",
                    k + 1,
                    plan.burn.delta_v()
                ));
            }
            Ok(_) => {}
            Err(e) => lines.push(format!("Node {} ungültig: {e}", k + 1)),
        }
    }
    if let Some(docking) = trajectory.docking()
        && let Err(e) = &docking.plan
    {
        let name = &world.vessels[world.host(docking.target)].name;
        lines.push(format!("Andocken an {name}: {e}"));
    }
    lines
}

/// Die Manöverkette des Schiffs, eine Zeile je Node; `chosen` ist der
/// gewählte. Leer ohne Nodes.
pub fn chain_lines(world: &World, i: usize, chosen: Option<usize>) -> Vec<String> {
    let nodes = world.vessels[i].trajectory.nodes();
    nodes
        .iter()
        .enumerate()
        .map(|(k, node)| {
            let marker = if chosen == Some(k) { ">" } else { " " };
            match node.plan {
                Ok(plan) => {
                    let when = if plan.burn.start > world.time {
                        format!(
                            "in {}",
                            format_span(plan.burn.start - world.time, day(world))
                        )
                    } else {
                        "brennt".into()
                    };
                    let after = world
                        .revolutions_before_node(i, k)
                        .filter(|&n| n > 0)
                        .map_or(String::new(), |n| format!("  +{n} U."));
                    format!(
                        "{marker} {}  {when:<12} {:6.1} m/s{after}",
                        k + 1,
                        plan.node.delta_v.length()
                    )
                }
                Err(_) => format!("{marker} {}  ungültig", k + 1),
            }
        })
        .collect()
}

/// Wo ein Bodenkontakt liegt: im Zielgebiet, sonst Abstand zum nächsten
/// (ohne Zielgebiet auf dem Körper die Koordinaten).
fn touchdown_place(world: &World, t: &Touchdown) -> String {
    match (t.zone, t.nearest) {
        (Some(k), _) => format!("im Zielgebiet {}", world.zones[k].name),
        (None, Some((k, d))) => format!("außerhalb, {} von {}", km(d), world.zones[k].name),
        (None, None) => format!("bei {}", place(&t.point)),
    }
}

/// Höhe in m, ab 1 km in km.
fn height(meters: f64) -> String {
    if meters < 1_000.0 {
        format!("{meters:.0} m")
    } else {
        km(meters)
    }
}

/// Vorausgesagter Bodenkontakt: Landung im Zielgebiet, Aufsetzen außerhalb
/// oder Absturz, jeweils mit Ort, dazu das Landebrennen davor. Beim Absturz
/// steht der Grund gleich in der ersten Zeile.
pub(crate) fn touchdown_lines(world: &World, i: usize) -> Vec<String> {
    let Some(t) = world.touchdown(i) else {
        return Vec::new();
    };
    let when = format_span(t.time - world.time, day(world));
    let place = touchdown_place(world, &t);
    let hop = if t.zone.is_some() {
        ""
    } else {
        ", danach Hop ins Ziel"
    };
    match t.landing {
        Ok(landing) => vec![
            if t.zone.is_some() {
                format!("Landung in {when} {place}")
            } else {
                format!("Aufsetzen in {when} {place}")
            },
            match landing {
                Some(LandingBurn { burn, altitude }) if burn.start > world.time => format!(
                    "  Landebrennen in {} ab {} Höhe: {:.0} m/s, {:.prec$} s{hop}",
                    format_span(burn.start - world.time, day(world)),
                    height(altitude),
                    burn.delta_v(),
                    burn.duration(),
                    prec = usize::from(burn.duration() < 10.0),
                ),
                Some(_) => format!("  Landebrennen läuft{hop}"),
                None => format!("  Aufsetzen mit {:.1} m/s ohne Bremsen{hop}", t.speed),
            },
        ],
        Err(crash) => {
            let (reason, detail) = match crash {
                Crash::NoFuel { available, missing } => (
                    format!("Δv an Bord nur {available:.0} m/s"),
                    format!("zum Bremsen fehlen mindestens {missing:.0} m/s"),
                ),
                Crash::TooWeak { thrust_to_weight } => (
                    "Schub zu schwach zum Bremsen".into(),
                    format!("Schub/Gewicht {thrust_to_weight:.2}, nötig über 1"),
                ),
                Crash::TooLate => (
                    "keine Zeit zum Bremsen".into(),
                    "das Landebrennen müsste früher beginnen".into(),
                ),
            };
            vec![
                format!("Absturz in {when} {place}: {reason}"),
                format!("  Aufprall {:.0} m/s, {detail}", t.speed),
            ]
        }
    }
}

/// Einzelheiten zum gewählten Node.
pub fn node_details(world: &World, i: usize, k: usize, plan: &Plan) -> Vec<String> {
    let vessel = &world.vessels[i];
    let dv = plan.node.delta_v;
    let burn = &plan.burn;
    let when = if burn.start > world.time {
        format!(
            "Brennbeginn in {}",
            format_span(burn.start - world.time, day(world))
        )
    } else {
        "Brennt".into()
    };
    let mut lines = vec![
        format!("Node {}: Δv {:.1} m/s", k + 1, dv.length()),
        format!("  prograd {:.1}", dv.x),
        format!("  normal  {:.1}", dv.y),
        format!("  radial  {:.1}", dv.z),
        when,
        format!("Dauer {:.1} s", burn.duration()),
        format!(
            "Treibstoff danach {:.0} kg",
            burn.end_mass() - world.params(i).dry_mass
        ),
    ];
    match vessel.trajectory.segment_after(burn) {
        Some(after @ Segment::Kepler { state, end, .. }) => {
            // Der Körper der Bahn danach (das Brennen kann die Sphäre wechseln).
            let body = &world.bodies[after.body()];
            let info = kepler::orbit_info(body.gm, state);
            let around = if after.body() == world.home {
                String::new()
            } else {
                format!(" um {}", body.name)
            };
            lines.push(format!(
                "Danach{around}:\n  Ap {}  Pe {}{}",
                info.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
                km(info.periapsis - body.radius),
                match end {
                    Some((_, e)) if e.is_descent() && body.atmosphere.exists() => {
                        "\n  führt in die Atmosphäre!"
                    }
                    Some((_, e)) if e.is_descent() => "\n  führt auf den Boden!",
                    _ => "",
                }
            ));
        }
        Some(Segment::Integrated { burn: None, .. }) => {
            lines.push("Danach: weiter durch die Atmosphäre".into());
        }
        _ => {}
    }
    lines
}

/// Die wichtigsten Flugdaten eines Schiffs als Paare aus Bezeichnung und
/// Wert: Bahn (Apoapsis, Periapsis als Höhen), Höhe, Geschwindigkeit auf der
/// Bahn und relativ zum Boden (senkrecht: steigend positiv; waagerecht).
/// Die Höhe zählt ab der Kugel des Körpers: Solange es kein Gelände gibt,
/// ist das zugleich die Höhe über dem Boden.
pub fn flight_data(world: &World, i: usize) -> Vec<(&'static str, String)> {
    let (body, state) = world.vessel_state(i);
    let body = &world.bodies[body];
    let landed = matches!(
        world.vessels[world.host(i)].trajectory.current(),
        Segment::Surface { .. }
    );
    let speed = |v: f64| format!("{v:.1} m/s");
    let up = state.position.normalize();
    let ground = state.velocity - body.co_rotating_velocity(state.position);
    let vertical = ground.dot(up);
    let horizontal = (ground - up * vertical).length();
    let (apoapsis, periapsis) = if landed {
        ("–".into(), "–".into())
    } else {
        let orbit = kepler::orbit_info(body.gm, &state);
        (
            orbit.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
            km(orbit.periapsis - body.radius),
        )
    };
    vec![
        ("Apoapsis", apoapsis),
        ("Periapsis", periapsis),
        ("Höhe", height(state.position.length() - body.radius)),
        ("Bahngeschw.", speed(state.velocity.length())),
        ("Zum Boden", String::new()),
        ("  senkrecht", speed(vertical)),
        ("  waagerecht", speed(horizontal)),
    ]
}

/// Der Körper, bei dem ein Schiff gerade ist.
pub fn ship_body(world: &World, i: usize) -> &str {
    &world.bodies[world.vessel_state(i).0].name
}

/// Name eines Schiffs; ein Verbund trägt die Namen seiner Teile.
pub fn ship_name(world: &World, i: usize) -> String {
    std::iter::once(world.vessels[i].name.as_str())
        .chain(
            world
                .parts(i)
                .into_iter()
                .map(|k| world.vessels[k].name.as_str()),
        )
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Zustand eines Schiffs in einer Zeile (die Zahlen dazu stehen in
/// `flight_data`).
pub fn ship_status(world: &World, i: usize) -> String {
    match world.vessel_status(i) {
        VesselStatus::Orbit { descending, .. } => {
            let body = &world.bodies[world.vessel_state(i).0];
            let around = if world.vessel_state(i).0 == world.home {
                String::new()
            } else {
                format!(" um {}", body.name)
            };
            let warning = match (descending, body.atmosphere.exists()) {
                (false, _) => "",
                (true, true) => ": Bahn führt in die Atmosphäre!",
                (true, false) => ": Bahn führt auf den Boden!",
            };
            format!("Orbit{around}{warning}")
        }
        VesselStatus::Atmosphere { .. } => "In der Atmosphäre".into(),
        VesselStatus::Burning { remaining } => {
            let (body_id, state) = world.vessel_state(i);
            let body = &world.bodies[body_id];
            if world.vessels[i]
                .trajectory
                .current()
                .landing_burn()
                .is_some()
            {
                format!(
                    "Landebrennen, noch {remaining:.1} s  Höhe {}  {:.0} m/s zum Boden",
                    height(state.position.length() - body.radius),
                    sim::landing::ground_speed(body, &state)
                )
            } else {
                // Momentane Bahn: ändert sich während des Brennens laufend.
                let info = kepler::orbit_info(body.gm, &state);
                format!(
                    "Brennt, noch {remaining:.1} s  Ap {}  Pe {}",
                    info.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
                    km(info.periapsis - body.radius)
                )
            }
        }
        VesselStatus::Landed { point } => match world.zone_at(world.vessel_state(i).0, &point) {
            Some(k) => format!("Gelandet in {}", world.zones[k].name),
            None => format!(
                "Gelandet bei {}{} (außerhalb)",
                place(&point),
                on_body(world, i)
            ),
        },
        VesselStatus::Wreck { point } => match world.zone_at(world.vessel_state(i).0, &point) {
            Some(k) => format!("Wrack in {}", world.zones[k].name),
            None => format!("Wrack bei {}{}", place(&point), on_body(world, i)),
        },
        VesselStatus::Approaching { distance } => {
            format!("Anflug zum Andocken, noch {}", distance_text(distance))
        }
        VesselStatus::Docked { host } => {
            format!("angedockt an {}", world.vessels[host].name)
        }
    }
}

/// Daten eines Zielgebiets und wer dort gelandet ist.
pub fn zone_lines(world: &World, k: usize) -> Vec<String> {
    let z = &world.zones[k];
    let on = match z.body {
        0 => String::new(),
        b => format!(" auf {}", world.bodies[b].name),
    };
    let mut lines = vec![
        format!("Zielgebiet {}{on}", z.name),
        format!("{}, Radius {}", place(&z.center), km(z.radius)),
    ];
    let landed: Vec<&str> = (0..world.vessels.len())
        .filter(|&i| match world.vessel_status(i) {
            VesselStatus::Landed { point } => world.zone_at(z.body, &point) == Some(k),
            _ => false,
        })
        .map(|i| world.vessels[i].name.as_str())
        .collect();
    if !landed.is_empty() {
        lines.push(format!("Gelandet: {}", landed.join(", ")));
    }
    lines
}

/// Bedienhilfe (Fenster "Hilfe").
pub const HELP: [&str; 9] = [
    "Linksklick: Schiff, Zielgebiet, Mond oder Node wählen; auf die Bahn: Node setzen",
    "Griffe ziehen: Δv ändern (Umschalt: fein)",
    "Node ziehen: verschieben (Strg: ohne Griffe)",
    "↑/↓: Node wählen   Entf: Node löschen   Tab: nächstes Schiff   Esc: nichts wählen",
    "H: Zeitraffer-Stopp bei Brennbeginn   , / .: Zeitraffer   Leertaste: Pause",
    "Rechte Maustaste ziehen: drehen   Mausrad: zoomen",
    "F: Kamera auf Auswahl (Node > Schiff > Planet)   N: Nahansicht",
    "F5: speichern   F9: laden",
    "F1: diese Hilfe",
];

/// Zeichnet die Warnungen zum gewählten Schiff rot in die freie Fläche
/// `free` der Karte (auf sie ist `painter` begrenzt), umbrochen auf ihre
/// Breite. Sie nehmen keine Klicks an und bleiben sichtbar, auch wenn die
/// Leisten zu sind.
pub fn show_warnings(
    painter: &egui::Painter,
    free: egui::Rect,
    font: &egui::FontId,
    lines: Vec<String>,
) {
    const MARGIN: f32 = 12.0;
    let mut at = free.min + egui::vec2(MARGIN, MARGIN);
    for line in lines {
        let width = free.width() - 2.0 * MARGIN;
        let text = painter.layout(line, font.clone(), WARNING_COLOR, width);
        let height = text.size().y;
        painter.galley(at, text, WARNING_COLOR);
        at.y += height + 3.0;
    }
}
