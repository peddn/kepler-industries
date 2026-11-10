//! Rendezvous: nächste Annäherung an ein Ziel und Lage der Bahnebenen.
//!
//! Reine Auswertung der vorausberechneten Zeitleisten beider Schiffe; hier
//! wird nichts verändert. Gesucht wird ab jetzt bis einige Umläufe nach dem
//! letzten geplanten Brennen, also auch auf den Bahnen nach geplanten
//! Manövern. So sieht man beim Planen sofort, ob ein Transfer trifft.

use crate::body::Body;
use crate::kepler;
use crate::trajectory::{Segment, Trajectory};
use glam::DVec3;

/// So viele Umläufe der eigenen Bahn nach dem letzten geplanten Brennen wird
/// nach der nächsten Annäherung gesucht.
pub const APPROACH_ORBITS: f64 = 4.0;
/// Stützstellen der groben Suche nach der Annäherung.
const SAMPLES: usize = 2_000;
/// Stützstellen pro Umlauf bei der Suche nach den Knoten.
const NODE_SAMPLES: usize = 360;
/// Schwingt das Schiff höchstens so weit (m) über und unter der Zielebene,
/// liegen beide Bahnen in derselben Ebene: Dann gibt es keine Knoten. Bei
/// 700 km Bahnradius entspricht das etwa 0,0001° Neigung, weit unter allem,
/// was spielerisch zählt, und weit über dem Rundungsrauschen (Bruchteile
/// eines Millimeters), in dem die Knoten sonst umherspringen.
pub const COPLANAR_HEIGHT: f64 = 1.0;

/// Engste Annäherung an das Ziel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Approach {
    pub time: f64,
    /// Abstand in m.
    pub distance: f64,
    /// Geschwindigkeit relativ zum Ziel in m/s.
    pub relative_speed: f64,
    /// Lage des eigenen Schiffs und des Ziels zu diesem Zeitpunkt (relativ zum
    /// Körper, wie `StateVector::position`).
    pub position: DVec3,
    pub target_position: DVec3,
    /// Volle Umläufe ab dem letzten geplanten Brennen (sonst ab jetzt).
    pub revolutions: u32,
}

/// Lage der eigenen Bahn zur Bahnebene des Ziels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneCrossing {
    /// Winkel zwischen den Bahnebenen in Radiant.
    pub inclination: f64,
    /// Nächste Zeitpunkte, an denen das Schiff die Ebene des Ziels von unten
    /// nach oben (aufsteigender Knoten) bzw. von oben nach unten kreuzt.
    pub ascending: Option<f64>,
    pub descending: Option<f64>,
    /// Beide Bahnen liegen in derselben Ebene (siehe `COPLANAR_HEIGHT`):
    /// keine Knoten.
    pub coplanar: bool,
}

/// Ab wann die geplante Bahn gilt: Ende des letzten geplanten Brennens
/// (auch des Aufstiegs), frühestens jetzt.
fn reference(own: &Trajectory, now: f64) -> f64 {
    own.burns().last().map_or(now, |b| b.end).max(now)
}

/// Die Bahn nach dem letzten geplanten Brennen: der erste Keplerabschnitt,
/// der danach noch läuft.
fn final_orbit(own: &Trajectory, from: f64) -> Option<&Segment> {
    own.segments()
        .find(|s| matches!(s, Segment::Kepler { .. }) && s.end() > from)
}

/// Ende der bekannten Zeitleiste (unendlich, wenn sie offen endet).
fn known_until(trajectory: &Trajectory) -> f64 {
    trajectory
        .segments()
        .last()
        .map_or(f64::NEG_INFINITY, |s| s.end())
}

/// Engste Annäherung von `own` an `target`, ab jetzt (am Boden: ab dem
/// Abheben) bis `APPROACH_ORBITS` Umläufe nach dem letzten geplanten Brennen
/// (oder bis eine der beiden Zeitleisten endet). Zeit am Boden zählt nicht.
/// Nimmt der Abstand im ganzen Zeitraum nur ab (z. B. beim langsamen
/// Aufholen auf fast derselben Bahn), ist das Ende des Zeitraums die engste
/// Stelle. `None` ohne eigene Bahn (z. B. am Boden ohne Start).
pub fn approach(
    bodies: &[Body],
    now: f64,
    own: &Trajectory,
    target: &Trajectory,
) -> Option<Approach> {
    let from = reference(own, now);
    let orbit = final_orbit(own, from)?;
    let gm = bodies[orbit.body()].gm;
    let span = kepler::period(gm, &orbit.state_at(bodies, orbit.start())).unwrap_or(3.0 * 3600.0);
    let begin = own
        .segments()
        .find(|s| !matches!(s, Segment::Surface { .. }) && s.end() > now)?
        .start()
        .max(now);
    let end = (from + APPROACH_ORBITS * span)
        .min(known_until(own))
        .min(known_until(target));
    if end <= begin {
        return None;
    }

    let distance = |t: f64| -> f64 {
        let Some(segment) = own.predicted_segment(t) else {
            return f64::INFINITY;
        };
        match target.predicted_state(bodies, t) {
            Some((body, z))
                if body == segment.body() && !matches!(segment, Segment::Surface { .. }) =>
            {
                segment.state_at(bodies, t).position.distance(z.position)
            }
            _ => f64::INFINITY,
        }
    };

    // Grob abtasten, dann jedes innere Minimum genau suchen. Das kleinste
    // davon oder das Ende des Zeitraums ist die engste Stelle (der Anfang
    // nicht: "jetzt am nächsten" hilft beim Planen nicht).
    let step = (end - begin) / SAMPLES as f64;
    let times: Vec<f64> = (0..=SAMPLES).map(|i| begin + step * i as f64).collect();
    let d: Vec<f64> = times.iter().map(|&t| distance(t)).collect();
    let mut best: Option<(f64, f64)> = None;
    let mut consider = |t: f64, found: f64| {
        if found.is_finite() && best.is_none_or(|(_, b)| found < b) {
            best = Some((t, found));
        }
    };
    consider(times[SAMPLES], d[SAMPLES]);
    for i in 1..SAMPLES {
        if d[i] <= d[i - 1] && d[i] <= d[i + 1] && d[i].is_finite() {
            let t = golden_section(&distance, times[i - 1], times[i + 1]);
            consider(t, distance(t));
        }
    }
    let (time, distance) = best?;
    let (_, s) = own.predicted_state(bodies, time)?;
    let (_, z) = target.predicted_state(bodies, time)?;
    Some(Approach {
        time,
        distance,
        relative_speed: s.velocity.distance(z.velocity),
        position: s.position,
        target_position: z.position,
        revolutions: own.revolutions(bodies, from, time),
    })
}

