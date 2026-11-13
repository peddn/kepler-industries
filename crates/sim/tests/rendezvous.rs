//! Tests fürs Rendezvous: Ziel, nächste Annäherung, Bahnebenen.

mod common;

use common::{FERRY, SATELLITE};
use glam::DVec3;
use sim::kepler::{period, state_from_apsides};
use sim::scenario::home_planet;
use sim::{ManeuverNode, World};
use std::f64::consts::{FRAC_PI_2, PI};

const ALTITUDE: f64 = 100_000.0;

/// Kreisbahn mit Neigung `inclination`, aufsteigendem Knoten `node` und dem
/// Schiff bei `angle` (vom Knoten aus in Bewegungsrichtung).
fn circular(
    world: &World,
    altitude: f64,
    inclination: f64,
    node: f64,
    angle: f64,
) -> sim::StateVector {
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius);
    state_from_apsides(
        gm,
        r + altitude,
        r + altitude,
        inclination,
        node,
        angle,
        false,
    )
}

/// Zwei Schiffe auf gleich hohen Kreisbahnen, 30° gegeneinander geneigt. Beide
/// erreichen den Schnittpunkt der Ebenen (+Y) nach einem Viertel Umlauf.
fn crossing_orbits() -> World {
    let mut world = World::new(vec![home_planet()]);
    let a = circular(&world, ALTITUDE, 0.0, 0.0, 0.0);
    let b = circular(&world, ALTITUDE, 30f64.to_radians(), FRAC_PI_2, -FRAC_PI_2);
    world.spawn_vessel("A", SATELLITE, 100.0, 0, a);
    world.spawn_vessel("B", SATELLITE, 100.0, 0, b);
    world.set_target(0, Some(1));
    world
}

/// Treffen am Schnittpunkt: Abstand null, Relativgeschwindigkeit aus dem
/// Winkel zwischen den Bahnen.
#[test]
fn approach_where_orbits_cross() {
    let world = crossing_orbits();
    let (_, state) = world.vessel_state(0);
    let t = period(world.bodies[0].gm, &state).unwrap();
    let approach = world.approach(0).expect("Annäherung");
    assert!(approach.distance < 1.0, "{approach:?}");
    // Treffen nach einem Viertel und nach drei Vierteln Umlauf (gegenüber).
    let halves = (approach.time - 0.25 * t) / (0.5 * t);
    assert!((halves - halves.round()).abs() < 1e-4, "{approach:?}");
    let speed = state.velocity.length();
    let expected = 2.0 * speed * 15f64.to_radians().sin();
    assert!(
        (approach.relative_speed - expected).abs() < 1e-3 * expected,
        "{} statt {expected}",
        approach.relative_speed
    );
}

/// Neigung 30°, die Knoten liegen einen halben Umlauf auseinander und auf der
/// Ebene des Ziels; am aufsteigenden Knoten geht es nach oben.
#[test]
fn plane_crossing_of_inclined_orbits() {
    let world = crossing_orbits();
    let plane = world.plane_to_target(0).expect("Bahnebene");
    assert!((plane.inclination.to_degrees() - 30.0).abs() < 1e-9);
    let (asc, desc) = (plane.ascending.unwrap(), plane.descending.unwrap());
    let (_, state) = world.vessel_state(0);
    let t = period(world.bodies[0].gm, &state).unwrap();
    assert!(((asc - desc).abs() - 0.5 * t).abs() < 1e-3, "{asc} {desc}");

    let trajectory = &world.vessels[0].trajectory;
    let (_, b) = world.vessel_state(1);
    let normal = b.position.cross(b.velocity).normalize();
    for (time, up) in [(asc, true), (desc, false)] {
        let (_, s) = trajectory.predicted_state(&world.bodies, time).unwrap();
        assert!(s.position.dot(normal).abs() < 1e-3, "auf der Ebene");
        assert_eq!(s.velocity.dot(normal) > 0.0, up);
    }
    // Von der anderen Seite gesehen dieselbe Neigung.
    let mut other = world.clone();
    other.set_target(1, Some(0));
    let back = other.plane_to_target(1).unwrap();
    assert!((back.inclination - plane.inclination).abs() < 1e-12);
}

/// Hohmann-Transfer von 100 km auf 200 km: Mit passendem Abstand der beiden
/// Schiffe trifft der Node das Ziel in der Apoapsis. Die Annäherung gilt für
/// die Bahn nach dem geplanten Manöver.
#[test]
fn approach_after_planned_transfer() {
    let mut world = World::new(vec![home_planet()]);
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius);
    let (r1, r2) = (r + ALTITUDE, r + 200_000.0);
    let (w1, w2) = ((gm / r1.powi(3)).sqrt(), (gm / r2.powi(3)).sqrt());
    let transfer = 0.5 * (r1 + r2);
    let half_transfer = PI * (transfer.powi(3) / gm).sqrt();
    let node_time = 300.0;
    let arrival = node_time + half_transfer;
    // Ziel so setzen, dass es bei der Ankunft gegenüber dem Node steht.
    let target_angle = w1 * node_time + PI - w2 * arrival;

    let a = circular(&world, ALTITUDE, 0.0, 0.0, 0.0);
    let b = circular(&world, 200_000.0, 0.0, 0.0, target_angle);
    world.spawn_vessel("Fähre", FERRY, 1_000.0, 0, a);
    world.spawn_vessel("Ziel", SATELLITE, 100.0, 0, b);
    world.set_target(0, Some(1));

    let before = world.approach(0).map_or(f64::INFINITY, |a| a.distance);
    let dv = (gm / r1).sqrt() * ((r2 / transfer).sqrt() - 1.0);
    world
        .add_node(
            0,
            ManeuverNode {
                time: node_time,
                delta_v: DVec3::new(dv, 0.0, 0.0),
            },
        )
        .unwrap();
    let approach = world.approach(0).expect("Annäherung");
    assert!(approach.distance < 2_000.0, "{approach:?}");
    assert!(before > 50_000.0, "ohne Manöver weit weg: {before}");
    assert!((approach.time - arrival).abs() < 30.0, "{approach:?}");
    assert_eq!(approach.revolutions, 0);
}

