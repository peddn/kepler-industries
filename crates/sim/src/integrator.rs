//! Schrittweise Berechnung von Bahnstücken, auf die außer der Schwerkraft noch
//! andere Kräfte wirken (in PR 1: Luftwiderstand, später auch Schub).
//!
//! Das Kraftmodell ist überall dasselbe. Wo die Luftdichte null ist, bleibt
//! nur die Schwerkraft übrig, und dafür gibt es mit der Keplerbahn eine exakte
//! Lösung. Deshalb wird nur gerechnet, solange Luft im Spiel ist.

use crate::body::Body;
use crate::kepler::StateVector;
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Feste Schrittweite in s. Fest, damit das Ergebnis nie von Bildrate oder
/// Zeitraffer abhängt.
pub const STEP: f64 = 0.1;
/// Jeder wievielte Schritt als Stützpunkt gespeichert wird (dazwischen wird
/// interpoliert). Hält den Speicherbedarf klein.
pub const SAMPLE_EVERY: usize = 10;
/// Maximale Länge eines Bahnstücks in s. Längere Flüge durch die Luft werden
/// in mehrere Stücke geteilt und erst berechnet, wenn sie gebraucht werden.
pub const MAX_ARC_DURATION: f64 = 1800.0;

/// Stützpunkt eines berechneten Bahnstücks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub time: f64,
    pub state: StateVector,
}

/// Womit ein berechnetes Bahnstück endet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArcEnd {
    /// Die Luft ist verlassen, weiter geht es auf einer Keplerbahn.
    AtmosphereExit,
    /// Bodenberührung.
    GroundContact,
    /// Maximale Stücklänge erreicht, es folgt ein weiteres berechnetes Stück.
    Continue,
}

/// Beschleunigung in m/s² aus Schwerkraft und Luftwiderstand.
///
/// `drag_per_mass` ist Widerstandsbeiwert × Fläche / Masse (m²/kg).
/// Der Widerstand wirkt gegen die Geschwindigkeit relativ zur mitrotierenden Luft.
pub fn acceleration(body: &Body, drag_per_mass: f64, s: &StateVector) -> DVec3 {
    let r = s.position.length();
    let gravity = -body.gm / (r * r * r) * s.position;
    let density = body.atmosphere.density(r - body.radius);
    if density == 0.0 || drag_per_mass == 0.0 {
        return gravity;
    }
    let v_rel = s.velocity - body.co_rotating_velocity(s.position);
    gravity - 0.5 * density * drag_per_mass * v_rel.length() * v_rel
}

/// Ein klassischer Runge-Kutta-Schritt (4. Ordnung).
fn rk4(body: &Body, k: f64, s: &StateVector, dt: f64) -> StateVector {
    let deriv = |s: &StateVector| (s.velocity, acceleration(body, k, s));
    let add = |s: &StateVector, (dp, dv): (DVec3, DVec3), h: f64| StateVector {
        position: s.position + dp * h,
        velocity: s.velocity + dv * h,
    };
    let k1 = deriv(s);
    let k2 = deriv(&add(s, k1, dt / 2.0));
    let k3 = deriv(&add(s, k2, dt / 2.0));
    let k4 = deriv(&add(s, k3, dt));
    StateVector {
        position: s.position + (k1.0 + 2.0 * k2.0 + 2.0 * k3.0 + k4.0) * (dt / 6.0),
        velocity: s.velocity + (k1.1 + 2.0 * k2.1 + 2.0 * k3.1 + k4.1) * (dt / 6.0),
    }
}

/// Berechnet ein Bahnstück ab `start` bis zum ersten Ereignis.
///
/// Der letzte Stützpunkt liegt genau auf dem Ereignis (Luftgrenze bzw. Boden).
pub fn integrate_arc(body: &Body, drag_per_mass: f64, start: Sample) -> (Vec<Sample>, ArcEnd) {
    let boundary = body.boundary_radius();
    let mut samples = vec![start];
    let mut t = start.time;
    let mut s = start.state;
    let mut step = 0usize;

    loop {
        let next = rk4(body, drag_per_mass, &s, STEP);
        let r0 = s.position.length();
        let r1 = next.position.length();

        // Ereignisse: Boden geht vor, dann Verlassen der Luft.
        let event = if r1 <= body.radius {
            Some((body.radius, ArcEnd::GroundContact))
        } else if body.atmosphere.exists() && r0 < boundary && r1 >= boundary {
            Some((boundary, ArcEnd::AtmosphereExit))
        } else {
            None
        };
        if let Some((radius, end)) = event {
            // Genauen Zeitpunkt im Schritt bestimmen und den Teilschritt rechnen.
            let fraction = ((radius - r0) / (r1 - r0)).clamp(0.0, 1.0);
            let exact = rk4(body, drag_per_mass, &s, STEP * fraction);
            samples.push(Sample {
                time: t + STEP * fraction,
                state: exact,
            });
            return (samples, end);
        }

        s = next;
        t = start.time + (step + 1) as f64 * STEP;
        step += 1;
        if step.is_multiple_of(SAMPLE_EVERY) {
            samples.push(Sample { time: t, state: s });
        }
        if t - start.time >= MAX_ARC_DURATION {
            if samples.last().is_some_and(|last| last.time != t) {
                samples.push(Sample { time: t, state: s });
            }
            return (samples, ArcEnd::Continue);
        }
    }
}

/// Zustand zum Zeitpunkt `t` zwischen zwei Stützpunkten
/// (kubische Hermite-Interpolation aus Position und Geschwindigkeit).
pub fn interpolate(a: &Sample, b: &Sample, t: f64) -> StateVector {
    let h = b.time - a.time;
    if h <= 0.0 {
        return a.state;
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
