//! Tests für die Körper auf Schienen: Sonne im Ursprung, Planeten und Monde
//! auf Kreisen und geneigten Ellipsen, Einflusssphären, gebundene Rotation.

use glam::DVec3;
use sim::body::{body_state, day_length, orbital_period, sphere_of_influence};
use sim::scenario::{NURA, ZIMM, ZOON, ZORB, ZUNN, demo};
use sim::{Event, StateVector, World, kepler};

/// Lage und Geschwindigkeit von `body` relativ zu seinem Mutterkörper.
fn relative(world: &World, body: usize, t: f64) -> StateVector {
    let parent = world.bodies[body].orbit.as_ref().unwrap().parent;
    let (s, p) = (
        body_state(&world.bodies, body, t),
        body_state(&world.bodies, parent, t),
    );
    StateVector {
        position: s.position - p.position,
        velocity: s.velocity - p.velocity,
    }
}

/// Zoon: Kreisbahn in 12.000 km Abstand um Zorb, im Drehsinn des Planeten;
/// nach einem Umlauf wieder am selben Ort, die Geschwindigkeit passt zur Lage.
#[test]
fn moon_follows_its_orbit() {
    let world = demo();
    let bodies = &world.bodies;
    let period = orbital_period(bodies, ZOON).unwrap();
    for t in [0.0, 12_345.0, 3.0e7] {
        let s = relative(&world, ZOON, t);
        assert!((s.position.length() - 12_000_000.0).abs() < 1e-5, "{s:?}");
        assert!(s.position.z.abs() < 1e-6, "Äquatorebene");
        let circular = (bodies[ZORB].gm / 12_000_000.0).sqrt();
        assert!((s.velocity.length() - circular).abs() < 1e-9, "{s:?}");

        let again = relative(&world, ZOON, t + period);
        assert!(again.position.distance(s.position) < 1e-3, "{again:?}");
        // Ein Viertel weiter: 90° gegen den Uhrzeigersinn (von Norden aus).
        let quarter = relative(&world, ZOON, t + 0.25 * period).position;
        assert!(s.position.normalize().dot(quarter.normalize()).abs() < 1e-9);
        assert!(s.position.cross(quarter).z > 0.0);

        // Geschwindigkeit = Ableitung der Lage.
        let before = relative(&world, ZOON, t - 1.0).position;
        let after = relative(&world, ZOON, t + 1.0).position;
        let slope = (after - before) / 2.0;
        assert!(slope.distance(s.velocity) < 1e-3, "{slope} {}", s.velocity);
    }
}

/// Werte wie beim Mun in KSP: Umlauf 138.984 s (38,6 h), Einflusssphäre
/// 2.429,6 km.
#[test]
fn moon_matches_the_mun() {
    let world = demo();
    let period = orbital_period(&world.bodies, ZOON).unwrap();
    assert!((period / 138_984.0 - 1.0).abs() < 1e-4, "{period}");
    let soi = sphere_of_influence(&world.bodies, ZOON);
    assert!((soi / 2_429_559.0 - 1.0).abs() < 1e-3, "{soi}");
}

/// Die Sonne ruht im Ursprung und reicht unendlich weit. Zorb kreist wie
/// Kerbin um sie (13.599.840 km, Jahr 9.203.545 s = 426 Tage, Sphäre
/// 84.159 km), Nura wie Duna (Umlauf 17.315.000 s, Sphäre 47.922 km).
#[test]
fn planets_go_around_the_sun() {
    let world = demo();
    let bodies = &world.bodies;
    assert_eq!(bodies[ZUNN].name, "Zunn");
    assert_eq!(world.home, ZORB);
    let sun = body_state(bodies, ZUNN, 1_000.0);
    assert_eq!((sun.position, sun.velocity), (DVec3::ZERO, DVec3::ZERO));
    assert_eq!(sphere_of_influence(bodies, ZUNN), f64::INFINITY);
    assert_eq!(orbital_period(bodies, ZUNN), None);

    let year = orbital_period(bodies, ZORB).unwrap();
    assert!((year / 9_203_545.0 - 1.0).abs() < 1e-6, "{year}");
    // Der Sonnentag dauert genau 6 h, das Jahr 426,09 Tage.
    let day = day_length(bodies, ZORB);
    assert!((day - 21_600.0).abs() < 0.01, "{day}");
    assert!((year / day - 426.09).abs() < 0.01);
    assert_eq!(day_length(bodies, ZUNN), bodies[ZUNN].rotation_period);
    let zorb = body_state(bodies, ZORB, 0.0);
    assert!((zorb.position.length() / 13_599_840_256.0 - 1.0).abs() < 1e-12);
    // Zum Zeitpunkt 0 steht Zunn in +X von Zorb aus.
    assert!((-zorb.position).normalize().distance(DVec3::X) < 1e-6);
    let home = sphere_of_influence(bodies, ZORB);
    assert!((home / 84_159_286.0 - 1.0).abs() < 1e-6, "{home}");

    assert_eq!(bodies[NURA].name, "Nura");
    let period = orbital_period(bodies, NURA).unwrap();
    assert!((period / 17_315_000.0 - 1.0).abs() < 1e-3, "{period}");
    let soi = sphere_of_influence(bodies, NURA);
    assert!((soi / 47_921_949.0 - 1.0).abs() < 1e-3, "{soi}");
    let g = bodies[NURA].gm / bodies[NURA].radius.powi(2);
    assert!((g - 2.943).abs() < 1e-9);
}

