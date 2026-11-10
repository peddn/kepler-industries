//! Einflusssphären (Patched Conics): Ein Schiff spürt immer nur die
//! Schwerkraft des Körpers, in dessen Einflusssphäre es ist. An der Grenze
//! wechselt die Zeitleiste den Körper; die Lage wird dabei umgerechnet (die
//! Bezugssysteme sind nur gegeneinander verschoben, nicht gedreht).
//!
//! - **Austritt** (Abstand zum eigenen Körper wird größer als der Radius der
//!   Sphäre): analytisch auf der Keplerbahn (`kepler::time_to_radius_outbound`).
//!   Aus dem Planeten heraus heißt das: Das Schiff verlässt das Heimatsystem.
//! - **Eintritt** in die Sphäre eines Monds: Der Mond bewegt sich (auch auf
//!   einer geneigten Ellipse), deshalb wird gesucht. Der Abstand zum Mond wird so fein abgetastet, dass beide
//!   sich pro Schritt höchstens um ein Viertel des Radius gegeneinander
//!   bewegen; ein Vorzeichenwechsel wird per Bisektion genau eingegrenzt,
//!   ein tiefer Punkt dazwischen per Goldenem Schnitt (für knappe
//!   Streifschüsse). Gesucht wird einen Umlauf weit (`window_end`); ohne
//!   Treffer endet der Keplerabschnitt dort und der nächste sucht weiter.
//! - **Schrittweise berechnete Abschnitte** (Brennen, Luft) prüfen beide
//!   Grenzen bei jedem Schritt (`Watch`), wie den Bodenkontakt.
//!
//! Alles hängt nur vom Anfangszustand ab, nicht von Zeitraffer oder
//! Schrittweite der Simulation.

use crate::body::{Body, BodyId, body_state, sphere_of_influence};
use crate::kepler::{self, StateVector};
use crate::trajectory::Event;
use glam::DVec3;

/// Pro Abtastschritt bewegen sich Schiff und Mond höchstens um diesen
/// Bruchteil des Radius der Sphäre gegeneinander.
const SCAN_FRACTION: f64 = 0.25;
/// Genauigkeit eines Wechselzeitpunkts in s.
const TIME_TOLERANCE: f64 = 1e-6;
/// Suchfenster auf offenen Bahnen, die nie eine Sphäre verlassen (nur in
/// alten Spielständen ohne Rand des Heimatsystems).
const OPEN_WINDOW: f64 = 86_400.0;

/// Lage im System von `to` statt `from` zum Zeitpunkt `t`.
pub fn convert(
    bodies: &[Body],
    from: BodyId,
    to: BodyId,
    t: f64,
    state: StateVector,
) -> StateVector {
    let (a, b) = (body_state(bodies, from, t), body_state(bodies, to, t));
    StateVector {
        position: state.position + a.position - b.position,
        velocity: state.velocity + a.velocity - b.velocity,
    }
}

/// Wohin das Schiff beim Verlassen der Sphäre von `body` kommt: in die des
/// Mutterkörpers, oder (beim Planeten) aus dem Heimatsystem hinaus.
pub fn exit_event(bodies: &[Body], body: BodyId) -> Event {
    match bodies[body].orbit {
        Some(_) => Event::SoiExit,
        None => Event::LeavesSystem,
    }
}

/// Der Körper nach dem Wechsel `event` aus der Sphäre von `body`.
pub fn next_body(bodies: &[Body], body: BodyId, event: Event) -> Option<BodyId> {
    match event {
        Event::SoiEntry(child) => Some(child),
        Event::SoiExit => bodies[body].orbit.as_ref().map(|o| o.parent),
        _ => None,
    }
}

/// Monde von `body`.
fn children(bodies: &[Body], body: BodyId) -> impl Iterator<Item = BodyId> + '_ {
    (0..bodies.len()).filter(move |&c| bodies[c].orbit.as_ref().is_some_and(|o| o.parent == body))
}

