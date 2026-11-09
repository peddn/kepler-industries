//! Andocken: Angleichen an der engsten Annäherung, Anflug, Kontakt.
//!
//! Der Spieler stellt mit normalen Nodes eine Annäherung unter
//! `DOCKING_RANGE` ein. Dann hängt die Kette an:
//! 1. **Angleichen:** ein automatischer Node an der engsten Annäherung mit
//!    Δv = Relativgeschwindigkeit (echtes, zentriertes Brennen).
//! 2. **Anflug:** ein Abschnitt (`Segment::Approach`) auf gerader Linie im
//!    mitbewegten System des Ziels bis zu dessen Mitte: beschleunigen mit
//!    `APPROACH_ACCELERATION` auf höchstens `APPROACH_SPEED`, Kontakt mit
//!    `CONTACT_SPEED`. Der Treibstoff folgt aus dem Schub, den diese Linie
//!    braucht (auch gegen die Bahnmechanik).
//! 3. **Kontakt:** Die Schiffe werden ein Verbund (`World`).
//!
//! Der Anflug speichert eine Kopie der Zielbahn (Keplerzustand zu einer
//! Epoche) und hängt damit nicht von einem anderen Schiff ab: keine Drift,
//! unabhängig von der Schrittgröße. Dafür muss das Ziel ab dem Angleichen auf
//! einer stabilen Bahn sein und darf nichts mehr planen.

use crate::body::Body;
use crate::kepler::{self, StateVector};
use crate::maneuver::Plan;
use glam::{DMat3, DVec3};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Größte Annäherung (m), bei der angedockt werden kann.
pub const DOCKING_RANGE: f64 = 1_000.0;
/// Höchste Geschwindigkeit im Anflug relativ zum Ziel (m/s).
pub const APPROACH_SPEED: f64 = 2.0;
/// Geschwindigkeit beim Kontakt (m/s).
pub const CONTACT_SPEED: f64 = 0.1;
/// Beschleunigen und Abbremsen im Anflug (m/s²).
pub const APPROACH_ACCELERATION: f64 = 0.05;
/// Zeitschritt (s) beim Aufsummieren des Schubs im Anflug.
const COST_STEP: f64 = 1.0;

/// Warum das Andocken (gerade) nicht geht.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DockError {
    NoTarget,
    /// Das Schiff brennt gerade oder dockt schon an.
    Busy,
    /// Keine Annäherung im Suchzeitraum (z. B. ohne Bahn).
    NoApproach,
    /// Engste Annäherung zu weit (m).
    TooFar {
        distance: f64,
    },
    /// Das Ziel ist ab dem Angleichen nicht auf einer stabilen Bahn oder plant
    /// noch etwas.
    TargetNotStable,
    /// Das Angleichen überschneidet sich mit dem vorigen Brennen.
    Overlaps,
    /// Ein Node davor ist ungültig.
    AfterInvalid,
    NoEngine,
    /// Nicht genug Δv (m/s) für Angleichen und Anflug.
    NoFuel {
        needed: f64,
        available: f64,
    },
    /// Schub reicht nicht für den Anflug.
    TooWeak,
}

impl fmt::Display for DockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockError::NoTarget => write!(f, "kein Ziel"),
            DockError::Busy => write!(f, "Schiff brennt gerade oder dockt schon an"),
            DockError::NoApproach => write!(f, "keine Annäherung in Sicht"),
            DockError::TooFar { distance } => write!(
                f,
                "Annäherung {:.2} km, nötig unter {:.0} km",
                distance / 1000.0,
                DOCKING_RANGE / 1000.0
            ),
            DockError::TargetNotStable => {
                write!(f, "Ziel nicht auf stabiler Bahn oder plant noch Manöver")
            }
            DockError::Overlaps => write!(f, "Angleichen überschneidet sich mit dem Brennen davor"),
            DockError::AfterInvalid => write!(f, "ein Node davor ist ungültig"),
            DockError::NoEngine => write!(f, "kein Triebwerk"),
            DockError::NoFuel { needed, available } => write!(
                f,
                "Δv reicht nicht: nötig {needed:.0} m/s, an Bord {available:.0} m/s"
            ),
            DockError::TooWeak => write!(f, "Schub reicht nicht für den Anflug"),
        }
    }
}

