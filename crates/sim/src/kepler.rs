//! Keplerbahnen: Freiflug ohne Luft und ohne Schub.
//!
//! Propagiert wird mit universellen Variablen (Stumpff-Funktionen). Eine
//! einzige Formel gilt für Ellipse, Parabel und Hyperbel, es gibt keine
//! Sonderfälle bei Exzentrizität nahe 1.
//!
//! Wichtig gegen Bahndrift: Eine Bahn wird immer als Zustand zu einem festen
//! Zeitpunkt (Epoche) gespeichert, und jede Position wird direkt aus diesem
//! Zustand berechnet. Es wird nie Schritt für Schritt weitergerechnet.

use glam::{DMat3, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// Position (m) und Geschwindigkeit (m/s) relativ zum Mittelpunkt eines Körpers,
/// im nicht rotierenden System.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateVector {
    pub position: DVec3,
    pub velocity: DVec3,
}

/// Kehrwert der großen Halbachse: > 0 Ellipse, < 0 Hyperbel.
fn alpha(gm: f64, s: &StateVector) -> f64 {
    2.0 / s.position.length() - s.velocity.length_squared() / gm
}

/// Umlaufdauer, falls die Bahn eine Ellipse ist.
pub fn period(gm: f64, s: &StateVector) -> Option<f64> {
    let a = alpha(gm, s);
    (a > 0.0).then(|| TAU / (gm * a * a * a).sqrt())
}

/// Stumpff-Funktionen C(z) und S(z).
fn stumpff(z: f64) -> (f64, f64) {
    if z > 1e-3 {
        let s = z.sqrt();
        ((1.0 - s.cos()) / z, (s - s.sin()) / (s * s * s))
    } else if z < -1e-3 {
        let s = (-z).sqrt();
        ((s.cosh() - 1.0) / -z, (s.sinh() - s) / (s * s * s))
    } else {
        // Reihenentwicklung um 0, vermeidet Auslöschung.
        let c = 1.0 / 2.0 - z / 24.0 + z * z / 720.0 - z * z * z / 40_320.0
            + z * z * z * z / 3_628_800.0;
        let s = 1.0 / 6.0 - z / 120.0 + z * z / 5_040.0 - z * z * z / 362_880.0
            + z * z * z * z / 39_916_800.0;
        (c, s)
    }
}

/// Zustand nach der Zeit `dt` (auch negativ) auf der Keplerbahn durch `s`.
pub fn propagate(gm: f64, s: StateVector, dt: f64) -> StateVector {
    let r0_vec = s.position;
    let v0_vec = s.velocity;
    let r0 = r0_vec.length();
    let alpha = alpha(gm, &s);

    // Bei Ellipsen ganze Umläufe abziehen: hält die Zahlen klein und genau,
    // auch nach tausenden Umläufen.
    let mut dt = dt;
    if let Some(period) = period(gm, &s) {
        dt = dt.rem_euclid(period);
        if dt > 0.5 * period {
            dt -= period;
        }
    }
    if dt == 0.0 {
        return s;
    }

    let sqrt_mu = gm.sqrt();
    let sigma0 = r0_vec.dot(v0_vec) / sqrt_mu;

    // Startwert nach Vallado.
    let mut chi = if alpha > 1e-15 {
        sqrt_mu * dt * alpha
    } else if alpha < -1e-15 {
        let a = 1.0 / alpha;
        let sign = dt.signum();
        let arg = (-2.0 * gm * alpha * dt)
            / (r0_vec.dot(v0_vec) + sign * (-gm * a).sqrt() * (1.0 - r0 * alpha));
        sign * (-a).sqrt() * arg.abs().ln()
    } else {
        sqrt_mu * dt / r0
    };

    // Newton-Verfahren auf der universellen Kepler-Gleichung. Die Ableitung
    // nach chi ist gerade der Bahnradius r.
    let mut c = 0.5;
    let mut s_ = 1.0 / 6.0;
    let mut z = 0.0;
    for _ in 0..100 {
        let chi2 = chi * chi;
        z = alpha * chi2;
        (c, s_) = stumpff(z);
        let r = chi2 * c + sigma0 * chi * (1.0 - z * s_) + r0 * (1.0 - z * c);
        let f = sigma0 * chi2 * c + (1.0 - alpha * r0) * chi2 * chi * s_ + r0 * chi - sqrt_mu * dt;
        let delta = f / r;
        chi -= delta;
        if delta.abs() <= 1e-14 * chi.abs().max(1.0) {
            let chi2 = chi * chi;
            z = alpha * chi2;
            (c, s_) = stumpff(z);
            break;
        }
    }

    let chi2 = chi * chi;
    let f = 1.0 - chi2 / r0 * c;
    let g = dt - chi2 * chi / sqrt_mu * s_;
    let position = f * r0_vec + g * v0_vec;
    let r = position.length();
    let f_dot = sqrt_mu / (r * r0) * chi * (z * s_ - 1.0);
    let g_dot = 1.0 - chi2 / r * c;
    let velocity = f_dot * r0_vec + g_dot * v0_vec;
    StateVector { position, velocity }
}

