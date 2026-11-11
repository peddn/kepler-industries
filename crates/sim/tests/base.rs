//! Tests für Fracht und Basen: Fracht als Masse, Gründen, Abladen, Treibstoff.

use glam::DVec3;
use sim::base::{BASE_KIT, FUEL_RATE, FUEL_STORAGE, SITE_RADIUS};
use sim::body::SurfacePoint;
use sim::scenario::{BASE_SHIP, FREIGHTER, ZOON, ZORB, demo};
use sim::{BaseError, Launch, LaunchProfile, LogKind, ManeuverNode, PlanError, World};

/// Demo ohne Schiffe; ein Basisschiff steht auf Zoon bei 10° Nord, 30° Ost.
fn on_zoon() -> (World, usize, SurfacePoint) {
    let mut world = demo();
    world.vessels.clear();
    let point = SurfacePoint::from_degrees(10.0, 30.0);
    let ship = world.spawn_landed("Basisschiff", BASE_SHIP, 1_000.0, ZOON, point);
    (world, ship, point)
}

/// Ein Frachter mit `cargo` kg Bauteilen und `fuel` kg Treibstoff, `east`
/// Grad östlich der Basis (1° sind auf Zoon rund 3,4 km).
fn freighter(world: &mut World, point: SurfacePoint, east: f64, cargo: f64, fuel: f64) -> usize {
    let at = SurfacePoint::from_degrees(10.0, 30.0 + east);
    assert!(point.latitude == at.latitude);
    let ship = world.spawn_landed("Frachter", FREIGHTER, fuel, ZOON, at);
    world.load_cargo(ship, cargo);
    ship
}

fn launch() -> Launch {
    Launch {
        time: 100.0,
        profile: LaunchProfile {
            heading: 90.0,
            turn_altitude: 2_000.0,
            target_apoapsis: 20_000.0,
            kick: 10.0,
        },
    }
}

/// Fracht zählt als Masse: weniger Δv, derselbe Treibstoff, dieselbe Bahn.
#[test]
fn cargo_counts_as_mass() {
    let world = demo();
    let (loaded, empty) = (5, {
        let mut w = demo();
        w.vessels[5].cargo = 0.0;
        w
    });
    assert_eq!(world.vessels[loaded].cargo, 2_000.0);
    assert_eq!(world.vessel_mass(loaded), 1_500.0 + 2_000.0 + 7_500.0);
    assert!((world.vessel_fuel(loaded) - 7_500.0).abs() < 1e-9);
    let expected = 3_200.0 * (11_000.0f64 / 3_500.0).ln();
    assert!((world.vessel_delta_v(loaded) - expected).abs() < 1e-9);
    assert!(world.delta_v_budget(loaded).full < FREIGHTER.full_delta_v());
    assert_eq!(world.vessel_state(loaded), empty.vessel_state(loaded));
    // Die Startausstattung reicht genau für eine Basis.
    let kit: f64 = world.vessels.iter().map(|v| v.cargo).sum();
    assert_eq!(kit, BASE_KIT);
}

