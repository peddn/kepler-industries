//! Textanzeige: Spielzeit, Zeitraffer, Zustand der Schiffe, Bedienhilfe.

use crate::camera::CameraRig;
use crate::focus;
use crate::node_ui::{self, SelectedBody, SelectedNode, SelectedZone, Selection};
use crate::rendezvous::distance_text;
use crate::{Sim, TimeWarp};
use bevy::prelude::*;
use bevy::text::FontSize;
use sim::body::{body_state, orbital_period, sphere_of_influence};
use sim::{Crash, Event, Plan, Segment, SurfacePoint, Touchdown, VesselStatus, World, kepler};
use std::ops::Range;

#[derive(Component)]
pub struct Hud;

/// Teil der Anzeige in Rot (ungültige Nodes) und der Rest danach.
#[derive(Component)]
pub struct HudWarning;

#[derive(Component)]
pub struct HudRest;

const WARNING_COLOR: Color = Color::srgb(1.0, 0.35, 0.3);

/// Schrift für alle Texte.
#[derive(Resource)]
pub struct UiFont(pub Handle<Font>);

pub fn load_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    // Bevys eingebaute Schrift hat nur ASCII (keine Umlaute). Fira Mono ist die
    // vollständige Fassung davon (SIL Open Font License, siehe assets/fonts).
    // Eingebettet statt geladen: funktioniert unabhängig vom Arbeitsverzeichnis.
    let font = fonts.add(Font::from_bytes(
        include_bytes!("../assets/fonts/FiraMono-Regular.ttf").to_vec(),
    ));
    commands.insert_resource(UiFont(font));
}

pub fn setup(mut commands: Commands, font: Res<UiFont>) {
    let text_font = TextFont {
        font: font.0.clone().into(),
        font_size: FontSize::Px(15.0),
        ..default()
    };
    commands
        .spawn((
            Hud,
            Text::new(""),
            text_font.clone(),
            Node {
                position_type: PositionType::Absolute,
                top: px(12),
                left: px(12),
                ..default()
            },
        ))
        .with_children(|hud| {
            hud.spawn((
                HudWarning,
                TextSpan::new(""),
                text_font.clone(),
                TextColor(WARNING_COLOR),
            ));
            hud.spawn((HudRest, TextSpan::new(""), text_font));
        });
}

pub(crate) fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (days, rest) = (total / 86_400, total % 86_400);
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
        0 => String::new(),
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
        let when = format_span(t - world.time);
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
            Event::LeavesSystem => format!("Verlässt das Heimatsystem in {when}!"),
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
            format_span(period)
        ));
        lines.push(format!(
            "Einflusssphäre {}",
            km(sphere_of_influence(&world.bodies, b))
        ));
    }
    lines
}

