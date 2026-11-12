//! Tests für Fracht und Basen: Fracht als Masse, Startbasis, Gründen,
//! Baustelle, Tankstelle, Mine und der ganze Kreislauf.

use glam::DVec3;
use sim::base::{
    BASE_KIT, FUEL_RATE, FUEL_STORAGE, PARTS_PRICE, SHIP_PRICE, SITE_RADIUS, ZORIT_RATE,
};
use sim::body::SurfacePoint;
use sim::kepler;
use sim::scenario::{
    BASE_SHIP, FERRY, FREIGHTER, NURA, START_ZORIT, ZILL, ZOON, ZORB, demo, start,
};
use sim::{
    BaseError, BaseKind, Cargo, Launch, LaunchProfile, LogKind, ManeuverNode, PlanError, Segment,
    ShipKind, VesselParams, World,
};

/// Demo ohne Schiffe; ein Basisschiff steht auf Zoon bei 10° Nord, 30° Ost.
fn on_zoon() -> (World, usize, SurfacePoint) {
    let mut world = demo();
    world.vessels.clear();
    let point = SurfacePoint::from_degrees(10.0, 30.0);
    let ship = world.spawn_landed("Basisschiff", BASE_SHIP, 1_000.0, ZOON, point);
    (world, ship, point)
}

/// Ein Frachter mit `parts` kg Bauteilen und `fuel` kg Treibstoff, `east`
/// Grad östlich der Basis bei 30° Ost (1° sind auf Zoon rund 3,4 km).
fn freighter(world: &mut World, east: f64, parts: f64, fuel: f64) -> usize {
    let at = SurfacePoint::from_degrees(10.0, 30.0 + east);
    let ship = world.spawn_landed("Frachter", FREIGHTER, fuel, ZOON, at);
    world.load_cargo(ship, parts);
    ship
}

