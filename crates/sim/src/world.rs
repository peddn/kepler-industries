//! Der gesamte Spielzustand. Er ist zugleich der Spielstand: alles hier ist
//! serialisierbar, und es gibt keinen Zustand außerhalb.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::kepler::{self, StateVector};
use crate::landing::{self, Crash, LandingZone};
use crate::launch::Launch;
use crate::log::{LOG_LIMIT, LogEntry, LogKind};
use crate::maneuver::{ManeuverNode, PlanError};
use crate::rendezvous::{self, Approach, PlaneCrossing};
use crate::trajectory::{Segment, Trajectory};
use serde::{Deserialize, Serialize};

/// Version des Spielstand-Formats. Bei inkompatiblen Änderungen erhöhen.
pub const SAVE_VERSION: u32 = 6;

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
    /// Fassungsvermögen des Tanks in kg.
    pub fuel_capacity: f64,
    /// Widerstandsbeiwert × Fläche des offenen Fallschirms in m² (0 = keiner).
    pub parachute_area: f64,
}

impl VesselParams {
    /// Δv mit vollem Tank in m/s.
    pub fn full_delta_v(&self) -> f64 {
        self.exhaust_velocity * ((self.dry_mass + self.fuel_capacity) / self.dry_mass).ln()
    }
}

/// Wofür ein Teil des Δv geplant ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetItem {
    Launch,
    /// Node der Kette (Stelle in der Kette).
    Node(usize),
    /// Bremsen beim Aufsetzen.
    Landing,
}

/// Vorausgesagter Bodenkontakt eines Schiffs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touchdown {
    pub time: f64,
    pub body: BodyId,
    pub point: SurfacePoint,
    /// Geschwindigkeit relativ zum Boden kurz vor dem Aufsetzen (m/s).
    pub speed: f64,
    /// Δv fürs Bremsen, oder warum es ein Absturz wird.
    pub braking: Result<f64, Crash>,
    /// Zielgebiet, in dem der Punkt liegt.
    pub zone: Option<usize>,
    /// Nächstes Zielgebiet auf dem Körper und Abstand zum Mittelpunkt (m).
    pub nearest: Option<(usize, f64)>,
}

/// Wie sich das Δv eines Schiffs auf die geplanten Brennvorgänge verteilt.
/// Die Teile gehen exakt auf: Bei fester Ausströmgeschwindigkeit addiert sich
/// das Δv der einzelnen Brennvorgänge zum Δv des Tanks.
#[derive(Clone, Debug, PartialEq)]
pub struct DeltaVBudget {
    /// Δv mit vollem Tank.
    pub full: f64,
    /// Δv jetzt (Treibstoff an Bord).
    pub available: f64,
    /// Geplante Brennvorgänge in Reihenfolge, je mit dem Δv, das sie noch
    /// verbrauchen (ein laufendes Brennen nur mit seinem Rest).
    pub burns: Vec<(BudgetItem, f64)>,
    /// Was danach im Tank bleibt.
    pub rest: f64,
    /// Was die Kette mehr verlangt, als im Tank ist.
    pub missing: f64,
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
    /// Ziel für ein Rendezvous (ein anderes Schiff). Fehlt in älteren
    /// Spielständen: keins.
    #[serde(default)]
    pub target: Option<usize>,
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
    /// Zielgebiete zum Landen.
    pub zones: Vec<LandingZone>,
    /// Was passiert ist, nach Zeit geordnet (höchstens `LOG_LIMIT`
    /// Einträge). Fehlt in älteren Spielständen: leer.
    #[serde(default)]
    pub log: Vec<LogEntry>,
}

impl World {
    pub fn new(bodies: Vec<Body>) -> Self {
        Self {
            save_version: SAVE_VERSION,
            time: 0.0,
            bodies,
            vessels: Vec::new(),
            bases: Vec::new(),
            zones: Vec::new(),
            log: Vec::new(),
        }
    }