/// Kenngrößen einer Bahn, z. B. für die Anzeige.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitInfo {
    pub eccentricity: f64,
    /// Radius der Periapsis (nicht Höhe).
    pub periapsis: f64,
    /// Radius der Apoapsis, `None` bei offenen Bahnen.
    pub apoapsis: Option<f64>,
    pub period: Option<f64>,
}

pub fn orbit_info(gm: f64, s: &StateVector) -> OrbitInfo {
    let r = s.position.length();
    let h = s.position.cross(s.velocity);
    let e_vec = ((s.velocity.length_squared() - gm / r) * s.position
        - s.position.dot(s.velocity) * s.velocity)
        / gm;
    let e = e_vec.length();
    let p = h.length_squared() / gm;
    OrbitInfo {
        eccentricity: e,
        periapsis: p / (1.0 + e),
        apoapsis: (e < 1.0).then(|| p / (1.0 - e)),
        period: period(gm, s),
    }
}

/// Zeit bis die Bahn als Nächstes den Radius `radius` von außen nach innen
/// kreuzt. `None`, wenn das nie passiert.
///
/// Analytisch über die exzentrische bzw. hyperbolische Anomalie: exakt und
/// unabhängig von irgendeiner Schrittweite.
pub fn time_to_radius_inbound(gm: f64, s: &StateVector, radius: f64) -> Option<f64> {
    let info = orbit_info(gm, s);
    if info.periapsis >= radius {
        return None;
    }
    let r = s.position.length();
    let rv = s.position.dot(s.velocity);
    let alpha = alpha(gm, s);
    let e = info.eccentricity;

    if alpha > 0.0 {
        // Ellipse
        let a = 1.0 / alpha;
        if info.apoapsis.is_some_and(|ra| ra <= radius) {
            return None; // ganz innerhalb, kreuzt nie von außen
        }
        let n = (gm * alpha * alpha * alpha).sqrt();
        let e_sin_e0 = rv / (gm * a).sqrt();
        let e_cos_e0 = 1.0 - r / a;
        let e0 = e_sin_e0.atan2(e_cos_e0);
        let m0 = e0 - e_sin_e0;
        // Einflug: vor der Periapsis, also exzentrische Anomalie negativ.
        let e_in = -((1.0 - radius / a) / e).clamp(-1.0, 1.0).acos();
        let m_in = e_in - e * e_in.sin();
        Some((m_in - m0).rem_euclid(TAU) / n)
    } else {
        // Hyperbel (Parabeln treten in Gleitkommazahlen praktisch nicht auf).
        let a = 1.0 / alpha;
        let n = (gm * -alpha * -alpha * -alpha).sqrt();
        let e_sinh_h0 = rv / (-gm * a).sqrt();
        let h0 = (e_sinh_h0 / e).asinh();
        let m0 = e_sinh_h0 - h0;
        let h_in = -((1.0 - radius / a) / e).max(1.0).acosh();
        if h_in <= h0 {
            return None; // Einflugpunkt liegt in der Vergangenheit
        }
        let m_in = e * h_in.sinh() - h_in;
        Some((m_in - m0) / n)
    }
}