/// Ein Schiff an der Startbasis.
fn at_depot(world: &mut World, params: VesselParams, fuel: f64) -> usize {
    let pad = world.bases[0].location;
    world.spawn_landed("Am Depot", params, fuel, ZORB, pad)
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

fn prograde(time: f64, delta_v: f64) -> ManeuverNode {
    ManeuverNode {
        time,
        delta_v: DVec3::new(delta_v, 0.0, 0.0),
    }
}

/// Fracht zählt als Masse: weniger Δv, derselbe Treibstoff, dieselbe Bahn.
#[test]
fn cargo_counts_as_mass() {
    let world = demo();
    let loaded = 6;
    let mut empty = demo();
    empty.vessels[loaded].cargo = Cargo::default();
    let cargo = world.vessels[loaded].cargo;
    assert_eq!(
        (cargo.parts, cargo.zorit, cargo.total()),
        (2_000.0, 0.0, 2_000.0)
    );
    let fuel = world.vessel_fuel(loaded);
    assert!((world.vessel_mass(loaded) - (2_500.0 + 2_000.0 + fuel)).abs() < 1e-9);
    let expected = 3_200.0 * ((4_500.0 + fuel) / 4_500.0).ln();
    assert!((world.vessel_delta_v(loaded) - expected).abs() < 1e-9);
    assert!(world.delta_v_budget(loaded).full < FREIGHTER.full_delta_v());
    assert_eq!(world.vessel_state(loaded), empty.vessel_state(loaded));
    // Die Startausstattung: zwei Basisschiffe, Bauteile für eine Basis.
    let kit: f64 = world.vessels.iter().map(|v| v.cargo.parts).sum();
    assert_eq!(kit, BASE_KIT);
    let ships = world.vessels.iter().filter(|v| v.params.base_kit).count();
    assert_eq!(ships, 2);
}

/// Die Startbasis: Bauteile laden (gegen Zorit), tanken (frei) und Zorit
/// abladen. Nur in ihrem Umkreis, nur ohne geplanten Start.
#[test]
fn depot_loads_fuels_and_takes_zorit() {
    let mut world = demo();
    assert_eq!(world.bases[0].kind, BaseKind::Depot);
    assert!(world.bases[0].complete());
    let ship = at_depot(&mut world, FREIGHTER, 500.0);
    assert_eq!(world.base_of(ship), Some(0));

    assert_eq!(world.deliver_zorit(ship), Err(BaseError::NothingToUnload));
    assert_eq!(world.load_parts(ship), Err(BaseError::NotEnoughZorit));
    world.zorit = 500.0;
    assert_eq!(world.load_parts(ship), Ok(2_000.0));
    assert_eq!(world.zorit, 0.0);
    assert_eq!(world.load_parts(ship), Err(BaseError::NothingToLoad));
    assert_eq!(world.refuel(ship), Ok(13_500.0));
    assert_eq!(world.refuel(ship), Err(BaseError::NothingToRefuel));
    assert_eq!(world.vessel_mass(ship), 2_500.0 + 2_000.0 + 14_000.0);
    assert!((world.vessel_fuel(ship) - 14_000.0).abs() < 1e-9);
    // Bauteile nimmt die Startbasis nicht zurück.
    assert_eq!(world.unload(ship), Err(BaseError::NothingToUnload));

    // Zorit abladen: alles kommt ins Lager der Startbasis.
    let back = at_depot(&mut world, FREIGHTER, 100.0);
    world.vessels[back].cargo.zorit = 800.0;
    world.vessels[back]
        .trajectory
        .set_landed_mass(2_500.0 + 100.0 + 800.0);
    let start = Launch {
        time: 100.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(back, start).unwrap();
    assert_eq!(world.deliver_zorit(back), Err(BaseError::Busy));
    world.clear_launch(back).unwrap();
    assert_eq!(world.deliver_zorit(back), Ok(800.0));
    assert_eq!(world.zorit, 800.0);
    assert_eq!(world.vessels[back].cargo, Cargo::default());
    assert_eq!(world.vessel_mass(back), 2_500.0 + 100.0);
    // Neben Zorit passen nur noch entsprechend weniger Bauteile.
    world.vessels[back].cargo.zorit = 500.0;
    world.vessels[back]
        .trajectory
        .set_landed_mass(2_500.0 + 100.0 + 500.0);
    assert_eq!(world.load_parts(back), Ok(1_500.0));
    assert_eq!(world.zorit, 800.0 - 375.0);

    // Auch die Fähre tankt dort; laden kann sie nichts.
    let ferry = at_depot(&mut world, FERRY, 0.0);
    assert_eq!(world.refuel(ferry), Ok(4_000.0));
    assert_eq!(world.load_parts(ferry), Err(BaseError::NothingToLoad));

    // Außerhalb des Umkreises (60 km östlich) geht nichts davon.
    let away = SurfacePoint::from_degrees(4.94, -14.2);
    let far = world.spawn_landed("Fern", FREIGHTER, 0.0, ZORB, away);
    assert_eq!(world.refuel(far), Err(BaseError::NoBase));
    assert_eq!(world.load_parts(far), Err(BaseError::NoBase));
    assert_eq!(world.deliver_zorit(far), Err(BaseError::NoBase));

    let json = serde_json::to_string(&world).unwrap();
    assert_eq!(serde_json::from_str::<World>(&json).unwrap(), world);
}

/// Gründen: nur ein gelandetes Basisschiff, nicht auf Zorb, nur einmal, als
/// Tankstelle oder Mine. Es wird zum Fundament (fliegt nicht mehr), der
/// Landepunkt zur Zone.
#[test]
fn base_ship_founds_a_base() {
    let mut flying = demo();
    assert_eq!(
        flying.found_base(4, BaseKind::Mine),
        Err(BaseError::NotLanded)
    );
    assert_eq!(
        flying.found_base(2, BaseKind::Mine),
        Err(BaseError::NoBaseKit)
    );
    let home = at_depot(&mut flying, BASE_SHIP, 0.0);
    assert_eq!(
        flying.found_base(home, BaseKind::Mine),
        Err(BaseError::HomePlanet)
    );

    let (mut world, ship, point) = on_zoon();
    world.set_launch(ship, launch()).unwrap();
    assert_eq!(world.found_base(ship, BaseKind::Mine), Err(BaseError::Busy));
    world.clear_launch(ship).unwrap();
    assert_eq!(
        world.found_base(ship, BaseKind::Depot),
        Err(BaseError::WrongBase)
    );

    let (bases, zones) = (world.bases.len(), world.zones.len());
    let base = world.found_base(ship, BaseKind::Mine).unwrap();
    assert_eq!(
        (base, world.bases.len(), world.zones.len()),
        (bases, bases + 1, zones + 1)
    );
    let site = &world.bases[base];
    assert_eq!(
        (site.name.as_str(), site.kind),
        ("Zoon-Mine 1", BaseKind::Mine)
    );
    assert_eq!((site.body, site.location), (ZOON, point));
    assert_eq!((site.required, site.delivered), (BASE_KIT, 0.0));
    assert!(!site.complete() && site.store.is_none());
    let zone = world.zones.last().unwrap();
    assert_eq!(
        (zone.name.as_str(), zone.radius),
        ("Zoon-Mine 1", SITE_RADIUS)
    );
    assert_eq!(world.zone_at(ZOON, &point), Some(zones));
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::BaseFounded { base: b } if b == base
    ));

    assert_eq!(world.vessels[ship].foundation_of, Some(base));
    assert_eq!(world.set_launch(ship, launch()), Err(PlanError::Foundation));
    assert_eq!(
        world.found_base(ship, BaseKind::Mine),
        Err(BaseError::NoBaseKit)
    );

    // Weitere Basisschiffe: im Umkreis nicht, daneben schon; gezählt wird
    // je Art.
    let near = SurfacePoint::from_degrees(10.0, 31.0);
    let second = world.spawn_landed("Zweites", BASE_SHIP, 0.0, ZOON, near);
    assert_eq!(
        world.found_base(second, BaseKind::FuelStation),
        Err(BaseError::Occupied)
    );
    for (east, kind, name) in [
        (33.0, BaseKind::FuelStation, "Zoon-Tankstelle 1"),
        (36.0, BaseKind::Mine, "Zoon-Mine 2"),
    ] {
        let at = SurfacePoint::from_degrees(10.0, east);
        let ship = world.spawn_landed("Weiteres", BASE_SHIP, 0.0, ZOON, at);
        let base = world.found_base(ship, kind).unwrap();
        assert_eq!(world.bases[base].name, name);
    }

    let json = serde_json::to_string(&world).unwrap();
    assert_eq!(serde_json::from_str::<World>(&json).unwrap(), world);
}