    /// Nächstes Zielgebiet auf dem Körper und Abstand zum Mittelpunkt (m).
    pub fn nearest_zone(&self, body: BodyId, point: &SurfacePoint) -> Option<(usize, f64)> {
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.body == body)
            .map(|(k, z)| (k, z.distance(&self.bodies[body], point)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Das Zielgebiet, in dem der Punkt liegt.
    pub fn zone_at(&self, body: BodyId, point: &SurfacePoint) -> Option<usize> {
        self.nearest_zone(body, point)
            .filter(|&(k, d)| d <= self.zones[k].radius)
            .map(|(k, _)| k)
    }

    /// Vorausgesagter Bodenkontakt (nur in der Zukunft): wann, wo, wie schnell,
    /// ob das Schiff aufsetzt, und das Zielgebiet.
    pub fn touchdown(&self, vessel: usize) -> Option<Touchdown> {
        let v = &self.vessels[vessel];
        let segment = v.trajectory.segments().find(|s| {
            s.end_event() == Some(crate::trajectory::Event::GroundContact) && s.end() > self.time
        })?;
        let (body_id, time) = (segment.body(), segment.end());
        let body = &self.bodies[body_id];
        let state = segment.state_at(&self.bodies, time);
        let mass = segment.mass_at(time);
        let (point, _) = body.inertial_to_surface(state.position, time);
        Some(Touchdown {
            time,
            body: body_id,
            point,
            speed: landing::ground_speed(body, &state),
            braking: landing::braking(body, &v.params, &state, mass),
            zone: self.zone_at(body_id, &point),
            nearest: self.nearest_zone(body_id, &point),
        })
    }

    /// Stellt ein Schiff mit `fuel` kg Treibstoff auf den Boden, z. B. an
    /// eine Basis.
    pub fn spawn_landed(
        &mut self,
        name: &str,
        params: VesselParams,
        fuel: f64,
        body: BodyId,
        point: SurfacePoint,
    ) -> usize {
        assert!(
            fuel <= params.fuel_capacity,
            "{name}: mehr Treibstoff als Tank"
        );
        let mass = params.dry_mass + fuel;
        self.vessels.push(Vessel {
            name: name.to_owned(),
            params,
            trajectory: Trajectory::new_landed(body, self.time, point, mass),
            halt_warp_at_burn: false,
            target: None,
        });
        self.vessels.len() - 1
    }

    /// Plant oder ändert den Start eines gelandeten Schiffs.
    pub fn set_launch(&mut self, vessel: usize, launch: Launch) -> Result<(), PlanError> {
        let v = &mut self.vessels[vessel];
        v.trajectory
            .set_launch(&self.bodies, &v.params, self.time, launch)
    }

    /// Verschiebt den Start um `dt` s; die ganze Kette wandert mit.
    pub fn shift_launch(&mut self, vessel: usize, dt: f64) -> Result<(), PlanError> {
        let v = &mut self.vessels[vessel];
        v.trajectory
            .shift_launch(&self.bodies, &v.params, self.time, dt)
    }

    /// Sagt den geplanten Start ab.
    pub fn clear_launch(&mut self, vessel: usize) -> Result<(), PlanError> {
        let v = &mut self.vessels[vessel];
        v.trajectory
            .clear_launch(&self.bodies, &v.params, self.time)
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
        assert!(
            fuel <= params.fuel_capacity,
            "{name}: mehr Treibstoff als Tank"
        );
        let mass = params.dry_mass + fuel;
        let trajectory =
            Trajectory::new_flying(&self.bodies, &params, body, self.time, state, mass);
        self.vessels.push(Vessel {
            name: name.to_owned(),
            params,
            trajectory,
            halt_warp_at_burn: false,
            target: None,
        });
        self.vessels.len() - 1
    }

    /// Lässt die Zeit um `dt` Sekunden voranschreiten.
    pub fn advance(&mut self, dt: f64) {
        self.advance_to(self.time + dt);
    }

    /// Lässt die Zeit bis `t` voranschreiten. Das Ergebnis hängt nur von `t`
    /// ab, nicht davon, in wie vielen Schritten man dorthin kommt.
    ///
    /// Was dabei passiert, kommt ins Protokoll (`log`), geordnet nach Zeit und
    /// bei gleicher Zeit nach Schiff.
    pub fn advance_to(&mut self, t: f64) {
        self.time = t;
        let mut entries = Vec::new();
        for (i, vessel) in self.vessels.iter_mut().enumerate() {
            let happened = vessel
                .trajectory
                .advance_to(&self.bodies, t, &vessel.params);
            entries.extend(happened.into_iter().map(|(time, kind)| LogEntry {
                time,
                vessel: i,
                kind,
            }));
        }
        entries.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.vessel.cmp(&b.vessel)));
        for entry in &mut entries {
            // Zielgebiete trägt erst die Welt ein (die Zeitleiste kennt sie nicht).
            if let LogKind::Landed { point, zone } | LogKind::Crashed { point, zone, .. } =
                &mut entry.kind
            {
                let body = self.vessels[entry.vessel].trajectory.current().body();
                *zone = self.zone_at(body, point);
            }
        }
        self.log.extend(entries);
        if self.log.len() > LOG_LIMIT {
            self.log.drain(..self.log.len() - LOG_LIMIT);
        }
    }