/// Lage des Monds `child` relativ zu seinem Mutterkörper `body`.
fn relative(bodies: &[Body], body: BodyId, child: BodyId, t: f64) -> DVec3 {
    body_state(bodies, child, t).position - body_state(bodies, body, t).position
}

/// Zeitpunkt, an dem die Keplerbahn (Zustand `state` bei `t0`) die Sphäre
/// von `body` verlässt; `None`, wenn nie.
pub fn exit_time(bodies: &[Body], body: BodyId, t0: f64, state: &StateVector) -> Option<f64> {
    let radius = sphere_of_influence(bodies, body);
    if !radius.is_finite() {
        return None;
    }
    kepler::time_to_radius_outbound(bodies[body].gm, state, radius).map(|dt| t0 + dt)
}

/// Kann die Bahn die Sphäre eines Monds von `body` überhaupt erreichen? Nur
/// dann wird gesucht: Ihr Abstand zum Körper muss in den Bereich fallen, den
/// der Mond mit seiner Sphäre überstreicht (von Periapsis bis Apoapsis).
pub fn reaches_moon(bodies: &[Body], body: BodyId, state: &StateVector) -> bool {
    let info = kepler::orbit_info(bodies[body].gm, state);
    children(bodies, body).any(|c| overlaps(bodies, c, &info))
}

/// Überschneiden sich die Abstände der Bahn (`info`) zum Mutterkörper mit
/// denen des Monds `child` samt seiner Sphäre?
fn overlaps(bodies: &[Body], child: BodyId, info: &kepler::OrbitInfo) -> bool {
    let orbit = bodies[child].orbit.as_ref().expect("Mond");
    let soi = sphere_of_influence(bodies, child);
    info.periapsis <= orbit.apoapsis() + soi
        && info.apoapsis.is_none_or(|ra| ra >= orbit.periapsis() - soi)
}

/// Ende des Suchfensters ab `t0`: ein Umlauf, höchstens bis zum Verlassen
/// der Sphäre.
pub fn window_end(bodies: &[Body], body: BodyId, t0: f64, state: &StateVector) -> f64 {
    let window = kepler::period(bodies[body].gm, state).unwrap_or(OPEN_WINDOW);
    let end = t0 + window;
    exit_time(bodies, body, t0, state).map_or(end, |exit| exit.min(end))
}

