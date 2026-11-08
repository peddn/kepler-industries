//! Schrittweise Berechnung von Bahnstücken, auf die außer der Schwerkraft noch
//! andere Kräfte wirken: Luftwiderstand und Schub.
//!
//! Das Kraftmodell ist überall dasselbe: Schwerkraft + Luftwiderstand + Schub.
//! Wo die Luftdichte null ist und kein Triebwerk läuft, bleibt nur die
//! Schwerkraft, und dafür ist die Keplerbahn die exakte Lösung. Deshalb wird
//! nur gerechnet, solange Luft oder Schub im Spiel ist.

use crate::body::Body;
use crate::kepler::StateVector;
use crate::maneuver::Burn;
use crate::trajectory::Event;
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Feste Schrittweite in s. Fest, damit das Ergebnis nie von Bildrate oder
/// Zeitraffer abhängt.
pub const STEP: f64 = 0.1;
/// Jeder wievielte Schritt als Stützpunkt gespeichert wird (dazwischen wird
/// interpoliert). Hält den Speicherbedarf klein.
pub const SAMPLE_EVERY: usize = 10;
/// Maximale Länge eines Bahnstücks in s. Längere Stücke werden geteilt und
/// erst berechnet, wenn sie gebraucht werden.
pub const MAX_ARC_DURATION: f64 = 1800.0;

/// Stützpunkt eines berechneten Bahnstücks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub time: f64,
    pub state: StateVector,
}

/// Alles, was neben dem Zustand die Bewegung bestimmt.
pub struct Forces<'a> {
    pub body: &'a Body,
    /// Widerstandsbeiwert × Querschnittsfläche in m².
    pub drag_area: f64,
    /// Masse in kg, solange kein Triebwerk läuft.
    pub mass: f64,
    pub burn: Option<&'a Burn>,
}

impl Forces<'_> {
    pub fn mass_at(&self, t: f64) -> f64 {
        self.burn.map_or(self.mass, |b| b.mass_at(t))
    }

    /// Beschleunigung in m/s² zum Zeitpunkt `t`.
    pub fn acceleration(&self, t: f64, s: &StateVector) -> DVec3 {
        let body = self.body;
        let r = s.position.length();
        let mut a = -body.gm / (r * r * r) * s.position;
        let mass = self.mass_at(t);

        // Luftwiderstand gegen die Geschwindigkeit relativ zur mitrotierenden Luft.
        let density = body.atmosphere.density(r - body.radius);
        if density > 0.0 && self.drag_area > 0.0 {
            let v_rel = s.velocity - body.co_rotating_velocity(s.position);
            a -= 0.5 * density * self.drag_area / mass * v_rel.length() * v_rel;
        }

        if let Some(burn) = self.burn
            && burn.is_active(t)
        {
            a += burn.direction_at(s.position) * (burn.thrust / mass);
        }
        a
    }
}

/// Ein klassischer Runge-Kutta-Schritt (4. Ordnung).
pub(crate) fn rk4(forces: &Forces, t: f64, s: &StateVector, dt: f64) -> StateVector {
    let deriv = |t: f64, s: &StateVector| (s.velocity, forces.acceleration(t, s));
    let add = |s: &StateVector, (dp, dv): (DVec3, DVec3), h: f64| StateVector {
        position: s.position + dp * h,
        velocity: s.velocity + dv * h,
    };
    let k1 = deriv(t, s);
    let k2 = deriv(t + dt / 2.0, &add(s, k1, dt / 2.0));
    let k3 = deriv(t + dt / 2.0, &add(s, k2, dt / 2.0));
    let k4 = deriv(t + dt, &add(s, k3, dt));
    StateVector {
        position: s.position + (k1.0 + 2.0 * k2.0 + 2.0 * k3.0 + k4.0) * (dt / 6.0),
        velocity: s.velocity + (k1.1 + 2.0 * k2.1 + 2.0 * k3.1 + k4.1) * (dt / 6.0),
    }
}

/// Berechnet ein Bahnstück ab `start` bis zum ersten Ereignis.
///
/// Ereignisse: Bodenkontakt; ohne Schub zusätzlich das Verlassen der Luft;
/// außerdem ein fester Zeitpunkt `stop` (Beginn oder Ende eines Brennmanövers).
/// Der letzte Stützpunkt liegt genau auf dem Ereignis.
pub fn integrate_arc(
    forces: &Forces,
    start: Sample,
    stop: Option<(f64, Event)>,
) -> (Vec<Sample>, Event) {
    let body = forces.body;
    let boundary = body.boundary_radius();
    let watch_exit = forces.burn.is_none() && body.atmosphere.exists();
    let mut samples = vec![start];
    let mut t = start.time;
    let mut s = start.state;
    let mut step = 0usize;

    loop {
        // Liegt der feste Zeitpunkt in diesem Schritt, genau bis dorthin rechnen.
        let (h, timed) = match stop {
            Some((stop_time, event)) if t + STEP >= stop_time => {
                ((stop_time - t).max(0.0), Some(event))
            }
            _ => (STEP, None),
        };
        let next = rk4(forces, t, &s, h);
        let r0 = s.position.length();
        let r1 = next.position.length();

        let spatial = if r1 <= body.radius {
            Some((body.radius, Event::GroundContact))
        } else if watch_exit && r0 < boundary && r1 >= boundary {
            Some((boundary, Event::AtmosphereExit))
        } else {
            None
        };
        if let Some((radius, event)) = spatial {
            // Genauen Zeitpunkt im Schritt bestimmen und den Teilschritt rechnen.
            let fraction = ((radius - r0) / (r1 - r0)).clamp(0.0, 1.0);
            let exact = rk4(forces, t, &s, h * fraction);
            samples.push(Sample {
                time: t + h * fraction,
                state: exact,
            });
            return (samples, event);
        }
        if let Some(event) = timed {
            samples.push(Sample {
                time: t + h,
                state: next,
            });
            return (samples, event);
        }

        s = next;
        step += 1;
        t = start.time + step as f64 * STEP;
        if step.is_multiple_of(SAMPLE_EVERY) {
            samples.push(Sample { time: t, state: s });
        }
        if t - start.time >= MAX_ARC_DURATION {
            if samples.last().is_some_and(|last| last.time != t) {
                samples.push(Sample { time: t, state: s });
            }
            return (samples, Event::Continue);
        }
    }
}

/// Zustand zum Zeitpunkt `t` zwischen zwei Stützpunkten
/// (kubische Hermite-Interpolation aus Position und Geschwindigkeit).
pub fn interpolate(a: &Sample, b: &Sample, t: f64) -> StateVector {
    let h = b.time - a.time;
    if h <= 0.0 {
        return b.state;
    }
    let u = ((t - a.time) / h).clamp(0.0, 1.0);
    let (u2, u3) = (u * u, u * u * u);
    let (p0, v0, p1, v1) = (
        a.state.position,
        a.state.velocity * h,
        b.state.position,
        b.state.velocity * h,
    );
    let position = (2.0 * u3 - 3.0 * u2 + 1.0) * p0
        + (u3 - 2.0 * u2 + u) * v0
        + (-2.0 * u3 + 3.0 * u2) * p1
        + (u3 - u2) * v1;
    let velocity = ((6.0 * u2 - 6.0 * u) * p0
        + (3.0 * u2 - 4.0 * u + 1.0) * v0
        + (-6.0 * u2 + 6.0 * u) * p1
        + (3.0 * u2 - 2.0 * u) * v1)
        / h;
    StateVector { position, velocity }
}
