//! Der gesamte Spielzustand. Er ist zugleich der Spielstand: alles hier ist
//! serialisierbar, und es gibt keinen Zustand außerhalb.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::kepler::{self, StateVector};
use crate::trajectory::{Segment, Trajectory};
use serde::{Deserialize, Serialize};

/// Version des Spielstand-Formats. Bei inkompatiblen Änderungen erhöhen.
pub const SAVE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vessel {
    pub name: String,
    /// Masse in kg.
    pub mass: f64,
    /// Widerstandsbeiwert × Querschnittsfläche in m².
    pub drag_area: f64,
    pub trajectory: Trajectory,
}

impl Vessel {
    pub fn drag_per_mass(&self) -> f64 {
        self.drag_area / self.mass
    }
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
    /// In der Atmosphäre.
    Atmosphere {
        altitude: f64,
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

    /// Setzt ein Schiff mit dem gegebenen Zustand zur aktuellen Zeit aus.
    pub fn spawn_vessel(
        &mut self,
        name: &str,
        mass: f64,
        drag_area: f64,
        body: BodyId,
        state: StateVector,
    ) -> usize {
        let trajectory =
            Trajectory::new_flying(&self.bodies, body, self.time, state, drag_area / mass);
        self.vessels.push(Vessel {
            name: name.to_owned(),
            mass,
            drag_area,
            trajectory,
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
            let k = vessel.drag_per_mass();
            vessel.trajectory.advance_to(&self.bodies, t, k);
        }
    }

    /// Körper und Zustand eines Schiffs zur aktuellen Zeit.
    pub fn vessel_state(&self, index: usize) -> (BodyId, StateVector) {
        self.vessels[index]
            .trajectory
            .state_at(&self.bodies, self.time)
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
            Segment::Integrated { .. } => {
                let state = segment.state_at(&self.bodies, self.time);
                VesselStatus::Atmosphere {
                    altitude: state.position.length() - body.radius,
                }
            }
            Segment::Kepler { end, .. } => {
                let state = segment.state_at(&self.bodies, self.time);
                let info = kepler::orbit_info(body.gm, &state);
                VesselStatus::Orbit {
                    periapsis_altitude: info.periapsis - body.radius,
                    apoapsis_altitude: info.apoapsis.map(|r| r - body.radius),
                    descending: end.is_some(),
                }
            }
        }
    }
}
