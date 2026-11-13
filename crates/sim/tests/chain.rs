//! Tests für Manöverketten: mehrere Nodes pro Schiff, jeder auf der Bahn
//! nach den vorherigen.

mod common;

use common::SATELLITE;
use glam::DVec3;
use sim::kepler::{self, state_from_apsides};
use sim::scenario::{demo, home_planet};
use sim::{ManeuverNode, Plan, PlanError, Segment, VesselStatus, World};

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

fn plans(world: &World, vessel: usize) -> Vec<Plan> {
    world.vessels[vessel].trajectory.plans().copied().collect()
}

/// Periapsis- und Apoapsis-Höhe der Keplerbahn nach dem Brennen `plan`.
fn orbit_after(world: &World, vessel: usize, plan: &Plan) -> (f64, f64) {
    let trajectory = &world.vessels[vessel].trajectory;
    let Some(Segment::Kepler { state, .. }) = trajectory.segment_after(&plan.burn) else {
        panic!("nach dem Brennen muss eine Keplerbahn folgen");
    };
    let body = &world.bodies[0];
    let info = kepler::orbit_info(body.gm, state);
    (
        info.periapsis - body.radius,
        info.apoapsis.unwrap() - body.radius,
    )
}

/// Δv für einen Kreis auf der Höhe der Apoapsis, geplant an der Apoapsis der
/// Bahn nach `plan`: (Zeitpunkt, prograd).
fn circularize_at_apoapsis(world: &World, vessel: usize, plan: &Plan) -> ManeuverNode {
    let trajectory = &world.vessels[vessel].trajectory;
    let after = trajectory.segment_after(&plan.burn).unwrap();
    let gm = world.bodies[0].gm;
    let state = after.state_at(&world.bodies, after.start());
    let time = after.start() + kepler::time_to_apoapsis(gm, &state).unwrap();
    let at_apoapsis = after.state_at(&world.bodies, time);
    let circular = (gm / at_apoapsis.position.length()).sqrt();
    ManeuverNode {
        time,
        delta_v: prograde(circular - at_apoapsis.velocity.length()),
    }
}

/// Hohmann-Übergang von 100 auf 500 km mit zwei Nodes: Der zweite liegt auf
/// der Übergangsbahn, die erst durch den ersten entsteht. Vorschau und
/// Ausführung stimmen überein.
#[test]
fn hohmann_transfer_with_two_nodes() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    world
        .add_node(
            0,
            ManeuverNode {
                time: 600.0,
                delta_v: prograde(237.0),
            },
        )
        .unwrap();
    let first = plans(&world, 0)[0];
    let (_, apoapsis) = orbit_after(&world, 0, &first);
    assert!(
        (apoapsis - 500_000.0).abs() < 10_000.0,
        "Apoapsis {apoapsis}"
    );

    let second = circularize_at_apoapsis(&world, 0, &first);
    assert_eq!(world.add_node(0, second), Ok(1));
    let [first, second] = plans(&world, 0)[..] else {
        panic!("zwei gültige Nodes erwartet");
    };
    assert!(second.burn.start > first.burn.end);
    let (periapsis, apoapsis) = orbit_after(&world, 0, &second);
    assert!(
        (apoapsis - periapsis).abs() < 3_000.0,
        "{periapsis} / {apoapsis}"
    );
    assert!(
        (periapsis - 500_000.0).abs() < 10_000.0,
        "Periapsis {periapsis}"
    );

    // Ausführen: nur eine scharfe Kette brennt.
    world.set_armed(0, true).unwrap();
    world.advance_to(second.burn.end + 10.0);
    assert!(world.vessels[0].trajectory.nodes().is_empty());
    let VesselStatus::Orbit {
        periapsis_altitude,
        apoapsis_altitude: Some(apoapsis_altitude),
        descending: false,
    } = world.vessel_status(0)
    else {
        panic!("stabile Bahn erwartet");
    };
    assert!((periapsis_altitude - periapsis).abs() < 1e-3);
    assert!((apoapsis_altitude - apoapsis).abs() < 1e-3);
}

/// Ändert sich der erste Node, wandert der zweite mit: gleiche Zeit, gleiches
/// Δv, aber auf der neuen Bahn. Zurückgeändert ist alles wieder wie vorher.
#[test]
fn later_node_moves_with_earlier() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    let first = ManeuverNode {
        time: 600.0,
        delta_v: prograde(100.0),
    };
    world.add_node(0, first).unwrap();
    world
        .add_node(
            0,
            ManeuverNode {
                time: 1_800.0,
                delta_v: prograde(20.0),
            },
        )
        .unwrap();
    let before = world.vessels[0].trajectory.clone();

    world
        .set_node(
            0,
            0,
            ManeuverNode {
                delta_v: prograde(150.0),
                ..first
            },
        )
        .unwrap();
    let changed = &world.vessels[0].trajectory.nodes()[1];
    assert_eq!(changed.node, before.nodes()[1].node);
    let moved = changed.plan.unwrap().node_state.position;
    let old = before.nodes()[1].plan.unwrap().node_state.position;
    assert!(
        (moved - old).length() > 10_000.0,
        "der zweite Node muss mitwandern"
    );

    world.set_node(0, 0, first).unwrap();
    assert_eq!(world.vessels[0].trajectory, before);
}

