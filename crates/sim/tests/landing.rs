//! Tests für die Landung: Fallschirm, Landebrennen, Aufsetzen, Wrack,
//! Zielgebiete, Hop.

use glam::DVec3;
use sim::kepler::state_from_apsides;
use sim::scenario::{FERRY, SATELLITE, ZORB, home_planet, moon};
use sim::{
    Crash, Launch, LaunchProfile, LogKind, ManeuverNode, StateVector, VesselParams, VesselStatus,
    World,
};

/// Das Demo-Szenario ohne Booster: Hier startet die Fähre aus eigener Kraft
/// (wie überall außerhalb der Startbasis).
fn demo() -> World {
    let mut world = sim::scenario::demo();
    world.booster = None;
    world
}

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

/// Das Landebrennen der Fähre aus dem Orbit (Δv und Höhe beim Zünden).
fn landing_burn(world: &World) -> (f64, f64) {
    let landing = world.touchdown(0).unwrap().landing.unwrap().unwrap();
    (landing.burn.delta_v(), landing.altitude)
}

/// Am Fallschirm kommt die Fähre langsam an, das Landebrennen ist kurz und
/// knapp über dem Boden; ohne Fallschirm braucht es viel mehr.
#[test]
fn parachute_slows_descent() {
    let burn = |parachute_area| {
        let params = VesselParams {
            parachute_area,
            ..FERRY
        };
        let mut world = in_orbit(params, 1_000.0);
        deorbit(&mut world, 150.0);
        landing_burn(&world)
    };
    let (with, altitude) = burn(FERRY.parachute_area);
    let (without, _) = burn(0.0);
    assert!(with < 15.0, "mit Fallschirm {with} m/s");
    assert!(altitude < 50.0, "Zünden in {altitude} m");
    assert!(without > 50.0, "ohne Fallschirm {without} m/s");
}

/// Aus dem Orbit gelandet: Das Landebrennen setzt die Fähre langsam auf,
/// Vorschau und Ausführung stimmen überein (Treibstoff bitgenau wie
/// vorausgesagt), die Kette ist erledigt.
#[test]
fn ferry_lands_from_orbit() {
    let mut world = in_orbit(FERRY, 1_000.0);
    deorbit(&mut world, 150.0);
    let touchdown = world.touchdown(0).unwrap();
    let landing = touchdown.landing.unwrap().unwrap();
    let delta_v = landing.burn.delta_v();
    assert!(delta_v > 5.0 && delta_v < 15.0, "Landebrennen {delta_v}");
    assert!(
        touchdown.speed < 2.5,
        "Aufsetzen mit {} m/s",
        touchdown.speed
    );
    let budget = world.delta_v_budget(0);
    assert!(matches!(budget.burns.last(), Some((sim::BudgetItem::Landing, dv)) if *dv == delta_v));

    world.advance_to(touchdown.time + 10.0);
    assert_eq!(
        world.vessel_status(0),
        VesselStatus::Landed {
            point: touchdown.point
        }
    );
    assert!(world.vessels[0].trajectory.nodes().is_empty());
    assert_eq!(world.vessel_mass(0), landing.burn.end_mass());
    let rest = world.vessel_delta_v(0);
    assert!(
        (rest - budget.rest).abs() < 1e-6,
        "{rest} statt {}",
        budget.rest
    );
}

/// Ohne Fallschirm und mit wenig Treibstoff reicht das Bremsen nicht: kein
/// Landebrennen, Absturz; angegeben ist, was mindestens fehlt.
#[test]
fn crash_without_enough_fuel() {
    let params = VesselParams {
        parachute_area: 0.0,
        ..FERRY
    };
    let mut world = in_orbit(params, 50.0);
    deorbit(&mut world, 60.0);
    let touchdown = world.touchdown(0).unwrap();
    let Err(Crash::NoFuel { available, missing }) = touchdown.landing else {
        panic!("Absturz erwartet, nicht {:?}", touchdown.landing);
    };
    assert!(available < touchdown.speed);
    assert!(
        missing > 0.0 && missing < touchdown.speed,
        "fehlen {missing}"
    );
    let budget = world.delta_v_budget(0);
    assert!(
        (budget.missing - missing).abs() < 1e-9,
        "{}",
        budget.missing
    );
    assert_eq!(budget.rest, 0.0);
    assert!(world.vessels[0].trajectory.landing().is_none());
    world.advance_to(touchdown.time + 10.0);
    assert!(matches!(world.vessel_status(0), VesselStatus::Wreck { .. }));
}