/// Abladen: nur im Umkreis, nur so viel, wie die Baustelle braucht. Das Schiff
/// wird leichter (mehr Δv), der Treibstoff bleibt. Mit dem letzten Bauteil
/// ist die Basis fertig.
#[test]
fn freighters_complete_the_base() {
    let (mut world, ship, _) = on_zoon();
    let outside = freighter(&mut world, 2.0, 2_000.0, 3_000.0);
    assert_eq!(world.unload(outside), Err(BaseError::NoBase));
    let base = world.found_base(ship, BaseKind::FuelStation).unwrap();
    assert_eq!(world.unload(outside), Err(BaseError::NoBase));
    assert_eq!(world.unload(ship), Err(BaseError::NothingToUnload));

    let first = freighter(&mut world, 1.0, 2_000.0, 3_000.0);
    assert_eq!(world.base_of(first), Some(base));
    assert_eq!(world.refuel(first), Err(BaseError::Unfinished));
    let before = world.vessel_delta_v(first);
    world.set_launch(first, launch()).unwrap();
    assert_eq!(world.unload(first), Err(BaseError::Busy));
    world.clear_launch(first).unwrap();
    assert_eq!(world.unload(first), Ok(2_000.0));
    assert_eq!(world.vessels[first].cargo.parts, 0.0);
    assert_eq!(world.vessel_mass(first), 2_500.0 + 3_000.0);
    assert!((world.vessel_fuel(first) - 3_000.0).abs() < 1e-9);
    assert!(world.vessel_delta_v(first) > before + 500.0);
    assert_eq!(world.unload(first), Err(BaseError::NothingToUnload));
    assert_eq!(world.bases[base].delivered, 2_000.0);
    assert!(!world.bases[base].complete());
    // Leer startet er wieder.
    world.set_launch(first, launch()).unwrap();

    // Der zweite bringt mehr, als fehlt: Der Rest bleibt an Bord.
    let mut big = FREIGHTER;
    big.cargo_capacity = 3_000.0;
    let at = SurfacePoint::from_degrees(10.0, 29.5);
    let second = world.spawn_landed("Großer", big, 1_000.0, ZOON, at);
    world.load_cargo(second, 3_000.0);
    world.advance(50.0);
    assert_eq!(world.unload(second), Ok(2_000.0));
    assert_eq!(world.vessels[second].cargo.parts, 1_000.0);
    let site = &world.bases[base];
    assert!(site.complete());
    assert_eq!(site.store.map(|s| (s.amount, s.since)), Some((0.0, 50.0)));
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::BaseCompleted { base: b } if b == base
    ));
    assert_eq!(world.unload(second), Err(BaseError::NothingToUnload));
}