/// Ein neuer Node wird zeitlich eingeordnet. Löschen stellt die Bahn wieder her.
#[test]
fn nodes_are_ordered_and_removable() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    let free = world.vessels[0].trajectory.clone();
    let node = |time, dv| ManeuverNode {
        time,
        delta_v: prograde(dv),
    };
    assert_eq!(world.add_node(0, node(1_800.0, 10.0)), Ok(0));
    assert_eq!(world.add_node(0, node(600.0, 20.0)), Ok(0));
    assert_eq!(world.add_node(0, node(1_200.0, 30.0)), Ok(1));
    let times: Vec<f64> = plans(&world, 0).iter().map(|p| p.node.time).collect();
    assert_eq!(times, [600.0, 1_200.0, 1_800.0]);
    // Jeder Node liegt auf der Bahn nach den vorherigen: drei Brennvorgänge.
    let burns = world.vessels[0]
        .trajectory
        .segments()
        .filter(|s| s.is_burn())
        .count();
    assert_eq!(burns, 3);

    world.remove_node(0, 1).unwrap();
    let times: Vec<f64> = plans(&world, 0).iter().map(|p| p.node.time).collect();
    assert_eq!(times, [600.0, 1_800.0]);
    world.remove_node(0, 0).unwrap();
    world.remove_node(0, 0).unwrap();
    assert_eq!(world.vessels[0].trajectory, free);
    assert_eq!(world.remove_node(0, 0), Err(PlanError::NoNode));
}

/// Stürzt das Schiff wegen des ersten Nodes vorher ab, wird der zweite
/// ungültig, bleibt aber erhalten. Zurückgeändert ist er wieder gültig.
#[test]
fn invalid_node_is_kept_and_recovers() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    let first = ManeuverNode {
        time: 600.0,
        delta_v: prograde(20.0),
    };
    world.add_node(0, first).unwrap();
    world
        .add_node(
            0,
            ManeuverNode {
                time: 2_400.0,
                delta_v: prograde(20.0),
            },
        )
        .unwrap();
    let before = world.vessels[0].trajectory.clone();

    // Stark bremsen: Absturz lange vor dem zweiten Node.
    world
        .set_node(
            0,
            0,
            ManeuverNode {
                delta_v: prograde(-300.0),
                ..first
            },
        )
        .unwrap();
    let trajectory = &world.vessels[0].trajectory;
    assert_eq!(trajectory.nodes().len(), 2);
    assert_eq!(trajectory.nodes()[1].plan, Err(PlanError::OnGround));
    assert_eq!(trajectory.plans().count(), 1);
    assert_eq!(trajectory.segments().filter(|s| s.is_burn()).count(), 1);
    // Den ungültigen direkt zu ändern geht nicht.
    let second = trajectory.nodes()[1].node;
    assert_eq!(world.set_node(0, 1, second), Err(PlanError::OnGround));

    world.set_node(0, 0, first).unwrap();
    assert_eq!(world.vessels[0].trajectory, before);
}

/// Liegt der zweite Node so, dass sein Brennen vor dem Ende des ersten
/// begänne, rückt er auf seiner Bahn einen Umlauf weiter.
#[test]
fn overlapping_node_moves_to_next_orbit() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    world
        .add_node(
            0,
            ManeuverNode {
                time: 600.0,
                delta_v: prograde(100.0),
            },
        )
        .unwrap();
    let first = plans(&world, 0)[0];
    // Kurz nach dem ersten Node, mitten in dessen Brennen.
    world
        .add_node(
            0,
            ManeuverNode {
                time: 610.0,
                delta_v: prograde(10.0),
            },
        )
        .unwrap();
    let [_, second] = plans(&world, 0)[..] else {
        panic!("zwei gültige Nodes erwartet");
    };
    let after = world.vessels[0]
        .trajectory
        .segment_after(&first.burn)
        .unwrap();
    let Segment::Kepler { state, .. } = after else {
        panic!("Keplerbahn erwartet");
    };
    let period = kepler::period(world.bodies[0].gm, state).unwrap();
    assert!((second.node.time - (610.0 + period)).abs() < 1e-6);
    assert!(second.burn.start > first.burn.end);
    // Gespeichert bleibt, wohin der Node gesetzt wurde.
    assert_eq!(world.vessels[0].trajectory.nodes()[1].node.time, 610.0);
}

