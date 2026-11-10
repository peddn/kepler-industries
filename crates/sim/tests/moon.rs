//! Tests für die Monde: Bahn auf Schienen (Kreis und geneigte Ellipse),
//! Einflusssphäre, gebundene Rotation.

use glam::DVec3;
use sim::body::{body_state, orbital_period, sphere_of_influence};
use sim::scenario::{demo, home_planet};
use sim::{BodyOrbit, Event, StateVector, World, kepler};
use std::f64::consts::PI;

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

const OUTER: usize = 2;

/// Zimm läuft auf einer geneigten Ellipse: Seine Lage stimmt mit einer
/// unabhängigen Keplerbahn aus seinem Anfangszustand überein, Abstände,
/// Neigung, Knoten und Umlauf passen zu den Bahnelementen.
#[test]
fn outer_moon_follows_its_ellipse() {
    let world = demo();
    let bodies = &world.bodies;
    assert_eq!(bodies[OUTER].name, "Zimm");
    let orbit = bodies[OUTER].orbit.clone().unwrap();
    let gm = bodies[0].gm;
    let start = body_state(bodies, OUTER, 0.0);
    let period = orbital_period(bodies, OUTER).unwrap();
    assert!((period / 1_077_311.0 - 1.0).abs() < 1e-5, "{period}");

    let (mut nearest, mut farthest) = (f64::INFINITY, 0.0f64);
    for i in 0..=1_000 {
        let t = period * i as f64 / 1_000.0 + 12_345.0;
        let s = body_state(bodies, OUTER, t);
        let kepler = kepler::propagate(gm, start, t);
        assert!(s.position.distance(kepler.position) < 1.0, "{t}");
        assert!(s.velocity.distance(kepler.velocity) < 1e-4, "{t}");
        nearest = nearest.min(s.position.length());
        farthest = farthest.max(s.position.length());
    }
    assert!(
        (nearest / orbit.periapsis() - 1.0).abs() < 1e-4,
        "{nearest}"
    );
    assert!(
        (farthest / orbit.apoapsis() - 1.0).abs() < 1e-4,
        "{farthest}"
    );
    assert!((orbit.periapsis() - 42_300_000.0).abs() < 1e-3);
    assert!((orbit.apoapsis() - 51_700_000.0).abs() < 1e-3);

    // Bahnebene: Neigung 6°, aufsteigender Knoten bei 78°.
    let h = start.position.cross(start.velocity);
    assert!(((h.z / h.length()).acos().to_degrees() - 6.0).abs() < 1e-9);
    let node = DVec3::Z.cross(h);
    assert!((node.y.atan2(node.x).to_degrees() - 78.0).abs() < 1e-9);

    // Nach einem Umlauf wieder am selben Ort, auch nach Jahren.
    for t in [0.0, 3.0e7] {
        let s = body_state(bodies, OUTER, t);
        let again = body_state(bodies, OUTER, t + period);
        assert!(again.position.distance(s.position) < 1e-2, "{t}");
    }
    // Geschwindigkeit = Ableitung der Lage.
    let t = 500_000.0;
    let before = body_state(bodies, OUTER, t - 1.0).position;
    let after = body_state(bodies, OUTER, t + 1.0).position;
    let slope = (after - before) / 2.0;
    assert!(slope.distance(body_state(bodies, OUTER, t).velocity) < 1e-3);

    // Einflusssphäre aus der großen Halbachse, wie bei Minmus.
    let soi = sphere_of_influence(bodies, OUTER);
    assert!((soi / 2_247_000.0 - 1.0).abs() < 1e-3, "{soi}");
}

/// Alte Spielstände mit Kreisbahnen (Radius und Winkel) laden als Ellipse
/// ohne Exzentrizität und Neigung.
#[test]
fn circular_orbit_from_old_saves() {
    let old = serde_json::json!({ "parent": 0, "radius": 12_000_000.0, "phase": PI });
    let orbit: BodyOrbit = serde_json::from_value(old).unwrap();
    assert_eq!(orbit, BodyOrbit::circular(0, 12_000_000.0, PI));
    let world = demo();
    assert_eq!(world.bodies[MOON].orbit, Some(orbit));
}

/// Ein Schiff, das auf Zimm zufliegt, tritt genau am Rand seiner Sphäre ein,
/// fliegt um ihn herum und verlässt sie wieder zum Planeten.
#[test]
fn flyby_of_the_outer_moon() {
    let mut world = World::new(demo().bodies);
    let moon = body_state(&world.bodies, OUTER, 0.0);
    let soi = sphere_of_influence(&world.bodies, OUTER);
    let toward = -moon.velocity.normalize();
    let aside = toward.cross(DVec3::Z).normalize();
    let state = StateVector {
        position: moon.position - toward * 1.3 * soi + aside * 0.2 * soi,
        velocity: moon.velocity + toward * 300.0,
    };
    world.spawn_vessel("Sonde", sim::scenario::SATELLITE, 100.0, 0, state);
    let segments: Vec<_> = world.vessels[0].trajectory.segments().cloned().collect();
    assert_eq!(segments[0].end_event(), Some(Event::SoiEntry(OUTER)));
    let entry = segments[0].end();
    let ship = segments[0].state_at(&world.bodies, entry).position;
    let center = body_state(&world.bodies, OUTER, entry).position;
    assert!(
        (ship.distance(center) - soi).abs() < 1.0,
        "{}",
        ship.distance(center)
    );
    assert_eq!(segments[1].body(), OUTER);
    assert_eq!(segments[1].end_event(), Some(Event::SoiExit));
    assert_eq!(segments[2].body(), 0);
}
