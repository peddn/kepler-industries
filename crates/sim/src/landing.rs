//! Landung: Landebrennen, Fallschirm und Zielgebiete.
//!
//! Würde ein Freiflug schneller als `SAFE_SPEED` (relativ zum Boden) auf den
//! Boden treffen, setzt die Zeitleiste davor automatisch ein Landebrennen:
//! voller Schub gegen die Bewegung relativ zum Boden (`Descent`), so spät wie
//! möglich, dass das Schiff knapp über dem Boden zum Stehen kommt
//! (`latest_start`). Sobald es steht, geht das Triebwerk aus; die letzten
//! Zentimeter fällt es und setzt mit höchstens der Hälfte von `SAFE_SPEED`
//! auf. Es ist ein normales Brennen: Treibstoff nach der Raketengleichung,
//! Verluste durch Schwerkraft und Luft inbegriffen, Vorschau = Ausführung.
//! Derselbe Code gilt für jeden Körper; mit Luft bremsen vorher Luft und
//! Fallschirm, das Landebrennen ist dann kurz.
//!
//! Geht das nicht (Schub nicht größer als das Gewicht, zu wenig Treibstoff,
//! oder keine Zeit mehr, weil der Freiflug erst knapp über dem Boden
//! beginnt), wird nicht gebremst: Absturz mit voller Geschwindigkeit, der
//! Grund steht in `Crash`. Ebenso, wenn ein geplantes Brennen in den Boden
//! führt.
//!
//! Wo das Schiff aufsetzt, entscheidet nicht darüber, ob es heil ankommt; ein
//! Zielgebiet zählt für Basis und Aufträge. Außerhalb geht es mit einem Hop
//! (Start mit niedriger Ziel-Apoapsis) weiter.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::kepler::StateVector;
use crate::world::VesselParams;
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Bis zu dieser Geschwindigkeit relativ zum Boden (m/s) setzt jedes Schiff
/// ohne Bremsen auf.
pub const SAFE_SPEED: f64 = 5.0;

/// Unterhalb dieser Höhe (m) ist im Sinkflug der Fallschirm offen.
pub const PARACHUTE_ALTITUDE: f64 = 3_000.0;

/// Bis zu dieser Geschwindigkeit relativ zum Boden (m/s) gilt das Schiff am
/// Ende des Landebrennens als stehend.
const STOP_SPEED: f64 = 1.0;

/// Genauigkeit des Brennbeginns in s (nur als Untergrenze; meist hört die
/// Suche viel früher auf, sobald die Höhe beim Stillstand passt).
const TIME_TOLERANCE: f64 = 1e-6;

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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Crash {
    /// Zu wenig Treibstoff: Δv an Bord, und wie schnell das Schiff am Boden
    /// noch wäre, wenn es allen Treibstoff zum Bremsen nutzte (so viel Δv
    /// fehlt mindestens; alte Spielstände: 0).
    NoFuel {
        available: f64,
        #[serde(default)]
        missing: f64,
    },
    /// Schub nicht größer als das Gewicht (Verhältnis Schub zu Gewicht).
    TooWeak { thrust_to_weight: f64 },
    /// Keine Zeit mehr zum Bremsen: Das Landebrennen müsste vor dem Freiflug
    /// beginnen, oder ein geplantes Brennen führt in den Boden.
    TooLate,
}

/// Geschwindigkeit relativ zum Boden (m/s).
pub fn ground_speed(body: &Body, state: &StateVector) -> f64 {
    (state.velocity - body.co_rotating_velocity(state.position)).length()
}

/// Steuerung beim Landebrennen: Schub gegen die Bewegung relativ zum Boden.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Descent {
    /// Drehung des Körpers (Achse mal Winkelgeschwindigkeit in rad/s).
    pub spin: DVec3,
}

impl Descent {
    pub fn new(body: &Body) -> Self {
        Self { spin: body.spin() }
    }

    /// Geschwindigkeit relativ zum Boden.
    pub fn ground_velocity(&self, state: &StateVector) -> DVec3 {
        state.velocity - self.spin.cross(state.position)
    }

