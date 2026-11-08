//! Tests für Manöver-Nodes und Brennmanöver.

use glam::DVec3;
use sim::kepler::{self, state_from_apsides};
use sim::scenario::{SATELLITE, demo, home_planet};
use sim::{ManeuverNode, PlanError, Segment, VesselParams, VesselStatus, World};

const DAY: f64 = 86_400.0;
const BOUNDARY_ALT: f64 = 70_000.0;

/// Ein Satellit (Werte wie in der Demo) auf einer Bahn mit den gegebenen Höhen.
fn satellite(periapsis_alt: f64, apoapsis_alt: f64, fuel: f64) -> World {
    let mut world = World::new(vec![home_planet()]);
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius);
    let state = state_from_apsides(gm, r + periapsis_alt, r + apoapsis_alt, 0.3, 0.0, 0.0, true);
    world.spawn_vessel("Test", SATELLITE, fuel, 0, state);
    world
}

fn prograde(dv: f64) -> DVec3 {
    DVec3::new(dv, 0.0, 0.0)
}

/// Bahnwerte (Periapsis-, Apoapsis-Höhe) des ersten Keplerabschnitts nach dem Brennen.
fn orbit_after_burn(world: &World, index: usize) -> (f64, f64) {
    let trajectory = &world.vessels[index].trajectory;
    let Some(Segment::Kepler { state, .. }) = trajectory.segment_after_burn() else {
        panic!("nach dem Brennen muss eine Keplerbahn folgen");
    };
    let body = &world.bodies[0];
    let info = kepler::orbit_info(body.gm, state);
    (
        info.periapsis - body.radius,
        info.apoapsis.unwrap() - body.radius,
    )
}

/// Ein kurzes Brennmanöver landet fast genau dort, wo ein sofortiger
/// Geschwindigkeitssprung am Node hinführen würde.
#[test]
fn short_burn_matches_impulse() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    let node = ManeuverNode {
        time: 600.0,
        delta_v: prograde(50.0),
    };
    world.set_node(0, node).unwrap();
    let (_, apoapsis) = orbit_after_burn(&world, 0);

    let gm = world.bodies[0].gm;
    let mut impulse = world.vessels[0].trajectory.plan().unwrap().node_state;
    impulse.velocity += impulse.velocity.normalize() * 50.0;
    let expected = kepler::orbit_info(gm, &impulse).apoapsis.unwrap() - world.bodies[0].radius;
    // 12 s Brenndauer: Abweichung gemessen rund 5 m.
    assert!(
        (apoapsis - expected).abs() < 20.0,
        "{apoapsis} vs {expected}"
    );
}

/// Der Verbrauch folgt exakt der Raketengleichung, und der Node verschwindet
/// nach der Ausführung.
#[test]
fn fuel_follows_rocket_equation() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    let m0 = world.vessel_mass(0);
    world
        .set_node(
            0,
            ManeuverNode {
                time: 600.0,
                delta_v: prograde(300.0),
            },
        )
        .unwrap();
    let burn = world.vessels[0].trajectory.plan().unwrap().burn;
    world.advance_to(burn.start + 1.0);
    assert!(matches!(
        world.vessel_status(0),
        VesselStatus::Burning { .. }
    ));

    world.advance_to(burn.end + 1.0);
    let expected = m0 * (-300.0 / SATELLITE.exhaust_velocity).exp();
    assert!((world.vessel_mass(0) - expected).abs() < 1e-9);
    assert!(world.vessels[0].trajectory.plan().is_none());
    assert!(matches!(world.vessel_status(0), VesselStatus::Orbit { .. }));
}