/// Gebundene Rotation: Länge 0° zeigt immer zum Planeten.
#[test]
fn moon_is_tidally_locked() {
    let world = demo();
    let moon = &world.bodies[ZOON];
    for t in [0.0, 5_000.0, 77_777.0, 3.0e7] {
        let to_planet = -relative(&world, ZOON, t).position;
        let fixed = moon.inertial_to_body_fixed(to_planet, t).normalize();
        assert!(fixed.distance(DVec3::X) < 1e-9, "{t}: {fixed}");
    }
}

/// Alle Körper gehören zum Spielstand.
#[test]
fn bodies_are_saved() {
    let world = demo();
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, world);
}

/// Zimm läuft auf einer geneigten Ellipse: Seine Lage stimmt mit einer
/// unabhängigen Keplerbahn aus seinem Anfangszustand überein, Abstände,
/// Neigung, Knoten und Umlauf passen zu den Bahnelementen.
#[test]
fn outer_moon_follows_its_ellipse() {
    let world = demo();
    let bodies = &world.bodies;
    assert_eq!(bodies[ZIMM].name, "Zimm");
    let orbit = bodies[ZIMM].orbit.clone().unwrap();
    assert_eq!(orbit.parent, ZORB);
    let gm = bodies[ZORB].gm;
    let start = relative(&world, ZIMM, 0.0);
    let period = orbital_period(bodies, ZIMM).unwrap();
    assert!((period / 1_077_311.0 - 1.0).abs() < 1e-5, "{period}");

    let (mut nearest, mut farthest) = (f64::INFINITY, 0.0f64);
    for i in 0..=1_000 {
        let t = period * i as f64 / 1_000.0 + 12_345.0;
        let s = relative(&world, ZIMM, t);
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
        let s = relative(&world, ZIMM, t);
        let again = relative(&world, ZIMM, t + period);
        assert!(again.position.distance(s.position) < 1e-2, "{t}");
    }
    // Geschwindigkeit = Ableitung der Lage.
    let t = 500_000.0;
    let before = relative(&world, ZIMM, t - 1.0).position;
    let after = relative(&world, ZIMM, t + 1.0).position;
    let slope = (after - before) / 2.0;
    assert!(slope.distance(relative(&world, ZIMM, t).velocity) < 1e-3);

    // Einflusssphäre aus der großen Halbachse, wie bei Minmus.
    let soi = sphere_of_influence(bodies, ZIMM);
    assert!((soi / 2_247_000.0 - 1.0).abs() < 1e-3, "{soi}");
}

/// Ein Schiff, das auf Zimm zufliegt, tritt genau am Rand seiner Sphäre ein,
/// fliegt um ihn herum und verlässt sie wieder zum Planeten.
#[test]
fn flyby_of_the_outer_moon() {
    let mut world = demo();
    world.vessels.clear();
    let moon = relative(&world, ZIMM, 0.0);
    let soi = sphere_of_influence(&world.bodies, ZIMM);
    let toward = -moon.velocity.normalize();
    let aside = toward.cross(DVec3::Z).normalize();
    let state = StateVector {
        position: moon.position - toward * 1.3 * soi + aside * 0.2 * soi,
        velocity: moon.velocity + toward * 300.0,
    };
    world.spawn_vessel("Sonde", sim::scenario::SATELLITE, 100.0, ZORB, state);
    let segments: Vec<_> = world.vessels[0].trajectory.segments().cloned().collect();
    assert_eq!(segments[0].end_event(), Some(Event::SoiEntry(ZIMM)));
    let entry = segments[0].end();
    let ship = segments[0].state_at(&world.bodies, entry).position;
    let center = relative(&world, ZIMM, entry).position;
    let distance = ship.distance(center);
    assert!((distance - soi).abs() < 1.0, "{distance}");
    assert_eq!(segments[1].body(), ZIMM);
    assert_eq!(segments[1].end_event(), Some(Event::SoiExit));
    assert_eq!(segments[2].body(), ZORB);
}
