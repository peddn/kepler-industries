//! Himmelskörper: Masse, Größe, Rotation, Atmosphäre und Bahn (Monde).
//!
//! Bezugssystem (siehe `docs/ARCHITEKTUR.md`): nicht rotierend, Ursprung im
//! Mittelpunkt des Körpers, +Z zeigt zum Nordpol. Die Rotation des Körpers
//! steckt ausschließlich in den Umrechnungen Oberfläche ↔ Raum in dieser Datei.
//!
//! Monde laufen auf Schienen (wie in KSP): eine feste Keplerbahn um den
//! Mutterkörper aus sechs Bahnelementen, für jeden Zeitpunkt direkt berechnet
//! (`body_state`). Der Körper ohne Bahn, die Sonne, ruht im Ursprung; ihre
//! Einflusssphäre ist unbegrenzt. Alle Körper drehen sich um +Z (keine Achsneigung, wie in KSP); die Neigung einer
//! Bahn gilt gegenüber dieser gemeinsamen Äquatorebene.

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

/// Bahn eines Körpers um seinen Mutterkörper: eine Ellipse (oder ein Kreis),
/// beschrieben durch die sechs Bahnelemente zum Zeitpunkt 0. Mit Neigung 0
/// läuft sie in der Äquatorebene im Drehsinn der Rotation (von Norden aus
/// gegen den Uhrzeigersinn); über 90° läuft sie rückwärts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyOrbit {
    pub parent: BodyId,
    /// Große Halbachse in m (beim Kreis der Radius).
    pub semi_major_axis: f64,
    /// Exzentrizität: 0 Kreis, bis unter 1 immer gestrecktere Ellipse.
    pub eccentricity: f64,
    /// Neigung gegen die Äquatorebene (rad).
    pub inclination: f64,
    /// Länge des aufsteigenden Knotens, von +X aus (rad).
    pub ascending_node: f64,
    /// Argument der Periapsis, vom aufsteigenden Knoten aus in Bahnrichtung
    /// (rad).
    pub argument_of_periapsis: f64,
    /// Mittlere Anomalie zum Zeitpunkt 0 (rad): wie weit der Körper seit der
    /// Periapsis gleichmäßig vorangekommen ist (beim Kreis der Winkel).
    pub mean_anomaly: f64,
}

/// Genauigkeit der exzentrischen Anomalie (rad).
const ANOMALY_TOLERANCE: f64 = 1e-15;

impl BodyOrbit {
    /// Kreisbahn in der Äquatorebene mit Radius `radius`, zum Zeitpunkt 0 beim
    /// Winkel `angle` (rad, von +X aus).
    pub fn circular(parent: BodyId, radius: f64, angle: f64) -> Self {
        Self {
            parent,
            semi_major_axis: radius,
            eccentricity: 0.0,
            inclination: 0.0,
            ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly: angle,
        }
    }

    /// Kleinster Abstand zum Mutterkörper (m).
    pub fn periapsis(&self) -> f64 {
        self.semi_major_axis * (1.0 - self.eccentricity)
    }

    /// Größter Abstand zum Mutterkörper (m).
    pub fn apoapsis(&self) -> f64 {
        self.semi_major_axis * (1.0 + self.eccentricity)
    }

    /// Drehung aus der Bahnebene (Periapsis auf +X, Bahnrichtung +Y) ins
    /// System des Mutterkörpers.
    pub fn orientation(&self) -> DMat3 {
        DMat3::from_rotation_z(self.ascending_node)
            * DMat3::from_rotation_x(self.inclination)
            * DMat3::from_rotation_z(self.argument_of_periapsis)
    }

    /// Lage auf der Bahn bei der exzentrischen Anomalie `e_anomaly`, relativ
    /// zum Mutterkörper (zum Zeichnen der ganzen Bahn).
    pub fn point_at(&self, e_anomaly: f64) -> DVec3 {
        let (a, e) = (self.semi_major_axis, self.eccentricity);
        let (sin, cos) = e_anomaly.sin_cos();
        self.orientation() * DVec3::new(a * (cos - e), a * (1.0 - e * e).sqrt() * sin, 0.0)
    }

