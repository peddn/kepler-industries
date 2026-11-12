//! Start vom Boden mit wenigen Werten: Kurs, Wendebeginn, Wendewinkel,
//! Ziel-Apoapsis. Die Voreinstellung hängt vom Körper ab (`for_body`).
//!
//! Der Aufstieg ist ein normales Brennen (`Burn`), nur mit Aufstiegssteuerung
//! statt fester Schubrichtung. Damit gelten Vorschau = Ausführung, Brennpunkte
//! und Nodes danach ohne Sonderweg. Das Ende (Brennschluss) wird beim Planen
//! berechnet: wenn die Apoapsis die Zielhöhe erreicht, sonst mit leerem Tank.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::booster::{self, BoosterPlan};
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
    /// Wendewinkel in Grad: so weit neigt sich das Schiff beim Wendebeginn
    /// aus der Senkrechten, und steiler als so fliegt es danach nicht (alte
    /// Spielstände: 10°).
    #[serde(default = "default_kick")]
    pub kick: f64,
}

fn default_kick() -> f64 {
    KICK_DEGREES
}

/// Grad in Radiant, gerechnet wie `KICK`: 10° ergibt bitgenau `KICK`, ein
/// Start mit der alten Voreinstellung also genau dasselbe wie früher.
pub fn kick_radians(degrees: f64) -> f64 {
    degrees * std::f64::consts::PI / 180.0
}

/// Ziel-Apoapsis der Voreinstellung: so hoch über der Luftgrenze (ohne Luft:
/// über dem Boden).
const ORBIT_MARGIN: f64 = 30_000.0;

impl Default for LaunchProfile {
    /// Wie auf Zorb (siehe `for_body`).
    fn default() -> Self {
        Self {
            heading: 90.0,
            turn_altitude: 1_500.0,
            target_apoapsis: 100_000.0,
            kick: default_kick(),
        }
    }
}

impl LaunchProfile {
    /// Voreinstellung für einen Start von `body`, nach Osten.
    ///
    /// Mit Luft: senkrecht bis 1,5 km, dann 10° (eine steilere Wende flöge
    /// zu lange durch dichte Luft), Ziel 30 km über der Luftgrenze, auf Zorb
    /// 100 km. Ohne Luft gibt es keinen Grund, senkrecht zu steigen: Wende ab
    /// 100 m mit 70°, Ziel 30 km. Auf Zoon kostet die 30-km-Bahn so etwa
    /// 670 m/s statt 820 m/s mit den Werten von Zorb.
    pub fn for_body(body: &Body) -> Self {
        let target_apoapsis = body.atmosphere.boundary_altitude() + ORBIT_MARGIN;
        if body.atmosphere.exists() {
            Self {
                target_apoapsis,
                ..Self::default()
            }
        } else {
            Self {
                heading: 90.0,
                turn_altitude: 100.0,
                target_apoapsis,
                kick: 70.0,
            }
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
    /// Start mit dem Booster der Startbasis (siehe `booster`).
    #[serde(default)]
    pub booster: Option<BoosterPlan>,
}

/// Aufstiegssteuerung, eine Schwerkraftwende wie in KSP: senkrecht bis zum
/// Wendebeginn, dort um den Wendewinkel zur Seite geneigt, danach immer
/// entlang der Bewegung relativ zur Luft (nie steiler als der Wendewinkel,
/// nie unter die Waagerechte). Die Schwerkraft
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
    /// Wendewinkel in Radiant (alte Spielstände: `KICK`).
    #[serde(default = "default_kick_radians")]
    pub kick: f64,
    /// Aufstieg als Stapel mit dem Booster.
    #[serde(default)]
    pub stack: Option<Stack>,
}

/// Schiff auf dem Booster: Der Booster brennt, das Schiff ist Nutzlast.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    /// Masse des Schiffs in kg; mit ihr fliegt es nach der Trennung weiter.
    pub payload: f64,
    /// Widerstandsfläche des Stapels in m²: Schiff und Booster stehen
    /// hintereinander, es zählt die größere der beiden.
    pub drag_area: f64,
}

fn default_kick_radians() -> f64 {
    KICK
}

/// Wendewinkel wie bisher, auch für alte Spielstände (10°).
const KICK_DEGREES: f64 = 10.0;
pub const KICK: f64 = KICK_DEGREES * std::f64::consts::PI / 180.0;

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
        let pitch = air
            .dot(forward)
            .atan2(air.dot(up))
            .clamp(self.kick, FRAC_PI_2);
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
///
/// Mit `booster` (seine Werte, vollgetankt) hebt das Schiff als Stapel ab:
/// Es brennt nur der Booster, und zwar höchstens, bis nur noch seine Reserve
/// für den Rückflug im Tank ist.
pub fn plan(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    point: SurfacePoint,
    mass: f64,
    launch: &Launch,
    booster: Option<&VesselParams>,
) -> Result<Burn, PlanError> {
    // Wer brennt, was beim Abheben insgesamt wiegt und was verbrannt werden darf.
    let engine = booster.unwrap_or(params);
    let (lift_mass, fuel, drag_area) = match booster {
        Some(b) => (
            mass + b.dry_mass + b.fuel_capacity,
            (b.fuel_capacity - booster::reserve(b)).max(0.0),
            params.drag_area.max(b.drag_area),
        ),
        None => (mass, (mass - params.dry_mass).max(0.0), params.drag_area),
    };
    let stack = booster.map(|_| Stack {
        payload: mass,
        drag_area,
    });
    if engine.thrust <= 0.0 || engine.exhaust_velocity <= 0.0 {
        return Err(PlanError::NoEngine);
    }
    let body = &bodies[body_id];
    let gravity = body.gm / (body.radius * body.radius);
    if engine.thrust <= lift_mass * gravity {
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
    let mass_flow = engine.thrust / engine.exhaust_velocity;
    let burnout = t0 + fuel / mass_flow;
    let mut burn = Burn {
        start: t0,
        end: burnout,
        start_mass: lift_mass,
        direction: up,
        thrust: engine.thrust,
        mass_flow,
        ascent: Some(Ascent {
            plane_normal: up.cross(forward).normalize(),
            turn_radius,
            spin: body.spin(),
            kick: kick_radians(launch.profile.kick),
            stack,
        }),
        descent: None,
    };

    // Dieselben Schritte wie später in der Zeitleiste (`integrate_arc`).
    let target = body.radius + launch.profile.target_apoapsis;
    let forces = Forces {
        body,
        drag_area,
        // Der Stapel hat keinen Fallschirm; der des Schiffs zählt erst nach
        // der Trennung.
        parachute_area: if booster.is_some() {
            0.0
        } else {
            params.parachute_area
        },
        mass: lift_mass,
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
