//! Tests für den Δv-Haushalt (Grundlage des Δv-Balkens).

use glam::DVec3;
use sim::kepler::state_from_apsides;
use sim::scenario::{FERRY, SATELLITE, home_planet};
use sim::{BudgetItem, DeltaVBudget, Launch, LaunchProfile, ManeuverNode, SurfacePoint, World};

fn satellite(fuel: f64) -> World {
    let mut world = World::new(vec![home_planet()]);
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius);
    let state = state_from_apsides(gm, r + 1e5, r + 1e5, 0.3, 0.0, 0.0, true);
    world.spawn_vessel("Test", SATELLITE, fuel, 0, state);
    world
}

fn prograde(time: f64, dv: f64) -> ManeuverNode {
    ManeuverNode {
        time,
        delta_v: DVec3::new(dv, 0.0, 0.0),
    }
}

/// Die Teile ergeben zusammen genau das Δv an Bord.
fn assert_adds_up(budget: &DeltaVBudget) {
    let used: f64 = budget.burns.iter().map(|(_, dv)| dv).sum();
    assert!(
        (used + budget.rest - budget.available).abs() < 1e-6,
        "{budget:?}"
    );
    assert!(budget.available <= budget.full + 1e-9);
}

#[test]
fn launch_and_nodes_add_up() {
    let mut world = World::new(vec![home_planet()]);
    world.spawn_landed(
        "Fähre",
        FERRY,
        3_600.0,
        0,
        SurfacePoint::from_degrees(5.0, -20.0),
    );
    let launch = Launch {
        time: 10.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(0, launch).unwrap();
    world.add_node(0, prograde(400.0, 50.0)).unwrap();
    let budget = world.delta_v_budget(0);
    assert_adds_up(&budget);
    let items: Vec<BudgetItem> = budget.burns.iter().map(|(item, _)| *item).collect();
    // Ohne Zirkularisieren kommt die Fähre wieder herunter und landet am
    // Fallschirm: Das Bremsen gehört mit in den Haushalt.
    assert_eq!(
        items,
        [BudgetItem::Launch, BudgetItem::Node(0), BudgetItem::Landing]
    );
    assert!((budget.burns[1].1 - 50.0).abs() < 1e-6);
    assert!(budget.missing < 1e-9, "{}", budget.missing);
    // Voller Tank wäre mehr.
    assert!((budget.full - FERRY.full_delta_v()).abs() < 1e-9);
    assert!(budget.available < budget.full);
}

/// Will die Kette mehr als im Tank ist, endet das letzte Brennen leer, und der
/// Rest steht unter "fehlt".
#[test]
fn missing_when_chain_wants_too_much() {
    let mut world = satellite(100.0);
    world.add_node(0, prograde(600.0, 400.0)).unwrap();
    world.add_node(0, prograde(2_500.0, 400.0)).unwrap();
    let budget = world.delta_v_budget(0);
    assert_adds_up(&budget);
    // 3000 m/s · ln(500/400) ≈ 669 m/s an Bord.
    assert!((budget.available - 3_000.0 * 1.25f64.ln()).abs() < 1e-9);
    assert!(budget.rest.abs() < 1e-6);
    assert!((budget.missing - (800.0 - budget.available)).abs() < 1e-6);
}

/// Ein laufendes Brennen zählt nur mit dem, was es noch verbraucht.
#[test]
fn running_burn_counts_its_rest() {
    let mut world = satellite(200.0);
    world.add_node(0, prograde(600.0, 300.0)).unwrap();
    world.set_armed(0, true).unwrap();
    let burn = world.vessels[0].trajectory.plans().next().unwrap().burn;
    world.advance_to(0.5 * (burn.start + burn.end));
    let budget = world.delta_v_budget(0);
    assert_adds_up(&budget);
    let (item, rest_of_burn) = budget.burns[0];
    assert_eq!(item, BudgetItem::Node(0));
    assert!(
        rest_of_burn > 100.0 && rest_of_burn < 200.0,
        "{rest_of_burn}"
    );
    assert!(budget.missing < 1e-9, "{}", budget.missing);
}
