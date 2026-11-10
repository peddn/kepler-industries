//! Manöver-Nodes und Brennmanöver.
//!
//! Ein Node ist der Plan selbst: Wird er gesetzt, wird die Zeitleiste des
//! Schiffs mit dem Brennmanöver neu berechnet. Vorschau und Ausführung sind
//! damit derselbe Code, die Vorschau kann nicht von der Ausführung abweichen.
//! Mehrere Nodes bilden eine Kette (siehe `trajectory`).

use crate::body::BodyId;
use crate::kepler::StateVector;
use crate::landing::Descent;
use crate::launch::Ascent;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Vom Spieler geplanter Node.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManeuverNode {
    /// Zeitpunkt in s; das Brennmanöver liegt zeitlich mittig darum.
    pub time: f64,
    /// Δv in m/s: x = prograd, y = normal, z = radial nach außen.
    pub delta_v: DVec3,
}

/// Ein Brennmanöver mit endlicher Dauer.
///
/// Die Masse sinkt linear mit der Zeit (konstanter Schub, konstanter
/// Massenstrom) und wird analytisch berechnet, nicht integriert. Dadurch gilt
/// die Raketengleichung exakt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Burn {
    pub start: f64,
    pub end: f64,
    pub start_mass: f64,
    /// Schubrichtung als Einheitsvektor im nicht rotierenden System. Bleibt
    /// während des ganzen Manövers fest (wie in KSP beim Zielen auf den Node).
    pub direction: DVec3,
    /// Schub in N.
    pub thrust: f64,
    /// Treibstoffverbrauch in kg/s.
    pub mass_flow: f64,
    /// Beim Start vom Boden: Aufstiegssteuerung statt fester Richtung.
    #[serde(default)]
    pub ascent: Option<Ascent>,
    /// Landebrennen: Schub gegen die Bewegung zum Boden statt fester
    /// Richtung (siehe `landing`).
    #[serde(default)]
    pub descent: Option<Descent>,
}

impl Burn {
    /// Brennmanöver mit dem Δv `delta_v`, zeitlich mittig um `node_time`.
    /// Reicht der Treibstoff nicht, endet es mit leerem Tank.
    pub fn centered(
        node_time: f64,
        start_mass: f64,
        params: &crate::world::VesselParams,
        direction: DVec3,
        delta_v: f64,
    ) -> Self {
        let mass_flow = params.thrust / params.exhaust_velocity;
        let wanted_end_mass = start_mass * (-delta_v / params.exhaust_velocity).exp();
        let end_mass = wanted_end_mass.max(params.dry_mass.min(start_mass));
        let duration = (start_mass - end_mass) / mass_flow;
        let start = node_time - duration / 2.0;
        Self {
            start,
            end: start + duration,
            start_mass,
            direction,
            thrust: params.thrust,
            mass_flow,
            ascent: None,
            descent: None,
        }
    }

    /// Landebrennen ab `start` im Zustand `state` mit vollem Schub gegen die
    /// Bewegung zum Boden von `body`. Das Ende ist hier der leere Tank; die
    /// Zeitleiste beendet das Brennen früher, sobald das Schiff steht.
    pub fn landing(
        start: f64,
        start_mass: f64,
        params: &crate::world::VesselParams,
        body: &crate::body::Body,
        state: &StateVector,
    ) -> Self {
        let mass_flow = params.thrust / params.exhaust_velocity;
        let fuel = (start_mass - params.dry_mass).max(0.0);
        let descent = Descent::new(body);
        Self {
            start,
            end: start + fuel / mass_flow,
            start_mass,
            direction: descent.direction(state),
            thrust: params.thrust,
            mass_flow,
            ascent: None,
            descent: Some(descent),
        }
    }

    /// Schubrichtung im Zustand `state`.
    pub fn direction_at(&self, state: &StateVector) -> DVec3 {
        if let Some(descent) = self.descent {
            return descent.direction(state);
        }
        self.ascent
            .map_or(self.direction, |ascent| ascent.direction(state))
    }

    /// Ist das ein Landebrennen?
    pub fn is_landing(&self) -> bool {
        self.descent.is_some()
    }

    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    pub fn is_active(&self, t: f64) -> bool {
        t >= self.start && t <= self.end
    }

    pub fn mass_at(&self, t: f64) -> f64 {
        self.start_mass - self.mass_flow * (t.clamp(self.start, self.end) - self.start)
    }

    pub fn end_mass(&self) -> f64 {
        self.mass_at(self.end)
    }

    /// Tatsächlich geliefertes Δv (Raketengleichung).
    pub fn delta_v(&self) -> f64 {
        let exhaust_velocity = self.thrust / self.mass_flow;
        exhaust_velocity * (self.start_mass / self.end_mass()).ln()
    }
}

/// Was aus einem Node folgt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// Der Node, wie er ausgeführt wird: Zeitpunkt nach der Regel "Brennen
    /// nicht zu früh" eventuell einen oder mehrere Umläufe später als gesetzt.
    pub node: ManeuverNode,
    /// Körper, um den das Schiff beim Node kreist.
    pub body: BodyId,
    /// Zustand am Node auf der Bahn *ohne* das Manöver. Legt die Richtungen
    /// prograd/normal/radial fest und ist der Ort der Node-Markierung.
    pub node_state: StateVector,
    pub burn: Burn,
}

