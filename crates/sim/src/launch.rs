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

/// Anteil der Ziel-Apoapsis, ab dem das Schiff waagerecht fliegt.
pub const FLAT_FRACTION: f64 = 0.5;

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
            turn_altitude: 2_000.0,
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

/// Aufstiegssteuerung: senkrecht bis zum Wendebeginn, dann zunehmend geneigt
/// (Wurzelform: anfangs schneller), waagerecht ab `flat_radius`. Die
/// Schubrichtung bleibt in einer festen Ebene durch den Startplatz (Großkreis
/// in Richtung des Kurses).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ascent {
    /// Normale der Aufstiegsebene im nicht rotierenden System.
    pub plane_normal: DVec3,
    /// Radius in m, ab dem gewendet wird.
    pub turn_radius: f64,
    /// Radius in m, ab dem waagerecht geflogen wird.
    pub flat_radius: f64,
}

impl Ascent {
    /// Schubrichtung (Einheitsvektor) an der Position `position`.
    pub fn direction(&self, position: DVec3) -> DVec3 {
        let r = position.length();
        let up = position / r;
        let progress =
            ((r - self.turn_radius) / (self.flat_radius - self.turn_radius)).clamp(0.0, 1.0);
        // Neigung aus der Senkrechten.
        let pitch = FRAC_PI_2 * progress.sqrt();
        let forward = self.plane_normal.cross(up).normalize();
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
    let flat_radius =
        (body.radius + FLAT_FRACTION * launch.profile.target_apoapsis).max(turn_radius + 1_000.0);
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
            flat_radius,
        }),
    };

    // Dieselben Schritte wie später in der Zeitleiste (`integrate_arc`).
    let target = body.radius + launch.profile.target_apoapsis;
    let forces = Forces {
        body,
        drag_area: params.drag_area,
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
