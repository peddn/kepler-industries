//! Der gesamte Spielzustand. Er ist zugleich der Spielstand: alles hier ist
//! serialisierbar, und es gibt keinen Zustand außerhalb.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::booster::{self, BoosterPlan};
use crate::docking::{
    self, DOCKING_RANGE, DockError, DockRequest, SEPARATION_SPEED, UndockError, UndockFuel,
};
use crate::kepler::{self, StateVector};
use crate::landing::{self, Crash, LandingZone};
use crate::launch::Launch;
use crate::log::{LOG_LIMIT, LogEntry, LogKind};
use crate::maneuver::{self, Burn, ManeuverNode, PlanError};
use crate::rendezvous::{self, Approach, PlaneCrossing};
use crate::trajectory::{Event, Segment, Trajectory};
use crate::transfer::{self, BodyApproach, Window};
use serde::{Deserialize, Serialize};

/// Version des Spielstand-Formats. Bei inkompatiblen Änderungen erhöhen.
pub const SAVE_VERSION: u32 = 12;

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
    /// Fassungsvermögen des Frachtraums in kg (0 = keiner).
    pub cargo_capacity: f64,
    /// Basisschiff: Es gründet dort, wo es landet, eine Basis und wird ihr
    /// Fundament (siehe `base`).
    pub base_kit: bool,
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
    /// Landebrennen vor dem Aufsetzen.
    Landing,
    /// Andocken: Angleichen und Anflug.
    Docking,
}

/// Vorausgesagter Bodenkontakt eines Schiffs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touchdown {
    pub time: f64,
    pub body: BodyId,
    pub point: SurfacePoint,
    /// Geschwindigkeit relativ zum Boden beim Aufsetzen (m/s).
    pub speed: f64,
    /// Das Landebrennen davor (keins, wenn das Schiff ohnehin langsam genug
    /// ist), oder warum es ein Absturz wird.
    pub landing: Result<Option<LandingBurn>, Crash>,
    /// Zielgebiet, in dem der Punkt liegt.
    pub zone: Option<usize>,
    /// Nächstes Zielgebiet auf dem Körper und Abstand zum Mittelpunkt (m).
    pub nearest: Option<(usize, f64)>,
}

/// Das Landebrennen vor einem Bodenkontakt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandingBurn {
    pub burn: Burn,
    /// Höhe beim Zünden (m).
    pub altitude: f64,
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
    /// Ziel für einen Flug: ein Himmelskörper. Höchstens eines der beiden
    /// Ziele ist gesetzt (`World::set_target`, `World::set_target_body`).
    pub target_body: Option<BodyId>,
    /// Fracht an Bord. Sie zählt beim Fliegen als Masse (`World::params`).
    pub cargo: Cargo,
    /// Dieses Schiff ist das Fundament der Basis mit dieser Nummer und
    /// fliegt nicht mehr.
    pub foundation_of: Option<usize>,
    /// Angedockt an dieses Schiff (das Hauptschiff des Verbunds). Dann zählen
    /// Lage und Kette des Hauptschiffs, die eigenen Werte gehen in dessen
    /// wirksame Werte ein (`World::params`). Fehlt in älteren Spielständen:
    /// frei.
    #[serde(default)]
    pub docked_to: Option<usize>,
}

/// Fracht an Bord in kg. Beide Stoffe teilen sich den Frachtraum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cargo {
    /// Bauteile: von der Startbasis zu den Baustellen.
    pub parts: f64,
    /// Zorit: von den Minen zur Startbasis.
    pub zorit: f64,
}

impl Cargo {
    pub fn total(&self) -> f64 {
        self.parts + self.zorit
    }
}

/// Was eine Basis ist. Jede stellt höchstens einen Stoff her.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaseKind {
    /// Die Startbasis auf dem Heimatplaneten: gibt Bauteile und Treibstoff
    /// aus und nimmt Zorit an.
    Depot,
    /// Stellt Treibstoff her; Schiffe tanken dort.
    FuelStation,
    /// Baut Zorit ab; Schiffe laden es dort.
    Mine,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Base {
    pub name: String,
    pub body: BodyId,
    pub location: SurfacePoint,
    pub kind: BaseKind,
    /// Bauteile, die zur Fertigstellung nötig sind, und was davon schon
    /// abgeladen ist (kg). Die Startbasis braucht nichts.
    pub required: f64,
    pub delivered: f64,
    /// Lager einer fertigen Tankstelle oder Mine für ihren Stoff; `None` bei
    /// der Startbasis und auf einer Baustelle.
    pub store: Option<Store>,
}

impl Base {
    /// Fertig gebaut (sonst eine Baustelle)?
    pub fn complete(&self) -> bool {
        self.delivered >= self.required
    }
}