    /// Schubrichtung: gegen die Bewegung zum Boden (steht das Schiff, nach oben).
    pub fn direction(&self, state: &StateVector) -> DVec3 {
        (-self.ground_velocity(state))
            .try_normalize()
            .unwrap_or_else(|| state.position.normalize())
    }
}

/// Wie ein Landebrennen ausgeht, das zu einem bestimmten Zeitpunkt beginnt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// Das Schiff steht in dieser Höhe (m) über dem Boden.
    Stops { altitude: f64 },
    /// Der Tank ist leer, bevor das Schiff steht; es ist dann noch so schnell
    /// (m/s, relativ zum Boden).
    RunsDry { speed: f64 },
    /// Noch während des Brennens auf dem Boden: zu spät gezündet.
    HitsGround,
}

impl Outcome {
    /// Ausgang eines Landebrennens, das im Zustand `end` endet (am Boden oder
    /// nicht).
    pub(crate) fn of(body: &Body, ground_contact: bool, end: &StateVector) -> Self {
        let speed = ground_speed(body, end);
        if ground_contact {
            Outcome::HitsGround
        } else if speed <= STOP_SPEED {
            Outcome::Stops {
                altitude: end.position.length() - body.radius,
            }
        } else {
            Outcome::RunsDry { speed }
        }
    }
}

/// Spätester Beginn des Landebrennens zwischen `from` (Beginn des Freiflugs)
/// und `contact` (Aufschlag ohne Bremsen), oder warum nicht gebremst wird.
/// `arc(t)` rechnet das Landebrennen ab `t` (genau wie die Zeitleiste).
///
/// Später zünden heißt tiefer zum Stehen kommen. Gesucht wird deshalb erst
/// rückwärts in sich verdoppelnden Schritten ein Beginn, der nicht zu spät
/// ist, dann per Bisektion die Grenze zu "zu spät", bis das Schiff höchstens
/// so hoch steht, dass es aus dieser Höhe mit der Hälfte von `SAFE_SPEED`
/// aufsetzt. Leert sich der Tank an dieser Grenze vorher, reicht der
/// Treibstoff nicht (früher zünden kostet nur mehr).
pub(crate) fn latest_start(
    body: &Body,
    params: &VesselParams,
    mass: f64,
    from: f64,
    contact: f64,
    contact_speed: f64,
    arc: impl Fn(f64) -> Outcome,
) -> Result<f64, Crash> {
    let gravity = body.gm / (body.radius * body.radius);
    let thrust_to_weight = params.thrust / (mass * gravity);
    if thrust_to_weight <= 1.0 || params.exhaust_velocity <= 0.0 {
        return Err(Crash::TooWeak { thrust_to_weight });
    }
    let available = params.exhaust_velocity * (mass / params.dry_mass).ln();
    if available <= 0.0 {
        return Err(Crash::NoFuel {
            available: 0.0,
            missing: contact_speed,
        });
    }
    let half_safe = 0.5 * SAFE_SPEED;
    let max_altitude = half_safe * half_safe / (2.0 * gravity);

    // Rückwärts, bis das Brennen nicht mehr zu spät beginnt.
    let mut late = contact;
    let mut back = 1.0;
    let (mut early, mut outcome) = loop {
        let t = (contact - back).max(from);
        match arc(t) {
            Outcome::HitsGround if t <= from => return Err(Crash::TooLate),
            Outcome::HitsGround => late = t,
            outcome => break (t, outcome),
        }
        back *= 2.0;
    };
    // Bisektion zwischen "nicht zu spät" (`early`) und "zu spät" (`late`).
    loop {
        if let Outcome::Stops { altitude } = outcome
            && altitude <= max_altitude
        {
            return Ok(early);
        }
        let mid = 0.5 * (early + late);
        if late - early <= TIME_TOLERANCE || mid <= early || mid >= late {
            break;
        }
        match arc(mid) {
            Outcome::HitsGround => late = mid,
            o => (early, outcome) = (mid, o),
        }
    }
    match outcome {
        // Steht etwas höher als gewünscht; der Fall danach entscheidet.
        Outcome::Stops { .. } => Ok(early),
        Outcome::RunsDry { speed } => Err(Crash::NoFuel {
            available,
            missing: speed,
        }),
        Outcome::HitsGround => unreachable!("früher Rand ist nie zu spät"),
    }
}
