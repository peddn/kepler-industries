//! Der Booster der Startbasis.
//!
//! Jeder Start von der Startbasis läuft mit ihm: Schiff und Booster heben als
//! Stapel ab, nur der Booster brennt. Bei Brennschluss trennen sie sich. Das
//! Schiff fliegt mit seinen eigenen Werten weiter, der Booster schiebt sich
//! waagerecht zurück (Boostback) und landet am Fallschirm an der Startbasis.
//!
//! Der Rückflug wird beim Planen des Starts berechnet (`plan_return`); kommt
//! der Booster nicht zurück, gibt es keinen Start. Bei der Trennung bekommt
//! er genau diesen Node, und seine Zeitleiste ist dieselbe wie beim Planen:
//! Vorschau = Ausführung.
//!
//! Für den Rückflug hält der Booster immer Treibstoff zurück (`reserve`). Der
//! Aufstieg endet an der Ziel-Apoapsis oder wenn nur noch diese Reserve im
//! Tank ist.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::kepler::StateVector;
use crate::maneuver::{self, ManeuverNode, PlanError};
use crate::trajectory::{Segment, Trajectory};
use crate::world::VesselParams;
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// So viel Δv hält der Booster für den Boostback zurück (m/s). Fliegt der
/// Stapel bei der Trennung schneller waagerecht, als sich damit umkehren
/// lässt, gibt es keinen Rückweg.
pub const RETURN_DELTA_V: f64 = 1_000.0;

/// So lange nach der Trennung liegt der Node des Boostbacks (s): Zeit, sich
/// zu drehen.
pub const BOOSTBACK_DELAY: f64 = 10.0;

/// So nah an der Startbasis muss der Booster aufsetzen (m).
pub const RETURN_RADIUS: f64 = 2_000.0;

/// So genau wird der Aufsetzpunkt entlang des Kurses gesucht (m).
const PRECISION: f64 = 10.0;

/// So genau wird er quer zum Kurs gesucht (m).
const ASIDE_PRECISION: f64 = 200.0;

/// Weiter als so wird der Boostback nicht zur Seite gedreht (Radiant).
const MAX_TURN: f64 = 0.6;

/// Treibstoff, den der Booster für den Rückflug zurückhält (kg).
pub fn reserve(booster: &VesselParams) -> f64 {
    booster.dry_mass * ((RETURN_DELTA_V / booster.exhaust_velocity).exp() - 1.0)
}

/// Der Booster eines geplanten Starts: seine Werte und sein Rückflug.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoosterPlan {
    pub params: VesselParams,
    /// Masse des Boosters bei der Trennung (kg).
    pub mass: f64,
    pub back: Return,
}

/// Der Rückflug des Boosters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Return {
    /// Der Boostback.
    pub node: ManeuverNode,
    /// Zeitpunkt des Aufsetzens.
    pub landing: f64,
    /// Abstand des Aufsetzpunkts von der Startbasis (m).
    pub miss: f64,
}

/// Zeitleiste des Boosters ab der Trennung (Zustand `state`, Masse `mass`
/// zum Zeitpunkt `t`) mit dem Boostback `node`.
pub fn flight(
    bodies: &[Body],
    params: &VesselParams,
    body: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
    node: ManeuverNode,
) -> Option<Trajectory> {
    let mut trajectory = Trajectory::new_flying(bodies, params, body, t, state, mass);
    trajectory.add_node(bodies, params, t, node).ok()?;
    trajectory.set_armed(true).ok()?;
    Some(trajectory)
}

