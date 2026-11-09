//! Landung: Zielgebiete, Fallschirm und Aufsetzen.
//!
//! Die letzte Bremsung vor dem Boden ist abstrahiert: Beim Bodenkontakt zählt
//! die Geschwindigkeit relativ zum Boden. Bis `SAFE_SPEED` setzt jedes Schiff
//! auf. Darüber muss es diese Geschwindigkeit als Δv abbremsen: mit genug
//! Treibstoff an Bord und Schub größer als das Gewicht. Sonst wird es zum
//! Wrack. Wo es aufsetzt, entscheidet nicht darüber, ob es heil ankommt; ein
//! Zielgebiet zählt für Basis und Aufträge. Außerhalb geht es mit einem Hop
//! (Start mit niedriger Ziel-Apoapsis) weiter.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::kepler::StateVector;
use crate::world::VesselParams;
use serde::{Deserialize, Serialize};

/// Bis zu dieser Geschwindigkeit relativ zum Boden (m/s) setzt jedes Schiff
/// ohne Treibstoff auf.
pub const SAFE_SPEED: f64 = 5.0;

/// Unterhalb dieser Höhe (m) ist im Sinkflug der Fallschirm offen.
pub const PARACHUTE_ALTITUDE: f64 = 3_000.0;

/// Ist der Fallschirm offen? Nur von Höhe und Richtung abhängig, damit sich die
/// Simulation nichts merken muss: beim Aufstieg zu, im Sinkflug unten offen.
pub fn parachute_open(body: &Body, state: &StateVector) -> bool {
    state.position.length() - body.radius < PARACHUTE_ALTITUDE
        && state.position.dot(state.velocity) < 0.0
}

/// Kreisförmiges Zielgebiet auf der Oberfläche.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LandingZone {
    pub name: String,
    pub body: BodyId,
    pub center: SurfacePoint,
    /// Radius in m (entlang der Oberfläche).
    pub radius: f64,
}

impl LandingZone {
    /// Abstand (m) des Punkts vom Mittelpunkt, entlang der Oberfläche.
    pub fn distance(&self, body: &Body, point: &SurfacePoint) -> f64 {
        let a = self.center.body_fixed_direction();
        let b = point.body_fixed_direction();
        body.radius * a.dot(b).clamp(-1.0, 1.0).acos()
    }
}

/// Warum ein Bodenkontakt zum Absturz wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Crash {
    /// Zu wenig Δv an Bord, um die Geschwindigkeit abzubremsen.
    NoFuel { available: f64 },
    /// Schub nicht größer als das Gewicht.
    TooWeak,
}

/// Geschwindigkeit relativ zum Boden (m/s).
pub fn ground_speed(body: &Body, state: &StateVector) -> f64 {
    (state.velocity - body.co_rotating_velocity(state.position)).length()
}

/// Bremsen beim Bodenkontakt: das nötige Δv (0 bis `SAFE_SPEED`), oder warum
/// es ein Absturz wird.
pub fn braking(
    body: &Body,
    params: &VesselParams,
    state: &StateVector,
    mass: f64,
) -> Result<f64, Crash> {
    let speed = ground_speed(body, state);
    if speed <= SAFE_SPEED {
        return Ok(0.0);
    }
    let gravity = body.gm / (body.radius * body.radius);
    if params.thrust <= mass * gravity || params.exhaust_velocity <= 0.0 {
        return Err(Crash::TooWeak);
    }
    let available = params.exhaust_velocity * (mass / params.dry_mass).ln();
    if available < speed {
        return Err(Crash::NoFuel { available });
    }
    Ok(speed)
}

/// Masse nach dem Bremsen um `delta_v` (Raketengleichung).
pub fn mass_after(params: &VesselParams, mass: f64, delta_v: f64) -> f64 {
    if delta_v <= 0.0 {
        mass
    } else {
        mass * (-delta_v / params.exhaust_velocity).exp()
    }
}
