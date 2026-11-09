//! Rendezvous-Anzeige: Ziel des gewählten Schiffs, nächste Annäherung und
//! Knoten zur Bahnebene des Ziels.
//!
//! Gerechnet wird in der Simulation (`World::approach`,
//! `World::plane_to_target`), einmal pro Bild in `update`. Die Karte "Ziel"
//! in der Seitenleiste, die Marker und die Beschriftungen lesen nur das
//! Ergebnis.

use crate::Sim;
use crate::camera::{CameraRig, MainCamera, to_view};
use crate::hud::{UiFont, km};
use crate::node_ui::{Projector, Selection};
use bevy::prelude::*;
use bevy::text::FontSize;
use sim::{Approach, PlaneCrossing};

/// Farbe des Ziels: Bahn, Markierung, Annäherung.
pub const TARGET_COLOR: Color = Color::srgb(1.0, 0.45, 0.8);
/// Eigene Lage bei der Annäherung (wie das gewählte Schiff).
const OWN_COLOR: Color = Color::srgb(1.0, 0.85, 0.2);
/// Knoten zur Bahnebene des Ziels.
const NODE_COLOR: Color = Color::srgb(0.95, 0.95, 0.95);
/// Größen relativ zum Kameraabstand (gleich groß auf dem Bildschirm).
const APPROACH_RADIUS: f32 = 0.009;
const NODE_RADIUS: f32 = 0.009;
const LABEL_OFFSET: Vec2 = Vec2::new(14.0, -8.0);

/// Ergebnis für das gewählte Schiff mit Ziel.
pub struct TargetInfo {
    pub vessel: usize,
    pub target: usize,
    pub approach: Option<Approach>,
    pub plane: Option<PlaneCrossing>,
}

#[derive(Resource, Default)]
pub struct Rendezvous(pub Option<TargetInfo>);

/// Abstand lesbar: unter 1 km in m, sonst in km.
pub fn distance_text(meters: f64) -> String {
    if meters < 1_000.0 {
        format!("{meters:.0} m")
    } else {
        km(meters)
    }
}

/// Einmal pro Bild: Annäherung und Bahnebene zum Ziel des gewählten Schiffs.
pub fn update(sim: Res<Sim>, selection: Res<Selection>, mut info: ResMut<Rendezvous>) {
    let world = &sim.0;
    info.0 = selection.0.and_then(|vessel| {
        Some(TargetInfo {
            vessel,
            target: world.vessels[vessel].target?,
            approach: world.approach(vessel),
            plane: world.plane_to_target(vessel),
        })
    });
}

/// Lage des gewählten Schiffs zum Zeitpunkt `t` (vorausberechnet).
fn own_position(sim: &Sim, info: &TargetInfo, t: f64) -> Option<bevy::math::DVec3> {
    let world = &sim.0;
    world.vessels[info.vessel]
        .trajectory
        .predicted_state(&world.bodies, t)
        .map(|(_, s)| s.position)
}

/// Marker: bei der Annäherung beide Schiffe als Ringe mit einer Linie
/// dazwischen; an den Knoten zur Zielebene kleine Rauten.
pub fn draw(
    sim: Res<Sim>,
    info: Res<Rendezvous>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let Some(info) = &info.0 else { return };
    let eye = camera.translation;
    let mut ring = |p: Vec3, size: f32, resolution: u32, color: Color| {
        let rotation = Quat::from_rotation_arc(Vec3::Z, (eye - p).normalize());
        gizmos
            .circle(Isometry3d::new(p, rotation), p.distance(eye) * size, color)
            .resolution(resolution);
    };
    if let Some(a) = &info.approach {
        let own = to_view(a.position, rig.focus);
        let target = to_view(a.target_position, rig.focus);
        ring(own, APPROACH_RADIUS, 24, OWN_COLOR);
        ring(target, APPROACH_RADIUS, 24, TARGET_COLOR);
    }
    if let Some(plane) = &info.plane {
        for t in [plane.ascending, plane.descending].into_iter().flatten() {
            if let Some(p) = own_position(&sim, info, t) {
                ring(to_view(p, rig.focus), NODE_RADIUS, 4, NODE_COLOR);
            }
        }
    }
    if let Some(a) = &info.approach {
        let own = to_view(a.position, rig.focus);
        let target = to_view(a.target_position, rig.focus);
        gizmos.line(own, target, TARGET_COLOR);
    }
}

/// Welche Beschriftung.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Approach,
    Ascending,
    Descending,
}

pub fn setup_labels(mut commands: Commands, font: Res<UiFont>) {
    for (label, color) in [
        (Label::Approach, TARGET_COLOR),
        (Label::Ascending, NODE_COLOR),
        (Label::Descending, NODE_COLOR),
    ] {
        commands.spawn((
            label,
            Text::new(""),
            TextFont {
                font: font.0.clone().into(),
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(color),
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
        ));
    }
}

/// Beschriftungen an die Marker schieben: Abstand bei der Annäherung,
/// "AN"/"DN" mit der Neigung an den Knoten. Hinter dem Planeten keine.
pub fn sync_labels(
    sim: Res<Sim>,
    info: Res<Rendezvous>,
    rig: Res<CameraRig>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
    mut labels: Query<(&Label, &mut Node, &mut Text)>,
) {
    let (camera, transform) = *camera;
    let projector = Projector::new(camera, transform, sim.0.bodies[0].radius);
    for (label, mut node, mut text) in &mut labels {
        let shown = info.0.as_ref().and_then(|info| {
            let (position, value) = match label {
                Label::Approach => {
                    let a = info.approach.as_ref()?;
                    (a.position, distance_text(a.distance))
                }
                Label::Ascending | Label::Descending => {
                    let plane = info.plane.as_ref()?;
                    let (time, name) = if *label == Label::Ascending {
                        (plane.ascending?, "AN")
                    } else {
                        (plane.descending?, "DN")
                    };
                    let position = own_position(&sim, info, time)?;
                    (
                        position,
                        format!("{name} {:.1}°", plane.inclination.to_degrees()),
                    )
                }
            };
            let screen = projector.project(to_view(position, rig.focus))?;
            Some((screen, value))
        });
        let Some((screen, value)) = shown else {
            node.display = Display::None;
            continue;
        };
        node.display = Display::Flex;
        node.left = px(screen.x + LABEL_OFFSET.x);
        node.top = px(screen.y + LABEL_OFFSET.y);
        if text.0 != value {
            text.0 = value;
        }
    }
}
