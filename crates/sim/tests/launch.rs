//! Tests für den Start vom Boden mit Startprofil.

use glam::DVec3;
use sim::kepler::{self, StateVector};
use sim::scenario::{FERRY, SATELLITE, demo, home_planet};
use sim::{
    Launch, LaunchProfile, ManeuverNode, PlanError, Segment, SurfacePoint, VesselParams,
    VesselStatus, World,
};

fn landed(params: VesselParams, fuel: f64) -> World {
    let mut world = World::new(vec![home_planet()]);
    let pad = SurfacePoint::from_degrees(5.0, -20.0);
    world.spawn_landed("Fähre", params, fuel, 0, pad);
    world
}

fn launch_at(time: f64) -> Launch {
    Launch {
        time,
        profile: LaunchProfile::default(),
    }
}

/// Erster Keplerabschnitt nach dem Aufstieg (über der Luft).
fn coast_after_ascent_of(world: &World, vessel: usize) -> (f64, StateVector) {
    let trajectory = &world.vessels[vessel].trajectory;
    let end = trajectory.launch().unwrap().burn.end;
    trajectory
        .segments()
        .find_map(|s| match s {
            Segment::Kepler { start, state, .. } if *start >= end => Some((*start, *state)),
            _ => None,
        })
        .expect("der Aufstieg muss über die Luft führen")
}

/// Erster Keplerabschnitt nach dem Aufstieg von Schiff 0.
fn coast_after_ascent(world: &World) -> (f64, StateVector) {
    coast_after_ascent_of(world, 0)
}

fn inclination(state: &StateVector) -> f64 {
    let h = state.position.cross(state.velocity);
    (h.z / h.length()).acos().to_degrees()
}

/// Brennschluss genau, wenn die Apoapsis die Zielhöhe erreicht; danach bremst
/// die obere Luft noch etwas. Es bleibt Treibstoff übrig.
#[test]
fn ascent_cuts_off_at_target_apoapsis() {
    let mut world = landed(FERRY, 4_000.0);
    world.set_launch(0, launch_at(10.0)).unwrap();
    let body = world.bodies[0].clone();
    let trajectory = &world.vessels[0].trajectory;
    let burn = trajectory.launch().unwrap().burn;
    let ascent = trajectory.segments().find(|s| s.is_burn()).unwrap();
    let at_cutoff = ascent.state_at(&world.bodies, burn.end);
    let apoapsis = kepler::orbit_info(body.gm, &at_cutoff).apoapsis.unwrap() - body.radius;
    assert!((apoapsis - 100_000.0).abs() < 1.0, "Apoapsis {apoapsis}");
    assert!(burn.end_mass() > FERRY.dry_mass + 300.0);

    let (_, coast) = coast_after_ascent(&world);
    let info = kepler::orbit_info(body.gm, &coast);
    let apoapsis = info.apoapsis.unwrap() - body.radius;
    assert!(
        (95_000.0..100_000.0).contains(&apoapsis),
        "Apoapsis {apoapsis}"
    );

    // Ausführen: Erst steht das Schiff, nach dem Brennschluss fliegt es.
    world.set_armed(0, true).unwrap();
    world.advance_to(5.0);
    assert!(matches!(
        world.vessel_status(0),
        VesselStatus::Landed { .. }
    ));
    world.advance_to(burn.end + 1.0);
    assert!(world.vessels[0].trajectory.launch().is_none());
    assert!((world.vessel_mass(0) - burn.end_mass()).abs() < 1e-6);
}