/// Gründen: nur ein gelandetes Basisschiff, nicht auf Zorb, nur einmal. Es
/// wird zum Fundament (fliegt nicht mehr), der Landepunkt zur Zone.
#[test]
fn base_ship_founds_a_base() {
    let world = demo();
    let mut flying = world.clone();
    assert_eq!(flying.found_base(4), Err(BaseError::NotLanded));
    assert_eq!(flying.found_base(2), Err(BaseError::NoBaseKit));
    flying.vessels.clear();
    let pad = SurfacePoint::from_degrees(5.0, 60.0);
    let home = flying.spawn_landed("Basisschiff", BASE_SHIP, 0.0, ZORB, pad);
    assert_eq!(flying.found_base(home), Err(BaseError::HomePlanet));

    let (mut world, ship, point) = on_zoon();
    world.set_launch(ship, launch()).unwrap();
    assert_eq!(world.found_base(ship), Err(BaseError::Busy));
    world.clear_launch(ship).unwrap();

    let (bases, zones) = (world.bases.len(), world.zones.len());
    let base = world.found_base(ship).unwrap();
    assert_eq!(
        (base, world.bases.len(), world.zones.len()),
        (bases, bases + 1, zones + 1)
    );
    let site = &world.bases[base];
    assert_eq!(site.name, "Zoon-Basis 1");
    assert_eq!((site.body, site.location), (ZOON, point));
    assert_eq!((site.required, site.delivered), (BASE_KIT, 0.0));
    assert!(!site.complete() && site.fuel.is_none());
    let zone = world.zones.last().unwrap();
    assert_eq!(
        (zone.name.as_str(), zone.radius),
        ("Zoon-Basis 1", SITE_RADIUS)
    );
    assert_eq!(world.zone_at(ZOON, &point), Some(zones));
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::BaseFounded { base: b } if b == base
    ));

    assert_eq!(world.vessels[ship].foundation_of, Some(base));
    assert_eq!(world.set_launch(ship, launch()), Err(PlanError::Foundation));
    assert_eq!(world.found_base(ship), Err(BaseError::NoBaseKit));

    // Ein zweites Basisschiff: im Umkreis nicht, daneben schon.
    let near = SurfacePoint::from_degrees(10.0, 31.0);
    let second = world.spawn_landed("Zweites", BASE_SHIP, 0.0, ZOON, near);
    assert_eq!(world.found_base(second), Err(BaseError::Occupied));
    let far = SurfacePoint::from_degrees(10.0, 33.0);
    let third = world.spawn_landed("Drittes", BASE_SHIP, 0.0, ZOON, far);
    let other = world.found_base(third).unwrap();
    assert_eq!(world.bases[other].name, "Zoon-Basis 2");

    // Alles gehört zum Spielstand.
    let json = serde_json::to_string(&world).unwrap();
    assert_eq!(serde_json::from_str::<World>(&json).unwrap(), world);
}

/// Abladen: nur im Umkreis, nur so viel, wie die Baustelle braucht. Das Schiff
/// wird leichter (mehr Δv), der Treibstoff bleibt. Mit dem letzten Bauteil
/// ist die Basis fertig.
#[test]
fn freighters_complete_the_base() {
    let (mut world, ship, point) = on_zoon();
    let outside = freighter(&mut world, point, 2.0, 2_000.0, 3_000.0);
    assert_eq!(world.unload(outside), Err(BaseError::NoBase));
    let base = world.found_base(ship).unwrap();
    assert_eq!(world.unload(outside), Err(BaseError::NoBase));
    assert_eq!(world.unload(ship), Err(BaseError::NothingToUnload));

    let first = freighter(&mut world, point, 1.0, 2_000.0, 3_000.0);
    assert_eq!(world.base_of(first), Some(base));
    assert_eq!(world.refuel(first), Err(BaseError::Unfinished));
    let before = world.vessel_delta_v(first);
    world.set_launch(first, launch()).unwrap();
    assert_eq!(world.unload(first), Err(BaseError::Busy));
    world.clear_launch(first).unwrap();
    assert_eq!(world.unload(first), Ok(2_000.0));
    assert_eq!(world.vessels[first].cargo, 0.0);
    assert_eq!(world.vessel_mass(first), 1_500.0 + 3_000.0);
    assert!((world.vessel_fuel(first) - 3_000.0).abs() < 1e-9);
    assert!(world.vessel_delta_v(first) > before + 500.0);
    assert_eq!(world.unload(first), Err(BaseError::NothingToUnload));
    assert_eq!(world.bases[base].delivered, 2_000.0);
    assert!(!world.bases[base].complete());
    // Leer startet er wie jede Fähre.
    world.set_launch(first, launch()).unwrap();

    // Der zweite bringt mehr, als fehlt: Der Rest bleibt an Bord.
    world.vessels[first].params.cargo_capacity = 0.0;
    let mut big = FREIGHTER;
    big.cargo_capacity = 3_000.0;
    let at = SurfacePoint::from_degrees(10.0, 29.5);
    let second = world.spawn_landed("Großer", big, 1_000.0, ZOON, at);
    world.load_cargo(second, 3_000.0);
    world.advance(50.0);
    assert_eq!(world.unload(second), Ok(2_000.0));
    assert_eq!(world.vessels[second].cargo, 1_000.0);
    let site = &world.bases[base];
    assert!(site.complete());
    assert_eq!(site.fuel.map(|s| (s.amount, s.since)), Some((0.0, 50.0)));
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::BaseCompleted { base: b } if b == base
    ));
    assert_eq!(world.unload(second), Err(BaseError::NothingToUnload));
}