/// Was `World` für das Andocken ausrechnet und der Zeitleiste übergibt: der
/// automatische Node und die Bahn des Ziels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DockRequest {
    /// Angleichen: Zeitpunkt der engsten Annäherung, Δv im Node-System.
    pub node: crate::maneuver::ManeuverNode,
    /// Bahn des Ziels: Keplerzustand zur Epoche `target_epoch`. Ab dann ist
    /// das Ziel auf dieser Bahn, ohne Ende (stabil, nichts geplant).
    pub target_epoch: f64,
    pub target_state: StateVector,
}

/// Berechnetes Andocken.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DockPlan {
    /// Der automatische Node „Angleichen“.
    pub matching: Plan,
    /// Abstand zum Ziel zu Beginn des Anflugs (m).
    pub distance: f64,
    /// Δv des Anflugs (m/s), samt Rest der Relativgeschwindigkeit nach dem
    /// Angleichen.
    pub approach_delta_v: f64,
    /// Zeitpunkt des Kontakts.
    pub contact: f64,
    /// Für den Anflug-Abschnitt: Zustand des Ziels und Lage relativ dazu bei
    /// Beginn des Anflugs, Masse zu Beginn und beim Kontakt.
    pub target: StateVector,
    pub offset: DVec3,
    pub mass: f64,
    pub end_mass: f64,
}

/// Andocken am Ende der Kette: Ziel, Anfrage von `World` und Ergebnis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Docking {
    pub target: usize,
    pub request: Result<DockRequest, DockError>,
    pub plan: Result<DockPlan, DockError>,
}

/// Geschwindigkeitsprofil des Anflugs über `distance` (m): beschleunigen,
/// gleiten, abbremsen auf die Kontaktgeschwindigkeit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    distance: f64,
    peak: f64,
    /// Dauern der drei Phasen (s).
    phases: [f64; 3],
}

impl Profile {
    pub fn new(distance: f64) -> Self {
        let a = APPROACH_ACCELERATION;
        let contact = CONTACT_SPEED;
        // Spitzengeschwindigkeit: höchstens APPROACH_SPEED, bei kurzen Wegen
        // so, dass Beschleunigen und Abbremsen genau passen.
        let peak = APPROACH_SPEED.min((a * distance + 0.5 * contact * contact).sqrt());
        if peak <= contact {
            // Sehr kurz: mit Kontaktgeschwindigkeit hin.
            return Self {
                distance,
                peak: contact,
                phases: [0.0, distance / contact, 0.0],
            };
        }
        let accelerate = peak / a;
        let brake = (peak - contact) / a;
        let covered = 0.5 * peak * accelerate + 0.5 * (peak + contact) * brake;
        Self {
            distance,
            peak,
            phases: [accelerate, (distance - covered).max(0.0) / peak, brake],
        }
    }

    pub fn duration(&self) -> f64 {
        self.phases.iter().sum()
    }

    /// Zurückgelegter Weg (m) nach der Zeit `t` (s).
    pub fn covered(&self, t: f64) -> f64 {
        let a = APPROACH_ACCELERATION;
        let [t1, t2, t3] = self.phases;
        let t = t.clamp(0.0, t1 + t2 + t3);
        if t <= t1 {
            0.5 * a * t * t
        } else if t <= t1 + t2 {
            0.5 * a * t1 * t1 + self.peak * (t - t1)
        } else {
            let s = t - t1 - t2;
            let start = if t1 > 0.0 { 0.5 * a * t1 * t1 } else { 0.0 };
            let brake = if t3 > 0.0 { a } else { 0.0 };
            (start + self.peak * t2 + self.peak * s - 0.5 * brake * s * s).min(self.distance)
        }
    }

