//! Aufgaben: Ziele, deren Erreichen Daten bringt (GB).
//!
//! Eine Aufgabe gehört zu einem Körper und hat ein Ziel: eine Umlaufbahn
//! nach Vorgabe, eine Landung oder eine fertige Basis. Jede zählt einmal.
//! Die Daten gehören dem Spieler, nicht einem Ort (`World::data`): Sie werden
//! gefunkt und sofort gutgeschrieben.
//!
//! - **Orbitvorgaben sind Bereiche** (`OrbitSpec`): Periapsis, Apoapsis und
//!   Bahnneigung je von–bis. Die Breite des Bereichs ist die Toleranz, man
//!   muss nichts auf Nachkommastellen treffen. Eine Bahn darf mehrere
//!   Aufgaben zugleich erfüllen.
//! - **Erfüllt wird an Ereignissen des Protokolls** (Brennschluss, Landung,
//!   Basis fertig; siehe `World::push_log`). Die haben genaue Zeitpunkte und
//!   tragen alles Nötige, das Ergebnis hängt also nicht von der Schrittgröße
//!   ab. Eine Umlaufbahn zählt, wenn ein Brennen auf ihr endet: Vorbeiflug
//!   oder eine Bahn, auf der ein Schiff schon steht, zählen nicht.
//! - **Vorschau:** `World::tasks_met_by` sagt für eine geplante Bahn, welche
//!   offenen Aufgaben sie erfüllte, mit derselben Prüfung.
//! - Die **feste Liste** (`standard`) leitet sich aus den Körpern ab: je
//!   Körper mit einem Grundwert (`Body::data`) eine niedrige, eine hohe und
//!   eine polare Umlaufbahn, außer auf dem Heimatplaneten dazu Landung und
//!   Basis. Spätere Aufgaben (Missionen, erzeugte Vorgaben) sind weitere
//!   Einträge derselben Art.
//!
//! Der Booster erfüllt nichts.

use crate::body::{Body, BodyId, sphere_of_influence};
use crate::kepler::{self, StateVector};
use crate::log::{LogEntry, LogKind};
use crate::world::World;
use serde::{Deserialize, Serialize};

/// Niedrige Umlaufbahn: Die Apoapsis liegt höchstens diesen Teil des Radius
/// über der Luftgrenze (ohne Luft: über dem Boden).
const LOW_ORBIT: f64 = 0.25;
/// Hohe Umlaufbahn: Die Periapsis liegt mindestens so viele Radien über dem
/// Boden.
const HIGH_ORBIT: f64 = 2.0;
/// Polare Umlaufbahn: Bahnneigung in Grad.
const POLAR: (f64, f64) = (80.0, 100.0);

/// Daten für eine Aufgabe als Vielfaches des Grundwerts des Körpers.
const ORBIT_FACTOR: f64 = 1.0;
const POLAR_FACTOR: f64 = 2.0;
const LANDING_FACTOR: f64 = 2.0;
const BASE_FACTOR: f64 = 3.0;

/// Vorgabe für eine Umlaufbahn: Bereiche, in denen Periapsis, Apoapsis
/// (Höhen über dem Boden in m) und Bahnneigung (Grad gegen den Äquator)
/// liegen müssen, Grenzen eingeschlossen.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitSpec {
    pub periapsis: (f64, f64),
    pub apoapsis: (f64, f64),
    pub inclination: (f64, f64),
}

/// Eine Bahn in den Größen, die Vorgaben prüfen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    /// Höhe der Periapsis über dem Boden (m).
    pub periapsis: f64,
    /// Höhe der Apoapsis über dem Boden (m); `None` bei offener Bahn.
    pub apoapsis: Option<f64>,
    /// Bahnneigung gegen den Äquator in Grad (über 90: rückläufig).
    pub inclination: f64,
}

impl Orbit {
    /// Die Bahn eines Schiffs im Zustand `state` um `body`.
    pub fn of(body: &Body, state: &StateVector) -> Self {
        let info = kepler::orbit_info(body.gm, state);
        let h = state.position.cross(state.velocity);
        Self {
            periapsis: info.periapsis - body.radius,
            apoapsis: info.apoapsis.map(|r| r - body.radius),
            inclination: (h.z / h.length()).clamp(-1.0, 1.0).acos().to_degrees(),
        }
    }
}

impl OrbitSpec {
    /// Liegt die Bahn in der Vorgabe? Offene Bahnen nie.
    pub fn met_by(&self, orbit: &Orbit) -> bool {
        let within = |(low, high): (f64, f64), value: f64| (low..=high).contains(&value);
        orbit.apoapsis.is_some_and(|ap| within(self.apoapsis, ap))
            && within(self.periapsis, orbit.periapsis)
            && within(self.inclination, orbit.inclination)
    }
}