/// Erster Eintritt in die Sphäre eines Monds von `body` zwischen `t0` und
/// `until` auf der Keplerbahn mit dem Zustand `state` bei `t0`.
pub fn entry_time(
    bodies: &[Body],
    body: BodyId,
    t0: f64,
    state: &StateVector,
    until: f64,
) -> Option<(f64, BodyId)> {
    children(bodies, body)
        .filter_map(|c| entry_into(bodies, body, c, t0, state, until).map(|t| (t, c)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

fn entry_into(
    bodies: &[Body],
    body: BodyId,
    child: BodyId,
    t0: f64,
    state: &StateVector,
    until: f64,
) -> Option<f64> {
    if until <= t0 {
        return None;
    }
    let gm = bodies[body].gm;
    let orbit = bodies[child].orbit.as_ref().expect("Mond");
    let radius = sphere_of_influence(bodies, child);
    let info = kepler::orbit_info(gm, state);
    if !overlaps(bodies, child, &info) {
        return None;
    }
    // Höchste Geschwindigkeit auf der Bahn und auf der des Monds (jeweils in
    // der Periapsis, Vis-Viva): So schnell bewegen sie sich höchstens
    // gegeneinander.
    let alpha = 2.0 / state.position.length() - state.velocity.length_squared() / gm;
    let fastest = (gm * (2.0 / info.periapsis - alpha)).sqrt();
    let e = orbit.eccentricity;
    let moon_speed = (gm / orbit.semi_major_axis * (1.0 + e) / (1.0 - e)).sqrt();
    let step = SCAN_FRACTION * radius / (fastest + moon_speed);
    let n = ((until - t0) / step).ceil().max(1.0) as usize;

    // Abstand über der Grenze (negativ: drin).
    let gap = |t: f64| {
        let p = kepler::propagate(gm, *state, t - t0).position;
        p.distance(relative(bodies, body, child, t)) - radius
    };
    let time = |i: usize| {
        if i == n {
            until
        } else {
            t0 + (until - t0) * i as f64 / n as f64
        }
    };
    let mut before: Option<(f64, f64)> = None; // vorletzter Punkt
    let mut last = (t0, gap(t0));
    for i in 1..=n {
        let next = (time(i), gap(time(i)));
        if last.1 > 0.0 && next.1 <= 0.0 {
            return Some(bisect(&gap, last.0, next.0));
        }
        // Tiefster Punkt zwischen zwei Abtastpunkten, ohne dass einer davon
        // drin liegt: knapper Streifschuss?
        if let Some(prev) = before
            && prev.1 >= last.1
            && last.1 <= next.1
        {
            let low = golden_minimum(&gap, prev.0, next.0);
            if gap(low) <= 0.0 {
                return Some(bisect(&gap, prev.0, low));
            }
        }
        before = Some(last);
        last = next;
    }
    None
}

/// Zeitpunkt in `[outside, inside]`, an dem `gap` von positiv auf höchstens
/// null fällt (das Ende liegt schon drin).
fn bisect(gap: &impl Fn(f64) -> f64, mut outside: f64, mut inside: f64) -> f64 {
    while inside - outside > TIME_TOLERANCE {
        let mid = 0.5 * (outside + inside);
        if mid <= outside || mid >= inside {
            break;
        }
        if gap(mid) > 0.0 {
            outside = mid;
        } else {
            inside = mid;
        }
    }
    inside
}

/// Minimum von `f` in `[a, b]` per Goldenem Schnitt.
fn golden_minimum(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64) -> f64 {
    let ratio = 0.5 * (5f64.sqrt() - 1.0);
    let mut c = b - ratio * (b - a);
    let mut d = a + ratio * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    while b - a > TIME_TOLERANCE {
        if fc < fd {
            b = d;
            (d, fd) = (c, fc);
            c = b - ratio * (b - a);
            fc = f(c);
        } else {
            a = c;
            (c, fc) = (d, fd);
            d = a + ratio * (b - a);
            fd = f(d);
        }
    }
    0.5 * (a + b)
}

/// Grenzen für schrittweise berechnete Abschnitte um `body`.
pub struct Watch<'a> {
    bodies: &'a [Body],
    body: BodyId,
    /// Radius der eigenen Sphäre (unendlich: kein Rand).
    exit: f64,
    /// Monde mit dem Radius ihrer Sphäre.
    moons: Vec<(BodyId, f64)>,
}

impl<'a> Watch<'a> {
    pub fn new(bodies: &'a [Body], body: BodyId) -> Self {
        Self {
            bodies,
            body,
            exit: sphere_of_influence(bodies, body),
            moons: children(bodies, body)
                .map(|c| (c, sphere_of_influence(bodies, c)))
                .collect(),
        }
    }

    /// Wird zwischen `(t0, s0)` und `(t1, s1)` eine Grenze überschritten?
    /// Dann der Bruchteil des Schritts bis dorthin (linear) und das Ereignis.
    pub fn crossing(
        &self,
        t0: f64,
        s0: &StateVector,
        t1: f64,
        s1: &StateVector,
    ) -> Option<(f64, Event)> {
        let (r0, r1) = (s0.position.length(), s1.position.length());
        if r0 < self.exit && r1 >= self.exit {
            let fraction = (self.exit - r0) / (r1 - r0);
            return Some((fraction, exit_event(self.bodies, self.body)));
        }
        self.moons.iter().find_map(|&(c, radius)| {
            let d0 = s0
                .position
                .distance(relative(self.bodies, self.body, c, t0));
            let d1 = s1
                .position
                .distance(relative(self.bodies, self.body, c, t1));
            (d0 > radius && d1 <= radius).then(|| ((d0 - radius) / (d0 - d1), Event::SoiEntry(c)))
        })
    }
}