/// Eine nicht scharfe Kette rückt bei Brennbeginn als Ganzes einen Umlauf
/// weiter und bleibt dabei an denselben Stellen der Bahn.
#[test]
fn unarmed_chain_moves_as_a_whole() {
    let mut world = satellite(100_000.0, 100_000.0, 200.0);
    let period = kepler::period(world.bodies[0].gm, &world.vessel_state(0).1).unwrap();
    let m0 = world.vessel_mass(0);
    world
        .add_node(
            0,
            ManeuverNode {
                time: 600.0,
                delta_v: prograde(100.0),
            },
        )
        .unwrap();
    world
        .add_node(
            0,
            ManeuverNode {
                time: 2_000.0,
                delta_v: prograde(-50.0),
            },
        )
        .unwrap();
    let before = plans(&world, 0);

    world.advance_to(700.0);
    let after = plans(&world, 0);
    assert_eq!(after.len(), 2);
    assert_eq!(world.vessel_mass(0), m0);
    for (b, a) in before.iter().zip(&after) {
        assert!((a.node.time - (b.node.time + period)).abs() < 1e-6);
        assert!((a.node_state.position - b.node_state.position).length() < 1.0);
    }
}

/// Auch ein Node mehrere Umläufe voraus auf einer zerfallenden Bahn zeigt die
/// Bahn nach dem Brennen, und die Umläufe bis dorthin werden gezählt.
#[test]
fn far_node_on_decaying_orbit() {
    let mut world = satellite(62_000.0, 150_000.0, 200.0);
    // Start in der Apoapsis. Vierter Freiflug über der Luft, hinter seiner
    // Apoapsis: drei Umläufe und etwas voraus.
    let segment = world.vessels[0]
        .trajectory
        .segments()
        .filter(|s| matches!(s, Segment::Kepler { .. }))
        .nth(3)
        .unwrap()
        .clone();
    let time = segment.start() + 0.75 * (segment.end() - segment.start());
    world
        .add_node(
            0,
            ManeuverNode {
                time,
                delta_v: prograde(30.0),
            },
        )
        .unwrap();
    let trajectory = &world.vessels[0].trajectory;
    let plan = trajectory.plans().next().unwrap();
    assert!(trajectory.segment_after(&plan.burn).is_some());
    assert_eq!(trajectory.revolutions(&world.bodies, 0.0, time), 3);
}

/// Auf einer stabilen Bahn zählen volle Umläufe.
#[test]
fn revolutions_on_stable_orbit() {
    let world = satellite(100_000.0, 300_000.0, 200.0);
    let period = kepler::period(world.bodies[0].gm, &world.vessel_state(0).1).unwrap();
    let trajectory = &world.vessels[0].trajectory;
    assert_eq!(trajectory.revolutions(&world.bodies, 0.0, 0.5 * period), 0);
    assert_eq!(trajectory.revolutions(&world.bodies, 0.0, 1.01 * period), 1);
    assert_eq!(trajectory.revolutions(&world.bodies, 0.0, 2.7 * period), 2);
}

/// Regel 6 mit Ketten: 1 Schritt == 10 000 Schritte, bitgenau. Frachter-1
/// führt eine scharfe Kette aus, die Kette von Frachter-2 ist nicht scharf.
#[test]
fn chain_result_is_independent_of_step_size() {
    let setup = || {
        let mut world = demo();
        world.advance_to(100.0);
        for (time, dv) in [(1_500.0, 120.0), (3_000.0, -40.0)] {
            world
                .add_node(
                    1,
                    ManeuverNode {
                        time,
                        delta_v: prograde(dv),
                    },
                )
                .unwrap();
        }
        world.set_armed(1, true).unwrap();
        world
            .add_node(
                2,
                ManeuverNode {
                    time: 1_500.0,
                    delta_v: DVec3::new(30.0, 10.0, 5.0),
                },
            )
            .unwrap();
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
    assert!(
        one_step.vessels[1].trajectory.nodes().is_empty(),
        "Kette ausgeführt"
    );
    for i in 1..3 {
        assert_eq!(one_step.vessel_state(i), many_steps.vessel_state(i));
        assert_eq!(one_step.vessel_status(i), many_steps.vessel_status(i));
        assert_eq!(one_step.vessel_mass(i), many_steps.vessel_mass(i));
    }
}

/// Ein Spielstand mit Kette, auch mit ungültigem Node, lädt bitgenau.
#[test]
fn save_and_load_with_chain() {
    let mut world = demo();
    let first = ManeuverNode {
        time: 600.0,
        delta_v: prograde(10.0),
    };
    world.add_node(1, first).unwrap();
    world
        .add_node(
            1,
            ManeuverNode {
                time: 2_400.0,
                delta_v: prograde(10.0),
            },
        )
        .unwrap();
    world
        .set_node(
            1,
            0,
            ManeuverNode {
                delta_v: prograde(-300.0),
                ..first
            },
        )
        .unwrap();
    assert!(world.vessels[1].trajectory.nodes()[1].plan.is_err());
    world
        .add_node(
            2,
            ManeuverNode {
                time: 2_000.0,
                delta_v: prograde(25.0),
            },
        )
        .unwrap();
    world.set_armed(2, true).unwrap();
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(world, loaded);
}