impl Plan {
    /// Derselbe Plan einen Umlauf später: gleiche Stelle der Bahn, gleiche
    /// Richtung, alle Zeiten um die Umlaufzeit verschoben.
    pub fn one_orbit_later(&self, period: f64) -> Plan {
        let mut later = *self;
        later.node.time += period;
        later.burn.start += period;
        later.burn.end += period;
        later
    }
}

/// Einheitsvektoren am Node: prograd, normal, radial nach außen.
pub fn node_frame(state: &StateVector) -> [DVec3; 3] {
    let prograde = state.velocity.normalize();
    let normal = state.position.cross(state.velocity).normalize();
    let radial = prograde.cross(normal);
    [prograde, normal, radial]
}

/// Rechnet ein Δv in Node-Koordinaten in das nicht rotierende System um.
pub fn to_inertial(state: &StateVector, delta_v: DVec3) -> DVec3 {
    let [p, n, r] = node_frame(state);
    p * delta_v.x + n * delta_v.y + r * delta_v.z
}

/// Warum ein Node nicht gesetzt oder gelöscht werden kann, oder warum ein
/// Node der Kette gerade ungültig ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanError {
    NotFlying,
    Burning,
    NoEngine,
    InPast,
    BeyondPrediction,
    /// Brennbeginn läge in der Vergangenheit, und es gibt keinen nächsten
    /// Umlauf ohne Luft oder Boden dazwischen.
    TooLate,
    /// Das Brennen begänne vor dem Ende des vorherigen, und es gibt keinen
    /// nächsten Umlauf ohne Luft oder Boden dazwischen.
    Overlaps,
    /// Das Schiff ist zu diesem Zeitpunkt schon am Boden.
    OnGround,
    /// Ein früherer Node der Kette ist ungültig.
    AfterInvalid,
    NoNode,
    /// Ein Start geht nur am Boden (und nicht als Wrack).
    NotLanded,
    /// Der Schub ist kleiner als das Gewicht.
    TooWeak,
    /// Das Schiff fliegt gerade zum Andocken an.
    Docking,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PlanError::NotFlying => "Schiff ist nicht im Flug",
            PlanError::Burning => "Schiff brennt gerade",
            PlanError::NoEngine => "Schiff hat kein Triebwerk",
            PlanError::InPast => "Zeitpunkt liegt in der Vergangenheit",
            PlanError::BeyondPrediction => "Zeitpunkt liegt hinter der Vorausschau",
            PlanError::TooLate => "Zu spät zum Brennen, und kein nächster Umlauf",
            PlanError::Overlaps => {
                "Brennen begänne vor dem Ende des vorherigen, und kein nächster Umlauf"
            }
            PlanError::OnGround => "Bahn endet vorher am Boden",
            PlanError::AfterInvalid => "Ein Node davor ist ungültig",
            PlanError::NotLanded => "Starten geht nur am Boden",
            PlanError::TooWeak => "Schub reicht nicht zum Abheben",
            PlanError::NoNode => "Kein Node geplant",
            PlanError::Docking => "Schiff fliegt gerade zum Andocken an",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::VesselParams;

    const PARAMS: VesselParams = VesselParams {
        dry_mass: 400.0,
        drag_area: 2.0,
        thrust: 2000.0,
        exhaust_velocity: 3000.0,
        fuel_capacity: 200.0,
        parachute_area: 0.0,
    };

    #[test]
    fn rocket_equation_holds() {
        let burn = Burn::centered(1000.0, 600.0, &PARAMS, DVec3::X, 500.0);
        assert!((burn.delta_v() - 500.0).abs() < 1e-9);
        assert!((burn.end_mass() - 600.0 * (-500.0f64 / 3000.0).exp()).abs() < 1e-9);
        // Mittig um den Node.
        assert!(((burn.start + burn.end) / 2.0 - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn empty_tank_limits_delta_v() {
        let burn = Burn::centered(1000.0, 600.0, &PARAMS, DVec3::X, 5000.0);
        assert!((burn.end_mass() - 400.0).abs() < 1e-9);
        let available = 3000.0 * (600.0f64 / 400.0).ln();
        assert!((burn.delta_v() - available).abs() < 1e-9);
    }

    #[test]
    fn frame_is_orthonormal_and_radial_points_outward() {
        let s = StateVector {
            position: DVec3::new(700_000.0, 0.0, 0.0),
            velocity: DVec3::new(100.0, 2_200.0, 300.0),
        };
        let [p, n, r] = node_frame(&s);
        for v in [p, n, r] {
            assert!((v.length() - 1.0).abs() < 1e-12);
        }
        assert!(p.dot(n).abs() < 1e-12 && p.dot(r).abs() < 1e-12 && n.dot(r).abs() < 1e-12);
        assert!(r.dot(s.position) > 0.0);
    }
}