/// Ohne Ziel keine Annäherung; ein Schiff am Boden ohne Start hat keine Bahn.
#[test]
fn no_target_or_no_orbit() {
    let mut world = crossing_orbits();
    world.set_target(0, None);
    assert_eq!(world.approach(0), None);
    assert_eq!(world.plane_to_target(0), None);

    world.spawn_landed(
        "Boden",
        FERRY,
        1_000.0,
        0,
        sim::SurfacePoint::from_degrees(0.0, 0.0),
    );
    world.set_target(2, Some(0));
    assert_eq!(world.approach(2), None);
    assert_eq!(world.plane_to_target(2), None);
    // Ein Ziel am Boden hat keine Bahnebene.
    world.set_target(0, Some(2));
    assert_eq!(world.plane_to_target(0), None);
}

/// Am Boden zählt keine Annäherung (auch nicht, wenn das Ziel darüber
/// hinwegfliegt); mit geplantem Start ab dem Abheben.
#[test]
fn approach_only_in_flight() {
    let mut world = sim::scenario::demo();
    world.set_target(3, Some(1));
    assert_eq!(world.approach(3), None);
    let liftoff = 11_333.0;
    let profile = sim::LaunchProfile {
        heading: 125.0,
        ..sim::LaunchProfile::default()
    };
    world
        .set_launch(
            3,
            sim::Launch {
                time: liftoff,
                profile,
            },
        )
        .unwrap();
    let approach = world.approach(3).expect("Annäherung nach dem Start");
    assert!(approach.time > liftoff, "{approach:?}");
    let plane = world.plane_to_target(3).unwrap();
    assert!(
        plane.inclination.to_degrees() < 1.0,
        "Startfenster: {plane:?}"
    );
}

/// Das Ziel gehört zum Spielstand.
#[test]
fn target_is_saved() {
    let world = crossing_orbits();
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.vessels[0].target, Some(1));
    assert_eq!(loaded, world);
}

/// Zwei Schiffe auf geneigten Bahnen in 100 und 200 km Höhe; das Ziel ist um
/// `extra` (rad) stärker geneigt, gleicher aufsteigender Knoten.
fn two_planes(extra: f64) -> World {
    let mut world = World::new(vec![home_planet()]);
    let a = circular(&world, ALTITUDE, 0.3, 1.0, 0.0);
    let b = circular(&world, 200_000.0, 0.3 + extra, 1.0, 2.0);
    world.spawn_vessel("A", SATELLITE, 100.0, 0, a);
    world.spawn_vessel("B", SATELLITE, 100.0, 0, b);
    world.set_target(0, Some(1));
    world
}

/// Dieselbe Ebene (wie nach dem Abdocken): keine Knoten, statt Knoten, die im
/// Rundungsrauschen umherspringen.
#[test]
fn same_plane_has_no_nodes() {
    let mut world = two_planes(0.0);
    for _ in 0..5 {
        let plane = world.plane_to_target(0).unwrap();
        assert!(plane.coplanar, "{plane:?}");
        assert_eq!((plane.ascending, plane.descending), (None, None));
        assert!(plane.inclination < 1e-12, "{plane:?}");
        world.advance_to(world.time + 1.0);
    }
}

/// Eine kleine, echte Neigung (0,01°): Die Knoten liegen auf der Zielebene
/// und bleiben an ihrer Stelle, während die Zeit vergeht.
#[test]
fn small_inclination_has_steady_nodes() {
    let mut world = two_planes(0.01f64.to_radians());
    let at = |world: &World, t: f64| {
        world.vessels[0]
            .trajectory
            .predicted_state(&world.bodies, t)
            .unwrap()
            .1
            .position
    };
    let (_, b) = world.vessel_state(1);
    let normal = b.position.cross(b.velocity).normalize();
    let first = world.plane_to_target(0).unwrap();
    assert!(!first.coplanar);
    assert!((first.inclination.to_degrees() - 0.01).abs() < 1e-9);
    let (asc, desc) = (first.ascending.unwrap(), first.descending.unwrap());
    for t in [asc, desc] {
        assert!(at(&world, t).dot(normal).abs() < 1e-3, "auf der Zielebene");
    }
    for _ in 0..10 {
        world.advance_to(world.time + 1.0);
        let plane = world.plane_to_target(0).unwrap();
        let moved = at(&world, plane.ascending.unwrap()).distance(at(&world, asc));
        assert!(moved < 1e-3, "AN um {moved} m gesprungen");
        let moved = at(&world, plane.descending.unwrap()).distance(at(&world, desc));
        assert!(moved < 1e-3, "DN um {moved} m gesprungen");
    }
}
