//! Tests für die Landung: Fallschirm, Aufsetzen, Wrack, Zielgebiete, Hop.

use glam::DVec3;
use sim::kepler::state_from_apsides;
use sim::scenario::{FERRY, SATELLITE, demo, home_planet};
use sim::{Crash, Launch, LaunchProfile, ManeuverNode, VesselParams, VesselStatus, World};

/// Ein Schiff auf einer Kreisbahn in 100 km Höhe.
fn in_orbit(params: VesselParams, fuel: f64) -> World {
    let mut world = World::new(vec![home_planet()]);
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius);
    let state = state_from_apsides(gm, r + 1e5, r + 1e5, 0.1, 0.0, 0.0, true);
    world.spawn_vessel("Test", params, fuel, 0, state);
    world
}

/// Retrograd bremsen, so dass die Bahn in die Luft und auf den Boden führt.
fn deorbit(world: &mut World, delta_v: f64) {
    let node = ManeuverNode {
        time: 300.0,
        delta_v: DVec3::new(-delta_v, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
    world.set_armed(0, true).unwrap();
}

/// Am Fallschirm kommt die Fähre langsam an; ohne Fallschirm schnell.
#[test]
fn parachute_slows_descent() {
    let speed = |parachute_area| {
        let params = VesselParams {
            parachute_area,
            ..FERRY
        };
        let mut world = in_orbit(params, 1_000.0);
        deorbit(&mut world, 150.0);
        world.touchdown(0).unwrap().speed
    };
    let with = speed(FERRY.parachute_area);
    let without = speed(0.0);
    assert!(with < 15.0, "mit Fallschirm {with} m/s");
    assert!(without > 50.0, "ohne Fallschirm {without} m/s");
}

/// Aus dem Orbit gelandet: Vorschau und Ausführung stimmen überein, der
/// Treibstoff fürs Bremsen ist abgezogen, die Kette ist erledigt.
#[test]
fn ferry_lands_from_orbit() {
    let mut world = in_orbit(FERRY, 1_000.0);
    deorbit(&mut world, 150.0);
    let touchdown = world.touchdown(0).unwrap();
    let braking = touchdown.braking.unwrap();
    assert!(braking > 5.0 && braking < 15.0, "Bremsen {braking}");
    let budget = world.delta_v_budget(0);

    world.advance_to(touchdown.time + 10.0);
    assert_eq!(
        world.vessel_status(0),
        VesselStatus::Landed {
            point: touchdown.point
        }
    );
    assert!(world.vessels[0].trajectory.nodes().is_empty());
    let rest = world.vessel_delta_v(0);
    assert!(
        (rest - budget.rest).abs() < 1e-6,
        "{rest} statt {}",
        budget.rest
    );
}

/// Ohne Fallschirm und mit wenig Treibstoff reicht das Bremsen nicht.
#[test]
fn crash_without_enough_fuel() {
    let params = VesselParams {
        parachute_area: 0.0,
        ..FERRY
    };
    let mut world = in_orbit(params, 50.0);
    deorbit(&mut world, 60.0);
    let touchdown = world.touchdown(0).unwrap();
    let Err(Crash::NoFuel { available }) = touchdown.braking else {
        panic!("Absturz erwartet, nicht {:?}", touchdown.braking);
    };
    assert!(available < touchdown.speed);
    world.advance_to(touchdown.time + 10.0);
    assert!(matches!(world.vessel_status(0), VesselStatus::Wreck { .. }));
}

/// Ein Satellit hat zu wenig Schub zum Abbremsen, auch mit Treibstoff.
#[test]
fn satellite_is_too_weak_to_land() {
    let mut world = in_orbit(SATELLITE, 200.0);
    deorbit(&mut world, 150.0);
    let touchdown = world.touchdown(0).unwrap();
    let Err(Crash::TooWeak { thrust_to_weight }) = touchdown.braking else {
        panic!("Absturz erwartet, nicht {:?}", touchdown.braking);
    };
    // 2000 N gegen 600 kg Gewicht (abzüglich verbrauchtem Treibstoff).
    assert!(
        thrust_to_weight > 0.3 && thrust_to_weight < 0.5,
        "{thrust_to_weight}"
    );
    world.advance_to(touchdown.time + 10.0);
    assert!(matches!(world.vessel_status(0), VesselStatus::Wreck { .. }));
}

/// Hop von der Startbasis zum Übungsfeld der Demo: Mit Kurs 90° und einer
/// passenden Ziel-Apoapsis landet die Fähre im Zielgebiet.
#[test]
fn hop_to_practice_field() {
    let field = demo()
        .zones
        .iter()
        .position(|z| z.name == "Übungsfeld")
        .unwrap();
    let hop = |apoapsis_km: f64| {
        let mut world = demo();
        let profile = LaunchProfile {
            target_apoapsis: apoapsis_km * 1000.0,
            ..LaunchProfile::default()
        };
        world
            .set_launch(
                2,
                Launch {
                    time: 60.0,
                    profile,
                },
            )
            .unwrap();
        world.set_armed(2, true).unwrap();
        world
    };
    let mut world = (20..35)
        .map(|km| hop(km as f64))
        .find(|w| w.touchdown(2).is_some_and(|t| t.zone == Some(field)))
        .expect("eine Ziel-Apoapsis zwischen 20 und 35 km trifft das Übungsfeld");
    let touchdown = world.touchdown(2).unwrap();
    assert!(touchdown.braking.is_ok());

    world.advance_to(touchdown.time + 10.0);
    let VesselStatus::Landed { point } = world.vessel_status(2) else {
        panic!("gelandet erwartet");
    };
    assert_eq!(world.zone_at(0, &point), Some(field));
}

/// Außerhalb eines Zielgebiets: aufgesetzt, die Entfernung zum nächsten wird
/// angegeben.
#[test]
fn outside_zone_reports_distance() {
    let mut world = demo();
    let profile = LaunchProfile {
        target_apoapsis: 20_000.0,
        heading: 0.0,
        ..LaunchProfile::default()
    };
    world
        .set_launch(
            2,
            Launch {
                time: 60.0,
                profile,
            },
        )
        .unwrap();
    let touchdown = world.touchdown(2).unwrap();
    assert!(touchdown.braking.is_ok());
    assert_eq!(touchdown.zone, None);
    let (nearest, distance) = touchdown.nearest.unwrap();
    assert_eq!(world.zones[nearest].name, "Startbasis");
    assert!(distance > 10_000.0, "{distance}");
}

/// Regel 6 mit Landung: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn landing_is_independent_of_step_size() {
    let setup = || {
        let mut world = demo();
        let profile = LaunchProfile {
            target_apoapsis: 25_000.0,
            ..LaunchProfile::default()
        };
        world
            .set_launch(
                2,
                Launch {
                    time: 60.0,
                    profile,
                },
            )
            .unwrap();
        world.set_armed(2, true).unwrap();
        world
    };
    let target = 1_200.0;
    let mut one_step = setup();
    one_step.advance_to(target);
    let mut many_steps = setup();
    for i in 1..=10_000 {
        many_steps.advance_to(target * i as f64 / 10_000.0);
    }
    assert!(matches!(
        one_step.vessel_status(2),
        VesselStatus::Landed { .. }
    ));
    assert_eq!(one_step.vessel_status(2), many_steps.vessel_status(2));
    assert_eq!(one_step.vessel_mass(2), many_steps.vessel_mass(2));
}