    /// Exzentrische Anomalie zum Zeitpunkt `t` (Mutterkörper mit `gm`). Die
    /// mittlere Anomalie wächst gleichmäßig, über den Rest der Umlaufzeit
    /// gerechnet (genau auch nach Jahren); die exzentrische folgt aus der
    /// Kepler-Gleichung M = E − e·sin E (Newton-Verfahren).
    pub fn eccentric_anomaly(&self, gm: f64, t: f64) -> f64 {
        let e = self.eccentricity;
        let period = circular_period(gm, self.semi_major_axis);
        let mean = self.mean_anomaly + t.rem_euclid(period) / period * TAU;
        let mut anomaly = if e > 0.8 { std::f64::consts::PI } else { mean };
        for _ in 0..50 {
            let step = (anomaly - e * anomaly.sin() - mean) / (1.0 - e * anomaly.cos());
            anomaly -= step;
            if step.abs() <= ANOMALY_TOLERANCE {
                break;
            }
        }
        anomaly
    }

    /// Lage und Geschwindigkeit relativ zum Mutterkörper (mit `gm`) zum
    /// Zeitpunkt `t`.
    pub fn state(&self, gm: f64, t: f64) -> StateVector {
        let (a, e) = (self.semi_major_axis, self.eccentricity);
        let period = circular_period(gm, a);
        let (sin, cos) = self.eccentric_anomaly(gm, t).sin_cos();
        let factor = (1.0 - e * e).sqrt();
        // Bahngeschwindigkeit beim Kreis; d E / d t = n / (1 − e·cos E).
        let speed = a * TAU / period / (1.0 - e * cos);
        let rotation = self.orientation();
        StateVector {
            position: rotation * DVec3::new(a * (cos - e), a * factor * sin, 0.0),
            velocity: rotation * DVec3::new(-sin * speed, factor * cos * speed, 0.0),
        }
    }
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
    /// Bahn um den Mutterkörper; `None` bei der Sonne im Ursprung.
    pub orbit: Option<BodyOrbit>,
    /// Häufigkeit von Zorit: so viel fasst das Lager einer Mine hier (kg),
    /// so viel gibt es also höchstens pro Abholung (siehe `base`). 0: keins.
    pub zorit: f64,
}

/// Umlaufzeit einer Kreisbahn mit Radius `radius` um einen Körper mit `gm`;
/// ebenso die einer Ellipse mit dieser großen Halbachse (drittes
/// Keplergesetz).
pub fn circular_period(gm: f64, radius: f64) -> f64 {
    TAU * (radius.powi(3) / gm).sqrt()
}

/// Umlaufzeit eines Körpers um seinen Mutterkörper; `None` beim Körper im
/// Ursprung.
pub fn orbital_period(bodies: &[Body], body: BodyId) -> Option<f64> {
    let orbit = bodies[body].orbit.as_ref()?;
    Some(circular_period(
        bodies[orbit.parent].gm,
        orbit.semi_major_axis,
    ))
}

/// Tag eines Körpers gegenüber seinem Mutterkörper (beim Planeten der
/// Sonnentag): bis derselbe Ort wieder zu ihm zeigt, aus Drehung und Umlauf.
/// Ohne Bahn, oder wenn er ihm immer dieselbe Seite zeigt: die Drehung.
pub fn day_length(bodies: &[Body], body: BodyId) -> f64 {
    let rotation = bodies[body].rotation_period;
    let (Some(orbit), Some(year)) = (&bodies[body].orbit, orbital_period(bodies, body)) else {
        return rotation;
    };
    // Rückwärts umlaufend kommt der Mutterkörper früher wieder.
    let sign = if orbit.inclination > std::f64::consts::FRAC_PI_2 {
        -1.0
    } else {
        1.0
    };
    let rate = 1.0 / rotation - sign / year;
    if rate.abs() * rotation < 1e-9 {
        rotation
    } else {
        1.0 / rate.abs()
    }
}

/// Lage und Geschwindigkeit eines Körpers zum Zeitpunkt `t`, relativ zur
/// Sonne im Ursprung.
pub fn body_state(bodies: &[Body], body: BodyId, t: f64) -> StateVector {
    let Some(orbit) = &bodies[body].orbit else {
        return StateVector {
            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
        };
    };
    let parent = body_state(bodies, orbit.parent, t);
    let own = orbit.state(bodies[orbit.parent].gm, t);
    StateVector {
        position: parent.position + own.position,
        velocity: parent.velocity + own.velocity,
    }
}

/// Radius der Einflusssphäre nach Laplace: große Halbachse · (m/M)^0,4
/// (wie in KSP, auch bei Ellipsen immer gleich groß). Darin zählt für ein
/// Schiff nur die Schwerkraft dieses Körpers. Die Sonne (ohne Bahn) reicht
/// unendlich weit.
pub fn sphere_of_influence(bodies: &[Body], body: BodyId) -> f64 {
    let Some(orbit) = &bodies[body].orbit else {
        return f64::INFINITY;
    };
    orbit.semi_major_axis * (bodies[body].gm / bodies[orbit.parent].gm).powf(0.4)
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
