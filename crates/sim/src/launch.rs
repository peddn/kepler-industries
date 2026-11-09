//! Start vom Boden mit wenigen Werten: Kurs, Wendebeginn, Ziel-Apoapsis.
//!
//! Der Aufstieg ist ein normales Brennen (`Burn`), nur mit Aufstiegssteuerung
//! statt fester Schubrichtung. Damit gelten Vorschau = Ausführung, Brennpunkte
//! und Nodes danach ohne Sonderweg. Das Ende (Brennschluss) wird beim Planen
//! berechnet: wenn die Apoapsis die Zielhöhe erreicht, sonst mit leerem Tank.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::integrator::{self, Forces, STEP};
use crate::kepler::{self, StateVector};
use crate::maneuver::{Burn, PlanError};
use crate::world::VesselParams;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::f64::consts::FRAC_PI_2;

/// Die Werte, die der Spieler einstellt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaunchProfile {
    /// Kurs in Grad: 0 Norden, 90 Osten.
    pub heading: f64,
    /// Höhe in m, ab der sich das Schiff aus der Senkrechten neigt.
    pub turn_altitude: f64,
    /// Ziel-Apoapsis als Höhe in m.
    pub target_apoapsis: f64,
}

impl Default for LaunchProfile {
    fn default() -> Self {
        Self {
            heading: 90.0,
            turn_altitude: 1_500.0,
            target_apoapsis: 100_000.0,
        }
    }
}

/// Geplanter Start: wann und wie.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Launch {
    /// Zeitpunkt des Abhebens.
    pub time: f64,
    pub profile: LaunchProfile,
}

/// Ein geplanter Start mit dem daraus berechneten Aufstieg.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub launch: Launch,
    /// Der Aufstieg: Brennen vom Abheben bis zum Brennschluss.
    pub burn: Burn,
}

/// Aufstiegssteuerung, eine Schwerkraftwende wie in KSP: senkrecht bis zum
/// Wendebeginn, dort um `KICK` zur Seite geneigt, danach immer entlang der
/// Bewegung relativ zur Luft (nie unter die Waagerechte). Die Schwerkraft
/// biegt die Bahn von selbst um. Schub entlang der Bewegung hebt die Apoapsis
/// stetig, deshalb wird die Zielhöhe sicher erreicht, auch bei einem Hop.
/// Die Schubrichtung bleibt in einer festen Ebene durch den Startplatz
/// (Großkreis in Richtung des Kurses).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ascent {
    /// Normale der Aufstiegsebene im nicht rotierenden System.
    pub plane_normal: DVec3,
    /// Radius in m, ab dem gewendet wird.
    pub turn_radius: f64,
    /// Drehung des Körpers (für die Bewegung relativ zur Luft).
    pub spin: DVec3,
}

/// Anfängliche Neigung aus der Senkrechten beim Wendebeginn (10°).
pub const KICK: f64 = 10.0 * std::f64::consts::PI / 180.0;

impl Ascent {
    /// Schubrichtung (Einheitsvektor) im Zustand `state`.
    pub fn direction(&self, state: &StateVector) -> DVec3 {
        let r = state.position.length();
        let up = state.position / r;
        if r < self.turn_radius {
            return up;
        }
        let forward = self.plane_normal.cross(up).normalize();
        let air = state.velocity - self.spin.cross(state.position);
        // Neigung der Bewegung aus der Senkrechten, in der Aufstiegsebene.
        let pitch = air.dot(forward).atan2(air.dot(up)).clamp(KICK, FRAC_PI_2);
        up * pitch.cos() + forward * pitch.sin()
    }
}

/// Apoapsis als Radius; unendlich auf offenen Bahnen.
fn apoapsis(gm: f64, state: &StateVector) -> f64 {
    kepler::orbit_info(gm, state)
        .apoapsis
        .unwrap_or(f64::INFINITY)
}

/// Zustand am Startplatz zum Zeitpunkt `t` (mitrotierend).
pub fn pad_state(body: &Body, point: SurfacePoint, t: f64) -> StateVector {
    let position = body.surface_to_inertial(point, 0.0, t);
    StateVector {
        position,
        velocity: body.co_rotating_velocity(position),
    }
}

/// Berechnet den Aufstieg für einen Start vom Punkt `point` mit der Masse
/// `mass`. Das Brennen endet, sobald die Apoapsis die Zielhöhe erreicht (auf
/// Bruchteile eines Meters genau), spätestens mit leerem Tank.
pub fn plan(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    point: SurfacePoint,
    mass: f64,
    launch: &Launch,
) -> Result<Burn, PlanError> {
    if params.thrust <= 0.0 || params.exhaust_velocity <= 0.0 {
        return Err(PlanError::NoEngine);
    }
    let body = &bodies[body_id];
    let gravity = body.gm / (body.radius * body.radius);
    if params.thrust <= mass * gravity {
        return Err(PlanError::TooWeak);
    }

    let t0 = launch.time;
    let start = pad_state(body, point, t0);
    let up = start.position.normalize();
    let east = DVec3::Z.cross(up).try_normalize().unwrap_or(DVec3::Y);
    let north = up.cross(east);
    let heading = launch.profile.heading.to_radians();
    let forward = north * heading.cos() + east * heading.sin();
    let turn_radius = body.radius + launch.profile.turn_altitude.max(0.0);
    let mass_flow = params.thrust / params.exhaust_velocity;
    let burnout = t0 + (mass - params.dry_mass).max(0.0) / mass_flow;
    let mut burn = Burn {
        start: t0,
        end: burnout,
        start_mass: mass,
        direction: up,
        thrust: params.thrust,
        mass_flow,
        ascent: Some(Ascent {
            plane_normal: up.cross(forward).normalize(),
            turn_radius,
            spin: body.spin(),
        }),
    };

    // Dieselben Schritte wie später in der Zeitleiste (`integrate_arc`).
    let target = body.radius + launch.profile.target_apoapsis;
    let forces = Forces {
        body,
        drag_area: params.drag_area,
        parachute_area: params.parachute_area,
        mass,
        burn: Some(&burn),
    };
    let mut t = t0;
    let mut s = start;
    let mut step = 0usize;
    let mut end = burnout;
    while t < burnout {
        let h = STEP.min(burnout - t);
        let next = integrator::rk4(&forces, t, &s, h);
        if next.position.length() <= body.radius {
            // Fällt zurück: Das Brennen liefe bis zum leeren Tank, der
            // Abschnitt endet vorher am Boden.
            break;
        }
        if apoapsis(body.gm, &next) >= target {
            // Brennschluss genau im Schritt suchen.
            let (mut lo, mut hi) = (0.0, h);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if apoapsis(body.gm, &integrator::rk4(&forces, t, &s, mid)) >= target {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            end = t + hi;
            break;
        }
        s = next;
        step += 1;
        t = t0 + step as f64 * STEP;
    }
    burn.end = end;
    Ok(burn)
}
