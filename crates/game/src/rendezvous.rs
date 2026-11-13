//! Rendezvous-Anzeige: Ziel des gewählten Schiffs, nächste Annäherung und
//! Knoten zur Bahnebene des Ziels. Ist das Ziel ein Himmelskörper: sein
//! Startfenster und die engste Annäherung der geplanten Bahn an ihn.
//!
//! Gerechnet wird in der Simulation (`World::approach`,
//! `World::plane_to_target`, `World::transfer_window`,
//! `World::body_approach`), einmal pro Bild in `update`. Die Karte "Ziel"
//! in der Seitenleiste, die Marker und die Beschriftungen lesen nur das
//! Ergebnis.

use crate::Sim;
use crate::camera::{CameraRig, MainCamera, to_view};
use crate::hud::km;
use crate::node_ui::{Projector, Selection};
use crate::ui::WorldLabels;
use crate::view;
use crate::zahl::De;
use bevy::math::DVec3;
use bevy::prelude::*;
use sim::{Approach, BodyApproach, BodyId, PlaneCrossing, Window, World};

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

/// Ergebnis für das gewählte Schiff mit einem Himmelskörper als Ziel.
pub struct TransferInfo {
    pub vessel: usize,
    pub body: BodyId,
    pub window: Option<Window>,
    pub approach: Option<BodyApproach>,
}

/// Höchstens eines von beiden ist gesetzt (ein Schiff hat nur ein Ziel).
#[derive(Resource, Default)]
pub struct Rendezvous {
    pub target: Option<TargetInfo>,
    pub transfer: Option<TransferInfo>,
}

/// Abstand lesbar: unter 1 km in m, sonst in km.
pub fn distance_text(meters: f64) -> String {
    if meters < 1_000.0 {
        format!("{meters:.0} m", meters = De(meters))
    } else {
        km(meters)
    }
}

/// Einmal pro Bild: Annäherung und Bahnebene zum Ziel des gewählten Schiffs.
pub fn update(sim: Res<Sim>, selection: Res<Selection>, mut info: ResMut<Rendezvous>) {
    let world = &sim.0;
    info.target = selection.0.and_then(|vessel| {
        Some(TargetInfo {
            vessel,
            target: world.vessels[vessel].target?,
            approach: world.approach(vessel),
            plane: world.plane_to_target(vessel),
        })
    });
    info.transfer = selection.0.and_then(|vessel| {
        Some(TransferInfo {
            vessel,
            body: world.vessels[vessel].target_body?,
            window: world.transfer_window(vessel),
            approach: world.body_approach(vessel),
        })
    });
}

/// Wo die Annäherung an den Zielkörper gezeichnet wird: das Schiff dort, wo
/// seine Bahn gezeichnet ist (`view::anchor_at`), der Körper im echten
/// Abstand und in der echten Richtung daneben.
fn transfer_points(world: &World, info: &TransferInfo) -> Option<(DVec3, DVec3)> {
    let a = info.approach.as_ref()?;
    let trajectory = &world.vessels[world.host(info.vessel)].trajectory;
    let (_, state) = trajectory.predicted_state(&world.bodies, a.time)?;
    let own = view::anchor_at(world, trajectory, a.time) + state.position;
    Some((own, own + a.body_position - a.position))
}

