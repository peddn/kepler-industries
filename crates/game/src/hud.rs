//! Textanzeige: Spielzeit, Zeitraffer, Zustand der Schiffe, Bedienhilfe.

use crate::node_ui::{Selection, UiMessage};
use crate::{Sim, TimeWarp};
use bevy::prelude::*;
use bevy::text::FontSize;
use sim::{Segment, SurfacePoint, VesselStatus, World, kepler};

#[derive(Component)]
pub struct Hud;

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
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont {
            font: font.0.clone().into(),
            font_size: FontSize::Px(15.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
    ));
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
fn format_span(seconds: f64) -> String {
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

fn km(meters: f64) -> String {
    format!("{:.1} km", meters / 1000.0)
}

fn place(point: &SurfacePoint) -> String {
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

/// Kenndaten und Node des gewählten Schiffs.
fn selected_details(world: &World, i: usize) -> Vec<String> {
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
    let flying = !matches!(trajectory.current(), Segment::Surface { .. });

    match trajectory.plan() {
        None if flying => lines.push("Linksklick auf die Bahn setzt einen Node.".into()),
        None => {}
        Some(plan) => {
            let dv = plan.node.delta_v;
            let burn = &plan.burn;
            let when = if burn.start > world.time {
                format!("Brennbeginn in {}", format_span(burn.start - world.time))
            } else {
                "Brennt".into()
            };
            lines.push(format!(
                "Node: Δv {:.1} m/s (prograd {:.1}, normal {:.1}, radial {:.1})",
                dv.length(),
                dv.x,
                dv.y,
                dv.z
            ));
            lines.push(format!(
                "{when}, Dauer {:.1} s, Treibstoff danach {:.1} kg",
                burn.duration(),
                burn.end_mass() - vessel.params.dry_mass
            ));
            // Im Spiel ist vorerst jeder Node scharf; nicht scharf nur über `--node`.
            if !plan.armed {
                lines.push("Nicht scharf: rückt bei Brennbeginn einen Umlauf weiter".into());
            }
            if burn.delta_v() < dv.length() - 0.05 {
                lines.push(format!(
                    "Treibstoff reicht nur für {:.0} m/s!",
                    burn.delta_v()
                ));
            }
            match trajectory.segment_after_burn() {
                Some(Segment::Kepler { state, end, .. }) => {
                    let body = &world.bodies[plan.body];
                    let info = kepler::orbit_info(body.gm, state);
                    lines.push(format!(
                        "Danach: Ap {}  Pe {}{}",
                        info.apoapsis.map_or("–".into(), |r| km(r - body.radius)),
                        km(info.periapsis - body.radius),
                        if end.is_some() {
                            "  – führt in die Atmosphäre!"
                        } else {
                            ""
                        }
                    ));
                }
                Some(Segment::Integrated { .. }) => {
                    lines.push("Danach: weiter durch die Atmosphäre".into());
                }
                _ => {}
            }
        }
    }
    if let Some((time, _, point)) = trajectory.predicted_impact()
        && time > world.time
    {
        lines.push(format!(
            "Aufschlag in {} bei {}",
            format_span(time - world.time),
            place(&point)
        ));
    }
    lines
}

pub fn update(
    sim: Res<Sim>,
    warp: Res<TimeWarp>,
    selection: Res<Selection>,
    message: Res<UiMessage>,
    real_time: Res<Time<Real>>,
    mut text: Single<&mut Text, With<Hud>>,
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
            VesselStatus::Landed { point } => format!("Gelandet bei {}", place(&point)),
            VesselStatus::Wreck { point } => format!("Wrack bei {}", place(&point)),
        };
        let marker = if selection.0 == Some(i) { ">" } else { " " };
        lines.push(format!("{marker} {:<9} {status}", vessel.name));
    }
    if let Some(i) = selection.0 {
        lines.push(String::new());
        lines.extend(selected_details(world, i));
    }
    lines.push(String::new());
    lines.push(
        "Linksklick: Schiff wählen / Node setzen   Griffe ziehen: Δv (Umschalt: fein)".into(),
    );
    lines.push(
        "Node ziehen: verschieben   Entf: Node löschen   Tab: nächstes Schiff   H: Zeitraffer-Stopp"
            .into(),
    );
    lines.push(
        "Rechte Maustaste ziehen: drehen   Mausrad: zoomen   , / .: Zeitraffer   Leertaste: Pause"
            .into(),
    );
    text.0 = lines.join("\n");
}
