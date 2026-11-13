//! Tests für Entwürfe aus Komponenten: Flugwerte, Preise und was sich bauen
//! lässt.

use sim::design::{Chassis, Design, DesignError, Engine, FOUNDATION_SPACE, Part, SPACE_STEP};
use sim::scenario::start;
use sim::{BaseError, VesselParams};

/// Die Startentwürfe ergeben die abgestimmten Schiffe: Massen, Tanks und
/// Preise; der Frachter hat zwei Z-1.
#[test]
fn start_designs_match_the_catalogue() {
    let freighter = Design::freighter();
    assert_eq!(
        freighter.params(),
        VesselParams {
            dry_mass: 2_500.0,
            drag_area: 5.0,
            thrust: 500_000.0,
            exhaust_velocity: 3_200.0,
            fuel_capacity: 14_000.0,
            parachute_area: 1_000.0,
            cargo_capacity: 2_000.0,
            base_kit: false,
        }
    );
    assert_eq!(freighter.cargo(), 2_000.0);
    assert_eq!(freighter.price(), 1_000.0);

    let base_ship = Design::base_ship();
    assert_eq!(
        base_ship.params(),
        VesselParams {
            dry_mass: 3_000.0,
            drag_area: 4.0,
            thrust: 250_000.0,
            exhaust_velocity: 3_200.0,
            fuel_capacity: 7_000.0,
            parachute_area: 400.0,
            cargo_capacity: 0.0,
            base_kit: true,
        }
    );
    assert_eq!(base_ship.free_space(), 10_000.0 - FOUNDATION_SPACE);
    assert_eq!(base_ship.cargo(), 0.0);
    assert_eq!(base_ship.price(), 1_000.0);

    let unlocked = Part::from_start();
    assert_eq!(freighter.check(&unlocked), Ok(()));
    assert_eq!(base_ship.check(&unlocked), Ok(()));
}

/// Der Innenraum teilt sich in Tank und Fracht: Was der Tank nicht nimmt,
/// ist Frachtraum, und Masse und Preis ändern sich dabei nicht.
#[test]
fn interior_splits_into_tank_and_cargo() {
    let mut design = Design::freighter();
    let before = design.params();
    design.tank = 10_000.0;
    let after = design.params();
    assert_eq!(
        (after.fuel_capacity, after.cargo_capacity),
        (10_000.0, 6_000.0)
    );
    assert_eq!(after.dry_mass, before.dry_mass);
    assert_eq!(design.price(), 1_000.0);
    // Ein Basisschiff mit kleinerem Tank trägt Fracht.
    let mut base_ship = Design::base_ship();
    base_ship.tank = 6_000.0;
    assert_eq!(base_ship.params().cargo_capacity, 1_000.0);
    assert!(base_ship.params().base_kit);
}

/// Triebwerke: Schub und Masse je Stück, die Sorte bestimmt die
/// Ausströmgeschwindigkeit und den Preis.
#[test]
fn engines_add_thrust_mass_and_price() {
    let mut design = Design::freighter();
    design.engines = 1;
    let one = design.params();
    assert_eq!((one.thrust, one.dry_mass), (250_000.0, 2_200.0));
    assert_eq!(design.price(), 800.0);
    design.engine = Engine::Z3;
    design.engines = 2;
    let better = design.params();
    assert_eq!(better.exhaust_velocity, 4_000.0);
    assert_eq!((better.thrust, better.dry_mass), (500_000.0, 2_700.0));
    assert_eq!(design.price(), 600.0 + 2.0 * 800.0);
}