/// Lager einer Basis: der Stand zu einem Zeitpunkt. Seitdem kommt gleichmäßig
/// dazu, bis das Lager voll ist (`World::base_stock`): für jeden Zeitpunkt
/// direkt berechnet, und Warten füllt es nur einmal (Regel 14).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub amount: f64,
    pub since: f64,
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
    /// Im Anflug zum Andocken, noch `distance` m bis zum Kontakt.
    Approaching {
        distance: f64,
    },
    /// Teil des Verbunds um das Hauptschiff `host`.
    Docked {
        host: usize,
    },
}

/// Wirksame Werte eines Verbunds: Hauptschiff `host` und angedockte Teile.
/// Massen, Tanks, Widerstand und Fallschirme addieren sich; der Schub auch,
/// die Ausströmgeschwindigkeit ist nach Massenstrom gemittelt (alle
/// Triebwerke brennen zusammen).
pub fn combine(host: VesselParams, parts: impl IntoIterator<Item = VesselParams>) -> VesselParams {
    let mut total = host;
    let mut mass_flow = if host.exhaust_velocity > 0.0 {
        host.thrust / host.exhaust_velocity
    } else {
        0.0
    };
    for p in parts {
        total.dry_mass += p.dry_mass;
        total.drag_area += p.drag_area;
        total.thrust += p.thrust;
        total.fuel_capacity += p.fuel_capacity;
        total.parachute_area += p.parachute_area;
        total.cargo_capacity += p.cargo_capacity;
        if p.exhaust_velocity > 0.0 {
            mass_flow += p.thrust / p.exhaust_velocity;
        }
    }
    if mass_flow > 0.0 {
        total.exhaust_velocity = total.thrust / mass_flow;
    }
    total
}

/// Wo der Booster der Startbasis gerade ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoosterStatus {
    /// Steht an der Startbasis bereit.
    Ready,
    /// Für den geplanten Start dieses Schiffs vorgesehen.
    Reserved { ship: usize },
    /// Trägt gerade dieses Schiff nach oben.
    Ascending { ship: usize },
    /// Fliegt nach der Trennung zurück (oder steht nicht an der Startbasis).
    Returning,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub save_version: u32,
    /// Spielzeit in s seit Spielbeginn.
    pub time: f64,
    pub bodies: Vec<Body>,
    /// Der Heimatplanet: Seine Drehung zählt die Tage (diese eine Zeit gilt
    /// überall), die Kamera beginnt bei ihm.
    pub home: BodyId,
    pub vessels: Vec<Vessel>,
    pub bases: Vec<Base>,
    /// Zorit im Lager der Startbasis in kg (siehe `base`).
    pub zorit: f64,
    /// Zielgebiete zum Landen.
    pub zones: Vec<LandingZone>,
    /// Was passiert ist, nach Zeit geordnet (höchstens `LOG_LIMIT`
    /// Einträge). Fehlt in älteren Spielständen: leer.
    #[serde(default)]
    pub log: Vec<LogEntry>,
    /// Der Booster der Startbasis: Nummer seines Fahrzeugs (siehe `booster`).
    #[serde(default)]
    pub booster: Option<usize>,
}

