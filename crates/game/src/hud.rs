//! Textanzeige: Spielzeit, Zeitraffer, Zustand der Schiffe, Bedienhilfe.

use crate::TimeWarp;
use crate::camera::CameraRig;
use crate::focus;
use crate::node_ui::{self, SelectedBody, SelectedNode, SelectedZone, Selection};
use crate::rendezvous::distance_text;
use bevy_egui::egui;
use sim::body::{body_state, orbital_period, sphere_of_influence};
use sim::{
    Crash, Event, LandingBurn, Plan, Segment, SurfacePoint, Touchdown, VesselStatus, World, kepler,
};
use std::ops::Range;

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
fn body_details(world: &World, b: usize) -> Vec<String> {
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

/// Kenndaten und Manöverkette des gewählten Schiffs; `chosen` ist der
/// gewählte Node. Dazu die Zeilen, die rot erscheinen (ungültige Nodes; sie
/// stehen immer am Ende der Kette, also zusammen).
fn selected_details(world: &World, i: usize, chosen: Option<usize>) -> (Vec<String>, Range<usize>) {
    let vessel = &world.vessels[i];
    let trajectory = &vessel.trajectory;
    let mut lines = vec![
        format!(
            "{}: Treibstoff {:.1} kg, Δv {:.0} m/s",
            vessel.name,
            world.vessel_fuel(i),
            world.vessel_delta_v(i)
        ),
        format!(
            "Zeitraffer-Stopp bei Brennbeginn: {} (H)",
            if vessel.halt_warp_at_burn {
                "an"
            } else {
                "aus"
            }
        ),
    ];
    lines.extend(next_sphere_change(world, trajectory));
    // Nodes gehen im Flug und am Boden mit geplantem Start (danach).
    let launch = trajectory.launch();
    let can_plan = launch.is_some() || !matches!(trajectory.current(), Segment::Surface { .. });
    if let Some(plan) = launch
        && plan.burn.start > world.time
    {
        lines.push(format!(
            "Start in {} (Einstellungen rechts)",
            format_span(plan.burn.start - world.time, day(world))
        ));
    }
    let nodes = trajectory.nodes();
    let mut warning = 0..0;
    if nodes.is_empty() {
        if can_plan {
            lines.push("Linksklick auf die Bahn setzt einen Node.".into());
        }
    } else {
        // Im Spiel ist vorerst jede Kette scharf; nicht scharf nur über `--node`.
        lines.push(if trajectory.armed() {
            "Manöverkette:".into()
        } else {
            "Manöverkette (nicht scharf: rückt bei Brennbeginn einen Umlauf weiter):".into()
        });
        for (k, node) in nodes.iter().enumerate() {
            let marker = if chosen == Some(k) { ">" } else { " " };
            lines.push(match node.plan {
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
                        .map_or(String::new(), |n| match n {
                            1 => "  nach 1 Umlauf".into(),
                            n => format!("  nach {n} Umläufen"),
                        });
                    format!(
                        "{marker} {}  {when:<12} Δv {:6.1} m/s{after}",
                        k + 1,
                        plan.node.delta_v.length()
                    )
                }
                Err(e) => {
                    if warning.is_empty() {
                        warning = lines.len()..lines.len();
                    }
                    warning.end = lines.len() + 1;
                    format!("{marker} {}  ungültig: {e}", k + 1)
                }
            });
        }
        if let Some(k) = chosen
            && let Ok(plan) = nodes[k].plan
        {
            lines.extend(node_details(world, i, k, &plan));
        }
    }
    if let Some(docking) = trajectory.docking() {
        let name = &world.vessels[world.host(docking.target)].name;
        lines.push(match &docking.plan {
            Ok(plan) => {
                let when = if plan.matching.burn.start > world.time {
                    format!(
                        "Angleichen in {}",
                        format_span(plan.matching.burn.start - world.time, day(world))
                    )
                } else {
                    "Anflug läuft".into()
                };
                format!(
                    "Andocken an {name}: {when}, Δv {:.0} + {:.0} m/s, Kontakt in {}",
                    plan.matching.node.delta_v.length(),
                    plan.approach_delta_v,
                    format_span(plan.contact - world.time, day(world))
                )
            }
            Err(e) => {
                // Rot, wenn es an die roten Node-Zeilen anschließt (oder allein).
                if warning.is_empty() {
                    warning = lines.len()..lines.len() + 1;
                } else if warning.end == lines.len() {
                    warning.end += 1;
                }
                format!("Andocken an {name}: {e}")
            }
        });
    }
    lines.extend(touchdown_lines(world, i));
    (lines, warning)
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
fn node_details(world: &World, i: usize, k: usize, plan: &Plan) -> Vec<String> {
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
        String::new(),
        format!(
            "Node {}: Δv {:.1} m/s (prograd {:.1}, normal {:.1}, radial {:.1})",
            k + 1,
            dv.length(),
            dv.x,
            dv.y,
            dv.z
        ),
        format!(
            "{when}, Dauer {:.1} s, Treibstoff danach {:.1} kg",
            burn.duration(),
            burn.end_mass() - world.params(i).dry_mass
        ),
    ];
    if burn.delta_v() < dv.length() - 0.05 {
        lines.push(format!(
            "Treibstoff reicht nur für {:.0} m/s!",
            burn.delta_v()
        ));
    }
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
                "Danach{around}: Ap {}  Pe {}{}",
                info.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
                km(info.periapsis - body.radius),
                match end {
                    Some((_, e)) if e.is_descent() && body.atmosphere.exists() => {
                        "  – führt in die Atmosphäre!"
                    }
                    Some((_, e)) if e.is_descent() => "  – führt auf den Boden!",
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

/// Die Zeilen der Anzeige links und welche davon rot sind (Warnung).
fn lines(
    world: &World,
    warp: &TimeWarp,
    selection: &Selection,
    selected: &SelectedNode,
    zone: &SelectedZone,
    body: &SelectedBody,
    rig: &CameraRig,
) -> (Vec<String>, Range<usize>) {
    let mut lines = vec![format!(
        "{}    Zeitraffer {}×{}    Kamera: {}{}",
        format_time(world.time, day(world)),
        warp.factor(),
        if warp.paused { "  (Pause)" } else { "" },
        focus::name(world, rig.target),
        if rig.near { " (Nahansicht)" } else { "" }
    )];
    lines.push(String::new());
    for (i, vessel) in world.vessels.iter().enumerate() {
        let status = match world.vessel_status(i) {
            VesselStatus::Orbit {
                periapsis_altitude,
                apoapsis_altitude,
                descending,
            } => {
                let body = &world.bodies[world.vessel_state(i).0];
                let around = if world.vessel_state(i).0 == world.home {
                    String::new()
                } else {
                    format!(" um {}", body.name)
                };
                let warning = match (descending, body.atmosphere.exists()) {
                    (false, _) => "",
                    (true, true) => "  – Bahn führt in die Atmosphäre!",
                    (true, false) => "  – Bahn führt auf den Boden!",
                };
                format!(
                    "Orbit{around}  Ap {}  Pe {}{warning}",
                    apoapsis_altitude.map_or("–".into(), km),
                    km(periapsis_altitude),
                )
            }
            VesselStatus::Atmosphere { altitude } => {
                format!("In der Atmosphäre, Höhe {}", km(altitude))
            }
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
            VesselStatus::Landed { point } => {
                match world.zone_at(world.vessel_state(i).0, &point) {
                    Some(k) => format!("Gelandet in {}", world.zones[k].name),
                    None => format!(
                        "Gelandet bei {}{} (außerhalb)",
                        place(&point),
                        on_body(world, i)
                    ),
                }
            }
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
        };
        let marker = if selection.0 == Some(i) { ">" } else { " " };
        // Ein Verbund trägt die Namen seiner Teile.
        let name = std::iter::once(vessel.name.as_str())
            .chain(
                world
                    .parts(i)
                    .into_iter()
                    .map(|k| world.vessels[k].name.as_str()),
            )
            .collect::<Vec<_>>()
            .join(" + ");
        lines.push(format!("{marker} {name:<9} {status}"));
    }
    let mut red = 0..0;
    if let Some(i) = selection.0 {
        let chosen = node_ui::selected_node(world, selection, selected).map(|(_, k)| k);
        lines.push(String::new());
        let (details, warning) = selected_details(world, i, chosen);
        red = lines.len() + warning.start..lines.len() + warning.end;
        lines.extend(details);
    }
    if let Some(k) = zone.0 {
        let z = &world.zones[k];
        lines.push(String::new());
        let on = match z.body {
            0 => String::new(),
            b => format!(" auf {}", world.bodies[b].name),
        };
        lines.push(format!(
            "Zielgebiet {}{on}: {}, Radius {}",
            z.name,
            place(&z.center),
            km(z.radius)
        ));
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
    }
    if let Some(b) = body.0 {
        lines.push(String::new());
        lines.extend(body_details(world, b));
    }
    if world.vessels.is_empty() {
        lines.push(String::new());
        lines.push("Noch keine Schiffe: in der Karte \"Startbasis\" rechts bestellen.".into());
        lines.push("(Karte weg? Die Raute ihres Zielgebiets anklicken; F: Kamera dorthin.)".into());
    }
    lines.push(String::new());
    // Kurze Zeilen, damit rechts Platz für die Seitenleiste bleibt.
    lines.extend(
        [
            "Linksklick: Schiff / Zielgebiet / Mond / Node wählen, Node setzen",
            "Griffe ziehen: Δv (Umschalt: fein)   Node ziehen: verschieben (Strg: ohne Griffe)",
            "↑/↓: Node wählen   Entf: Node löschen   Tab: nächstes Schiff",
            "H: Zeitraffer-Stopp   , / .: Zeitraffer   Leertaste: Pause",
            "Rechte Maustaste ziehen: drehen   Mausrad: zoomen",
            "F: Kamera auf Auswahl (Node > Schiff > Planet)   N: Nahansicht",
        ]
        .map(String::from),
    );
    (lines, red)
}

/// Zeichnet die Anzeige links oben in die freie Fläche (`painter` ist auf
/// sie begrenzt, `origin` ihre Ecke): reine Information ohne eigene Fläche,
/// Klicks gehen hindurch an die Karte.
#[allow(clippy::too_many_arguments)]
pub fn show(
    painter: &egui::Painter,
    origin: egui::Pos2,
    font: &egui::FontId,
    world: &World,
    warp: &TimeWarp,
    selection: &Selection,
    selected: &SelectedNode,
    zone: &SelectedZone,
    body: &SelectedBody,
    rig: &CameraRig,
) {
    let (lines, red) = lines(world, warp, selection, selected, zone, body, rig);
    let mut at = origin + egui::vec2(12.0, 12.0);
    for (k, line) in lines.into_iter().enumerate() {
        let color = if red.contains(&k) {
            WARNING_COLOR
        } else {
            egui::Color32::WHITE
        };
        painter.text(at, egui::Align2::LEFT_TOP, line, font.clone(), color);
        at.y += font.size + 3.0;
    }
}
