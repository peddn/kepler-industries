//! Flug zu einem Himmelskörper als Ziel: Startfenster und engste Annäherung.
//!
//! Reine Auswertung, hier wird nichts verändert (wie `rendezvous`).
//!
//! **Startfenster:** Für einen Hohmann-Übergang muss das Ziel dem
//! Ausgangspunkt um einen bestimmten Winkel vorauslaufen, gesehen vom
//! gemeinsamen Mutterkörper. Ausgangspunkt ist der Körper, um den das Schiff
//! kreist (Zorb für den Flug zu Nura), oder das Schiff selbst, wenn es schon
//! um den Mutterkörper des Ziels kreist (um Zorb für den Flug zu Zoon). Die
//! Formel nimmt Kreisbahnen in einer Ebene an: ein Richtwert, bei leicht
//! elliptischen Bahnen auf wenige Tage genau.
//!
//! **Annäherung:** der kleinste Abstand der vorausberechneten Bahn zum
//! Zielkörper, auch wenn sie seine Einflusssphäre verfehlt. So sieht man
//! beim Ziehen am Node, ob es besser oder schlechter wird.

use crate::body::{Body, BodyId, body_state, orbital_period};
use crate::kepler;
use crate::trajectory::{Event, Segment, Trajectory};
use glam::DVec3;
use std::f64::consts::{PI, TAU};

/// Stützstellen der groben Suche nach der Annäherung.
const SAMPLES: usize = 2_000;

/// Startfenster für einen Hohmann-Übergang zum Zielkörper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// Der Körper, von dem aus gestartet wird; `None`: das Schiff selbst
    /// kreist schon um den Mutterkörper des Ziels.
    pub origin: Option<BodyId>,
    /// Phasenwinkel jetzt (rad, −π bis π): so weit läuft das Ziel dem
    /// Ausgangspunkt voraus.
    pub phase: f64,
    /// Phasenwinkel, bei dem gestartet werden sollte.
    pub needed: f64,
    /// Zeit bis dahin in s.
    pub wait: f64,
}

/// Engste Annäherung der vorausberechneten Bahn an einen Körper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyApproach {
    pub time: f64,
    /// Abstand zum Mittelpunkt des Körpers in m.
    pub distance: f64,
    /// Lage von Schiff und Körper zu diesem Zeitpunkt, relativ zur Sonne.
    pub position: DVec3,
    pub body_position: DVec3,
    /// Die Bahn führt in die Einflusssphäre des Körpers: Dann sind `time` der
    /// Eintritt und `distance` die Periapsis der Bahn um ihn.
    pub encounter: bool,
}

/// Winkel auf den Bereich −π bis π bringen.
fn wrap(angle: f64) -> f64 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// Startfenster von der Bahn `own` zum Körper `target`. `None`, wenn das
/// Schiff am Boden steht, schon bei `target` ist, oder das Ziel nicht von
/// hier aus mit einem einzigen Übergang zu erreichen ist (z. B. der Mond
/// eines anderen Planeten: erst zu dessen Planeten).
pub fn window(bodies: &[Body], now: f64, own: &Trajectory, target: BodyId) -> Option<Window> {
    let parent = bodies[target].orbit.as_ref()?.parent;
    let (body, state) = own.predicted_state(bodies, now)?;
    if matches!(own.predicted_segment(now)?, Segment::Surface { .. }) {
        return None;
    }
    // Vom Körper des Schiffs hinauf, bis er um denselben Mutterkörper
    // kreist wie das Ziel (oder dieser Mutterkörper selbst ist).
    let mut origin = body;
    loop {
        if origin == target {
            return None;
        }
        if origin == parent {
            break;
        }
        let up = bodies[origin].orbit.as_ref()?.parent;
        if up == parent {
            break;
        }
        origin = up;
    }
    let center = body_state(bodies, parent, now).position;
    let gm = bodies[parent].gm;
    // Lage und Winkelgeschwindigkeit des Ausgangspunkts um den Mutterkörper.
    let (origin, from, rate) = if origin == parent {
        // Das Schiff selbst; auf einer offenen Bahn gibt es kein Fenster.
        let period = kepler::period(gm, &state)?;
        (None, state.position, TAU / period)
    } else {
        let period = orbital_period(bodies, origin)?;
        let position = body_state(bodies, origin, now).position - center;
        (Some(origin), position, TAU / period)
    };
    let to = body_state(bodies, target, now).position - center;
    let target_rate = TAU / orbital_period(bodies, target)?;
    let longitude = |p: DVec3| p.y.atan2(p.x);
    let phase = wrap(longitude(to) - longitude(from));
    // Während des halben Umlaufs auf der Übergangsbahn läuft das Ziel weiter.
    let (r1, r2) = (
        from.length(),
        bodies[target].orbit.as_ref()?.semi_major_axis,
    );
    let needed = wrap(PI * (1.0 - ((r1 + r2) / (2.0 * r2)).powf(1.5)));
    // Der Phasenwinkel ändert sich gleichmäßig; bis er passt:
    let drift = target_rate - rate;
    if drift == 0.0 {
        return None;
    }
    let wait = if drift > 0.0 {
        (needed - phase).rem_euclid(TAU) / drift
    } else {
        (phase - needed).rem_euclid(TAU) / -drift
    };
    Some(Window {
        origin,
        phase,
        needed,
        wait,
    })
}