impl World {
    pub fn new(bodies: Vec<Body>) -> Self {
        Self {
            save_version: SAVE_VERSION,
            time: 0.0,
            bodies,
            home: 0,
            vessels: Vec::new(),
            bases: Vec::new(),
            zorit: 0.0,
            zones: Vec::new(),
            log: Vec::new(),
            booster: None,
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
    /// mit welchem Landebrennen oder warum ein Absturz, und das Zielgebiet.
    pub fn touchdown(&self, vessel: usize) -> Option<Touchdown> {
        let trajectory = &self.vessels[self.host(vessel)].trajectory;
        let segment = trajectory
            .segments()
            .find(|s| s.end_event() == Some(Event::GroundContact) && s.end() > self.time)?;
        let (body_id, time) = (segment.body(), segment.end());
        let body = &self.bodies[body_id];
        let state = segment.state_at(&self.bodies, time);
        let (point, _) = body.inertial_to_surface(state.position, time);
        let speed = landing::ground_speed(body, &state);
        let landing = if speed > landing::SAFE_SPEED {
            Err(segment.crash().unwrap_or(Crash::TooLate))
        } else {
            Ok(trajectory.landing().map(|&burn| {
                let ignition = trajectory
                    .predicted_state(&self.bodies, burn.start)
                    .map_or(f64::NAN, |(_, s)| s.position.length());
                LandingBurn {
                    burn,
                    altitude: ignition - body.radius,
                }
            }))
        };
        Some(Touchdown {
            time,
            body: body_id,
            point,
            speed,
            landing,
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
            target_body: None,
            cargo: Cargo::default(),
            foundation_of: None,
            docked_to: None,
        });
        self.vessels.len() - 1
    }

    /// Plant oder ändert den Start eines gelandeten Schiffs.
    pub fn set_launch(&mut self, vessel: usize, launch: Launch) -> Result<(), PlanError> {
        if self.vessels[vessel].foundation_of.is_some() {
            return Err(PlanError::Foundation);
        }
        let booster = self.booster_for(vessel)?;
        self.command(vessel, |t, bodies, params, now| {
            t.set_launch(bodies, params, now, launch, booster)
        })
    }

    /// Verschiebt den Start um `dt` s; die ganze Kette wandert mit.
    pub fn shift_launch(&mut self, vessel: usize, dt: f64) -> Result<(), PlanError> {
        self.command(vessel, |t, bodies, params, now| {
            t.shift_launch(bodies, params, now, dt)
        })
    }

    /// Sagt den geplanten Start ab.
    pub fn clear_launch(&mut self, vessel: usize) -> Result<(), PlanError> {
        self.command(vessel, |t, bodies, params, now| {
            t.clear_launch(bodies, params, now)
        })
    }

    /// Ein Befehl an die Zeitleiste eines Schiffs (bei einem Verbund: des
    /// Hauptschiffs, mit dessen wirksamen Werten). Danach wird jedes geplante
    /// Andocken neu berechnet, denn es hängt von beiden Schiffen ab.
    fn command<T>(
        &mut self,
        vessel: usize,
        f: impl FnOnce(&mut Trajectory, &[Body], &VesselParams, f64) -> Result<T, PlanError>,
    ) -> Result<T, PlanError> {
        if self.booster == Some(vessel) {
            return Err(PlanError::Booster);
        }
        let host = self.host(vessel);
        let params = self.params(host);
        let result = f(
            &mut self.vessels[host].trajectory,
            &self.bodies,
            &params,
            self.time,
        );
        self.refresh_dockings();
        result
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
            target_body: None,
            cargo: Cargo::default(),
            foundation_of: None,
            docked_to: None,
        });
        self.vessels.len() - 1
    }

    /// Lässt die Zeit um `dt` Sekunden voranschreiten.
    pub fn advance(&mut self, dt: f64) {
        self.advance_to(self.time + dt);
    }