/// Was zu erreichen ist.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Goal {
    /// Ein Brennen endet auf einer Bahn in dieser Vorgabe.
    Orbit(OrbitSpec),
    /// Heil aufsetzen.
    Landing,
    /// Eine Basis fertig bauen.
    Base,
}

/// Eine Aufgabe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// Worum es geht, z. B. "Niedrige Umlaufbahn".
    pub title: String,
    pub body: BodyId,
    pub goal: Goal,
    /// Daten, die sie bringt (GB).
    pub data: f64,
    /// Wann sie erfüllt wurde (Spielzeit in s).
    pub done: Option<f64>,
}

/// Die festen Aufgaben für ein Sonnensystem (siehe Modul).
pub fn standard(bodies: &[Body], home: BodyId) -> Vec<Task> {
    let mut tasks = Vec::new();
    for (id, body) in bodies.iter().enumerate() {
        if body.data <= 0.0 {
            continue;
        }
        // Über Luft und Boden, innerhalb der Einflusssphäre.
        let floor = body.atmosphere.boundary_altitude();
        let ceiling = sphere_of_influence(bodies, id) - body.radius;
        let any = (0.0, 180.0);
        let mut add = |title: &str, goal: Goal, factor: f64| {
            tasks.push(Task {
                title: title.into(),
                body: id,
                goal,
                data: body.data * factor,
                done: None,
            });
        };
        let low = floor + LOW_ORBIT * body.radius;
        add(
            "Niedrige Umlaufbahn",
            Goal::Orbit(OrbitSpec {
                periapsis: (floor, low),
                apoapsis: (floor, low),
                inclination: any,
            }),
            ORBIT_FACTOR,
        );
        let high = HIGH_ORBIT * body.radius;
        add(
            "Hohe Umlaufbahn",
            Goal::Orbit(OrbitSpec {
                periapsis: (high, ceiling),
                apoapsis: (high, ceiling),
                inclination: any,
            }),
            ORBIT_FACTOR,
        );
        add(
            "Polare Umlaufbahn",
            Goal::Orbit(OrbitSpec {
                periapsis: (floor, ceiling),
                apoapsis: (floor, ceiling),
                inclination: POLAR,
            }),
            POLAR_FACTOR,
        );
        if id != home {
            add("Landung", Goal::Landing, LANDING_FACTOR);
            add("Basis", Goal::Base, BASE_FACTOR);
        }
    }
    tasks
}

impl World {
    /// Offene Aufgaben, die eine Bahn im Zustand `state` um `body` erfüllt
    /// (für die Vorschau; gezählt wird beim Brennschluss).
    pub fn tasks_met_by(&self, body: BodyId, state: &StateVector) -> Vec<usize> {
        self.open_orbit_tasks(body, &Orbit::of(&self.bodies[body], state))
    }

    fn open_orbit_tasks(&self, body: BodyId, orbit: &Orbit) -> Vec<usize> {
        (0..self.tasks.len())
            .filter(|&k| {
                let task = &self.tasks[k];
                task.done.is_none()
                    && task.body == body
                    && matches!(&task.goal, Goal::Orbit(spec) if spec.met_by(orbit))
            })
            .collect()
    }

    fn open_tasks(&self, body: BodyId, goal: Goal) -> Vec<usize> {
        (0..self.tasks.len())
            .filter(|&k| {
                let task = &self.tasks[k];
                task.done.is_none() && task.body == body && task.goal == goal
            })
            .collect()
    }

    /// Was ein Ereignis des Protokolls an Aufgaben erfüllt: Sie werden
    /// abgehakt, die Daten gutgeschrieben. Liefert die Einträge dazu.
    pub(crate) fn complete_tasks(&mut self, entry: &LogEntry) -> Vec<LogEntry> {
        if self.booster == Some(entry.vessel) {
            return Vec::new();
        }
        let met = match entry.kind {
            LogKind::BurnEnd {
                apoapsis,
                periapsis,
                body,
                inclination,
            } => self.open_orbit_tasks(
                body,
                &Orbit {
                    periapsis,
                    apoapsis,
                    inclination,
                },
            ),
            LogKind::Landed { body, .. } => self.open_tasks(body, Goal::Landing),
            LogKind::BaseCompleted { base } => self.open_tasks(self.bases[base].body, Goal::Base),
            _ => Vec::new(),
        };
        met.into_iter()
            .map(|task| {
                self.tasks[task].done = Some(entry.time);
                self.data += self.tasks[task].data;
                LogEntry {
                    time: entry.time,
                    vessel: entry.vessel,
                    kind: LogKind::TaskDone { task },
                }
            })
            .collect()
    }
}