/// Reicht der Treibstoff nicht, endet das Manöver mit leerem Tank.
#[test]
fn empty_tank_ends_burn_early() {
    let mut world = satellite(100_000.0, 100_000.0, 50.0);
    let available = world.vessel_delta_v(0);
    world
        .set_node(
            0,
            ManeuverNode {
                time: 600.0,
                delta_v: prograde(5_000.0),
            },
        )
        .unwrap();
    let burn = world.vessels[0].trajectory.plan().unwrap().burn;
    assert!((burn.delta_v() - available).abs() < 1e-9);
    world.advance_to(burn.end + 1.0);
    assert!(world.vessel_fuel(0).abs() < 1e-9);
}

/// Brennen funktioniert auch in der Atmosphäre: dasselbe Kraftmodell, kein
/// Sonderfall. Prograd an der Periapsis hebt die Apoapsis deutlich an.
#[test]
fn burn_inside_atmosphere() {
    let without = satellite(62_000.0, 150_000.0, 200.0);
    // Mitte des ersten Durchgangs durch die Luft.
    let pass = without.vessels[0]
        .trajectory
        .segments()
        .find(|s| matches!(s, Segment::Integrated { .. }))
        .unwrap();
    let node_time = 0.5 * (pass.start() + pass.end());
    let apoapsis_after_pass = |world: &World| {
        let mut seen_pass = false;
        for s in world.vessels[0].trajectory.segments() {
            if matches!(s, Segment::Integrated { .. }) {
                seen_pass = true;
            } else if let (true, Segment::Kepler { state, .. }) = (seen_pass, s) {
                let body = &world.bodies[0];
                return kepler::orbit_info(body.gm, state).apoapsis.unwrap() - body.radius;
            }
        }
        panic!("kein Keplerabschnitt nach dem Durchgang");
    };

    let mut with = without.clone();
    with.set_node(
        0,
        ManeuverNode {
            time: node_time,
            delta_v: prograde(100.0),
        },
    )
    .unwrap();
    let burn = with.vessels[0].trajectory.plan().unwrap().burn;
    let body = &with.bodies[0];
    let altitude = with.vessels[0]
        .trajectory
        .segments()
        .nth(1)
        .unwrap()
        .state_at(&with.bodies, burn.start);
    assert!(
        altitude.position.length() - body.radius < BOUNDARY_ALT,
        "Brennbeginn muss in der Luft liegen"
    );
    assert!(apoapsis_after_pass(&with) > apoapsis_after_pass(&without) + 100_000.0);
}

/// Der vorhergesagte Aufschlagpunkt ist genau dort, wo das Wrack liegt.
#[test]
fn impact_prediction_matches_wreck() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    assert!(world.vessels[0].trajectory.predicted_impact().is_none());
    world
        .set_node(
            0,
            ManeuverNode {
                time: 600.0,
                delta_v: prograde(-300.0),
            },
        )
        .unwrap();
    let (time, body, point) = world.vessels[0].trajectory.predicted_impact().unwrap();
    assert_eq!(body, 0);
    world.advance_to(time + 10.0);
    assert_eq!(world.vessel_status(0), VesselStatus::Wreck { point });
}

/// Sonde-2 aus der Demo lässt sich mit einem kleinen Manöver an der Apoapsis
/// retten: Die Periapsis steigt über die Atmosphäre, die Bahn bleibt stabil.
#[test]
fn rescue_probe_at_apoapsis() {
    let mut world = demo();
    // Erst den ersten Durchgang durch die Luft abwarten.
    world.advance_to(1_300.0);
    let (_, state) = world.vessel_state(1);
    let apoapsis_in = kepler::time_to_apoapsis(world.bodies[0].gm, &state).unwrap();
    world
        .set_node(
            1,
            ManeuverNode {
                time: world.time + apoapsis_in,
                delta_v: prograde(30.0),
            },
        )
        .unwrap();
    let (periapsis, _) = orbit_after_burn(&world, 1);
    assert!(periapsis > BOUNDARY_ALT, "Periapsis {periapsis}");
    assert!(matches!(
        world.vessels[1].trajectory.segment_after_burn(),
        Some(Segment::Kepler { end: None, .. })
    ));

    world.advance_to(world.time + DAY);
    assert!(matches!(
        world.vessel_status(1),
        VesselStatus::Orbit {
            descending: false,
            ..
        }
    ));
}

