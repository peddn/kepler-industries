//! Tests für den Techtree: Preise, Voraussetzungen und was das Freischalten
//! bewirkt.

use sim::scenario::{BOOSTER, BOOSTER_B2, ZORB, start};
use sim::{
    BoosterStatus, Chassis, Design, DesignError, Engine, Launch, LaunchProfile, Part, PlanError,
    Tech, TechError, World,
};

/// Spielbeginn mit `data` GB.
fn with_data(data: f64) -> World {
    let mut world = start();
    world.data = data;
    world
}

/// Freischalten kostet Daten, gilt einmal und gibt das Teil für Entwürfe
/// frei.
#[test]
fn unlocking_costs_data_and_frees_the_part() {
    let mut world = with_data(40.0);
    let mut design = Design::freighter();
    design.engine = Engine::Z2;
    let z2 = Part::Engine(Engine::Z2);
    assert_eq!(design.check(&world.unlocked), Err(DesignError::Locked(z2)));

    assert_eq!(world.can_unlock(Tech::EngineZ2), Ok(()));
    world.unlock(Tech::EngineZ2).unwrap();
    assert_eq!(world.data, 10.0);
    assert_eq!(world.techs, vec![Tech::EngineZ2]);
    assert_eq!(design.check(&world.unlocked), Ok(()));
    // Im Protokoll steht es nicht: Das gehört den Schiffen.
    assert!(world.log.is_empty());
    assert_eq!(world.unlock(Tech::EngineZ2), Err(TechError::Done));
    // Der Rest reicht nicht für das große Chassis.
    assert_eq!(
        world.unlock(Tech::ChassisLarge),
        Err(TechError::NotEnoughData)
    );
    assert_eq!(world.data, 10.0);

    // Ein Schiff mit dem neuen Triebwerk lässt sich bestellen.
    world.designs.push(design);
    let ship = world.order_ship(2).unwrap();
    assert_eq!(world.vessels[ship].params.exhaust_velocity, 3_600.0);
}

/// Z-3 setzt Z-2 voraus.
#[test]
fn prerequisites_come_first() {
    let mut world = with_data(500.0);
    assert_eq!(
        world.unlock(Tech::EngineZ3),
        Err(TechError::Requires(Tech::EngineZ2))
    );
    world.unlock(Tech::EngineZ2).unwrap();
    world.unlock(Tech::EngineZ3).unwrap();
    world.unlock(Tech::ChassisLarge).unwrap();
    assert_eq!(world.data, 500.0 - 30.0 - 100.0 - 30.0);
    for part in [
        Part::Engine(Engine::Z2),
        Part::Engine(Engine::Z3),
        Part::Chassis(Chassis::Large),
    ] {
        assert!(world.unlocked.contains(&part), "{part:?}");
    }
    // Alle zusammen kosten 210 GB.
    let total: f64 = Tech::ALL.iter().map(|t| t.spec().data).sum();
    assert_eq!(total, 210.0);
}

/// Der B-2 kostet Daten und Zorit und ersetzt den Booster, aber nur, wenn
/// der frei an der Startbasis steht.
#[test]
fn booster_b2_replaces_the_booster() {
    let mut world = with_data(60.0);
    let booster = world.booster.unwrap();
    assert_eq!(world.vessels[booster].params, BOOSTER);

    world.zorit = 2_000.0;
    assert_eq!(
        world.unlock(Tech::BoosterB2),
        Err(TechError::NotEnoughZorit)
    );
    world.zorit = 4_000.0;
    // Verplant für einen Start: kein Tausch.
    let ship = world.order_ship(1).unwrap();
    let launch = Launch {
        time: 100.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(ship, launch).unwrap();
    assert_eq!(world.unlock(Tech::BoosterB2), Err(TechError::BoosterBusy));
    world.clear_launch(ship).unwrap();

    world.unlock(Tech::BoosterB2).unwrap();
    assert_eq!((world.data, world.zorit), (10.0, 0.0));
    assert_eq!(world.vessels[booster].params, BOOSTER_B2);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
    assert!((world.vessel_fuel(booster) - BOOSTER_B2.fuel_capacity).abs() < 1e-9);
    // Der nächste Start läuft mit ihm.
    world.set_launch(ship, launch).unwrap();
    let plan = world.vessels[ship].trajectory.launch().unwrap();
    assert_eq!(plan.booster.unwrap().params, BOOSTER_B2);
}

/// Wofür der B-2 da ist: Ein volles großes Chassis trägt der erste Booster
/// nicht, der B-2 schon, und er kommt zurück.
#[test]
fn b2_carries_a_full_large_chassis() {
    let mut world = with_data(500.0);
    world.zorit = 10_000.0;
    world.unlock(Tech::ChassisLarge).unwrap();
    let large = Design {
        name: "Schwer".into(),
        chassis: Chassis::Large,
        engine: Engine::Z1,
        engines: 4,
        foundation: false,
        tank: 28_000.0,
    };
    assert_eq!(large.check(&world.unlocked), Ok(()));
    assert_eq!(
        world.launch_estimate(&large, large.cargo()),
        Err(PlanError::BoosterNoReturn)
    );

    world.unlock(Tech::BoosterB2).unwrap();
    let estimate = world.launch_estimate(&large, large.cargo()).unwrap();
    let left = estimate.left().unwrap();
    assert!(left > 2_700.0, "{left}");
    // Und er fliegt wirklich: Start, Trennung, Rückkehr.
    world.designs.push(large);
    let ship = world.order_ship(2).unwrap();
    world.load_cargo(ship, 4_000.0);
    let launch = Launch {
        time: 100.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(ship, launch).unwrap();
    world.set_armed(ship, true).unwrap();
    let plan = *world.vessels[ship].trajectory.launch().unwrap();
    world.advance_to(plan.booster.unwrap().back.landing + 1.0);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
    let _ = ZORB;
}
