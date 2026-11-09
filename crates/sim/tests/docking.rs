//! Tests fürs Andocken: Angleichen, Anflug, Kontakt, Verbund.

use glam::DVec3;
use sim::docking::{CONTACT_SPEED, DOCKING_RANGE, angular_velocity};
use sim::scenario::{FERRY, SATELLITE, demo};
use sim::{
    BudgetItem, DockError, Launch, LaunchProfile, LogKind, ManeuverNode, Segment, VesselStatus,
    World,
};

const RELAIS: usize = 0;
const FERRY_ID: usize = 2;

/// Das Rendezvous aus dem README: Fähre-3 startet im Startfenster zu
/// Relais-1, zirkularisiert und holt auf (engste Annäherung etwa 640 m).
/// `phasing`: Δv des zweiten Nodes.
fn rendezvous(phasing: f64) -> World {
    let mut world = demo();
    let profile = LaunchProfile {
        heading: 120.0,
        ..LaunchProfile::default()
    };
    world
        .set_launch(
            FERRY_ID,
            Launch {
                time: 11_325.0,
                profile,
            },
        )
        .unwrap();
    world.set_armed(FERRY_ID, true).unwrap();
    for (time, dv) in [(11_638.0, 682.0), (12_616.0, phasing)] {
        world
            .add_node(
                FERRY_ID,
                ManeuverNode {
                    time,
                    delta_v: DVec3::new(dv, 0.0, 0.0),
                },
            )
            .unwrap();
    }
    world.set_target(FERRY_ID, Some(RELAIS));
    world
}

fn docking() -> World {
    let mut world = rendezvous(99.75);
    world.set_docking(FERRY_ID, true).unwrap();
    world
}

fn plan(world: &World) -> sim::DockPlan {
    *world.vessels[FERRY_ID]
        .trajectory
        .dock_plan()
        .unwrap_or_else(|| panic!("{:?}", world.vessels[FERRY_ID].trajectory.docking()))
}

/// Angleichen an der engsten Annäherung mit Δv = Relativgeschwindigkeit,
/// danach ein kurzer Anflug; der Δv-Haushalt zählt beides.
#[test]
fn plan_after_rendezvous() {
    let before = rendezvous(99.75).approach(FERRY_ID).unwrap();
    let world = docking();
    let plan = plan(&world);
    assert_eq!(plan.matching.node.time, before.time);
    let matching = plan.matching.node.delta_v.length();
    assert!(
        (matching - before.relative_speed).abs() < 1e-6,
        "{matching} statt {}",
        before.relative_speed
    );
    assert!(plan.distance < DOCKING_RANGE, "{}", plan.distance);
    assert!(
        plan.approach_delta_v > 0.1 && plan.approach_delta_v < 20.0,
        "Anflug {} m/s",
        plan.approach_delta_v
    );
    let duration = plan.contact - plan.matching.burn.end;
    assert!(duration > 60.0 && duration < 1200.0, "Anflug {duration} s");

    let budget = world.delta_v_budget(FERRY_ID);
    let (item, delta_v) = *budget.burns.last().unwrap();
    assert_eq!(item, BudgetItem::Docking);
    assert!(
        (delta_v - matching - plan.approach_delta_v).abs() < 0.1,
        "{delta_v}"
    );
    let used: f64 = budget.burns.iter().map(|(_, dv)| dv).sum();
    assert!((used + budget.rest - budget.available).abs() < 1e-6);
}

/// Der Anflug schließt lückenlos an das Angleichen an und endet in der Mitte
/// des Ziels mit Kontaktgeschwindigkeit.
#[test]
fn approach_ends_at_target() {
    let world = docking();
    let plan = plan(&world);
    let trajectory = &world.vessels[FERRY_ID].trajectory;
    let segments: Vec<&Segment> = trajectory.segments().collect();
    let approach = segments.last().unwrap();
    assert!(matches!(approach, Segment::Approach { .. }));
    let burn = segments[segments.len() - 2];
    assert!(burn.is_burn());
    let t = plan.matching.burn.end;
    let gap =
        approach.state_at(&world.bodies, t).position - burn.state_at(&world.bodies, t).position;
    assert!(gap.length() < 1e-6, "Lücke {gap:?}");
    assert_eq!(approach.end(), plan.contact);

    let own = approach.state_at(&world.bodies, plan.contact);
    let (_, target) = world.vessels[RELAIS]
        .trajectory
        .predicted_state(&world.bodies, plan.contact)
        .unwrap();
    assert!(own.position.distance(target.position) < 1e-6);
    let relative = own.velocity - target.velocity;
    let rotating = angular_velocity(&target).cross(own.position - target.position);
    assert!(((relative - rotating).length() - CONTACT_SPEED).abs() < 1e-6);
}