/// Zeitspanne, z. B. "00:12:34" oder "2 d 03:04:05".
pub(crate) fn format_span(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (days, rest) = (total / 86_400, total % 86_400);
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

pub(crate) fn km(meters: f64) -> String {
    format!("{:.1} km", meters / 1000.0)
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
    let can_plan = launch.is_some()
        || !(matches!(trajectory.current(), Segment::Surface { .. })
            || world.vessel_status(i) == VesselStatus::Escaped);
    if let Some(plan) = launch
        && plan.burn.start > world.time
    {
        lines.push(format!(
            "Start in {} (Einstellungen rechts)",
            format_span(plan.burn.start - world.time)
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
                        format!("in {}", format_span(plan.burn.start - world.time))
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
                        format_span(plan.matching.burn.start - world.time)
                    )
                } else {
                    "Anflug läuft".into()
                };
                format!(
                    "Andocken an {name}: {when}, Δv {:.0} + {:.0} m/s, Kontakt in {}",
                    plan.matching.node.delta_v.length(),
                    plan.approach_delta_v,
                    format_span(plan.contact - world.time)
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

/// Vorausgesagter Bodenkontakt: Landung im Zielgebiet, Aufsetzen außerhalb
/// oder Absturz, jeweils mit Ort und Geschwindigkeit am Boden. Beim Absturz
/// steht der Grund gleich in der ersten Zeile.
pub(crate) fn touchdown_lines(world: &World, i: usize) -> Vec<String> {
    let Some(t) = world.touchdown(i) else {
        return Vec::new();
    };
    let when = format_span(t.time - world.time);
    let place = touchdown_place(world, &t);
    match t.braking {
        Ok(dv) => vec![
            if t.zone.is_some() {
                format!("Landung in {when} {place}")
            } else {
                format!("Aufsetzen in {when} {place}")
            },
            format!(
                "  Aufsetzen mit {:.0} m/s, Bremsen {dv:.0} m/s{}",
                t.speed,
                if t.zone.is_some() {
                    ""
                } else {
                    ", danach Hop ins Ziel"
                }
            ),
        ],
        Err(crash) => {
            let (reason, detail) = match crash {
                Crash::NoFuel { available } => (
                    format!("Δv an Bord nur {available:.0} m/s"),
                    format!("zum Bremsen fehlen {:.0} m/s", t.speed - available),
                ),
                Crash::TooWeak { thrust_to_weight } => (
                    "Schub zu schwach zum Bremsen".into(),
                    format!("Schub/Gewicht {thrust_to_weight:.2}, nötig über 1"),
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
        format!("Brennbeginn in {}", format_span(burn.start - world.time))
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
            let around = if after.body() == 0 {
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

#[allow(clippy::too_many_arguments)]
pub fn update(
    sim: Res<Sim>,
    warp: Res<TimeWarp>,
    selection: Res<Selection>,
    selected: Res<SelectedNode>,
    zone: Res<SelectedZone>,
    body: Res<SelectedBody>,
    rig: Res<CameraRig>,
    mut text: Single<&mut Text, With<Hud>>,
    mut warning: Single<&mut TextSpan, (With<HudWarning>, Without<HudRest>)>,
    mut rest: Single<&mut TextSpan, (With<HudRest>, Without<HudWarning>)>,
) {
    let world = &sim.0;
    let mut lines = vec![format!(
        "{}    Zeitraffer {}×{}    Kamera: {}{}",
        format_time(world.time),
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
                let around = if world.vessel_state(i).0 == 0 {
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
                // Momentane Bahn: ändert sich während des Brennens laufend.
                let (body_id, state) = world.vessel_state(i);
                let body = &world.bodies[body_id];
                let info = kepler::orbit_info(body.gm, &state);
                format!(
                    "Brennt, noch {remaining:.1} s  Ap {}  Pe {}",
                    info.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
                    km(info.periapsis - body.radius)
                )
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
            VesselStatus::Escaped => "im Sonnenorbit (nicht simuliert)".into(),
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
        let chosen = node_ui::selected_node(world, &selection, &selected).map(|(_, k)| k);
        lines.push(String::new());
        let (details, warning) = selected_details(world, i, chosen);
        red = lines.len() + warning.start..lines.len() + warning.end;
        lines.extend(details);
    }
    if let Some(k) = zone.0 {
        let z = &world.zones[k];
        lines.push(String::new());
        lines.push(format!(
            "Zielgebiet {}: {}, Radius {}",
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
    lines.push(String::new());
    // Kurze Zeilen, damit rechts Platz für die Seitenleiste bleibt.
    lines.extend(
        [
            "Linksklick: Schiff / Zielgebiet / Mond / Node wählen, Node setzen",
            "Griffe ziehen: Δv (Umschalt: fein)   Node ziehen: verschieben",
            "↑/↓: Node wählen   Entf: Node löschen   Tab: nächstes Schiff",
            "H: Zeitraffer-Stopp   , / .: Zeitraffer   Leertaste: Pause",
            "Rechte Maustaste ziehen: drehen   Mausrad: zoomen",
            "F: Kamera auf Auswahl / Planet   N: Nahansicht",
        ]
        .map(String::from),
    );
    // Drei Teile: davor, rot, danach. Ohne rote Zeilen steht alles im ersten.
    let (before, red, after) = if red.is_empty() {
        (lines.join("\n"), String::new(), String::new())
    } else {
        (
            lines[..red.start].join("\n") + "\n",
            lines[red.clone()].join("\n"),
            "\n".to_owned() + &lines[red.end..].join("\n"),
        )
    };
    for (target, value) in [
        (&mut text.0, before),
        (&mut warning.0, red),
        (&mut rest.0, after),
    ] {
        if *target != value {
            *target = value;
        }
    }
}