/// Node setzen und wieder löschen ergibt exakt die ursprüngliche Zeitleiste.
#[test]
fn clearing_node_restores_trajectory() {
    let mut world = demo();
    world.advance_to(500.0);
    let before = world.vessels[1].trajectory.clone();
    world
        .set_node(
            1,
            ManeuverNode {
                time: 2_000.0,
                delta_v: DVec3::new(20.0, 5.0, -3.0),
            },
        )
        .unwrap();
    assert_ne!(world.vessels[1].trajectory, before);
    world.clear_node(1).unwrap();
    assert_eq!(world.vessels[1].trajectory, before);
}

/// Regel 6 gilt auch mit Manöver: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn node_result_is_independent_of_step_size() {
    let setup = || {
        let mut world = demo();
        world.advance_to(100.0);
        for (index, delta_v) in [(0, prograde(-250.0)), (1, DVec3::new(30.0, 10.0, 5.0))] {
            world
                .set_node(
                    index,
                    ManeuverNode {
                        time: 1_500.0,
                        delta_v,
                    },
                )
                .unwrap();
        }
        world
    };
    let target = 4.0 * 3600.0 + 17.0;
    let mut one_step = setup();
    one_step.advance_to(target);
    let mut many_steps = setup();
    for i in 1..=10_000 {
        many_steps.advance_to(100.0 + (target - 100.0) * i as f64 / 10_000.0);
    }
    many_steps.advance_to(target);
    for i in 0..2 {
        assert_eq!(one_step.vessel_state(i), many_steps.vessel_state(i));
        assert_eq!(one_step.vessel_status(i), many_steps.vessel_status(i));
        assert_eq!(one_step.vessel_mass(i), many_steps.vessel_mass(i));
    }
}

/// Ein Spielstand mit geplantem Node lädt bitgenau.
#[test]
fn save_and_load_with_node() {
    let mut world = demo();
    world
        .set_node(
            1,
            ManeuverNode {
                time: 2_000.0,
                delta_v: prograde(25.0),
            },
        )
        .unwrap();
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(world, loaded);
}

#[test]
fn invalid_nodes_are_rejected() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    world.advance_to(100.0);
    let node = |time| ManeuverNode {
        time,
        delta_v: prograde(10.0),
    };
    assert_eq!(world.set_node(0, node(50.0)), Err(PlanError::InPast));

    let mut no_engine = World::new(vec![home_planet()]);
    let (gm, r) = (no_engine.bodies[0].gm, no_engine.bodies[0].radius);
    let params = VesselParams {
        thrust: 0.0,
        ..SATELLITE
    };
    let state = state_from_apsides(gm, r + 1e5, r + 1e5, 0.0, 0.0, 0.0, false);
    no_engine.spawn_vessel("Passiv", params, 0.0, 0, state);
    assert_eq!(no_engine.set_node(0, node(500.0)), Err(PlanError::NoEngine));

    // Nach dem Absturz gibt es nichts mehr zu planen.
    let mut crashed = satellite(30_000.0, 60_000.0, 200.0);
    crashed.advance_to(DAY);
    assert_eq!(
        crashed.set_node(0, node(DAY + 100.0)),
        Err(PlanError::NotFlying)
    );

    // Während des Brennens lässt sich der Node nicht ändern.
    world.set_node(0, node(600.0)).unwrap();
    let burn = world.vessels[0].trajectory.plan().unwrap().burn;
    world.advance_to(0.5 * (burn.start + burn.end));
    assert_eq!(world.set_node(0, node(900.0)), Err(PlanError::Burning));
    assert_eq!(world.clear_node(0), Err(PlanError::Burning));
}
