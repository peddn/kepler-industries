//! Der gesamte Spielzustand. Er ist zugleich der Spielstand: alles hier ist
//! serialisierbar, und es gibt keinen Zustand außerhalb.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::kepler::{self, StateVector};
use crate::maneuver::{ManeuverNode, PlanError};
use crate::trajectory::{Segment, Trajectory};
use serde::{Deserialize, Serialize};

/// Version des Spielstand-Formats. Bei inkompatiblen Änderungen erhöhen.
pub const SAVE_VERSION: u32 = 2;

/// Feste Werte eines Schiffs. Später kommen sie aus Vorlagen (Vessel-Klassen),
/// Upgrades ändern nur diese Werte.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VesselParams {
    /// Masse ohne Treibstoff in kg.
    pub dry_mass: f64,
    /// Widerstandsbeiwert × Querschnittsfläche in m².
    pub drag_area: f64,
    /// Schub in N (0 = kein Triebwerk).
    pub thrust: f64,
    /// Ausströmgeschwindigkeit in m/s (spezifischer Impuls × g₀).
    pub exhaust_velocity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vessel {
    pub name: String,
    pub params: VesselParams,
    pub trajectory: Trajectory,
    /// Zeitraffer anhalten, wenn dieses Schiff zu brennen beginnt (vom
    /// Spieler pro Schiff gesetzt). Fehlt in älteren Spielständen: dann aus.
    #[serde(default)]
    pub halt_warp_at_burn: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Base {
    pub name: String,
    pub body: BodyId,
    pub location: SurfacePoint,
}

/// Was die Anzeige über ein Schiff wissen muss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VesselStatus {
    /// Freiflug. Höhen in m über dem Boden.
    Orbit {
        periapsis_altitude: f64,
        apoapsis_altitude: Option<f64>,
        /// Die Bahn führt in die Atmosphäre oder auf den Boden.
        descending: bool,
    },
    /// Freiflug durch die Atmosphäre.
    Atmosphere {
        altitude: f64,
    },
    /// Das Triebwerk läuft noch `remaining` Sekunden.
    Burning {
        remaining: f64,
    },
    Landed {
        point: SurfacePoint,
    },
    Wreck {
        point: SurfacePoint,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub save_version: u32,
    /// Spielzeit in s seit Spielbeginn.
    pub time: f64,
    pub bodies: Vec<Body>,
    pub vessels: Vec<Vessel>,
    pub bases: Vec<Base>,
}

impl World {
    pub fn new(bodies: Vec<Body>) -> Self {
        Self {
            save_version: SAVE_VERSION,
            time: 0.0,
            bodies,
            vessels: Vec::new(),
            bases: Vec::new(),
        }
    }

    /// Setzt ein Schiff mit dem gegebenen Zustand und `fuel` kg Treibstoff zur
    /// aktuellen Zeit aus.
    pub fn spawn_vessel(
        &mut self,
        name: &str,
        params: VesselParams,
        fuel: f64,
        body: BodyId,
        state: StateVector,
    ) -> usize {
        let mass = params.dry_mass + fuel;
        let trajectory =
            Trajectory::new_flying(&self.bodies, &params, body, self.time, state, mass);
        self.vessels.push(Vessel {
            name: name.to_owned(),
            params,
            trajectory,
            halt_warp_at_burn: false,
        });
        self.vessels.len() - 1
    }

    /// Lässt die Zeit um `dt` Sekunden voranschreiten.
    pub fn advance(&mut self, dt: f64) {
        self.advance_to(self.time + dt);
    }

    /// Lässt die Zeit bis `t` voranschreiten. Das Ergebnis hängt nur von `t`
    /// ab, nicht davon, in wie vielen Schritten man dorthin kommt.
    pub fn advance_to(&mut self, t: f64) {
        self.time = t;
        for vessel in &mut self.vessels {
            vessel
                .trajectory
                .advance_to(&self.bodies, t, &vessel.params);
        }
    }

    /// Setzt oder ändert den Manöver-Node eines Schiffs.
    pub fn set_node(&mut self, index: usize, node: ManeuverNode) -> Result<(), PlanError> {
        let vessel = &mut self.vessels[index];
        vessel
            .trajectory
            .set_node(&self.bodies, &vessel.params, self.time, node)
    }

    /// Frühester Brennbeginn nach jetzt unter den Schiffen, bei denen der
    /// Zeitraffer dafür anhalten soll.
    pub fn next_halting_burn(&self) -> Option<f64> {
        self.vessels
            .iter()
            .filter(|v| v.halt_warp_at_burn)
            .filter_map(|v| v.trajectory.plan().map(|p| p.burn.start))
            .filter(|&start| start > self.time)
            .min_by(f64::total_cmp)
    }

    pub fn clear_node(&mut self, index: usize) -> Result<(), PlanError> {
        let vessel = &mut self.vessels[index];
        vessel.trajectory.clear_node(&self.bodies, &vessel.params)
    }

    /// Körper und Zustand eines Schiffs zur aktuellen Zeit.
    pub fn vessel_state(&self, index: usize) -> (BodyId, StateVector) {
        self.vessels[index]
            .trajectory
            .state_at(&self.bodies, self.time)
    }

    /// Aktuelle Masse in kg.
    pub fn vessel_mass(&self, index: usize) -> f64 {
        self.vessels[index].trajectory.current().mass_at(self.time)
    }

    /// Verbleibender Treibstoff in kg.
    pub fn vessel_fuel(&self, index: usize) -> f64 {
        self.vessel_mass(index) - self.vessels[index].params.dry_mass
    }

    /// Verbleibendes Δv in m/s (Raketengleichung).
    pub fn vessel_delta_v(&self, index: usize) -> f64 {
        let params = &self.vessels[index].params;
        params.exhaust_velocity * (self.vessel_mass(index) / params.dry_mass).ln()
    }

    pub fn vessel_status(&self, index: usize) -> VesselStatus {
        let segment = self.vessels[index].trajectory.current();
        let body = &self.bodies[segment.body()];
        match segment {
            Segment::Surface { point, wreck, .. } => {
                if *wreck {
                    VesselStatus::Wreck { point: *point }
                } else {
                    VesselStatus::Landed { point: *point }
                }
            }
            Segment::Integrated {
                burn: Some(burn), ..
            } => VesselStatus::Burning {
                remaining: burn.end - self.time,
            },
            Segment::Integrated { .. } => {
                let state = segment.state_at(&self.bodies, self.time);
                VesselStatus::Atmosphere {
                    altitude: state.position.length() - body.radius,
                }
            }
            Segment::Kepler { state, end, .. } => {
                let info = kepler::orbit_info(body.gm, state);
                VesselStatus::Orbit {
                    periapsis_altitude: info.periapsis - body.radius,
                    apoapsis_altitude: info.apoapsis.map(|r| r - body.radius),
                    descending: end.is_some_and(|(_, e)| e != crate::trajectory::Event::BurnStart),
                }
            }
        }
    }
}