    /// Lässt die Zeit bis `t` voranschreiten. Das Ergebnis hängt nur von `t`
    /// ab, nicht davon, in wie vielen Schritten man dorthin kommt: Ein Kontakt
    /// beim Andocken wird immer genau zu seinem Zeitpunkt ausgeführt.
    ///
    /// Was dabei passiert, kommt ins Protokoll (`log`), geordnet nach Zeit und
    /// bei gleicher Zeit nach Schiff.
    pub fn advance_to(&mut self, t: f64) {
        loop {
            let free = || (0..self.vessels.len()).filter(|&i| self.vessels[i].docked_to.is_none());
            let contact = free()
                .filter_map(|i| {
                    let plan = self.vessels[i].trajectory.dock_plan()?;
                    (plan.contact <= t).then_some((plan.contact, i, None))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            // Trennung vom Booster bei Brennschluss des Aufstiegs.
            let separation = free()
                .filter_map(|i| {
                    let plan = self.vessels[i].trajectory.launch()?;
                    let booster = plan.booster?;
                    (plan.burn.end <= t).then_some((plan.burn.end, i, Some(booster)))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let next = [contact, separation]
                .into_iter()
                .flatten()
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            match next {
                Some((time, i, None)) => {
                    self.advance_all(time.max(self.time));
                    self.dock(i);
                }
                Some((time, i, Some(booster))) => {
                    self.advance_all(time.max(self.time));
                    // Nur wenn der Aufstieg wirklich geflogen ist: Eine nicht
                    // scharfe Kette ist stattdessen einen Tag weitergerückt.
                    if self.vessels[i].trajectory.launch().is_none() {
                        self.separate(i, &booster);
                    }
                }
                None => return self.advance_all(t),
            }
        }
    }

    /// Alle freien Schiffe bis `t` vorrücken (ohne Andocken).
    fn advance_all(&mut self, t: f64) {
        self.time = t;
        let params: Vec<VesselParams> = (0..self.vessels.len()).map(|i| self.params(i)).collect();
        let mut entries = Vec::new();
        for (i, vessel) in self.vessels.iter_mut().enumerate() {
            if vessel.docked_to.is_some() {
                continue;
            }
            let happened = vessel.trajectory.advance_to(&self.bodies, t, &params[i]);
            entries.extend(happened.into_iter().map(|(time, kind)| LogEntry {
                time,
                vessel: i,
                kind,
            }));
        }
        entries.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.vessel.cmp(&b.vessel)));
        for entry in &mut entries {
            // Zielgebiete trägt erst die Welt ein (die Zeitleiste kennt sie nicht).
            if let LogKind::Landed { point, zone, body }
            | LogKind::Crashed {
                point, zone, body, ..
            } = &mut entry.kind
            {
                *zone = self.zone_at(*body, point);
            }
        }
        self.push_log(entries);
    }

    /// Der Booster für einen Start von `vessel`: An der Startbasis startet
    /// jedes Schiff mit ihm (seine Werte), anderswo aus eigener Kraft
    /// (`None`). Fehler, wenn er nicht bereitsteht.
    fn booster_for(&self, vessel: usize) -> Result<Option<VesselParams>, PlanError> {
        let Some(booster) = self.booster else {
            return Ok(None);
        };
        if booster == vessel {
            return Err(PlanError::Booster);
        }
        let at_depot =
            |i: usize| matches!(self.base_of(i), Some(k) if self.bases[k].kind == BaseKind::Depot);
        if !at_depot(self.host(vessel)) {
            return Ok(None);
        }
        let reserved = (0..self.vessels.len()).any(|i| {
            i != self.host(vessel)
                && self.vessels[i]
                    .trajectory
                    .launch()
                    .is_some_and(|l| l.booster.is_some())
        });
        if reserved {
            return Err(PlanError::BoosterBusy);
        }
        if !at_depot(booster) {
            return Err(PlanError::BoosterAway);
        }
        Ok(Some(self.vessels[booster].params))
    }

    /// Zahl der Schiffe; der Booster zählt nicht dazu.
    pub fn ship_count(&self) -> usize {
        self.vessels.len() - usize::from(self.booster.is_some())
    }

    /// Wo der Booster gerade ist (für die Anzeige).
    pub fn booster_status(&self) -> Option<BoosterStatus> {
        let booster = self.booster?;
        let with = (0..self.vessels.len()).find_map(|i| {
            let plan = self.vessels[i].trajectory.launch()?;
            plan.booster?;
            Some((i, self.vessels[i].trajectory.current().is_burn()))
        });
        Some(match with {
            Some((ship, true)) => BoosterStatus::Ascending { ship },
            Some((ship, false)) => BoosterStatus::Reserved { ship },
            None => match self.vessels[booster].trajectory.current() {
                Segment::Surface { wreck: false, .. } if matches!(self.base_of(booster), Some(k) if self.bases[k].kind == BaseKind::Depot) => {
                    BoosterStatus::Ready
                }
                _ => BoosterStatus::Returning,
            },
        })
    }

    /// Trennung bei Brennschluss: Der Booster von `ship` fliegt ab jetzt
    /// selbst, mit dem beim Planen berechneten Boostback (siehe `booster`).
    fn separate(&mut self, ship: usize, plan: &BoosterPlan) {
        let Some(index) = self.booster else { return };
        let time = self.time;
        let (body, state) = self.vessel_state(ship);
        let flight = booster::flight(
            &self.bodies,
            &plan.params,
            body,
            time,
            state,
            plan.mass,
            plan.back.node,
        );
        // Ohne gültigen Boostback (kann nach dem Planen nicht mehr eintreten)
        // fällt er antriebslos zurück.
        self.vessels[index].trajectory = flight.unwrap_or_else(|| {
            Trajectory::new_flying(&self.bodies, &plan.params, body, time, state, plan.mass)
        });
        self.push_log([LogEntry {
            time,
            vessel: index,
            kind: LogKind::Separated { ship },
        }]);
    }

    fn push_log(&mut self, entries: impl IntoIterator<Item = LogEntry>) {
        self.log.extend(entries);
        if self.log.len() > LOG_LIMIT {
            self.log.drain(..self.log.len() - LOG_LIMIT);
        }
    }

    /// Hauptschiff des Verbunds, zu dem ein Schiff gehört (sonst es selbst).
    pub fn host(&self, vessel: usize) -> usize {
        self.vessels[vessel].docked_to.unwrap_or(vessel)
    }

    /// Die an `host` angedockten Schiffe.
    pub fn parts(&self, host: usize) -> Vec<usize> {
        (0..self.vessels.len())
            .filter(|&k| self.vessels[k].docked_to == Some(host))
            .collect()
    }

    /// Werte eines einzelnen Schiffs mit seiner Fracht: Sie zählt zur Masse
    /// ohne Treibstoff.
    fn loaded(&self, vessel: usize) -> VesselParams {
        let v = &self.vessels[vessel];
        VesselParams {
            dry_mass: v.params.dry_mass + v.cargo.total(),
            ..v.params
        }
    }

    /// Wirksame Werte: bei einem Hauptschiff samt angedockter Teile, mit
    /// Fracht (siehe `loaded`).
    pub fn params(&self, vessel: usize) -> VesselParams {
        combine(
            self.loaded(vessel),
            self.parts(vessel).into_iter().map(|k| self.loaded(k)),
        )
    }

    /// Andocken an das Ziel (genauer: an das Hauptschiff von dessen Verbund)
    /// als letzter Schritt der Kette einschalten oder zurücknehmen.
    pub fn set_docking(&mut self, vessel: usize, on: bool) -> Result<(), DockError> {
        let trajectory = &self.vessels[vessel].trajectory;
        if trajectory.docking_underway() || trajectory.current().is_burn() {
            return Err(DockError::Busy);
        }
        let docking = if on {
            let target = self.vessels[vessel].target.ok_or(DockError::NoTarget)?;
            Some((target, self.dock_request(vessel, target)))
        } else {
            None
        };
        let params = self.params(vessel);
        self.vessels[vessel]
            .trajectory
            .set_docking(&self.bodies, &params, self.time, docking);
        self.refresh_dockings();
        Ok(())
    }

    /// Rechnet jedes geplante Andocken neu (nach jeder Änderung an einer
    /// Kette: Annäherung und Zielbahn können sich verschoben haben). Zweimal,
    /// damit auch ein Ziel, dessen eigenes Andocken sich ändert, stimmt.
    fn refresh_dockings(&mut self) {
        for _ in 0..2 {
            for i in 0..self.vessels.len() {
                let v = &self.vessels[i];
                let Some(target) = v.trajectory.docking().map(|d| d.target) else {
                    continue;
                };
                if v.docked_to.is_some() || v.trajectory.docking_underway() {
                    continue;
                }
                let request = self.dock_request(i, target);
                if v.trajectory.docking().is_some_and(|d| d.request == request) {
                    continue;
                }
                let params = self.params(i);
                self.vessels[i].trajectory.set_docking(
                    &self.bodies,
                    &params,
                    self.time,
                    Some((target, request)),
                );
            }
        }
    }

    /// Was das Andocken von `vessel` an `target` braucht: die engste Annäherung
    /// (ohne das Andocken selbst gesucht), dort Δv = Geschwindigkeit des Ziels
    /// minus eigene, und die Bahn des Ziels, die ab dann stabil sein muss.
    fn dock_request(&self, vessel: usize, target: usize) -> Result<DockRequest, DockError> {
        let host = self.host(target);
        if host == self.host(vessel) {
            return Err(DockError::NoTarget);
        }
        let params = self.params(vessel);
        let base =
            self.vessels[vessel]
                .trajectory
                .without_docking(&self.bodies, &params, self.time);
        let goal = &self.vessels[host].trajectory;
        let approach = rendezvous::approach(&self.bodies, self.time, &base, goal)
            .ok_or(DockError::NoApproach)?;
        if approach.distance > DOCKING_RANGE {
            return Err(DockError::TooFar {
                distance: approach.distance,
            });
        }
        let Some(
            segment @ Segment::Kepler {
                start,
                state,
                end: None,
                ..
            },
        ) = goal.predicted_segment(approach.time)
        else {
            return Err(DockError::TargetNotStable);
        };
        let (_, own) = base
            .predicted_state(&self.bodies, approach.time)
            .ok_or(DockError::NoApproach)?;
        let relative = segment.state_at(&self.bodies, approach.time).velocity - own.velocity;
        let [prograde, normal, radial] = maneuver::node_frame(&own);
        Ok(DockRequest {
            node: ManeuverNode {
                time: approach.time,
                delta_v: glam::DVec3::new(
                    relative.dot(prograde),
                    relative.dot(normal),
                    relative.dot(radial),
                ),
            },
            target_epoch: *start,
            target_state: *state,
        })
    }

    /// Wie viel Treibstoff ein angedocktes Schiff beim Abdocken mitnehmen kann.
    pub fn undock_fuel(&self, part: usize) -> Result<UndockFuel, UndockError> {
        let host = self.vessels[part].docked_to.ok_or(UndockError::NotDocked)?;
        let total = self.params(host);
        let pool = (self.vessel_mass(host) - total.dry_mass).max(0.0);
        let capacity = self.vessels[part].params.fuel_capacity;
        let rest = total.fuel_capacity - capacity;
        let min = (pool - rest).max(0.0);
        let max = capacity.min(pool);
        let share = if total.fuel_capacity > 0.0 {
            pool * capacity / total.fuel_capacity
        } else {
            0.0
        };
        Ok(UndockFuel {
            min,
            max,
            suggested: share.clamp(min, max),
            pool,
        })
    }

    /// Δv (m/s) des abdockenden Schiffs und des verbleibenden Verbunds, wenn
    /// das Schiff `fuel` kg mitnimmt.
    pub fn undock_delta_v(&self, part: usize, fuel: f64) -> (f64, f64) {
        let host = self.host(part);
        let own = self.loaded(part);
        let rest = combine(
            self.loaded(host),
            self.parts(host)
                .into_iter()
                .filter(|&k| k != part)
                .map(|k| self.loaded(k)),
        );
        let pool = self.vessel_mass(host) - self.params(host).dry_mass;
        let delta_v = |p: &VesselParams, fuel: f64| {
            p.exhaust_velocity * ((p.dry_mass + fuel.max(0.0)) / p.dry_mass).ln()
        };
        (delta_v(&own, fuel), delta_v(&rest, pool - fuel))
    }

    /// Abdocken jetzt: `part` verlässt seinen Verbund mit `fuel` kg Treibstoff
    /// und einem Stoß von `SEPARATION_SPEED` gegen die Flugrichtung. Es fängt
    /// ohne Kette an, mit dem Hauptschiff als Ziel (zum Wiederandocken). Das
    /// Hauptschiff fliegt mit dem Rest weiter; seine Kette wird mit den neuen
    /// Werten neu berechnet.
    pub fn undock(&mut self, part: usize, fuel: f64) -> Result<(), UndockError> {
        let range = self.undock_fuel(part)?;
        let host = self.host(part);
        let trajectory = &self.vessels[host].trajectory;
        match trajectory.current() {
            Segment::Surface { .. } => return Err(UndockError::OnGround),
            segment if segment.is_burn() || trajectory.docking_underway() => {
                return Err(UndockError::Busy);
            }
            _ => {}
        }
        if !(range.min - 1e-9..=range.max + 1e-9).contains(&fuel) {
            return Err(UndockError::FuelOutOfRange {
                min: range.min,
                max: range.max,
            });
        }
        let fuel = fuel.clamp(range.min, range.max);
        let time = self.time;
        let (body, state) = self.vessel_state(host);
        let host_mass = self.vessel_mass(host);
        let own = self.loaded(part);
        let part_mass = own.dry_mass + fuel;

        self.vessels[part].docked_to = None;
        let [prograde, ..] = maneuver::node_frame(&state);
        let pushed = StateVector {
            position: state.position,
            velocity: state.velocity - prograde * SEPARATION_SPEED,
        };
        let v = &mut self.vessels[part];
        v.trajectory = Trajectory::new_flying(&self.bodies, &own, body, time, pushed, part_mass);
        v.target = Some(host);

        let params = self.params(host);
        self.vessels[host].trajectory.restart_flying(
            &self.bodies,
            &params,
            body,
            time,
            state,
            host_mass - part_mass,
        );
        self.push_log([LogEntry {
            time,
            vessel: part,
            kind: LogKind::Undocked { host, fuel },
        }]);
        self.refresh_dockings();
        Ok(())
    }

    /// Kontakt: `vessel` wird Teil des Verbunds um das Hauptschiff seines
    /// Ziels. Das Hauptschiff fliegt mit der Summe der Massen auf seiner Bahn
    /// weiter (sie plant ab dem Angleichen nichts, siehe `docking`).
    fn dock(&mut self, vessel: usize) {
        let target = self.vessels[vessel]
            .trajectory
            .docking()
            .expect("Andocken geplant")
            .target;
        let host = self.host(target);
        let time = self.time;
        let part_mass = self.vessels[vessel].trajectory.current().mass_at(time);
        let (body, state) = self.vessel_state(host);
        let host_mass = self.vessel_mass(host);
        // Das Schiff und alles, was schon an ihm hing, hängt jetzt am Hauptschiff.
        for k in 0..self.vessels.len() {
            if k == vessel || self.vessels[k].docked_to == Some(vessel) {
                self.vessels[k].docked_to = Some(host);
            }
        }
        self.vessels[vessel].target = None;
        self.vessels[vessel].trajectory.finish_docking();
        let params = self.params(host);
        self.vessels[host].trajectory = Trajectory::new_flying(
            &self.bodies,
            &params,
            body,
            time,
            state,
            host_mass + part_mass,
        );
        self.push_log([LogEntry {
            time,
            vessel,
            kind: LogKind::Docked { host },
        }]);
        self.refresh_dockings();
    }

    /// Fügt einen Node in die Manöverkette eines Schiffs ein und liefert seine
    /// Stelle in der Kette.
    pub fn add_node(&mut self, vessel: usize, node: ManeuverNode) -> Result<usize, PlanError> {
        self.command(vessel, |t, bodies, params, now| {
            t.add_node(bodies, params, now, node)
        })
    }

    /// Ändert den Node `index` der Kette eines Schiffs.
    pub fn set_node(
        &mut self,
        vessel: usize,
        index: usize,
        node: ManeuverNode,
    ) -> Result<(), PlanError> {
        self.command(vessel, |t, bodies, params, now| {
            t.set_node(bodies, params, now, index, node)
        })
    }

    /// Löscht den Node `index` der Kette eines Schiffs.
    pub fn remove_node(&mut self, vessel: usize, index: usize) -> Result<(), PlanError> {
        self.command(vessel, |t, bodies, params, now| {
            t.remove_node(bodies, params, now, index)
        })
    }

    /// Schaltet die Kette eines Schiffs scharf (oder ab). Nur eine scharfe
    /// Kette wird ausgeführt.
    pub fn set_armed(&mut self, vessel: usize, armed: bool) -> Result<(), PlanError> {
        self.vessels[vessel].trajectory.set_armed(armed)
    }

    /// Die Zeitleiste eines Schiffs nur mit den ersten `count` Nodes.
    pub fn trajectory_with_first_nodes(&self, vessel: usize, count: usize) -> Trajectory {
        let host = self.host(vessel);
        self.vessels[host].trajectory.with_first_nodes(
            &self.bodies,
            &self.params(host),
            self.time,
            count,
        )
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
    /// Zeitraffer dafür anhalten soll: das erste Brennen einer scharfen Kette
    /// oder das Landebrennen (das kommt immer).
    pub fn next_halting_burn(&self) -> Option<f64> {
        self.vessels
            .iter()
            .filter(|v| v.halt_warp_at_burn && v.docked_to.is_none())
            .flat_map(|v| {
                let chain = v
                    .trajectory
                    .armed()
                    .then(|| v.trajectory.burns().first().copied());
                [chain.flatten(), v.trajectory.landing().copied()]
            })
            .flatten()
            .map(|b| b.start)
            .filter(|&start| start > self.time)
            .min_by(f64::total_cmp)
    }

    /// Setzt das Ziel eines Schiffs: ein anderes Schiff oder keins.
    /// Ein geplantes Andocken an ein anderes Ziel wird zurückgenommen.
    pub fn set_target(&mut self, vessel: usize, target: Option<usize>) {
        assert!(
            target.is_none_or(|t| t != vessel && t < self.vessels.len()),
            "ungültiges Ziel {target:?} für Schiff {vessel}"
        );
        self.vessels[vessel].target = target;
        if target.is_some() {
            self.vessels[vessel].target_body = None;
        }
        let trajectory = &self.vessels[vessel].trajectory;
        if trajectory
            .docking()
            .is_some_and(|d| Some(d.target) != target)
            && !trajectory.docking_underway()
        {
            let params = self.params(vessel);
            self.vessels[vessel]
                .trajectory
                .set_docking(&self.bodies, &params, self.time, None);
            self.refresh_dockings();
        }
    }

    /// Setzt einen Himmelskörper als Ziel eines Schiffs (oder keinen); ein
    /// Schiff als Ziel entfällt dann.
    pub fn set_target_body(&mut self, vessel: usize, target: Option<BodyId>) {
        assert!(
            target.is_none_or(|b| b < self.bodies.len()),
            "ungültiger Zielkörper {target:?}"
        );
        if target.is_some() {
            self.set_target(vessel, None);
        }
        self.vessels[vessel].target_body = target;
    }

    /// Startfenster zum Zielkörper eines Schiffs (siehe `transfer::window`).
    pub fn transfer_window(&self, vessel: usize) -> Option<Window> {
        let target = self.vessels[vessel].target_body?;
        let own = &self.vessels[self.host(vessel)].trajectory;
        transfer::window(&self.bodies, self.time, own, target)
    }

    /// Engste Annäherung an den Zielkörper eines Schiffs (siehe
    /// `transfer::approach`).
    pub fn body_approach(&self, vessel: usize) -> Option<BodyApproach> {
        let target = self.vessels[vessel].target_body?;
        let own = &self.vessels[self.host(vessel)].trajectory;
        transfer::approach(&self.bodies, self.time, own, target)
    }

    /// Engste Annäherung eines Schiffs an sein Ziel (siehe
    /// `rendezvous::approach`).
    pub fn approach(&self, vessel: usize) -> Option<Approach> {
        let host = self.host(self.vessels[vessel].target?);
        if host == self.host(vessel) {
            return None;
        }
        rendezvous::approach(
            &self.bodies,
            self.time,
            &self.vessels[self.host(vessel)].trajectory,
            &self.vessels[host].trajectory,
        )
    }

    /// Lage der Bahn eines Schiffs zur Bahnebene seines Ziels (siehe
    /// `rendezvous::plane_crossing`).
    pub fn plane_to_target(&self, vessel: usize) -> Option<PlaneCrossing> {
        let host = self.host(self.vessels[vessel].target?);
        if host == self.host(vessel) {
            return None;
        }
        rendezvous::plane_crossing(
            &self.bodies,
            self.time,
            &self.vessels[self.host(vessel)].trajectory,
            &self.vessels[host].trajectory,
        )
    }

    /// Körper und Zustand eines Schiffs zur aktuellen Zeit (angedockt: des
    /// Hauptschiffs).
    pub fn vessel_state(&self, index: usize) -> (BodyId, StateVector) {
        self.vessels[self.host(index)]
            .trajectory
            .state_at(&self.bodies, self.time)
    }

    /// Aktuelle Masse in kg (angedockt: des ganzen Verbunds).
    pub fn vessel_mass(&self, index: usize) -> f64 {
        self.vessels[self.host(index)]
            .trajectory
            .current()
            .mass_at(self.time)
    }

    /// Verbleibender Treibstoff in kg (im Verbund: der gemeinsame Vorrat).
    pub fn vessel_fuel(&self, index: usize) -> f64 {
        self.vessel_mass(index) - self.params(self.host(index)).dry_mass
    }

    /// Verbleibendes Δv in m/s (Raketengleichung, wirksame Werte).
    pub fn vessel_delta_v(&self, index: usize) -> f64 {
        let params = self.params(self.host(index));
        params.exhaust_velocity * (self.vessel_mass(index) / params.dry_mass).ln()
    }

    /// Δv-Haushalt eines Schiffs: voller Tank, jetzt, geplante Brennvorgänge
    /// (Start und gültige Nodes), Rest und was fehlt.
    pub fn delta_v_budget(&self, index: usize) -> DeltaVBudget {
        let index = self.host(index);
        let params = &self.params(index);
        let trajectory = &self.vessels[index].trajectory;
        let ve = params.exhaust_velocity;
        let now = self.time;
        let mass = self.vessel_mass(index);

        let items = trajectory
            .launch()
            // Auf dem Booster verbraucht das Schiff beim Aufstieg nichts.
            .filter(|l| l.booster.is_none())
            .map(|l| (BudgetItem::Launch, l.burn, None))
            .into_iter()
            .chain(
                trajectory
                    .plans()
                    .enumerate()
                    .map(|(k, p)| (BudgetItem::Node(k), p.burn, Some(p.node.delta_v.length()))),
            )
            .chain(trajectory.dock_plan().map(|p| {
                (
                    BudgetItem::Docking,
                    p.matching.burn,
                    Some(p.matching.node.delta_v.length()),
                )
            }));
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
        // Der Anflug nach dem Angleichen gehört mit zum Andocken.
        if let Some(plan) = trajectory.dock_plan()
            && plan.contact > now
        {
            let from = if now < plan.matching.burn.end {
                plan.mass
            } else {
                mass
            };
            let approach = ve * (from / plan.end_mass).ln();
            match burns.last_mut() {
                Some((BudgetItem::Docking, delta_v)) => *delta_v += approach,
                _ => burns.push((BudgetItem::Docking, approach)),
            }
            end_mass = plan.end_mass;
        }
        // Das Landebrennen; reicht der Treibstoff nicht, alles, was nach der
        // Kette bleibt, und was dann noch fehlt.
        if let Some(touchdown) = self.touchdown(index) {
            match touchdown.landing {
                Ok(Some(LandingBurn { burn, .. })) if burn.end > now => {
                    let from = burn.mass_at(now.max(burn.start));
                    burns.push((BudgetItem::Landing, ve * (from / burn.end_mass()).ln()));
                    end_mass = burn.end_mass();
                }
                Err(Crash::NoFuel { missing: short, .. }) => {
                    burns.push((BudgetItem::Landing, ve * (end_mass / params.dry_mass).ln()));
                    missing += short;
                    end_mass = params.dry_mass;
                }
                _ => {}
            }
        }
        DeltaVBudget {
            full: params.full_delta_v(),
            available: ve * (mass / params.dry_mass).ln(),
            burns,
            rest: ve * (end_mass / params.dry_mass).ln(),
            missing,
        }
    }

    pub fn vessel_status(&self, index: usize) -> VesselStatus {
        if let Some(host) = self.vessels[index].docked_to {
            return VesselStatus::Docked { host };
        }
        let segment = self.vessels[index].trajectory.current();
        let body = &self.bodies[segment.body()];
        match segment {
            Segment::Approach { start, offset, .. } => {
                let profile = docking::Profile::new(offset.length());
                VesselStatus::Approaching {
                    distance: offset.length() - profile.covered(self.time - start),
                }
            }
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
                    descending: end.is_some_and(|(_, e)| e.is_descent()),
                }
            }
        }
    }
}
