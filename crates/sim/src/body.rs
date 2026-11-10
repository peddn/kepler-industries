//! Himmelskörper: Masse, Größe, Rotation, Atmosphäre und Bahn (Monde).
//!
//! Bezugssystem (siehe `docs/ARCHITEKTUR.md`): nicht rotierend, Ursprung im
//! Mittelpunkt des Körpers, +Z zeigt zum Nordpol. Die Rotation des Körpers
//! steckt ausschließlich in den Umrechnungen Oberfläche ↔ Raum in dieser Datei.
//!
//! Monde laufen auf Schienen: eine Kreisbahn um den Mutterkörper, für jeden
//! Zeitpunkt direkt berechnet (`body_state`). Der erste Körper (`bodies[0]`)
//! ruht im Ursprung.

use crate::kepler::StateVector;
use glam::{DMat3, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// Index eines Himmelskörpers in `World::bodies`.
pub type BodyId = usize;

/// Atmosphärenmodell. Ein Körper ohne Luft ist nur der Spezialfall
/// `surface_density == 0`, es gibt also nur ein einziges Modell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Atmosphere {
    /// Luftdichte am Boden in kg/m³.
    pub surface_density: f64,
    /// Skalenhöhe in m: grob die Höhe, auf der die Dichte auf 1/e fällt.
    pub scale_height: f64,
    /// Höhe in m, ab der die Dichte exakt null ist.
    pub top_altitude: f64,
}

impl Atmosphere {
    /// Keine Atmosphäre.
    pub const NONE: Atmosphere = Atmosphere {
        surface_density: 0.0,
        scale_height: 1.0,
        top_altitude: 0.0,
    };

    /// Luftdichte in kg/m³ auf der gegebenen Höhe über dem Boden.
    ///
    /// Verschobene Exponentialfunktion: am Boden `surface_density`, bei
    /// `top_altitude` exakt 0, dazwischen stetig und fallend. Oberhalb ist die
    /// Dichte exakt null; dort ist die Keplerbahn die exakte Lösung desselben
    /// Modells und keine Näherung.
    pub fn density(&self, altitude: f64) -> f64 {
        if self.surface_density <= 0.0 || altitude >= self.top_altitude {
            return 0.0;
        }
        let h = altitude.max(0.0);
        let top = (-self.top_altitude / self.scale_height).exp();
        self.surface_density * ((-h / self.scale_height).exp() - top) / (1.0 - top)
    }

    /// Höhe, oberhalb derer die Dichte null ist (0 bei Körpern ohne Luft).
    pub fn boundary_altitude(&self) -> f64 {
        if self.surface_density > 0.0 {
            self.top_altitude
        } else {
            0.0
        }
    }

    pub fn exists(&self) -> bool {
        self.surface_density > 0.0
    }
}

/// Ein Punkt auf der Oberfläche, Winkel in Radiant.
/// Breite positiv nach Norden, Länge positiv nach Osten.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfacePoint {
    pub latitude: f64,
    pub longitude: f64,
}

impl SurfacePoint {
    pub fn from_degrees(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude: latitude.to_radians(),
            longitude: longitude.to_radians(),
        }
    }

    /// Einheitsvektor im körperfesten (mitrotierenden) System.
    pub fn body_fixed_direction(&self) -> DVec3 {
        let (sin_lat, cos_lat) = self.latitude.sin_cos();
        let (sin_lon, cos_lon) = self.longitude.sin_cos();
        DVec3::new(cos_lat * cos_lon, cos_lat * sin_lon, sin_lat)
    }
}

/// Bahn eines Körpers um seinen Mutterkörper: ein Kreis in dessen
/// Äquatorebene, im Drehsinn der Rotation (von Norden aus gegen den
/// Uhrzeigersinn).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyOrbit {
    pub parent: BodyId,
    /// Bahnradius in m.
    pub radius: f64,
    /// Winkel (rad) zum Zeitpunkt 0, von +X aus.
    pub phase: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub name: String,
    /// Gravitationsparameter μ = G·M in m³/s².
    pub gm: f64,
    /// Radius in m.
    pub radius: f64,
    /// Siderische Rotationsdauer in s (gegenüber den Sternen, nicht der Sonne).
    pub rotation_period: f64,
    pub atmosphere: Atmosphere,
    /// Bahn um den Mutterkörper; `None` beim Körper im Ursprung.
    #[serde(default)]
    pub orbit: Option<BodyOrbit>,
}

/// Umlaufzeit einer Kreisbahn mit Radius `radius` um einen Körper mit `gm`.
pub fn circular_period(gm: f64, radius: f64) -> f64 {
    TAU * (radius.powi(3) / gm).sqrt()
}

/// Umlaufzeit eines Körpers um seinen Mutterkörper; `None` beim Körper im
/// Ursprung.
pub fn orbital_period(bodies: &[Body], body: BodyId) -> Option<f64> {
    let orbit = bodies[body].orbit.as_ref()?;
    Some(circular_period(bodies[orbit.parent].gm, orbit.radius))
}

/// Lage und Geschwindigkeit eines Körpers zum Zeitpunkt `t`, relativ zum
/// Körper im Ursprung.
pub fn body_state(bodies: &[Body], body: BodyId, t: f64) -> StateVector {
    let Some(orbit) = &bodies[body].orbit else {
        return StateVector {
            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
        };
    };
    let parent = body_state(bodies, orbit.parent, t);
    let period = circular_period(bodies[orbit.parent].gm, orbit.radius);
    // Über den Rest der Umlaufzeit (wie die Rotation): genau auch nach Jahren.
    let angle = orbit.phase + t.rem_euclid(period) / period * TAU;
    let (sin, cos) = angle.sin_cos();
    let speed = orbit.radius * TAU / period;
    StateVector {
        position: parent.position + DVec3::new(cos, sin, 0.0) * orbit.radius,
        velocity: parent.velocity + DVec3::new(-sin, cos, 0.0) * speed,
    }
}