/// Vorschau = Ausführung: Kontakt zur geplanten Zeit, der Verbund wiegt so
/// viel wie beide Schiffe beim Kontakt, das Protokoll vermerkt es.
#[test]
fn docking_executes_as_planned() {
    let mut world = docking();
    let plan = plan(&world);
    let relais_mass = world.vessel_mass(RELAIS);
    world.advance_to(plan.contact + 100.0);
    assert_eq!(
        world.vessel_status(FERRY_ID),
        VesselStatus::Docked { host: RELAIS }
    );
    assert_eq!(world.vessel_state(FERRY_ID), world.vessel_state(RELAIS));
    // Die eigene Kette ist erledigt.
    assert!(world.vessels[FERRY_ID].trajectory.docking().is_none());
    let mass = world.vessel_mass(RELAIS);
    assert!(
        (mass - relais_mass - plan.end_mass).abs() < 1e-9,
        "{mass} statt {}",
        relais_mass + plan.end_mass
    );
    let entry = world.log.last().unwrap();
    assert_eq!(entry.time, plan.contact);
    assert_eq!(entry.vessel, FERRY_ID);
    assert_eq!(entry.kind, LogKind::Docked { host: RELAIS });
    // Relais-1 bleibt auf seiner Bahn.
    assert!(matches!(
        world.vessel_status(RELAIS),
        VesselStatus::Orbit {
            descending: false,
            ..
        }
    ));
}

/// Der Verbund plant und brennt mit allen Triebwerken; Befehle an das
/// angedockte Schiff gehen an das Hauptschiff.
#[test]
fn verbund_burns_with_all_engines() {
    let mut world = docking();
    let contact = plan(&world).contact;
    world.advance_to(contact + 10.0);
    let params = world.params(RELAIS);
    assert_eq!(params.thrust, FERRY.thrust + SATELLITE.thrust);
    assert_eq!(params.dry_mass, FERRY.dry_mass + SATELLITE.dry_mass);
    let flow =
        FERRY.thrust / FERRY.exhaust_velocity + SATELLITE.thrust / SATELLITE.exhaust_velocity;
    assert!((params.exhaust_velocity - params.thrust / flow).abs() < 1e-9);

    world
        .add_node(
            FERRY_ID,
            ManeuverNode {
                time: contact + 600.0,
                delta_v: DVec3::new(20.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let burn = world.vessels[RELAIS]
        .trajectory
        .plans()
        .next()
        .unwrap()
        .burn;
    assert_eq!(burn.thrust, params.thrust);
    assert!((burn.delta_v() - 20.0).abs() < 1e-6);
    assert!(world.vessels[FERRY_ID].trajectory.nodes().is_empty());
}

/// Regel 6 mit Andocken: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn docking_is_independent_of_step_size() {
    let contact = plan(&docking()).contact;
    let target = contact + 600.0;
    let mut one_step = docking();
    one_step.advance_to(target);
    let mut many_steps = docking();
    for i in 1..=10_000 {
        many_steps.advance_to(target * i as f64 / 10_000.0);
    }
    assert_eq!(
        one_step.vessel_state(RELAIS),
        many_steps.vessel_state(RELAIS)
    );
    assert_eq!(one_step.vessel_mass(RELAIS), many_steps.vessel_mass(RELAIS));
    assert_eq!(one_step.log, many_steps.log);
}

/// Zu weit weg: ungültig mit Abstand.
#[test]
fn too_far_is_invalid() {
    let mut world = rendezvous(99.0);
    world.set_docking(FERRY_ID, true).unwrap();
    let docking = world.vessels[FERRY_ID].trajectory.docking().unwrap();
    let Err(DockError::TooFar { distance }) = docking.plan else {
        panic!("{docking:?}");
    };
    assert!(distance > DOCKING_RANGE);
    // Ungültig: kein Angleichen, kein Anflug.
    assert!(
        !world.vessels[FERRY_ID]
            .trajectory
            .segments()
            .any(|s| matches!(s, Segment::Approach { .. }))
    );
}

/// Plant das Ziel nach der Annäherung noch etwas, geht es nicht; ohne den
/// Node wieder schon. (Ein Node davor verschöbe die Annäherung selbst.)
#[test]
fn target_must_stay_on_its_orbit() {
    let mut world = docking();
    let node = ManeuverNode {
        time: plan(&world).contact + 300.0,
        delta_v: DVec3::new(5.0, 0.0, 0.0),
    };
    world.add_node(RELAIS, node).unwrap();
    let docking = world.vessels[FERRY_ID].trajectory.docking().unwrap();
    assert_eq!(docking.plan, Err(DockError::TargetNotStable));
    world.remove_node(RELAIS, 0).unwrap();
    assert!(world.vessels[FERRY_ID].trajectory.dock_plan().is_some());
}

/// Geplantes Andocken und Verbund gehören zum Spielstand.
#[test]
fn docking_is_saved() {
    let mut world = docking();
    let round_trip = |world: &World| {
        let json = serde_json::to_string(world).unwrap();
        let loaded: World = serde_json::from_str(&json).unwrap();
        assert_eq!(&loaded, world);
    };
    round_trip(&world);
    world.advance_to(plan(&world).contact + 10.0);
    round_trip(&world);
}
