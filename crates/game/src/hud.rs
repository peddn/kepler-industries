//! Textanzeige: Spielzeit, Zeitraffer, Zustand der Schiffe, Bedienhilfe.

use crate::{Sim, TimeWarp};
use bevy::prelude::*;
use bevy::text::FontSize;
use sim::{SurfacePoint, VesselStatus};

#[derive(Component)]
pub struct Hud;

pub fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    // Bevys eingebaute Schrift hat nur ASCII (keine Umlaute). Fira Mono ist die
    // vollständige Fassung davon (SIL Open Font License, siehe assets/fonts).
    // Eingebettet statt geladen: funktioniert unabhängig vom Arbeitsverzeichnis.
    let font = fonts.add(Font::from_bytes(
        include_bytes!("../assets/fonts/FiraMono-Regular.ttf").to_vec(),
    ));
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont {
            font: font.into(),
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

pub fn update(sim: Res<Sim>, warp: Res<TimeWarp>, mut text: Single<&mut Text, With<Hud>>) {
    let world = &sim.0;
    let mut lines = vec![format!(
        "{}    Zeitraffer {}×{}",
        format_time(world.time),
        warp.factor(),
        if warp.paused { "  (Pause)" } else { "" }
    )];
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
            VesselStatus::Landed { point } => format!("Gelandet bei {}", place(&point)),
            VesselStatus::Wreck { point } => format!("Wrack bei {}", place(&point)),
        };
        lines.push(format!("{:<9} {status}", vessel.name));
    }
    lines.push(String::new());
    lines.push(
        "Maus ziehen: drehen   Mausrad: zoomen   , / .: Zeitraffer   Leertaste: Pause".into(),
    );
    text.0 = lines.join("\n");
}