/// Minimum einer Funktion mit genau einem Tal in `[a, b]` (Goldener Schnitt).
fn golden_section(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64) -> f64 {
    const INV_PHI: f64 = 0.618_033_988_749_894_8;
    let mut c = b - INV_PHI * (b - a);
    let mut d = a + INV_PHI * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..60 {
        if fc <= fd {
            b = d;
            d = c;
            fd = fc;
            c = b - INV_PHI * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + INV_PHI * (b - a);
            fd = f(d);
        }
    }
    0.5 * (a + b)
}

/// Neigung der eigenen Bahn nach dem letzten geplanten Brennen gegen die
/// Bahnebene des Ziels, und wann das Schiff dessen Ebene als Nächstes kreuzt.
/// Liegen beide Bahnen in derselben Ebene, gibt es keine Knoten
/// (`coplanar`). `None` ohne eigene Bahn oder wenn das Ziel am Boden steht.
pub fn plane_crossing(
    bodies: &[Body],
    now: f64,
    own: &Trajectory,
    target: &Trajectory,
) -> Option<PlaneCrossing> {
    let orbit = final_orbit(own, reference(own, now))?;
    let from = orbit.start().max(now);
    let state = orbit.state_at(bodies, from);
    let target_segment = target.predicted_segment(from)?;
    if target_segment.body() != orbit.body() || matches!(target_segment, Segment::Surface { .. }) {
        return None;
    }
    let target_state = target_segment.state_at(bodies, from);
    let h = state.position.cross(state.velocity);
    let normal = target_state
        .position
        .cross(target_state.velocity)
        .normalize();
    // atan2 statt acos (`angle_between`): auch bei fast gleichen Ebenen genau.
    let inclination = h.cross(normal).length().atan2(h.dot(normal));
    // Gleiche Ebene: Die Schnittlinie fehlt, die "Höhe über der Zielebene"
    // wäre nur noch Rundungsrauschen.
    let coplanar = state.position.length() * inclination.sin() <= COPLANAR_HEIGHT;
    if coplanar {
        return Some(PlaneCrossing {
            inclination,
            ascending: None,
            descending: None,
            coplanar,
        });
    }

    // Knoten: Vorzeichenwechsel des Abstands zur Zielebene, einen Umlauf weit
    // (höchstens bis zum Ende des Abschnitts).
    let gm = bodies[orbit.body()].gm;
    let span = kepler::period(gm, &state)
        .unwrap_or(3.0 * 3600.0)
        .min(orbit.end() - from);
    let height = |t: f64| orbit.state_at(bodies, t).position.dot(normal);
    let step = span / NODE_SAMPLES as f64;
    let (mut ascending, mut descending) = (None, None);
    let mut t0 = from;
    let mut h0 = height(t0);
    for i in 1..=NODE_SAMPLES {
        let t1 = from + step * i as f64;
        let h1 = height(t1);
        if (h0 < 0.0) != (h1 < 0.0) {
            let up = h0 < 0.0;
            let slot = if up { &mut ascending } else { &mut descending };
            if slot.is_none() {
                *slot = Some(bisect(&height, t0, t1, up));
            }
        }
        t0 = t1;
        h0 = h1;
    }
    Some(PlaneCrossing {
        inclination,
        ascending,
        descending,
        coplanar,
    })
}

/// Nullstelle von `f` in `[a, b]`; `rising`: f(a) < 0 ≤ f(b).
fn bisect(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64, rising: bool) -> f64 {
    for _ in 0..60 {
        let mid = 0.5 * (a + b);
        if (f(mid) < 0.0) == rising {
            a = mid;
        } else {
            b = mid;
        }
    }
    0.5 * (a + b)
}