/// Ein Satellit hat zu wenig Schub zum Abbremsen, auch mit Treibstoff.
#[test]
fn satellite_is_too_weak_to_land() {
    let mut world = in_orbit(SATELLITE, 200.0);
    deorbit(&mut world, 150.0);
    let touchdown = world.touchdown(0).unwrap();
    let Err(Crash::TooWeak { thrust_to_weight }) = touchdown.landing else {
        panic!("Absturz erwartet, nicht {:?}", touchdown.landing);
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
    assert!(touchdown.landing.is_ok());

    world.advance_to(touchdown.time + 10.0);
    let VesselStatus::Landed { point } = world.vessel_status(2) else {
        panic!("gelandet erwartet");
    };
    assert_eq!(world.zone_at(ZORB, &point), Some(field));
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
    assert!(touchdown.landing.is_ok());
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

/// Zorb und Zoon, eine Fähre mit `thrust` N und 2000 kg Treibstoff auf einer
/// Bahn um Zoon (Apoapsis 30 km, Periapsis 50 km unter dem Boden), ohne
/// Kette. Startet in der Apoapsis.
fn falling_to_zoon(thrust: f64) -> World {
    let planet = home_planet();
    let moon = moon(0, &planet);
    let mut world = World::new(vec![planet, moon]);
    let (gm, r) = (world.bodies[1].gm, world.bodies[1].radius);
    let state = state_from_apsides(gm, r - 50_000.0, r + 30_000.0, 0.0, 0.0, 0.0, true);
    let params = VesselParams { thrust, ..FERRY };
    world.spawn_vessel("Lander", params, 2_000.0, 1, state);
    world
}

/// Auf Zoon ohne Luft bremst allein das Landebrennen, aus voller Fahrt. Wie
/// bei einem echten "Suicide Burn" kostet es mit wenig Schub mehr (die
/// Schwerkraft zieht länger dagegen), und es beginnt höher.
#[test]
fn landing_burn_on_zoon() {
    let burn = |thrust| {
        let world = falling_to_zoon(thrust);
        let touchdown = world.touchdown(0).unwrap();
        assert_eq!(touchdown.body, 1);
        assert!(
            touchdown.speed < 2.5,
            "Aufsetzen mit {} m/s",
            touchdown.speed
        );
        let landing = touchdown.landing.unwrap().unwrap();
        (landing.burn.delta_v(), landing.altitude)
    };
    // Schub/Gewicht etwa 14 und 1,5.
    let (strong, strong_altitude) = burn(FERRY.thrust);
    let (weak, weak_altitude) = burn(FERRY.thrust / 9.0);
    assert!(strong > 540.0 && strong < 570.0, "{strong}");
    assert!(weak > strong + 100.0, "{weak} gegen {strong}");
    assert!(strong_altitude < 3_000.0, "{strong_altitude}");
    assert!(weak_altitude > 10_000.0, "{weak_altitude}");
}

/// Das Landebrennen kommt von selbst, auch ohne Kette. Es ist ein normales
/// Brennen (Zeitraffer-Stopp, Protokoll), gehört aber nicht zur Kette.
/// Regel 6: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn landing_burn_without_chain() {
    let mut world = falling_to_zoon(FERRY.thrust);
    let trajectory = &world.vessels[0].trajectory;
    assert!(!trajectory.armed() && trajectory.burns().is_empty());
    let burn = *trajectory.landing().expect("Landebrennen");
    world.vessels[0].halt_warp_at_burn = true;
    assert_eq!(world.next_halting_burn(), Some(burn.start));
    let touchdown = world.touchdown(0).unwrap();

    let target = touchdown.time + 10.0;
    let mut many_steps = world.clone();
    world.advance_to(target);
    for i in 1..=10_000 {
        many_steps.advance_to(target * i as f64 / 10_000.0);
    }
    assert_eq!(
        world.vessel_status(0),
        VesselStatus::Landed {
            point: touchdown.point
        }
    );
    assert_eq!(world.vessel_status(0), many_steps.vessel_status(0));
    assert_eq!(world.vessel_mass(0), burn.end_mass());
    assert_eq!(many_steps.vessel_mass(0), burn.end_mass());
    let kinds: Vec<LogKind> = world.log.iter().map(|e| e.kind).collect();
    assert!(
        matches!(
            kinds.as_slice(),
            [LogKind::LandingBurn, LogKind::Landed { body: 1, .. }]
        ),
        "{kinds:?}"
    );
}

/// Ein Spielstand mitten im Landebrennen lädt und landet genauso.
#[test]
fn landing_burn_is_saved() {
    let mut world = falling_to_zoon(FERRY.thrust);
    let burn = *world.vessels[0].trajectory.landing().unwrap();
    world.advance_to(0.5 * (burn.start + burn.end));
    assert!(
        world.vessels[0]
            .trajectory
            .current()
            .landing_burn()
            .is_some()
    );
    let json = serde_json::to_string(&world).unwrap();
    let mut loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, world);
    let end = burn.end + 10.0;
    world.advance_to(end);
    loaded.advance_to(end);
    assert!(matches!(
        loaded.vessel_status(0),
        VesselStatus::Landed { .. }
    ));
    assert_eq!(loaded, world);
}

/// Erst 100 m über dem Boden und schon 200 m/s schnell: Zum Bremsen ist es
/// zu spät, auch mit genug Schub und Treibstoff.
#[test]
fn too_late_to_brake() {
    let planet = home_planet();
    let moon = moon(0, &planet);
    let mut world = World::new(vec![planet, moon]);
    let r = world.bodies[1].radius;
    let state = StateVector {
        position: DVec3::new(r + 100.0, 0.0, 0.0),
        velocity: DVec3::new(-200.0, 0.0, 0.0),
    };
    world.spawn_vessel("Lander", FERRY, 2_000.0, 1, state);
    let touchdown = world.touchdown(0).unwrap();
    assert_eq!(touchdown.landing, Err(Crash::TooLate));
    assert!(touchdown.speed > 200.0);
    world.advance_to(touchdown.time + 1.0);
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::Crashed {
            reason: Crash::TooLate,
            body: 1,
            ..
        }
    ));
}