/// Eine fertige Basis der Art `kind` auf `body` mit einem leeren Frachter
/// (kein Treibstoff) daneben.
fn finished(kind: BaseKind, body: usize) -> (World, usize, usize) {
    let mut world = demo();
    world.vessels.clear();
    let point = SurfacePoint::from_degrees(10.0, 30.0);
    let ship = world.spawn_landed("Basisschiff", BASE_SHIP, 0.0, body, point);
    let base = world.found_base(ship, kind).unwrap();
    for _ in 0..2 {
        let f = world.spawn_landed("Frachter", FREIGHTER, 0.0, body, point);
        world.load_cargo(f, 2_000.0);
        world.unload(f).unwrap();
    }
    assert!(world.bases[base].complete());
    (world, base, 1)
}

/// Die Tankstelle stellt gleichmäßig Treibstoff her, bis das Lager voll ist.
/// Der Stand hängt nur von der Zeit ab, nicht von den Schritten dahin
/// (Regel 6). Zorit gibt es dort nicht.
#[test]
fn fuel_station_makes_fuel() {
    let (mut world, base, ship) = finished(BaseKind::FuelStation, ZOON);
    assert_eq!(world.production(base), (FUEL_RATE, FUEL_STORAGE));
    assert_eq!(world.base_stock(base), 0.0);
    assert_eq!(world.refuel(ship), Err(BaseError::NothingToRefuel));
    assert_eq!(world.load_zorit(ship), Err(BaseError::WrongBase));
    assert_eq!(world.load_parts(ship), Err(BaseError::WrongBase));

    let mut stepped = world.clone();
    world.advance(36_000.0);
    for _ in 0..3_600 {
        stepped.advance(10.0);
    }
    assert_eq!(world.base_stock(base), FUEL_RATE * 36_000.0);
    assert_eq!(
        stepped.base_stock(base).to_bits(),
        world.base_stock(base).to_bits()
    );

    // Tanken: alles aus dem Lager in den Tank, das Schiff wird schwerer.
    assert_eq!(world.refuel(ship), Ok(1_800.0));
    assert_eq!(world.base_stock(base), 0.0);
    assert!((world.vessel_fuel(ship) - 1_800.0).abs() < 1e-9);
    assert_eq!(world.vessel_mass(ship), 2_500.0 + 1_800.0);
    assert_eq!(world.refuel(ship), Err(BaseError::NothingToRefuel));

    // Das Lager läuft nicht über: Warten füllt es nur einmal.
    world.advance(1.0e7);
    assert_eq!(world.base_stock(base), FUEL_STORAGE);
    assert_eq!(world.refuel(ship), Ok(FUEL_STORAGE));
    assert_eq!(world.base_stock(base), 0.0);
    // Danach füllt es sich wieder.
    world.advance(100.0);
    assert!((world.base_stock(base) - FUEL_RATE * 100.0).abs() < 1e-9);

    // Getankt hebt der Frachter wieder ab.
    let again = Launch {
        time: world.time + 100.0,
        ..launch()
    };
    world.set_launch(ship, again).unwrap();
}