/// Was sich nicht bauen lässt: zu viele Triebwerke, ein Tank außerhalb des
/// freien Innenraums oder neben dem Raster, gesperrte Teile.
#[test]
fn invalid_designs_are_refused() {
    let unlocked = Part::from_start();
    let with = |change: fn(&mut Design)| {
        let mut design = Design::freighter();
        change(&mut design);
        design.check(&unlocked)
    };
    assert_eq!(
        with(|d| d.engines = 3),
        Err(DesignError::TooManyEngines { slots: 2 })
    );
    assert_eq!(
        with(|d| d.tank = 16_500.0),
        Err(DesignError::TankOutOfRange { free: 16_000.0 })
    );
    assert_eq!(
        with(|d| d.tank = -SPACE_STEP),
        Err(DesignError::TankOutOfRange { free: 16_000.0 })
    );
    assert_eq!(with(|d| d.tank = 14_200.0), Err(DesignError::TankOffStep));
    assert_eq!(
        with(|d| d.engine = Engine::Z2),
        Err(DesignError::Locked(Part::Engine(Engine::Z2)))
    );
    assert_eq!(
        with(|d| d.chassis = Chassis::Large),
        Err(DesignError::Locked(Part::Chassis(Chassis::Large)))
    );
    // Mit Fundament ist weniger Platz.
    assert_eq!(
        with(|d| d.foundation = true),
        Err(DesignError::TankOutOfRange { free: 13_000.0 })
    );
    // Ganz ohne Tank oder ohne Triebwerk lässt es sich bauen (fliegen nicht).
    assert_eq!(with(|d| d.tank = 0.0), Ok(()));
    assert_eq!(with(|d| d.engines = 0), Ok(()));
}

/// Bestellen: nach einem Entwurf aus der Liste, zu seinem Preis; das Schiff
/// merkt sich den Entwurf. Was sich nicht bauen lässt, wird nicht bestellt.
#[test]
fn ships_are_ordered_from_designs() {
    let mut world = start();
    assert_eq!(
        world.designs,
        vec![Design::base_ship(), Design::freighter()]
    );
    assert_eq!(world.unlocked, Part::from_start());
    assert_eq!(world.order_ship(7), Err(BaseError::NoDesign));

    let mut cheap = Design::freighter();
    cheap.name = "Leichter".into();
    cheap.engines = 1;
    cheap.tank = 12_000.0;
    world.designs.push(cheap.clone());
    let ship = world.order_ship(2).unwrap();
    let v = &world.vessels[ship];
    assert_eq!(v.name, "Leichter-1");
    assert_eq!(v.params, cheap.params());
    assert_eq!(v.design, Some(cheap));
    assert_eq!(world.zorit, 3_000.0 - 800.0);
    assert!((world.vessel_fuel(ship) - 12_000.0).abs() < 1e-9);

    world.designs[2].engine = Engine::Z2;
    assert_eq!(
        world.order_ship(2),
        Err(BaseError::Design(DesignError::Locked(Part::Engine(
            Engine::Z2
        ))))
    );
    world.unlocked.push(Part::Engine(Engine::Z2));
    world.order_ship(2).unwrap();
    assert_eq!(world.zorit, 3_000.0 - 800.0 - 1_000.0);
}

/// Der Editor fragt, was ein Start auf dem Booster ergibt: dieselbe Planung
/// wie im Spiel, unabhängig davon, was der Booster gerade tut.
#[test]
fn launch_estimate_matches_the_real_launch() {
    let mut world = start();
    let design = Design::freighter();
    let full = world.launch_estimate(&design, 2_000.0).unwrap();
    let needed = full.to_orbit.unwrap();
    assert!((1_400.0..1_600.0).contains(&needed), "{needed}");
    assert!(full.left().unwrap() > 2_900.0, "{:?}", full.left());
    // Ohne Fracht bleibt mehr.
    let empty = world.launch_estimate(&design, 0.0).unwrap();
    assert!(empty.left().unwrap() > full.left().unwrap() + 1_000.0);

    // Auch während der Booster verplant ist.
    let ship = world.order_ship(1).unwrap();
    let start = sim::Launch {
        time: 100.0,
        profile: sim::LaunchProfile::default(),
    };
    world.set_launch(ship, start).unwrap();
    assert_eq!(world.launch_estimate(&design, 2_000.0), Ok(full));

    // Zu schwer für den Booster: gesperrtes großes Chassis, voll.
    let mut heavy = design.clone();
    heavy.chassis = Chassis::Large;
    heavy.tank = 30_000.0;
    assert_eq!(
        world.launch_estimate(&heavy, 2_000.0),
        Err(sim::PlanError::BoosterNoReturn)
    );
}

/// Ein Entwurf ohne Namen ergibt trotzdem ein benanntes Schiff.
#[test]
fn unnamed_design_names_the_ship() {
    let mut world = start();
    world.designs[1].name = "  ".into();
    let ship = world.order_ship(1).unwrap();
    assert_eq!(world.vessels[ship].name, "Schiff-1");
}