    /// Geschwindigkeit (m/s) nach der Zeit `t`.
    pub fn speed(&self, t: f64) -> f64 {
        let a = APPROACH_ACCELERATION;
        let [t1, t2, t3] = self.phases;
        if t <= t1 {
            a * t.max(0.0)
        } else if t <= t1 + t2 {
            self.peak
        } else if t3 > 0.0 {
            (self.peak - a * (t - t1 - t2)).max(CONTACT_SPEED)
        } else {
            self.peak
        }
    }
}

/// Mitbewegtes System des Ziels: Spalten radial, entlang der Bahn, normal.
/// Seine Drehung ist `angular_velocity`.
pub fn frame(target: &StateVector) -> DMat3 {
    let radial = target.position.normalize();
    let normal = target.position.cross(target.velocity).normalize();
    DMat3::from_cols(radial, normal.cross(radial), normal)
}

/// Lage des Schiffs `own` im mitbewegten System des Ziels und wie schnell
/// (m/s) es sich darin noch bewegt (Rest der Relativgeschwindigkeit).
pub fn relative(target: &StateVector, own: &StateVector) -> (DVec3, f64) {
    let relative = own.position - target.position;
    let offset = frame(target).transpose() * relative;
    let at_rest = target.velocity + angular_velocity(target).cross(relative);
    (offset, (own.velocity - at_rest).length())
}

/// Drehgeschwindigkeit des mitbewegten Systems (rad/s, als Vektor) auf einer
/// Keplerbahn: h / r².
pub fn angular_velocity(target: &StateVector) -> DVec3 {
    let h = target.position.cross(target.velocity);
    h / target.position.length_squared()
}

/// Lage und Geschwindigkeit im Anflug zur Zeit `t`: Zielbahn ab `start` mit
/// `target`, Anfangslage `offset` im mitbewegten System.
pub fn approach_state(
    gm: f64,
    start: f64,
    target: &StateVector,
    offset: DVec3,
    t: f64,
) -> StateVector {
    let tgt = kepler::propagate(gm, *target, t - start);
    let profile = Profile::new(offset.length());
    let tau = t - start;
    let (direction, distance) = (offset.normalize_or_zero(), offset.length());
    let rest = distance - profile.covered(tau);
    let rotation = frame(&tgt);
    let relative = rotation * (direction * rest);
    let omega = angular_velocity(&tgt);
    let moving = rotation * (direction * -profile.speed(tau));
    StateVector {
        position: tgt.position + relative,
        velocity: tgt.velocity + omega.cross(relative) + moving,
    }
}

/// Δv (m/s) und größte nötige Beschleunigung (m/s²) des Anflugs: Summe des
/// Schubs, der die Linie gegen die Schwerkraft hält und das Profil fliegt.
pub fn approach_cost(body: &Body, start: f64, target: &StateVector, offset: DVec3) -> (f64, f64) {
    let gm = body.gm;
    let duration = Profile::new(offset.length()).duration();
    let position = |t: f64| approach_state(gm, start, target, offset, t).position;
    let h = 0.5;
    let steps = (duration / COST_STEP).ceil().max(1.0) as usize;
    let dt = duration / steps as f64;
    let (mut delta_v, mut peak) = (0.0, 0.0f64);
    for i in 0..steps {
        let t = start + (i as f64 + 0.5) * dt;
        let p = position(t);
        let accel = (position(t + h) - 2.0 * p + position(t - h)) / (h * h);
        let gravity = -gm * p / p.length().powi(3);
        let thrust = (accel - gravity).length();
        delta_v += thrust * dt;
        peak = peak.max(thrust);
    }
    (delta_v, peak)
}