/// Lage des gewählten Schiffs zum Zeitpunkt `t` (vorausberechnet).
fn own_position(sim: &Sim, info: &TargetInfo, t: f64) -> Option<bevy::math::DVec3> {
    let world = &sim.0;
    let trajectory = &world.vessels[info.vessel].trajectory;
    trajectory
        .predicted_state(&world.bodies, t)
        .map(|(_, s)| view::anchor_at(world, trajectory, t) + s.position)
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
    let eye = camera.translation;
    if let Some(transfer) = &info.transfer
        && let Some((own, body)) = transfer_points(&sim.0, transfer)
    {
        let (own, body) = (to_view(own, rig.focus), to_view(body, rig.focus));
        for (p, color) in [(own, OWN_COLOR), (body, TARGET_COLOR)] {
            let rotation = Quat::from_rotation_arc(Vec3::Z, (eye - p).normalize());
            gizmos
                .circle(
                    Isometry3d::new(p, rotation),
                    p.distance(eye) * APPROACH_RADIUS,
                    color,
                )
                .resolution(24);
        }
        gizmos.line(own, body, TARGET_COLOR);
    }
    let Some(info) = &info.target else { return };
    let mut ring = |p: Vec3, size: f32, resolution: u32, color: Color| {
        let rotation = Quat::from_rotation_arc(Vec3::Z, (eye - p).normalize());
        gizmos
            .circle(Isometry3d::new(p, rotation), p.distance(eye) * size, color)
            .resolution(resolution);
    };
    // Beide relativ zum Bahnstück des eigenen Schiffs (gleicher Körper).
    let trajectory = &sim.0.vessels[info.vessel].trajectory;
    let approach = info.approach.map(|a| {
        let anchor = view::anchor_at(&sim.0, trajectory, a.time);
        (
            to_view(anchor + a.position, rig.focus),
            to_view(anchor + a.target_position, rig.focus),
        )
    });
    if let Some((own, target)) = approach {
        ring(own, APPROACH_RADIUS, 24, OWN_COLOR);
        ring(target, APPROACH_RADIUS, 24, TARGET_COLOR);
    }
    // Automatischer Node "Angleichen" beim Andocken: doppelter rosa Ring.
    if let Some(plan) = trajectory.dock_plan()
        && plan.matching.burn.end > sim.0.time
    {
        let anchor = view::anchor_at(&sim.0, trajectory, plan.matching.node.time);
        let p = to_view(anchor + plan.matching.node_state.position, rig.focus);
        ring(p, APPROACH_RADIUS * 1.4, 24, TARGET_COLOR);
        ring(p, APPROACH_RADIUS * 0.8, 24, TARGET_COLOR);
    }
    if let Some(plane) = &info.plane {
        for t in [plane.ascending, plane.descending].into_iter().flatten() {
            if let Some(p) = own_position(&sim, info, t) {
                ring(to_view(p, rig.focus), NODE_RADIUS, 4, NODE_COLOR);
            }
        }
    }
    if let Some((own, target)) = approach {
        gizmos.line(own, target, TARGET_COLOR);
    }
}

/// Welche Beschriftung.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Label {
    Approach,
    Ascending,
    Descending,
}

/// Beschriftungen an den Markern: Abstand bei der Annäherung, "AN"/"DN" mit
/// der Neigung an den Knoten. Hinter dem Planeten keine.
pub fn labels(
    sim: Res<Sim>,
    info: Res<Rendezvous>,
    rig: Res<CameraRig>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
    mut labels: ResMut<WorldLabels>,
) {
    let (camera, transform) = *camera;
    let projector = Projector::new(camera, transform, rig.focus, &sim.0);
    for (label, color) in [
        (Label::Approach, TARGET_COLOR),
        (Label::Ascending, NODE_COLOR),
        (Label::Descending, NODE_COLOR),
    ] {
        // Zielkörper: Abstand (bei einer Begegnung die Periapsis) am Schiff.
        let transfer = info
            .transfer
            .as_ref()
            .filter(|_| label == Label::Approach)
            .and_then(|transfer| {
                let a = transfer.approach.as_ref()?;
                let (own, _) = transfer_points(&sim.0, transfer)?;
                let value = if a.encounter {
                    format!("Pe {}", km(a.distance - sim.0.bodies[transfer.body].radius))
                } else {
                    distance_text(a.distance)
                };
                let screen = projector.project(to_view(own, rig.focus))?;
                Some((screen, value))
            });
        let shown = transfer.or_else(|| {
            info.target.as_ref().and_then(|info| {
                let (position, value) = match label {
                    Label::Approach => {
                        let a = info.approach.as_ref()?;
                        let trajectory = &sim.0.vessels[info.vessel].trajectory;
                        let anchor = view::anchor_at(&sim.0, trajectory, a.time);
                        (anchor + a.position, distance_text(a.distance))
                    }
                    Label::Ascending | Label::Descending => {
                        let plane = info.plane.as_ref()?;
                        let (time, name) = if label == Label::Ascending {
                            (plane.ascending?, "AN")
                        } else {
                            (plane.descending?, "DN")
                        };
                        let position = own_position(&sim, info, time)?;
                        (
                            position,
                            format!("{name} {:.1}°", De(plane.inclination.to_degrees())),
                        )
                    }
                };
                let screen = projector.project(to_view(position, rig.focus))?;
                Some((screen, value))
            })
        });
        if let Some((screen, value)) = shown {
            labels.add(screen + LABEL_OFFSET, value, color);
        }
    }
}
