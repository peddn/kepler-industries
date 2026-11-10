//! Tests für den Mond: Bahn auf Schienen, Einflusssphäre, gebundene Rotation.

use glam::DVec3;
use sim::World;
use sim::body::{body_state, orbital_period, sphere_of_influence};
use sim::scenario::{demo, home_planet};

const MOON: usize = 1;

/// Kreisbahn in 12.000 km Abstand, im Drehsinn des Planeten; nach einem
/// Umlauf wieder am selben Ort, die Geschwindigkeit passt zur Lage.
#[test]
fn moon_follows_its_orbit() {
    let world = demo();
    let bodies = &world.bodies;
    let period = orbital_period(bodies, MOON).unwrap();
    for t in [0.0, 12_345.0, 3.0e7] {
        let s = body_state(bodies, MOON, t);
        assert!((s.position.length() - 12_000_000.0).abs() < 1e-6, "{s:?}");
        assert_eq!(s.position.z, 0.0, "Äquatorebene");
        let circular = (bodies[0].gm / 12_000_000.0).sqrt();
        assert!((s.velocity.length() - circular).abs() < 1e-9, "{s:?}");

        let again = body_state(bodies, MOON, t + period);
        assert!(again.position.distance(s.position) < 1e-3, "{again:?}");
        // Ein Viertel weiter: 90° gegen den Uhrzeigersinn (von Norden aus).
        let quarter = body_state(bodies, MOON, t + 0.25 * period).position;
        assert!(s.position.normalize().dot(quarter.normalize()).abs() < 1e-9);
        assert!(s.position.cross(quarter).z > 0.0);

        // Geschwindigkeit = Ableitung der Lage.
        let dt = 1.0;
        let before = body_state(bodies, MOON, t - dt).position;
        let after = body_state(bodies, MOON, t + dt).position;
        let slope = (after - before) / (2.0 * dt);
        assert!(slope.distance(s.velocity) < 1e-3, "{slope} {}", s.velocity);
    }
    // Der Planet ruht im Ursprung.
    let planet = body_state(bodies, 0, 1_000.0);
    assert_eq!(
        (planet.position, planet.velocity),
        (DVec3::ZERO, DVec3::ZERO)
    );
}

/// Werte wie beim Mun in KSP: Umlauf 138.984 s (38,6 h), Einflusssphäre
/// 2.429,6 km. Der Planet reicht wie Kerbin 84.159 km weit (Rand des
/// Heimatsystems).
#[test]
fn moon_matches_the_mun() {
    let world = demo();
    let period = orbital_period(&world.bodies, MOON).unwrap();
    assert!((period / 138_984.0 - 1.0).abs() < 1e-4, "{period}");
    let soi = sphere_of_influence(&world.bodies, MOON);
    assert!((soi / 2_429_559.0 - 1.0).abs() < 1e-3, "{soi}");
    let home = sphere_of_influence(&world.bodies, 0);
    assert!((home / 84_159_286.0 - 1.0).abs() < 1e-6, "{home}");
    assert_eq!(orbital_period(&world.bodies, 0), None);
}

/// Gebundene Rotation: Länge 0° zeigt immer zum Planeten.
#[test]
fn moon_is_tidally_locked() {
    let world = demo();
    let moon = &world.bodies[MOON];
    for t in [0.0, 5_000.0, 77_777.0, 3.0e7] {
        let to_planet = -body_state(&world.bodies, MOON, t).position;
        let fixed = moon.inertial_to_body_fixed(to_planet, t).normalize();
        assert!(fixed.distance(DVec3::X) < 1e-9, "{t}: {fixed}");
    }
}

/// Der Mond gehört zum Spielstand; alte Spielstände ohne Bahnangabe laden
/// weiter (Körper ohne Bahn).
#[test]
fn moon_is_saved() {
    let world = demo();
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, world);

    let mut old = serde_json::to_value(home_planet()).unwrap();
    old.as_object_mut().unwrap().remove("orbit");
    let planet: sim::Body = serde_json::from_value(old).unwrap();
    assert_eq!(planet, home_planet());
}