/// Sucht den Boostback, mit dem der Booster nach der Trennung an `pad`
/// aufsetzt. Der Schub geht waagerecht gegen die Bewegung über dem Boden.
/// Gesucht wird sein Betrag (mehr davon trägt den Aufsetzpunkt entlang des
/// Kurses `heading`, in Grad, weiter zurück) und, wo die Drehung des Planeten
/// den Booster zur Seite trägt, ein Winkel, um den er dagegen gedreht ist.
#[allow(clippy::too_many_arguments)]
pub fn plan_return(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    pad: SurfacePoint,
    heading: f64,
    t: f64,
    state: StateVector,
    mass: f64,
) -> Result<Return, PlanError> {
    let body = &bodies[body_id];
    let time = t + BOOSTBACK_DELAY;
    // Lage am Node ohne Boostback: legt die Richtungen fest.
    let coast = Trajectory::new_flying(bodies, params, body_id, t, state, mass);
    let (_, at_node) = coast
        .predicted_state(bodies, time)
        .ok_or(PlanError::BoosterNoReturn)?;
    let up = at_node.position.normalize();
    let ground = at_node.velocity - body.co_rotating_velocity(at_node.position);
    let Some(back) = (up * ground.dot(up) - ground).try_normalize() else {
        return Err(PlanError::BoosterNoReturn);
    };
    let side = up.cross(back);
    let [prograde, normal, radial] = maneuver::node_frame(&at_node);
    // Boostback mit dem Betrag `amount`, um den Winkel `turn` zur Seite gedreht.
    let node = |amount: f64, turn: f64| {
        let push = (back * turn.cos() + side * turn.sin()) * amount;
        ManeuverNode {
            time,
            delta_v: DVec3::new(push.dot(prograde), push.dot(normal), push.dot(radial)),
        }
    };

    // Kurs am Startplatz, im mitdrehenden System.
    let pad_up = pad.body_fixed_direction();
    let east = DVec3::Z.cross(pad_up).try_normalize().unwrap_or(DVec3::Y);
    let north = pad_up.cross(east);
    let heading = heading.to_radians();
    let along = north * heading.cos() + east * heading.sin();
    let left = pad_up.cross(along);
    // Wo der Booster heil aufsetzt, vom Startplatz aus gesehen.
    let landing = |amount: f64, turn: f64| -> Option<Landing> {
        let trajectory = flight(bodies, params, body_id, t, state, mass, node(amount, turn))?;
        let (time, point) = trajectory.segments().find_map(|s| match s {
            Segment::Surface {
                start,
                point,
                wreck: false,
                ..
            } => Some((*start, *point)),
            _ => None,
        })?;
        let at = point.body_fixed_direction();
        Some(Landing {
            time,
            ahead: body.radius * at.dot(along),
            aside: body.radius * at.dot(left),
        })
    };

    // Mehr als die Reserve hergibt, wird nicht versucht (bleibt vom Aufstieg
    // Treibstoff übrig, wäre mehr im Tank): Weit stärkere Stöße trügen den
    // Booster um den halben Planeten.
    let available = (params.exhaust_velocity * (mass / params.dry_mass).ln()).min(RETURN_DELTA_V);
    // Ohne Boostback landet er so weit vor dem Startplatz.
    let range = landing(0.0, 0.0).ok_or(PlanError::BoosterNoReturn)?.ahead;
    if range <= 0.0 {
        return Err(PlanError::BoosterNoReturn);
    }
    // Der Betrag für die Richtung `turn`: Mehr davon trägt den Aufsetzpunkt
    // stetig weiter zurück, mit allem Verfügbaren bis hinter den Startplatz.
    // Regula falsi (Illinois), wenige Schritte genügen.
    let solve = |turn: f64| -> Result<(f64, Landing), PlanError> {
        let failed = PlanError::BoosterNoReturn;
        let (mut low, mut high) = (0.0, available);
        let mut ahead_low = landing(0.0, turn).ok_or(failed)?.ahead;
        let mut ahead_high = landing(available, turn).ok_or(failed)?.ahead;
        if ahead_low < 0.0 || ahead_high > 0.0 {
            return Err(failed);
        }
        let mut found = Err(failed);
        for _ in 0..30 {
            let amount = (low * ahead_high - high * ahead_low) / (ahead_high - ahead_low);
            let at = landing(amount, turn).ok_or(failed)?;
            found = Ok((amount, at));
            if at.ahead.abs() < PRECISION {
                break;
            }
            if at.ahead > 0.0 {
                (low, ahead_low) = (amount, at.ahead);
                ahead_high *= 0.5;
            } else {
                (high, ahead_high) = (amount, at.ahead);
                ahead_low *= 0.5;
            }
        }
        found
    };

    // Die Drehung des Planeten trägt den Aufsetzpunkt zur Seite, je nach
    // Kurs verschieden weit: die Richtung nachdrehen, bis es passt
    // (Sekantenverfahren; der erste Schritt nach der Geometrie).
    let mut turn = 0.0;
    let mut best = solve(turn)?;
    let mut previous: Option<(f64, f64)> = None;
    for _ in 0..5 {
        let aside = best.1.aside;
        if aside.abs() < ASIDE_PRECISION {
            break;
        }
        let next = match previous {
            Some((turn_before, aside_before)) if aside != aside_before => {
                turn - aside * (turn - turn_before) / (aside - aside_before)
            }
            _ => turn + aside / range,
        };
        previous = Some((turn, aside));
        turn = next.clamp(-MAX_TURN, MAX_TURN);
        match solve(turn) {
            Ok(better) => best = better,
            Err(_) => break,
        }
    }
    let (amount, at) = best;
    let miss = at.ahead.hypot(at.aside);
    if miss > RETURN_RADIUS {
        return Err(PlanError::BoosterNoReturn);
    }
    Ok(Return {
        node: node(amount, turn),
        landing: at.time,
        miss,
    })
}

/// Aufsetzpunkt vom Startplatz aus: wann, wie weit vor (+) oder hinter (−)
/// ihm entlang des Kurses und wie weit links (+) oder rechts davon (m).
#[derive(Clone, Copy)]
struct Landing {
    time: f64,
    ahead: f64,
    aside: f64,
}
