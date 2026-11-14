//! Tests für das Zusammenspiel: Zeitleiste, Atmosphäre, Zeitraffer, Spielstand.
//! Jeder Test sichert eine Regel aus `docs/ARCHITEKTUR.md` ab.

use sim::kepler::{self, state_from_apsides};
use sim::scenario::{demo, home_planet};
use sim::{Modules, Segment, VesselParams, VesselStatus, World};

const DAY: f64 = 86_400.0;

fn single_vessel(periapsis_alt: f64, apoapsis_alt: f64, drag_area: f64) -> World {
    let mut world = World::new(vec![home_planet()]);
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius);
    let state = state_from_apsides(gm, r + periapsis_alt, r + apoapsis_alt, 0.3, 0.0, 0.0, true);
    let params = VesselParams {
        dry_mass: 500.0,
        drag_area,
        thrust: 0.0,
        exhaust_velocity: 3_000.0,
        fuel_capacity: 0.0,
        parachute_area: 0.0,
        cargo_capacity: 0.0,
        modules: Modules::NONE,
    };
    world.spawn_vessel("Test", params, 0.0, 0, state);
    world
}

/// Regel: Über der Atmosphäre ist die Bahn driftfrei, ohne Ereignisse.
#[test]
fn stable_orbit_never_changes() {
    let mut world = single_vessel(100_000.0, 300_000.0, 2.0);
    let start = world.vessel_state(0).1;
    let gm = world.bodies[0].gm;
    let period = kepler::period(gm, &start).unwrap();

    world.advance_to(1000.0 * period + 123.0);
    let expected = kepler::propagate(gm, start, 1000.0 * period + 123.0);
    let (_, now) = world.vessel_state(0);
    assert!((now.position - expected.position).length() < 1e-2);
    assert!(matches!(
        world.vessels[0].trajectory.current(),
        Segment::Kepler { end: None, .. }
    ));
}

/// Regel: Kein spürbarer Wechsel zwischen "Weltall" und "Atmosphäre".
/// Ohne Luftwiderstand muss der schrittweise berechnete Teil exakt der
/// Keplerbahn folgen, und danach geht die Bahn unverändert weiter.
#[test]
fn transition_is_seamless_without_drag() {
    let mut world = single_vessel(40_000.0, 200_000.0, 0.0);
    let gm = world.bodies[0].gm;
    let start = world.vessel_state(0).1;
    let start_info = kepler::orbit_info(gm, &start);

    let mut max_error: f64 = 0.0;
    let mut saw_integrated = false;
    let mut t = 0.0;
    while t < 3.0 * start_info.period.unwrap() {
        t += 7.3;
        world.advance_to(t);
        if matches!(
            world.vessels[0].trajectory.current(),
            Segment::Integrated { .. }
        ) {
            saw_integrated = true;
        }
        let expected = kepler::propagate(gm, start, t);
        let (_, now) = world.vessel_state(0);
        max_error = max_error.max((now.position - expected.position).length());
    }
    assert!(saw_integrated, "die Bahn muss durch die Atmosphäre führen");
    assert!(max_error < 1.0, "Abweichung {max_error} m");

    let info = kepler::orbit_info(gm, &world.vessel_state(0).1);
    assert!((info.periapsis - start_info.periapsis).abs() < 1.0);
    assert!((info.apoapsis.unwrap() - start_info.apoapsis.unwrap()).abs() < 1.0);
}

/// Regel: Eine zu tiefe Bahn wird von der Luft abgebremst, die Apoapsis sinkt
/// mit jedem Durchgang, am Ende steht ein Wrack.
#[test]
fn too_low_orbit_decays_and_crashes() {
    let mut world = single_vessel(62_000.0, 150_000.0, 2.0);
    let mut apoapsides = Vec::new();
    let mut t = 0.0;
    let mut was_in_atmosphere = false;
    while t < 2.0 * DAY {
        t += 10.0;
        world.advance_to(t);
        match world.vessel_status(0) {
            VesselStatus::Atmosphere { .. } => was_in_atmosphere = true,
            VesselStatus::Orbit {
                apoapsis_altitude, ..
            } => {
                if was_in_atmosphere {
                    apoapsides.push(apoapsis_altitude.unwrap());
                    was_in_atmosphere = false;
                }
            }
            VesselStatus::Wreck { .. } => break,
            status => panic!("darf weder landen noch brennen: {status:?}"),
        }
    }
    assert!(
        matches!(world.vessel_status(0), VesselStatus::Wreck { .. }),
        "nach zwei Tagen muss die Sonde abgestürzt sein"
    );
    assert!(apoapsides.len() >= 3, "{apoapsides:?}");
    assert!(apoapsides[0] < 150_000.0);
    assert!(apoapsides.windows(2).all(|w| w[1] < w[0]), "{apoapsides:?}");
}

/// Ein Satellit, der in der Luft ausgesetzt wird, stürzt ab (Fehlschlag).
#[test]
fn deployed_inside_atmosphere_crashes() {
    let mut world = single_vessel(40_000.0, 60_000.0, 2.0);
    assert!(matches!(
        world.vessels[0].trajectory.current(),
        Segment::Integrated { .. }
    ));
    world.advance_to(DAY);
    assert!(matches!(world.vessel_status(0), VesselStatus::Wreck { .. }));
}

/// Das Wrack liegt fest auf dem Boden und dreht sich mit dem Planeten.
#[test]
fn wreck_rotates_with_planet() {
    let mut world = single_vessel(40_000.0, 60_000.0, 2.0);
    world.advance_to(DAY);
    let body = &world.bodies[0];
    let (_, a) = world.vessel_state(0);
    assert!((a.position.length() - body.radius).abs() < 1e-6);
    let period = body.rotation_period;
    world.advance(period);
    let (_, b) = world.vessel_state(0);
    assert!((a.position - b.position).length() < 1e-3);
}

/// Regel: Das Ergebnis hängt nicht vom Zeitraffer ab. Ein großer Schritt und
/// viele kleine führen zum exakt gleichen Zustand.
#[test]
fn result_is_independent_of_step_size() {
    let target = 5.0 * 3600.0 + 17.0;
    let mut one_step = demo();
    one_step.advance_to(target);

    let mut many_steps = demo();
    for i in 1..=10_000 {
        many_steps.advance_to(target * i as f64 / 10_000.0);
    }
    many_steps.advance_to(target);

    for i in 0..one_step.vessels.len() {
        assert_eq!(one_step.vessel_state(i), many_steps.vessel_state(i));
        assert_eq!(one_step.vessel_status(i), many_steps.vessel_status(i));
    }
}

/// Der Zustand ist der Spielstand: speichern und laden ergibt dieselbe Welt.
#[test]
fn save_and_load_round_trip() {
    let mut world = demo();
    world.advance_to(2.0 * 3600.0);
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(world, loaded);
}