/// Engste Annäherung von `own` an den Körper `target` nach dem letzten
/// geplanten Brennen, einen Umlauf weit auf der Bahn, bei der die Zeitleiste
/// ankommt (oder bis sie endet). Führt die Bahn in seine Einflusssphäre,
/// zählt der erste Eintritt. `None` am Boden oder schon in seiner Sphäre.
pub fn approach(
    bodies: &[Body],
    now: f64,
    own: &Trajectory,
    target: BodyId,
) -> Option<BodyApproach> {
    let current = own.predicted_segment(now)?;
    if current.body() == target {
        return None;
    }
    let absolute = |t: f64| -> Option<(DVec3, DVec3)> {
        let segment = own.predicted_segment(t)?;
        if matches!(segment, Segment::Surface { .. }) {
            return None;
        }
        let ship =
            body_state(bodies, segment.body(), t).position + segment.state_at(bodies, t).position;
        Some((ship, body_state(bodies, target, t).position))
    };

    // Begegnung: der erste Eintritt in die Sphäre des Ziels.
    let segments: Vec<&Segment> = own.segments().collect();
    if let Some(k) = segments
        .iter()
        .position(|s| s.end_event() == Some(Event::SoiEntry(target)) && s.end() > now)
    {
        let time = segments[k].end();
        let around = segments.get(k + 1)?.state_at(bodies, time);
        let (position, body_position) = absolute(time)?;
        return Some(BodyApproach {
            time,
            distance: kepler::orbit_info(bodies[target].gm, &around).periapsis,
            position,
            body_position,
            encounter: true,
        });
    }

    // Zeitraum: ab dem Ende des letzten geplanten Brennens (sonst ab jetzt,
    // am Boden ab dem Abheben) einen Umlauf weit auf der letzten Bahn, also
    // um den Körper, bei dem die Zeitleiste ankommt. Beim Flug zu einem
    // Planeten ist das der erste Umlauf um die Sonne: der erste Vorbeiflug.
    let begin = segments
        .iter()
        .find(|s| !matches!(s, Segment::Surface { .. }) && s.end() > now)?
        .start()
        .max(now);
    let from = own.burns().last().map_or(now, |b| b.end).max(begin);
    let last_body = segments.last()?.body();
    let orbit = segments
        .iter()
        .find(|s| matches!(s, Segment::Kepler { .. }) && s.end() > from && s.body() == last_body)?;
    let gm = bodies[last_body].gm;
    let span = kepler::period(gm, &orbit.state_at(bodies, orbit.start())).unwrap_or(3.0 * 3600.0);
    let known = segments.last().map_or(f64::NEG_INFINITY, |s| s.end());
    let end = (orbit.start().max(from) + span).min(known);
    if end <= from {
        return None;
    }
    let begin = from;
    let distance = |t: f64| absolute(t).map_or(f64::INFINITY, |(ship, body)| ship.distance(body));

    // Grob abtasten, dann jedes innere Minimum genau suchen; das kleinste
    // davon oder das Ende des Zeitraums ist die engste Stelle.
    let step = (end - begin) / SAMPLES as f64;
    let times: Vec<f64> = (0..=SAMPLES).map(|i| begin + step * i as f64).collect();
    let d: Vec<f64> = times.iter().map(|&t| distance(t)).collect();
    let mut best = (times[SAMPLES], d[SAMPLES]);
    for i in 1..SAMPLES {
        if d[i] <= d[i - 1] && d[i] <= d[i + 1] && d[i].is_finite() {
            let t = golden_section(&distance, times[i - 1], times[i + 1]);
            let found = distance(t);
            if found < best.1 || !best.1.is_finite() {
                best = (t, found);
            }
        }
    }
    let (time, distance) = best;
    let (position, body_position) = absolute(time)?;
    Some(BodyApproach {
        time,
        distance,
        position,
        body_position,
        encounter: false,
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
            (b, d, fd) = (d, c, fc);
            c = b - INV_PHI * (b - a);
            fc = f(c);
        } else {
            (a, c, fc) = (c, d, fd);
            d = a + INV_PHI * (b - a);
            fd = f(d);
        }
    }
    0.5 * (a + b)
}