/// Die Mine baut Zorit ab, bis ihr Lager voll ist. Wie viel es fasst, hängt
/// vom Körper ab: Das ist die Menge pro Abholung. Tanken geht dort nicht.
#[test]
fn mine_yield_depends_on_the_body() {
    let world = demo();
    let stores = [ZORB, ZOON, ZILL, NURA].map(|b| world.bodies[b].zorit);
    assert_eq!(stores, [0.0, 1_000.0, 2_000.0, 4_000.0]);

    let (mut world, base, ship) = finished(BaseKind::Mine, ZOON);
    assert_eq!(world.production(base), (ZORIT_RATE, 1_000.0));
    assert_eq!(world.load_zorit(ship), Err(BaseError::NothingToLoad));
    assert_eq!(world.refuel(ship), Err(BaseError::WrongBase));
    world.advance(3_600.0);
    assert!((world.base_stock(base) - 100.0).abs() < 1e-9);
    let loaded = world.load_zorit(ship).unwrap();
    assert!((loaded - 100.0).abs() < 1e-9);
    assert!((world.vessel_mass(ship) - 2_600.0).abs() < 1e-9);
    // Auch nach langem Warten gibt es auf Zoon nur 1 t pro Abholung.
    world.advance(1.0e7);
    assert_eq!(world.base_stock(base), 1_000.0);
    assert_eq!(world.load_zorit(ship), Ok(1_000.0));
    assert!((world.vessels[ship].cargo.zorit - 1_100.0).abs() < 1e-9);
    assert_eq!(world.base_stock(base), 0.0);

    // Auf Nura fasst das Lager 4 t; der Frachtraum (2 t) begrenzt dann.
    let (mut world, base, ship) = finished(BaseKind::Mine, NURA);
    world.advance(1.0e7);
    assert_eq!(world.base_stock(base), 4_000.0);
    assert_eq!(world.load_zorit(ship), Ok(2_000.0));
    assert_eq!(world.base_stock(base), 2_000.0);
    assert_eq!(world.load_zorit(ship), Err(BaseError::NothingToLoad));
}