/// Radius der Einflusssphäre nach Laplace: Bahnradius · (m/M)^0,4. Darin
/// zählt für ein Schiff nur die Schwerkraft dieses Körpers. Der Körper im
/// Ursprung reicht vorerst unendlich weit (später bis zur Sonne).
pub fn sphere_of_influence(bodies: &[Body], body: BodyId) -> f64 {
    match &bodies[body].orbit {
        Some(orbit) => orbit.radius * (bodies[body].gm / bodies[orbit.parent].gm).powf(0.4),
        None => f64::INFINITY,
    }
}

impl Body {
    /// Rotationswinkel zum Zeitpunkt `t`, in [0, 2π).
    /// Über den Rest der Rotationsdauer berechnet, damit auch nach Jahren
    /// Spielzeit keine Genauigkeit verloren geht.
    pub fn rotation_angle(&self, t: f64) -> f64 {
        t.rem_euclid(self.rotation_period) / self.rotation_period * TAU
    }

    /// Winkelgeschwindigkeit der Rotation als Vektor (um +Z).
    pub fn spin(&self) -> DVec3 {
        DVec3::Z * (TAU / self.rotation_period)
    }

    /// Radius, oberhalb dessen keine Luft ist (Boden bei Körpern ohne Luft).
    pub fn boundary_radius(&self) -> f64 {
        self.radius + self.atmosphere.boundary_altitude()
    }

    /// Dreht einen körperfesten Vektor ins nicht rotierende System.
    pub fn body_fixed_to_inertial(&self, v: DVec3, t: f64) -> DVec3 {
        DMat3::from_rotation_z(self.rotation_angle(t)) * v
    }

    /// Dreht einen Vektor des nicht rotierenden Systems ins körperfeste System.
    pub fn inertial_to_body_fixed(&self, v: DVec3, t: f64) -> DVec3 {
        DMat3::from_rotation_z(-self.rotation_angle(t)) * v
    }

    /// Position eines Oberflächenpunkts (plus Höhe) zum Zeitpunkt `t`.
    pub fn surface_to_inertial(&self, point: SurfacePoint, altitude: f64, t: f64) -> DVec3 {
        self.body_fixed_to_inertial(point.body_fixed_direction() * (self.radius + altitude), t)
    }

    /// Oberflächenpunkt und Höhe unter einer Position zum Zeitpunkt `t`.
    pub fn inertial_to_surface(&self, position: DVec3, t: f64) -> (SurfacePoint, f64) {
        let fixed = self.inertial_to_body_fixed(position, t);
        let r = fixed.length();
        let point = SurfacePoint {
            latitude: (fixed.z / r).clamp(-1.0, 1.0).asin(),
            longitude: fixed.y.atan2(fixed.x),
        };
        (point, r - self.radius)
    }

    /// Geschwindigkeit der mitrotierenden Oberfläche bzw. Luft an `position`.
    pub fn co_rotating_velocity(&self, position: DVec3) -> DVec3 {
        self.spin().cross(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::home_planet;

    #[test]
    fn density_is_continuous_and_reaches_zero() {
        let atm = home_planet().atmosphere;
        assert!((atm.density(0.0) - atm.surface_density).abs() < 1e-12);
        assert_eq!(atm.density(atm.top_altitude), 0.0);
        assert_eq!(atm.density(atm.top_altitude + 1.0), 0.0);
        // Knapp unterhalb der Grenze winzig, aber positiv: kein Sprung.
        let just_below = atm.density(atm.top_altitude - 1e-3);
        assert!(just_below > 0.0 && just_below < 1e-12);
        // Monoton fallend.
        let mut last = f64::INFINITY;
        for i in 0..=700 {
            let d = atm.density(i as f64 * 100.0);
            assert!(d <= last);
            last = d;
        }
    }

    #[test]
    fn vacuum_is_special_case() {
        assert_eq!(Atmosphere::NONE.density(0.0), 0.0);
        assert_eq!(Atmosphere::NONE.boundary_altitude(), 0.0);
    }

    #[test]
    fn surface_point_rotates_with_body() {
        let body = home_planet();
        let p = SurfacePoint::from_degrees(10.0, 20.0);
        let start = body.surface_to_inertial(p, 0.0, 0.0);
        let after_one_day = body.surface_to_inertial(p, 0.0, body.rotation_period);
        assert!((start - after_one_day).length() < 1e-6);

        // Nach einer Vierteldrehung ist der Punkt um 90° nach Osten gewandert.
        let quarter = body.surface_to_inertial(p, 0.0, body.rotation_period / 4.0);
        let expected = DMat3::from_rotation_z(TAU / 4.0) * start;
        assert!((quarter - expected).length() < 1e-6);
    }

    #[test]
    fn surface_round_trip() {
        let body = home_planet();
        let p = SurfacePoint::from_degrees(-33.0, 151.0);
        let t = 12_345.6;
        let pos = body.surface_to_inertial(p, 1_000.0, t);
        let (back, altitude) = body.inertial_to_surface(pos, t);
        assert!((back.latitude - p.latitude).abs() < 1e-12);
        assert!((back.longitude - p.longitude).abs() < 1e-12);
        assert!((altitude - 1_000.0).abs() < 1e-6);
    }
}
