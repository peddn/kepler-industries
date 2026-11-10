//! Kamerafokus: `F` richtet die Kamera auf die Auswahl (gewählter Node, sonst
//! Schiff, sonst Zielgebiet oder Mond, ohne Auswahl den Planeten); steht sie
//! schon darauf, geht sie zurück zum Planeten. `N` schaltet die Nahansicht.
//!
//! Der Fokus ist unabhängig von der Auswahl: Die Kamera bleibt auf ihrem
//! Objekt und folgt ihm, bis man wieder `F` drückt. So kann man z. B. ein
//! Zielgebiet im Blick behalten und dabei das Schiff wählen und dessen
//! Brems-Node einstellen.

use crate::camera::{CameraRig, Focus};
use crate::node_ui::{Drag, SelectedBody, SelectedNode, SelectedZone, Selection, UiMessage};
use crate::{Sim, view};
use bevy::math::DVec3;
use bevy::prelude::*;
use sim::World;
use sim::body::body_state;

/// Was `F` in den Fokus nähme: ausdrücklich gewählter Node, sonst das Schiff,
/// sonst das Zielgebiet oder der Mond, sonst der Planet.
pub fn focus_of_selection(
    world: &World,
    selection: &Selection,
    selected: &SelectedNode,
    zone: &SelectedZone,
    body: &SelectedBody,
) -> Focus {
    if let Some(i) = selection.0 {
        let nodes = world.vessels[i].trajectory.nodes();
        return match selected.0 {
            Some(time) if nodes.iter().any(|n| n.node.time == time) => {
                Focus::Node { vessel: i, time }
            }
            _ => Focus::Vessel(i),
        };
    }
    match (zone.0, body.0) {
        (Some(k), _) => Focus::Zone(k),
        (None, Some(b)) => Focus::Body(b),
        (None, None) => Focus::Body(world.home),
    }
}

/// Lage des Fokus im Simulationssystem und der Mittelpunkt des Körpers, bei
/// dem er ist (für "oben" in der Nahansicht); `None`, wenn es das Objekt
/// nicht (mehr) gibt. Schiffe und Nodes wie gezeichnet (`view::anchors`).
pub fn position(world: &World, focus: Focus) -> Option<(DVec3, DVec3)> {
    let center = |b| body_state(&world.bodies, b, world.time).position;
    let at_vessel = |i: usize| {
        (
            view::vessel_position(world, i),
            center(world.vessel_state(i).0),
        )
    };
    Some(match focus {
        Focus::Body(b) => {
            world.bodies.get(b)?;
            (center(b), center(b))
        }
        Focus::Vessel(i) => {
            world.vessels.get(i)?;
            at_vessel(i)
        }
        Focus::Node { vessel, time } => {
            let node = world
                .vessels
                .get(vessel)?
                .trajectory
                .nodes()
                .iter()
                .find(|n| n.node.time == time)?;
            // Ein ungültiger Node hat keinen Ort: solange beim Schiff.
            match node.plan {
                Ok(plan) => {
                    let trajectory = &world.vessels[vessel].trajectory;
                    let anchor = view::anchor_at(world, trajectory, plan.node.time);
                    (anchor + plan.node_state.position, anchor)
                }
                Err(_) => at_vessel(vessel),
            }
        }
        Focus::Zone(k) => {
            let zone = world.zones.get(k)?;
            let body = &world.bodies[zone.body];
            let surface = body.body_fixed_to_inertial(
                zone.center.body_fixed_direction() * body.radius,
                world.time,
            );
            (center(zone.body) + surface, center(zone.body))
        }
    })
}

/// Name des Fokus für die Anzeige.
pub fn name(world: &World, focus: Focus) -> String {
    match focus {
        Focus::Body(b) => world.bodies[b].name.clone(),
        Focus::Vessel(i) => world.vessels[i].name.clone(),
        Focus::Node { vessel, time } => {
            let nodes = world.vessels[vessel].trajectory.nodes();
            let k = nodes.iter().position(|n| n.node.time == time).unwrap_or(0);
            format!("Node {} von {}", k + 1, world.vessels[vessel].name)
        }
        Focus::Zone(k) => world.zones[k].name.clone(),
    }
}

/// Tasten `F` und `N`.
#[allow(clippy::too_many_arguments)]
pub fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    selection: Res<Selection>,
    selected: Res<SelectedNode>,
    zone: Res<SelectedZone>,
    body: Res<SelectedBody>,
    real_time: Res<Time<Real>>,
    mut rig: ResMut<CameraRig>,
    mut message: ResMut<UiMessage>,
) {
    if keys.just_pressed(KeyCode::KeyF) {
        let wanted = focus_of_selection(&sim.0, &selection, &selected, &zone, &body);
        let target = if wanted == rig.target {
            rig.home()
        } else {
            wanted
        };
        rig.set_target(target);
    }
    if keys.just_pressed(KeyCode::KeyN) {
        if matches!(rig.target, Focus::Body(_)) {
            message.say(
                "Nahansicht: erst mit F ein Schiff, einen Node oder ein Zielgebiet in den Fokus nehmen"
                    .into(),
                sim.0.time,
                real_time.elapsed_secs_f64(),
            );
        } else {
            let near = !rig.near;
            rig.set_near(near);
        }
    }
}

/// Fokuspunkt jedes Bild aus dem Objekt berechnen. Wird der Node im Fokus
/// gerade verschoben, bleibt die Kamera bis zum Loslassen stehen (sonst
/// rutschte die Bahn unter der Maus weg). Gibt es das Objekt nicht mehr, geht
/// der Fokus auf sein Schiff (Node), sonst auf den Planeten.
pub fn follow(sim: Res<Sim>, drag: Res<Drag>, mut rig: ResMut<CameraRig>) {
    let world = &sim.0;
    if matches!(rig.target, Focus::Node { .. }) && matches!(*drag, Drag::Node { moved: true, .. }) {
        return;
    }
    if let Some((p, center)) = position(world, rig.target) {
        rig.focus = p;
        rig.center = center;
        return;
    }
    match rig.target {
        // Abstand behalten: Der Node lag beim Schiff.
        Focus::Node { vessel, .. } if vessel < world.vessels.len() => {
            rig.target = Focus::Vessel(vessel);
        }
        _ => {
            let home = rig.home();
            rig.set_target(home);
        }
    }
    (rig.focus, rig.center) = position(world, rig.target).unwrap_or((DVec3::ZERO, DVec3::ZERO));
}
