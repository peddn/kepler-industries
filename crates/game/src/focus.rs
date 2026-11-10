//! Kamerafokus: `F` richtet die Kamera auf die Auswahl (gewählter Node, sonst
//! Schiff, sonst Zielgebiet oder Mond, ohne Auswahl den Planeten); steht sie
//! schon darauf, geht sie zurück zum Planeten. `N` schaltet die Nahansicht.
//!
//! Der Fokus ist unabhängig von der Auswahl: Die Kamera bleibt auf ihrem
//! Objekt und folgt ihm, bis man wieder `F` drückt. So kann man z. B. ein
//! Zielgebiet im Blick behalten und dabei das Schiff wählen und dessen
//! Brems-Node einstellen.

use crate::Sim;
use crate::camera::{CameraRig, Focus, PLANET};
use crate::node_ui::{Drag, SelectedBody, SelectedNode, SelectedZone, Selection, UiMessage};
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
        (None, None) => PLANET,
    }
}

/// Lage des Fokus im Simulationssystem; `None`, wenn es das Objekt nicht
/// (mehr) gibt.
pub fn position(world: &World, focus: Focus) -> Option<DVec3> {
    Some(match focus {
        Focus::Body(b) => {
            world.bodies.get(b)?;
            body_state(&world.bodies, b, world.time).position
        }
        Focus::Vessel(i) => {
            world.vessels.get(i)?;
            world.vessel_state(i).1.position
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
                Ok(plan) => plan.node_state.position,
                Err(_) => world.vessel_state(vessel).1.position,
            }
        }
        Focus::Zone(k) => {
            let zone = world.zones.get(k)?;
            let body = &world.bodies[zone.body];
            body.body_fixed_to_inertial(
                zone.center.body_fixed_direction() * body.radius,
                world.time,
            )
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
        let target = if wanted == rig.target { PLANET } else { wanted };
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
    if let Some(p) = position(world, rig.target) {
        rig.focus = p;
        return;
    }
    match rig.target {
        // Abstand behalten: Der Node lag beim Schiff.
        Focus::Node { vessel, .. } if vessel < world.vessels.len() => {
            rig.target = Focus::Vessel(vessel);
        }
        _ => rig.set_target(PLANET),
    }
    rig.focus = position(world, rig.target).unwrap_or(DVec3::ZERO);
}