/// Zeit bis zur nächsten Apoapsis (nur bei Ellipsen), in [0, Umlaufzeit).
pub fn time_to_apoapsis(gm: f64, s: &StateVector) -> Option<f64> {
    let alpha = alpha(gm, s);
    if alpha <= 0.0 {
        return None;
    }
    let a = 1.0 / alpha;
    let n = (gm * alpha * alpha * alpha).sqrt();
    let e_sin_e0 = s.position.dot(s.velocity) / (gm * a).sqrt();
    let e_cos_e0 = 1.0 - s.position.length() / a;
    let e0 = e_sin_e0.atan2(e_cos_e0);
    let m0 = e0 - e_sin_e0;
    Some((std::f64::consts::PI - m0).rem_euclid(TAU) / n)
}

/// Zustand in der Periapsis (oder Apoapsis) einer Bahn mit den gegebenen
/// Radien und Lagewinkeln (Inklination, aufsteigender Knoten, Argument der
/// Periapsis; alle in Radiant).
pub fn state_from_apsides(
    gm: f64,
    periapsis: f64,
    apoapsis: f64,
    inclination: f64,
    ascending_node: f64,
    argument_of_periapsis: f64,
    at_apoapsis: bool,
) -> StateVector {
    let a = 0.5 * (periapsis + apoapsis);
    let rotation = DMat3::from_rotation_z(ascending_node)
        * DMat3::from_rotation_x(inclination)
        * DMat3::from_rotation_z(argument_of_periapsis);
    let p_dir = rotation * DVec3::X;
    let q_dir = rotation * DVec3::Y;
    let (r, sign) = if at_apoapsis {
        (apoapsis, -1.0)
    } else {
        (periapsis, 1.0)
    };
    let speed = (gm * (2.0 / r - 1.0 / a)).sqrt();
    StateVector {
        position: sign * p_dir * r,
        velocity: sign * q_dir * speed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GM: f64 = 3.5316e12;
    const R: f64 = 600_000.0;

    fn eccentric() -> StateVector {
        state_from_apsides(GM, R + 80_000.0, R + 900_000.0, 0.5, 1.0, 2.0, false)
    }

    fn hyperbolic() -> StateVector {
        let mut s = state_from_apsides(GM, R + 100_000.0, R + 100_000.0, 0.3, 0.0, 0.0, false);
        s.velocity *= 1.6; // über Fluchtgeschwindigkeit
        s
    }

    fn assert_close(a: StateVector, b: StateVector, pos_tol: f64, vel_tol: f64) {
        let dp = (a.position - b.position).length();
        let dv = (a.velocity - b.velocity).length();
        assert!(dp < pos_tol, "Positionsfehler {dp} m");
        assert!(dv < vel_tol, "Geschwindigkeitsfehler {dv} m/s");
    }

    #[test]
    fn forward_and_back_returns_to_start() {
        for s in [eccentric(), hyperbolic()] {
            for dt in [1.0, 600.0, 5_000.0, 40_000.0] {
                let there = propagate(GM, s, dt);
                let back = propagate(GM, there, -dt);
                assert_close(back, s, 1e-3, 1e-6);
            }
        }
    }

    #[test]
    fn one_period_returns_to_start() {
        let s = eccentric();
        let t = period(GM, &s).unwrap();
        assert_close(propagate(GM, s, t), s, 1e-4, 1e-7);
    }

    #[test]
    fn energy_and_angular_momentum_are_conserved() {
        for s in [eccentric(), hyperbolic()] {
            let energy =
                |s: &StateVector| 0.5 * s.velocity.length_squared() - GM / s.position.length();
            let h0 = s.position.cross(s.velocity);
            for dt in [123.0, 4_567.0, 89_012.0] {
                let s1 = propagate(GM, s, dt);
                assert!((energy(&s1) - energy(&s)).abs() / energy(&s).abs() < 1e-10);
                assert!((s1.position.cross(s1.velocity) - h0).length() / h0.length() < 1e-10);
            }
        }
    }

    #[test]
    fn circular_orbit_quarter_period() {
        let s = state_from_apsides(GM, R + 100_000.0, R + 100_000.0, 0.0, 0.0, 0.0, false);
        let t = period(GM, &s).unwrap();
        let q = propagate(GM, s, t / 4.0);
        // Startet auf +X, fliegt nach +Y.
        assert!((q.position - DVec3::Y * (R + 100_000.0)).length() < 1e-3);
    }

    #[test]
    fn no_drift_after_thousand_orbits() {
        let s = eccentric();
        let t = period(GM, &s).unwrap();
        assert_close(propagate(GM, s, 1000.0 * t), s, 1e-2, 1e-5);
        // Und die Position zu einem beliebigen Zeitpunkt hängt nicht davon ab,
        // wie viele Umläufe dazwischen liegen.
        let a = propagate(GM, s, 777.0);
        let b = propagate(GM, s, 777.0 + 1000.0 * t);
        assert_close(a, b, 1e-2, 1e-5);
    }

    #[test]
    fn inbound_crossing_time_is_exact() {
        let s = state_from_apsides(GM, R + 50_000.0, R + 150_000.0, 0.2, 0.0, 0.0, true);
        let boundary = R + 70_000.0;
        let dt = time_to_radius_inbound(GM, &s, boundary).unwrap();
        let at = propagate(GM, s, dt);
        assert!((at.position.length() - boundary).abs() < 1e-3);
        assert!(
            at.position.dot(at.velocity) < 0.0,
            "muss nach innen fliegen"
        );
        // Kurz vorher noch außerhalb.
        assert!(propagate(GM, s, dt - 1.0).position.length() > boundary);
    }

    #[test]
    fn no_crossing_for_high_orbit() {
        let s = eccentric(); // Periapsis 80 km
        assert!(time_to_radius_inbound(GM, &s, R + 70_000.0).is_none());
    }

    #[test]
    fn hyperbola_crossing() {
        // Hyperbel, die auf einen Körper zufällt: Start weit draußen, nach innen.
        let s = hyperbolic();
        let back = propagate(GM, s, -3_000.0);
        let dt = time_to_radius_inbound(GM, &back, R + 200_000.0);
        // Periapsis 100 km < 200 km Grenze, also Einflug vorhanden.
        let dt = dt.unwrap();
        let at = propagate(GM, back, dt);
        assert!((at.position.length() - (R + 200_000.0)).abs() < 1e-2);
        // Nach der Periapsis gibt es keinen weiteren Einflug.
        assert!(time_to_radius_inbound(GM, &propagate(GM, s, 10.0), R + 200_000.0).is_none());
    }

    #[test]
    fn orbit_info_matches_construction() {
        let s = eccentric();
        let info = orbit_info(GM, &s);
        assert!((info.periapsis - (R + 80_000.0)).abs() < 1e-3);
        assert!((info.apoapsis.unwrap() - (R + 900_000.0)).abs() < 1e-3);
    }

    #[test]
    fn apoapsis_time_is_exact() {
        let s = propagate(GM, eccentric(), 1234.0);
        let dt = time_to_apoapsis(GM, &s).unwrap();
        let at = propagate(GM, s, dt);
        assert!((at.position.length() - (R + 900_000.0)).abs() < 1e-3);
        assert!(time_to_apoapsis(GM, &hyperbolic()).is_none());
    }
}