/// Aufstieg von der Startbasis auf dem Booster und Zirkularisieren: so viel
/// Δv bleibt im Orbit übrig.
fn left_in_orbit(params: VesselParams, parts: f64) -> f64 {
    let mut world = start();
    let ship = at_depot(&mut world, params, 0.0);
    world.refuel(ship).unwrap();
    world.zorit = 1_000.0;
    if parts > 0.0 {
        assert_eq!(world.load_parts(ship), Ok(parts));
    }
    let start = Launch {
        time: 60.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(ship, start).unwrap();
    let trajectory = &world.vessels[ship].trajectory;
    let burn = trajectory.launch().unwrap().burn;
    let state = trajectory
        .segments()
        .find_map(|s| match s {
            Segment::Kepler { start, state, .. } if *start >= burn.end => Some(*state),
            _ => None,
        })
        .expect("der Aufstieg muss über die Luft führen");
    let gm = world.bodies[ZORB].gm;
    let apoapsis = kepler::orbit_info(gm, &state).apoapsis.unwrap();
    let circularize =
        (gm / apoapsis).sqrt() - state.position.cross(state.velocity).length() / apoapsis;
    // Das Schiff verbraucht auf dem Booster nichts.
    let full = params.dry_mass + parts + params.fuel_capacity;
    assert_eq!(burn.ship_end_mass(), full);
    let in_orbit = full * (-circularize / params.exhaust_velocity).exp();
    params.exhaust_velocity * (in_orbit / (params.dry_mass + parts)).ln()
}

/// Beide Schiffe kommen voll von der Startbasis in den Orbit und haben dort
/// genug für Zoon: Der Flug hin und die Landung kosten rund 1.760 m/s.
#[test]
fn ships_reach_orbit_from_the_depot() {
    let freighter = left_in_orbit(FREIGHTER, 2_000.0);
    assert!(freighter > 2_900.0, "{freighter}");
    let base_ship = left_in_orbit(BASE_SHIP, 0.0);
    assert!(base_ship > 2_100.0, "{base_ship}");
}

/// Der ganze Kreislauf mit der Startausstattung: Ein Basisschiff gründet auf
/// Zoon eine Mine, das andere woanders eine Tankstelle. Die Frachter bauen
/// die Mine, einer lädt Zorit, startet ohne Tanken und landet auf Zorb.
#[test]
fn start_kit_mines_zorit_on_zoon() {
    let mut world = demo();
    for (vessel, time) in [(4, 647.5), (5, 600.0), (6, 620.0), (7, 610.0)] {
        world.add_node(vessel, prograde(time, 845.0)).unwrap();
        world.set_armed(vessel, true).unwrap();
        let touchdown = world.touchdown(vessel).unwrap();
        assert_eq!(touchdown.body, ZOON);
        assert!(
            touchdown.landing.is_ok(),
            "{vessel}: {:?}",
            touchdown.landing
        );
    }
    world.advance_to(world.touchdown(5).unwrap().time + 1.0);
    let station = world.found_base(5, BaseKind::FuelStation).unwrap();
    world.advance_to(world.touchdown(4).unwrap().time + 1.0);
    let mine = world.found_base(4, BaseKind::Mine).unwrap();
    assert_eq!(world.bases[station].name, "Zoon-Tankstelle 1");
    assert_eq!(world.bases[mine].name, "Zoon-Mine 1");
    // Die Frachter stehen schon an der Mine.
    for vessel in [6, 7] {
        assert_eq!(world.base_of(vessel), Some(mine), "{vessel}");
        assert_eq!(world.unload(vessel), Ok(2_000.0));
    }
    assert!(world.bases[mine].complete() && !world.bases[station].complete());

    world.advance(40_000.0);
    assert_eq!(world.load_zorit(6), Ok(1_000.0));
    assert_eq!(world.load_zorit(7), Err(BaseError::NothingToLoad));

    // Zurück: Start in eine niedrige Bahn um Zoon, dann mit 280 m/s hinaus
    // und in Zorbs Luft.
    let start = Launch {
        time: world.time + 60.0,
        profile: LaunchProfile::for_body(&world.bodies[ZOON]),
    };
    world.set_launch(6, start).unwrap();
    let end = world.vessels[6].trajectory.launch().unwrap().burn.end;
    world.add_node(6, prograde(end + 230.0, 240.0)).unwrap();
    world.add_node(6, prograde(end + 1_280.0, 280.0)).unwrap();
    world.set_armed(6, true).unwrap();
    let touchdown = world.touchdown(6).unwrap();
    assert_eq!(touchdown.body, ZORB);
    assert!(touchdown.landing.is_ok(), "{:?}", touchdown.landing);
    // Es bleibt eine gute Reserve.
    let rest = world.delta_v_budget(6).rest;
    assert!(rest > 400.0, "{rest}");
    world.advance_to(touchdown.time + 1.0);
    assert!(matches!(
        world.vessels[6].trajectory.current(),
        Segment::Surface {
            body: ZORB,
            wreck: false,
            ..
        }
    ));
    assert_eq!(world.vessels[6].cargo.zorit, 1_000.0);
}

/// Spielbeginn: keine Schiffe, nur die Startbasis mit ihrem Booster und ein
/// Vorrat an Zorit, der genau für eine Basis reicht. Ein bestelltes Schiff steht sofort
/// vollgetankt an der Startbasis.
#[test]
fn game_starts_with_zorit_for_one_base() {
    let mut world = start();
    assert_eq!(world.vessels.len(), 1);
    assert_eq!(world.booster, Some(0));
    assert_eq!((world.bases.len(), world.zones.len()), (1, 1));
    assert_eq!(world.bases[0].kind, BaseKind::Depot);
    assert_eq!(world.zones[0].name, "Startbasis");
    assert_eq!(world.zorit, START_ZORIT);
    assert_eq!(START_ZORIT, 2.0 * SHIP_PRICE + BASE_KIT * PARTS_PRICE);

    let ship = world.order_ship(ShipKind::BaseShip).unwrap();
    let v = &world.vessels[ship];
    assert_eq!((v.name.as_str(), v.params), ("Basisschiff-1", BASE_SHIP));
    assert!((world.vessel_fuel(ship) - BASE_SHIP.fuel_capacity).abs() < 1e-9);
    assert_eq!(world.base_of(ship), Some(0));
    assert!(matches!(world.log.last().unwrap().kind, LogKind::Ordered));
    // Es startet von dort auf dem Booster.
    let start = Launch {
        time: 100.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(ship, start).unwrap();
    let plan = world.vessels[ship].trajectory.launch().unwrap();
    assert!(plan.booster.is_some());

    let freighter = world.order_ship(ShipKind::Freighter).unwrap();
    assert_eq!(world.vessels[freighter].name, "Frachter-2");
    assert_eq!(world.zorit, 1_000.0);
    // Zwei Ladungen Bauteile, dann ist das Lager leer.
    assert_eq!(world.load_parts(freighter), Ok(2_000.0));
    world.vessels[freighter].cargo = Cargo::default();
    assert_eq!(world.load_parts(freighter), Ok(2_000.0));
    assert_eq!(world.zorit, 0.0);
    for kind in [ShipKind::BaseShip, ShipKind::Freighter] {
        assert_eq!(world.order_ship(kind), Err(BaseError::NotEnoughZorit));
    }

    let json = serde_json::to_string(&world).unwrap();
    assert_eq!(serde_json::from_str::<World>(&json).unwrap(), world);
}

/// Schiffe und Bauteile kosten Zorit aus dem Lager der Startbasis.
#[test]
fn zorit_pays_for_ships_and_parts() {
    let (mut world, _, _) = finished(BaseKind::Mine, ZOON);
    for kind in [ShipKind::BaseShip, ShipKind::Freighter] {
        assert_eq!(world.order_ship(kind), Err(BaseError::NotEnoughZorit));
    }
    let empty = at_depot(&mut world, FREIGHTER, 0.0);
    assert_eq!(world.load_parts(empty), Err(BaseError::NotEnoughZorit));

    // 2,5 t Zorit: zwei Schiffe, dann reicht der Rest genau für 2 t Bauteile.
    world.zorit = 2_500.0;
    world.order_ship(ShipKind::BaseShip).unwrap();
    let freighter = world.order_ship(ShipKind::Freighter).unwrap();
    assert_eq!(world.zorit, 500.0);
    assert_eq!(world.load_parts(freighter), Ok(2_000.0));
    assert_eq!(world.zorit, 0.0);
    assert_eq!(
        world.vessel_mass(freighter),
        FREIGHTER.dry_mass + FREIGHTER.fuel_capacity + 2_000.0
    );
    // Eine neue Basis kostet damit 2 t Zorit.
    assert_eq!(SHIP_PRICE + BASE_KIT * PARTS_PRICE, 2_000.0);

    // Reicht das Zorit nicht für den ganzen Frachtraum, gibt es so viel,
    // wie es bezahlt. Tanken bleibt frei.
    world.zorit = 100.0;
    assert_eq!(world.load_parts(empty), Ok(400.0));
    assert_eq!(world.zorit, 0.0);
    assert_eq!(world.load_parts(empty), Err(BaseError::NotEnoughZorit));
    assert_eq!(world.refuel(empty), Ok(FREIGHTER.fuel_capacity));
}