/// Eine fertige Basis mit einem leeren Frachter daneben.
fn finished() -> (World, usize, usize) {
    let (mut world, ship, point) = on_zoon();
    let base = world.found_base(ship).unwrap();
    for east in [0.5, -0.5] {
        let f = freighter(&mut world, point, east, 2_000.0, 0.0);
        world.unload(f).unwrap();
    }
    assert!(world.bases[base].complete());
    (world, base, 1)
}

/// Die Basis stellt gleichmäßig Treibstoff her, bis das Lager voll ist. Der
/// Stand hängt nur von der Zeit ab, nicht von den Schritten dahin (Regel 6),
/// und Tanken ändert nichts daran, wie viel insgesamt entsteht.
#[test]
fn base_makes_fuel() {
    let (mut world, base, ship) = finished();
    assert_eq!(world.base_fuel(base), 0.0);
    assert_eq!(world.refuel(ship), Err(BaseError::NothingToRefuel));

    let mut stepped = world.clone();
    world.advance(36_000.0);
    for _ in 0..3_600 {
        stepped.advance(10.0);
    }
    assert_eq!(world.base_fuel(base), FUEL_RATE * 36_000.0);
    assert_eq!(
        stepped.base_fuel(base).to_bits(),
        world.base_fuel(base).to_bits()
    );

    // Tanken: alles aus dem Lager in den Tank, das Schiff wird schwerer.
    assert_eq!(world.refuel(ship), Ok(1_800.0));
    assert_eq!(world.base_fuel(base), 0.0);
    assert!((world.vessel_fuel(ship) - 1_800.0).abs() < 1e-9);
    assert_eq!(world.vessel_mass(ship), 1_500.0 + 1_800.0);
    assert_eq!(world.refuel(ship), Err(BaseError::NothingToRefuel));

    // Das Lager läuft nicht über; getankt wird nur, was in den Tank passt.
    world.advance(1.0e7);
    assert_eq!(world.base_fuel(base), FUEL_STORAGE);
    let room = FREIGHTER.fuel_capacity - 1_800.0;
    let amount = world.refuel(ship).unwrap();
    assert!((amount - room).abs() < 1e-9, "{amount}");
    assert!((world.base_fuel(base) - (FUEL_STORAGE - room)).abs() < 1e-9);
    assert_eq!(world.refuel(ship), Err(BaseError::NothingToRefuel));
    // Danach füllt es sich wieder.
    world.advance(100.0);
    let stock = FUEL_STORAGE - room + FUEL_RATE * 100.0;
    assert!((world.base_fuel(base) - stock).abs() < 1e-9);

    // Vollgetankt hebt der Frachter wieder ab.
    world
        .set_launch(
            ship,
            Launch {
                time: world.time + 100.0,
                ..launch()
            },
        )
        .unwrap();
    assert!(world.vessels[ship].trajectory.launch().is_some());
}

/// Der ganze Ablauf aus der Demo: Basisschiff und beide Frachter fliegen aus
/// dem Orbit um Zorb direkt nach Zoon, landen nebeneinander und bauen die
/// Basis.
#[test]
fn start_kit_builds_a_base_on_zoon() {
    let mut world = demo();
    for (vessel, time) in [(4, 598.3), (5, 614.0), (6, 604.0)] {
        let node = ManeuverNode {
            time,
            delta_v: DVec3::new(845.0, 0.0, 0.0),
        };
        world.add_node(vessel, node).unwrap();
        world.set_armed(vessel, true).unwrap();
        let touchdown = world.touchdown(vessel).unwrap();
        assert_eq!(touchdown.body, ZOON);
        assert!(
            touchdown.landing.is_ok(),
            "{vessel}: {:?}",
            touchdown.landing
        );
    }
    world.advance_to(world.touchdown(4).unwrap().time + 1.0);
    let base = world.found_base(4).unwrap();
    // Die Vorschau der Frachter zeigt die neue Zone.
    for vessel in [5, 6] {
        let touchdown = world.touchdown(vessel).unwrap();
        assert_eq!(touchdown.zone, Some(world.zones.len() - 1), "{vessel}");
    }
    world.advance_to(world.touchdown(6).unwrap().time + 1.0);
    assert_eq!(world.unload(6), Ok(2_000.0));
    assert_eq!(world.unload(5), Ok(2_000.0));
    assert!(world.bases[base].complete());
    world.advance(3_600.0);
    assert!((world.refuel(5).unwrap() - FUEL_RATE * 3_600.0).abs() < 1e-6);
}