/// Start und ein Node an der Apoapsis, vor dem Abheben geplant: Am Ende
/// kreist die Fähre auf etwa 100 km.
#[test]
fn launch_and_circularize() {
    let mut world = landed(FERRY, 4_000.0);
    world.set_launch(0, launch_at(10.0)).unwrap();
    let gm = world.bodies[0].gm;
    let (start, state) = coast_after_ascent(&world);
    let time = start + kepler::time_to_apoapsis(gm, &state).unwrap();
    let at_apoapsis = kepler::propagate(gm, state, time - start);
    let circular = (gm / at_apoapsis.position.length()).sqrt();
    let delta_v = DVec3::new(circular - at_apoapsis.velocity.length(), 0.0, 0.0);
    assert_eq!(world.add_node(0, ManeuverNode { time, delta_v }), Ok(0));
    world.set_armed(0, true).unwrap();

    world.advance_to(time + 200.0);
    assert!(world.vessels[0].trajectory.nodes().is_empty());
    let VesselStatus::Orbit {
        periapsis_altitude,
        apoapsis_altitude: Some(apoapsis_altitude),
        descending: false,
    } = world.vessel_status(0)
    else {
        panic!("stabile Bahn erwartet, nicht {:?}", world.vessel_status(0));
    };
    assert!(periapsis_altitude > 90_000.0, "Pe {periapsis_altitude}");
    assert!(apoapsis_altitude < 105_000.0, "Ap {apoapsis_altitude}");
    assert!(world.vessel_fuel(0) > 100.0);
}

/// Ohne genug Schub kein Start; im Flug oder ohne Triebwerk auch nicht.
#[test]
fn invalid_launches_are_rejected() {
    let mut world = landed(SATELLITE, 200.0);
    assert_eq!(
        world.set_launch(0, launch_at(10.0)),
        Err(PlanError::TooWeak)
    );
    let mut world = landed(FERRY, 4_000.0);
    assert_eq!(world.set_launch(0, launch_at(0.0)), Err(PlanError::InPast));
    let mut world = demo();
    assert_eq!(
        world.set_launch(0, launch_at(10.0)),
        Err(PlanError::NotLanded)
    );
}

/// Reicht der Treibstoff nicht, fällt die Fähre zurück; die Vorschau zeigt
/// den Absturz.
#[test]
fn ascent_without_enough_fuel_falls_back() {
    let mut world = landed(FERRY, 800.0);
    world.set_launch(0, launch_at(10.0)).unwrap();
    let trajectory = &world.vessels[0].trajectory;
    let burn = trajectory.launch().unwrap().burn;
    assert!((burn.end_mass() - FERRY.dry_mass).abs() < 1e-6, "Tank leer");
    assert!(trajectory.predicted_impact().is_some());
}

/// Kurs Osten: Bahnneigung etwa die Breite des Startplatzes. Kurs Norden:
/// fast polar (die Drehung des Planeten schiebt etwas nach Osten).
#[test]
fn heading_sets_inclination() {
    let inclination_for = |heading| {
        let mut world = landed(FERRY, 4_000.0);
        let profile = LaunchProfile {
            heading,
            ..LaunchProfile::default()
        };
        world
            .set_launch(
                0,
                Launch {
                    time: 10.0,
                    profile,
                },
            )
            .unwrap();
        inclination(&coast_after_ascent(&world).1)
    };
    let east = inclination_for(90.0);
    assert!((east - 5.0).abs() < 0.5, "Kurs 90°: {east}");
    let north = inclination_for(0.0);
    assert!((80.0..90.0).contains(&north), "Kurs 0°: {north}");
}

/// Derselbe Start eine Stunde später: dieselbe Bahn, nur um die Drehung des
/// Planeten verdreht.
#[test]
fn later_launch_gives_same_orbit() {
    let orbit_for = |time| {
        let mut world = landed(FERRY, 4_000.0);
        world.set_launch(0, launch_at(time)).unwrap();
        let (_, state) = coast_after_ascent(&world);
        let info = kepler::orbit_info(world.bodies[0].gm, &state);
        (info.periapsis, info.apoapsis.unwrap(), inclination(&state))
    };
    let (pe0, ap0, i0) = orbit_for(10.0);
    let (pe1, ap1, i1) = orbit_for(3_610.0);
    assert!((pe0 - pe1).abs() < 1.0 && (ap0 - ap1).abs() < 1.0);
    assert!((i0 - i1).abs() < 1e-6);
}