    /// Fügt einen Node in die Manöverkette eines Schiffs ein und liefert seine
    /// Stelle in der Kette.
    pub fn add_node(&mut self, vessel: usize, node: ManeuverNode) -> Result<usize, PlanError> {
        let v = &mut self.vessels[vessel];
        v.trajectory
            .add_node(&self.bodies, &v.params, self.time, node)
    }

    /// Ändert den Node `index` der Kette eines Schiffs.
    pub fn set_node(
        &mut self,
        vessel: usize,
        index: usize,
        node: ManeuverNode,
    ) -> Result<(), PlanError> {
        let v = &mut self.vessels[vessel];
        v.trajectory
            .set_node(&self.bodies, &v.params, self.time, index, node)
    }

    /// Löscht den Node `index` der Kette eines Schiffs.
    pub fn remove_node(&mut self, vessel: usize, index: usize) -> Result<(), PlanError> {
        let v = &mut self.vessels[vessel];
        v.trajectory
            .remove_node(&self.bodies, &v.params, self.time, index)
    }

    /// Schaltet die Kette eines Schiffs scharf (oder ab). Nur eine scharfe
    /// Kette wird ausgeführt.
    pub fn set_armed(&mut self, vessel: usize, armed: bool) -> Result<(), PlanError> {
        self.vessels[vessel].trajectory.set_armed(armed)
    }

    /// Die Zeitleiste eines Schiffs nur mit den ersten `count` Nodes.
    pub fn trajectory_with_first_nodes(&self, vessel: usize, count: usize) -> Trajectory {
        let v = &self.vessels[vessel];
        v.trajectory
            .with_first_nodes(&self.bodies, &v.params, self.time, count)
    }

    /// Volle Umläufe vor dem Node `index` eines Schiffs: ab dem Ende des
    /// vorherigen Brennens (auch des Aufstiegs), sonst ab jetzt. `None` für
    /// ungültige Nodes.
    pub fn revolutions_before_node(&self, vessel: usize, index: usize) -> Option<u32> {
        let trajectory = &self.vessels[vessel].trajectory;
        let plan = trajectory.nodes().get(index)?.plan.ok()?;
        let from = match index {
            0 => trajectory.launch().map_or(self.time, |l| l.burn.end),
            _ => trajectory.nodes()[index - 1].plan.ok()?.burn.end,
        };
        Some(trajectory.revolutions(&self.bodies, from.max(self.time), plan.node.time))
    }

