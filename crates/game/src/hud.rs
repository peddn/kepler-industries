//! Textanzeige: Spielzeit, Zeitraffer, Zustand der Schiffe, Bedienhilfe.

use crate::node_ui::{self, SelectedNode, Selection, UiMessage};
use crate::{Sim, TimeWarp};
use bevy::prelude::*;
use bevy::text::FontSize;
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

fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (days, rest) = (total / 86_400, total % 86_400);
    format!(
        "Tag {days}, {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
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
    // Nodes gehen im Flug und am Boden mit geplantem Start (danach).
    let launch = trajectory.launch();
    let can_plan = launch.is_some() || !matches!(trajectory.current(), Segment::Surface { .. });
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
            burn.end_mass() - vessel.params.dry_mass
        ),
    ];
    if burn.delta_v() < dv.length() - 0.05 {
        lines.push(format!(
            "Treibstoff reicht nur für {:.0} m/s!",
            burn.delta_v()
        ));
    }
    match vessel.trajectory.segment_after(burn) {
        Some(Segment::Kepler { state, end, .. }) => {
            let body = &world.bodies[plan.body];
            let info = kepler::orbit_info(body.gm, state);
            lines.push(format!(
                "Danach: Ap {}  Pe {}{}",
                info.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
                km(info.periapsis - body.radius),
                if end.is_some_and(|(_, e)| e != Event::BurnStart) {
                    "  – führt in die Atmosphäre!"
                } else {
                    ""
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
    message: Res<UiMessage>,
    real_time: Res<Time<Real>>,
    mut text: Single<&mut Text, With<Hud>>,
    mut warning: Single<&mut TextSpan, (With<HudWarning>, Without<HudRest>)>,
    mut rest: Single<&mut TextSpan, (With<HudRest>, Without<HudWarning>)>,
) {
    let world = &sim.0;
    let mut lines = vec![format!(
        "{}    Zeitraffer {}×{}",
        format_time(world.time),
        warp.factor(),
        if warp.paused { "  (Pause)" } else { "" }
    )];
    if real_time.elapsed_secs_f64() < message.until {
        lines.push(format!("» {}", message.text));
    }
    lines.push(String::new());
    for (i, vessel) in world.vessels.iter().enumerate() {
        let status = match world.vessel_status(i) {
            VesselStatus::Orbit {
                periapsis_altitude,
                apoapsis_altitude,
                descending,
            } => format!(
                "Orbit  Ap {}  Pe {}{}",
                apoapsis_altitude.map_or("–".into(), km),
                km(periapsis_altitude),
                if descending {
                    "  – Bahn führt in die Atmosphäre!"
                } else {
                    ""
                }
            ),
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
            VesselStatus::Landed { point } => match world.zone_at(0, &point) {
                Some(k) => format!("Gelandet in {}", world.zones[k].name),
                None => format!("Gelandet bei {} (außerhalb)", place(&point)),
            },
            VesselStatus::Wreck { point } => match world.zone_at(0, &point) {
                Some(k) => format!("Wrack in {}", world.zones[k].name),
                None => format!("Wrack bei {}", place(&point)),
            },
        };
        let marker = if selection.0 == Some(i) { ">" } else { " " };
        lines.push(format!("{marker} {:<9} {status}", vessel.name));
    }
    let mut red = 0..0;
    if let Some(i) = selection.0 {
        let chosen = node_ui::selected_node(world, &selection, &selected).map(|(_, k)| k);
        lines.push(String::new());
        let (details, warning) = selected_details(world, i, chosen);
        red = lines.len() + warning.start..lines.len() + warning.end;
        lines.extend(details);
    }
    lines.push(String::new());
    // Kurze Zeilen, damit rechts Platz für die Seitenleiste bleibt.
    lines.extend(
        [
            "Linksklick: Schiff wählen / Node setzen / Node wählen",
            "Griffe ziehen: Δv (Umschalt: fein)   Node ziehen: verschieben",
            "↑/↓: Node wählen   Entf: Node löschen   Tab: nächstes Schiff",
            "H: Zeitraffer-Stopp   , / .: Zeitraffer   Leertaste: Pause",
            "Rechte Maustaste ziehen: drehen   Mausrad: zoomen",
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