/// Wird der Start verschoben, wandert die Kette mit.
#[test]
fn shifting_launch_moves_chain() {
    let mut world = landed(FERRY, 4_000.0);
    world.set_launch(0, launch_at(30.0)).unwrap();
    let (start, _) = coast_after_ascent(&world);
    let node = ManeuverNode {
        time: start + 60.0,
        delta_v: DVec3::new(50.0, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
    let before = world.vessels[0].trajectory.nodes()[0].plan.unwrap();

    world.shift_launch(0, 90.0).unwrap();
    let trajectory = &world.vessels[0].trajectory;
    assert_eq!(trajectory.launch().unwrap().launch.time, 120.0);
    let after = trajectory.nodes()[0].plan.unwrap();
    assert_eq!(after.node.time, node.time + 90.0);
    assert!((after.node_state.position.length() - before.node_state.position.length()).abs() < 1.0);
    assert!((after.burn.delta_v() - before.burn.delta_v()).abs() < 1e-6);

    assert_eq!(world.shift_launch(0, -200.0), Err(PlanError::InPast));
}

/// Nodes gibt es am Boden nur mit geplantem Start. Wird der Start abgesagt,
/// bleiben sie ungültig erhalten.
#[test]
fn nodes_need_flight_or_launch() {
    let mut world = landed(FERRY, 4_000.0);
    let node = ManeuverNode {
        time: 400.0,
        delta_v: DVec3::new(10.0, 0.0, 0.0),
    };
    assert_eq!(world.add_node(0, node), Err(PlanError::NotFlying));
    world.set_launch(0, launch_at(10.0)).unwrap();
    world.add_node(0, node).unwrap();
    world.clear_launch(0).unwrap();
    let trajectory = &world.vessels[0].trajectory;
    assert_eq!(trajectory.nodes().len(), 1);
    assert_eq!(trajectory.nodes()[0].plan, Err(PlanError::OnGround));
    assert!(matches!(
        world.vessel_status(0),
        VesselStatus::Landed { .. }
    ));
}

/// Ein nicht scharfer Start wartet eine Planetendrehung.
#[test]
fn unarmed_launch_waits_a_day() {
    let mut world = landed(FERRY, 4_000.0);
    world.set_launch(0, launch_at(10.0)).unwrap();
    world.advance_to(20.0);
    let day = world.bodies[0].rotation_period;
    let launch = world.vessels[0].trajectory.launch().unwrap();
    assert_eq!(launch.launch.time, 10.0 + day);
    assert!(matches!(
        world.vessel_status(0),
        VesselStatus::Landed { .. }
    ));
}

/// Regel 6 mit Start: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn launch_result_is_independent_of_step_size() {
    let setup = || {
        let mut world = demo();
        world.set_launch(2, launch_at(60.0)).unwrap();
        let (start, _) = coast_after_ascent_of(&world, 2);
        let node = ManeuverNode {
            time: start + 100.0,
            delta_v: DVec3::new(200.0, 0.0, 0.0),
        };
        world.add_node(2, node).unwrap();
        world.set_armed(2, true).unwrap();
        world
    };
    let target = 3_600.0 + 7.0;
    let mut one_step = setup();
    one_step.advance_to(target);
    let mut many_steps = setup();
    for i in 1..=10_000 {
        many_steps.advance_to(target * i as f64 / 10_000.0);
    }
    assert!(one_step.vessels[2].trajectory.launch().is_none());
    assert_eq!(one_step.vessel_state(2), many_steps.vessel_state(2));
    assert_eq!(one_step.vessel_mass(2), many_steps.vessel_mass(2));
    assert_eq!(one_step.vessel_status(2), many_steps.vessel_status(2));
}

/// Ein Spielstand mit geplantem Start lädt bitgenau.
#[test]
fn save_and_load_with_launch() {
    let mut world = demo();
    world.set_launch(2, launch_at(60.0)).unwrap();
    world.set_armed(2, true).unwrap();
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(world, loaded);
}

/// Voreinstellung nach dem Körper: mit Luft wie bisher (Ziel 30 km über der
/// Luftgrenze), ohne Luft früh und kräftig wenden, Ziel 30 km.
#[test]
fn profile_defaults_follow_the_body() {
    let planet = home_planet();
    let moon = sim::scenario::moon(&planet);
    assert_eq!(LaunchProfile::for_body(&planet), LaunchProfile::default());
    assert_eq!(LaunchProfile::default().kick, 10.0);
    assert_eq!(LaunchProfile::default().target_apoapsis, 100_000.0);
    let airless = LaunchProfile::for_body(&moon);
    assert_eq!(
        (airless.turn_altitude, airless.kick, airless.target_apoapsis),
        (100.0, 70.0, 30_000.0)
    );
}

/// 10° ergibt bitgenau den alten Wendewinkel: Starts mit der alten
/// Voreinstellung rechnen genau wie früher.
#[test]
fn ten_degrees_is_the_old_kick() {
    assert_eq!(sim::launch::kick_radians(10.0), sim::launch::KICK);
    let mut world = landed(FERRY, 4_000.0);
    world.set_launch(0, launch_at(60.0)).unwrap();
    let ascent = world.vessels[0].trajectory.launch().unwrap().burn.ascent;
    assert_eq!(ascent.unwrap().kick, sim::launch::KICK);
}

/// Aufstieg plus Zirkularisieren an der Apoapsis, in m/s.
fn orbit_cost(world: &World, vessel: usize) -> f64 {
    let plan = world.vessels[vessel].trajectory.launch().unwrap();
    let burn = plan.burn;
    let ascent = FERRY.exhaust_velocity * (burn.start_mass / burn.end_mass()).ln();
    let (_, state) = coast_after_ascent_of(world, vessel);
    let gm = world.bodies[1].gm;
    let info = kepler::orbit_info(gm, &state);
    let apoapsis = info.apoapsis.unwrap();
    let at_apoapsis = state.position.cross(state.velocity).length() / apoapsis;
    ascent + (gm / apoapsis).sqrt() - at_apoapsis
}

/// Start von Zoon (ohne Luft): Mit der Voreinstellung kostet die
/// 30-km-Bahn höchstens 680 m/s, mit den Werten von Zorb über 800 m/s.
#[test]
fn launch_from_the_moon() {
    let planet = home_planet();
    let moon = sim::scenario::moon(&planet);
    let mut world = World::new(vec![planet, moon.clone()]);
    world.spawn_landed(
        "Fähre",
        FERRY,
        4_000.0,
        1,
        SurfacePoint::from_degrees(0.0, 0.0),
    );
    let start = |profile| Launch {
        time: 60.0,
        profile,
    };

    let mut good = world.clone();
    good.set_launch(0, start(LaunchProfile::for_body(&moon)))
        .unwrap();
    let (_, state) = coast_after_ascent_of(&good, 0);
    let apoapsis = kepler::orbit_info(moon.gm, &state).apoapsis.unwrap() - moon.radius;
    assert!((apoapsis - 30_000.0).abs() < 1.0, "{apoapsis}");
    let cost = orbit_cost(&good, 0);
    assert!(cost < 680.0, "{cost}");

    let zorb_like = LaunchProfile {
        target_apoapsis: 30_000.0,
        ..LaunchProfile::default()
    };
    world.set_launch(0, start(zorb_like)).unwrap();
    assert!(orbit_cost(&world, 0) > 800.0);
}

/// Alte Spielstände ohne Wendewinkel: 10°, wie früher.
#[test]
fn old_profiles_get_ten_degrees() {
    let mut json = serde_json::to_value(LaunchProfile::default()).unwrap();
    json.as_object_mut().unwrap().remove("kick");
    let profile: LaunchProfile = serde_json::from_value(json).unwrap();
    assert_eq!(profile.kick, 10.0);

    let mut world = landed(FERRY, 4_000.0);
    world.set_launch(0, launch_at(60.0)).unwrap();
    let ascent = world.vessels[0]
        .trajectory
        .launch()
        .unwrap()
        .burn
        .ascent
        .unwrap();
    let mut json = serde_json::to_value(ascent).unwrap();
    json.as_object_mut().unwrap().remove("kick");
    let loaded: sim::launch::Ascent = serde_json::from_value(json).unwrap();
    assert_eq!(loaded, ascent);
}