    /// Frühester Brennbeginn nach jetzt unter den Schiffen, bei denen der
    /// Zeitraffer dafür anhalten soll.
    pub fn next_halting_burn(&self) -> Option<f64> {
        self.vessels
            .iter()
            .filter(|v| v.halt_warp_at_burn && v.trajectory.armed())
            .filter_map(|v| v.trajectory.burns().first().map(|b| b.start))
            .filter(|&start| start > self.time)
            .min_by(f64::total_cmp)
    }

    /// Setzt das Ziel eines Schiffs: ein anderes Schiff oder keins.
    pub fn set_target(&mut self, vessel: usize, target: Option<usize>) {
        assert!(
            target.is_none_or(|t| t != vessel && t < self.vessels.len()),
            "ungültiges Ziel {target:?} für Schiff {vessel}"
        );
        self.vessels[vessel].target = target;
    }

    /// Engste Annäherung eines Schiffs an sein Ziel (siehe
    /// `rendezvous::approach`).
    pub fn approach(&self, vessel: usize) -> Option<Approach> {
        let v = &self.vessels[vessel];
        let target = &self.vessels[v.target?];
        rendezvous::approach(&self.bodies, self.time, &v.trajectory, &target.trajectory)
    }

    /// Lage der Bahn eines Schiffs zur Bahnebene seines Ziels (siehe
    /// `rendezvous::plane_crossing`).
    pub fn plane_to_target(&self, vessel: usize) -> Option<PlaneCrossing> {
        let v = &self.vessels[vessel];
        let target = &self.vessels[v.target?];
        rendezvous::plane_crossing(&self.bodies, self.time, &v.trajectory, &target.trajectory)
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

    /// Δv-Haushalt eines Schiffs: voller Tank, jetzt, geplante Brennvorgänge
    /// (Start und gültige Nodes), Rest und was fehlt.
    pub fn delta_v_budget(&self, index: usize) -> DeltaVBudget {
        let vessel = &self.vessels[index];
        let params = &vessel.params;
        let trajectory = &vessel.trajectory;
        let ve = params.exhaust_velocity;
        let now = self.time;
        let mass = self.vessel_mass(index);

        let items = trajectory
            .launch()
            .map(|l| (BudgetItem::Launch, l.burn, None))
            .into_iter()
            .chain(
                trajectory
                    .plans()
                    .enumerate()
                    .map(|(k, p)| (BudgetItem::Node(k), p.burn, Some(p.node.delta_v.length()))),
            );
        let mut burns = Vec::new();
        let mut missing = 0.0;
        let mut end_mass = mass;
        for (item, burn, wanted) in items {
            if burn.end <= now {
                continue;
            }
            // Ab jetzt bzw. ab Brennbeginn bis Brennende.
            let from = burn.mass_at(now.max(burn.start));
            let delta_v = ve * (from / burn.end_mass()).ln();
            if let Some(wanted) = wanted {
                let done = ve * (burn.start_mass / from).ln();
                missing += (wanted - done - delta_v).max(0.0);
            }
            end_mass = burn.end_mass();
            burns.push((item, delta_v));
        }
        let mut rest = ve * (end_mass / params.dry_mass).ln();
        // Bremsen beim Aufsetzen; reicht der Rest nicht, fehlt die Differenz.
        if let Some(touchdown) = self.touchdown(index) {
            match touchdown.braking {
                Ok(delta_v) if delta_v > 0.0 => {
                    burns.push((BudgetItem::Landing, delta_v));
                    rest -= delta_v;
                }
                Err(Crash::NoFuel { available }) => {
                    burns.push((BudgetItem::Landing, available));
                    missing += touchdown.speed - available;
                    rest = 0.0;
                }
                _ => {}
            }
        }
        DeltaVBudget {
            full: params.full_delta_v(),
            available: ve * (mass / params.dry_mass).ln(),
            burns,
            rest: rest.max(0.0),
            missing,
        }
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
