//! Tests für Aufgaben und Daten: die feste Liste, Orbitvorgaben als
//! Bereiche, Landung und Basis; jede zählt einmal, an ihrem Ereignis.

use glam::DVec3;
use sim::kepler::state_from_apsides;
use sim::scenario::{BASE_SHIP, FERRY, FREIGHTER, NURA, ZILL, ZOON, ZORB, demo, start};
use sim::{BaseKind, Goal, LogKind, ManeuverNode, Orbit, OrbitSpec, SurfacePoint, World};

fn prograde(time: f64, delta_v: f64) -> ManeuverNode {
    ManeuverNode {
        time,
        delta_v: DVec3::new(delta_v, 0.0, 0.0),
    }
}

/// Die Aufgabe mit diesem Titel am Körper `body`.
fn task(world: &World, body: usize, title: &str) -> usize {
    world
        .tasks
        .iter()
        .position(|t| t.body == body && t.title == title)
        .unwrap_or_else(|| panic!("keine Aufgabe {title} an {body}"))
}

fn done(world: &World) -> Vec<(usize, &str)> {
    world
        .tasks
        .iter()
        .filter(|t| t.done.is_some())
        .map(|t| (t.body, t.title.as_str()))
        .collect()
}

/// Ein Schiff auf einer Kreisbahn in `altitude` m Höhe um `body`, mit der
/// Bahnneigung `inclination` (Grad).
fn in_orbit(world: &mut World, body: usize, altitude: f64, inclination: f64) -> usize {
    let (gm, r) = (world.bodies[body].gm, world.bodies[body].radius + altitude);
    let state = state_from_apsides(gm, r, r, inclination.to_radians(), 0.0, 0.0, false);
    world.spawn_vessel("Schiff", FERRY, 4_000.0, body, state)
}

/// Die feste Liste: je Körper niedrige, hohe und polare Umlaufbahn, außer
/// auf Zorb dazu Landung und Basis, mit Vielfachen des Grundwerts.
#[test]
fn standard_tasks_follow_from_the_bodies() {
    let world = start();
    let of = |body: usize| -> Vec<(&str, f64)> {
        world
            .tasks
            .iter()
            .filter(|t| t.body == body)
            .map(|t| (t.title.as_str(), t.data))
            .collect()
    };
    let orbits = |base: f64| {
        vec![
            ("Niedrige Umlaufbahn", base),
            ("Hohe Umlaufbahn", base),
            ("Polare Umlaufbahn", 2.0 * base),
        ]
    };
    let all = |base: f64| {
        let mut tasks = orbits(base);
        tasks.extend([("Landung", 2.0 * base), ("Basis", 3.0 * base)]);
        tasks
    };
    assert_eq!(of(ZORB), orbits(5.0));
    assert_eq!(of(ZOON), all(5.0));
    assert_eq!(of(ZILL), all(10.0));
    assert_eq!(of(NURA), all(15.0));
    assert_eq!(world.tasks.len(), 18);
    assert_eq!(world.tasks.iter().map(|t| t.data).sum::<f64>(), 290.0);
    assert_eq!(world.data, 0.0);

    // Die Vorgaben um Zorb: niedrig zwischen Luftgrenze und 220 km, hoch ab
    // 1.200 km, polar bei 80° bis 100°.
    let Goal::Orbit(low) = world.tasks[task(&world, ZORB, "Niedrige Umlaufbahn")].goal else {
        panic!()
    };
    assert_eq!(
        (low.periapsis, low.apoapsis),
        ((70e3, 220e3), (70e3, 220e3))
    );
    let Goal::Orbit(high) = world.tasks[task(&world, ZORB, "Hohe Umlaufbahn")].goal else {
        panic!()
    };
    assert_eq!(high.periapsis.0, 1_200e3);
    let Goal::Orbit(polar) = world.tasks[task(&world, ZORB, "Polare Umlaufbahn")].goal else {
        panic!()
    };
    assert_eq!(polar.inclination, (80.0, 100.0));
}

/// Vorgaben sind Bereiche, Grenzen eingeschlossen; offene Bahnen nie.
#[test]
fn orbit_spec_is_a_range() {
    let spec = OrbitSpec {
        periapsis: (70e3, 220e3),
        apoapsis: (70e3, 220e3),
        inclination: (0.0, 180.0),
    };
    let orbit = |periapsis: f64, apoapsis: Option<f64>| Orbit {
        periapsis,
        apoapsis,
        inclination: 28.0,
    };
    assert!(spec.met_by(&orbit(70e3, Some(220e3))));
    assert!(spec.met_by(&orbit(100e3, Some(150e3))));
    assert!(!spec.met_by(&orbit(69e3, Some(150e3))));
    assert!(!spec.met_by(&orbit(100e3, Some(221e3))));
    assert!(!spec.met_by(&orbit(100e3, None)));
}

/// Eine Umlaufbahn zählt, wenn ein Brennen auf ihr endet, und nur einmal.
/// Die Vorschau sagt es vorher; ein Schiff, das schon dort kreist, zählt
/// nicht.
#[test]
fn orbit_counts_when_a_burn_ends_on_it() {
    let mut world = demo();
    let ship = in_orbit(&mut world, ZORB, 100_000.0, 0.0);
    world.advance(600.0);
    assert!(world.data == 0.0 && done(&world).is_empty());

    world
        .add_node(ship, prograde(world.time + 300.0, 2.0))
        .unwrap();
    world.set_armed(ship, true).unwrap();
    let low = task(&world, ZORB, "Niedrige Umlaufbahn");
    let plan = *world.vessels[ship].trajectory.plans().next().unwrap();
    let after = world.vessels[ship]
        .trajectory
        .segment_after(&plan.burn)
        .unwrap();
    let state = after.state_at(&world.bodies, plan.burn.end);
    assert_eq!(world.tasks_met_by(ZORB, &state), vec![low]);

    world.advance(600.0);
    assert_eq!(world.data, 5.0);
    assert_eq!(done(&world), vec![(ZORB, "Niedrige Umlaufbahn")]);
    assert_eq!(world.tasks[low].done, Some(plan.burn.end));
    let last = world.log.last().unwrap();
    assert_eq!(
        (last.vessel, last.kind),
        (ship, LogKind::TaskDone { task: low })
    );
    assert_eq!(last.time, plan.burn.end);
    // Erfüllt ist erfüllt: Die Vorschau nennt sie nicht mehr, ein zweites
    // Brennen bringt nichts.
    assert!(world.tasks_met_by(ZORB, &state).is_empty());
    world
        .add_node(ship, prograde(world.time + 300.0, 2.0))
        .unwrap();
    world.set_armed(ship, true).unwrap();
    world.advance(600.0);
    assert_eq!(world.data, 5.0);
}

/// Eine Bahn darf mehrere Aufgaben zugleich erfüllen: polar und niedrig.
/// Eine hohe Bahn ist nicht niedrig.
#[test]
fn one_orbit_can_meet_several_tasks() {
    let mut world = demo();
    let polar = in_orbit(&mut world, ZORB, 150_000.0, 88.0);
    let high = in_orbit(&mut world, ZORB, 2_000_000.0, 10.0);
    for ship in [polar, high] {
        world.add_node(ship, prograde(300.0, 1.0)).unwrap();
        world.set_armed(ship, true).unwrap();
    }
    world.advance(600.0);
    assert_eq!(
        done(&world),
        vec![
            (ZORB, "Niedrige Umlaufbahn"),
            (ZORB, "Hohe Umlaufbahn"),
            (ZORB, "Polare Umlaufbahn")
        ]
    );
    assert_eq!(world.data, 5.0 + 5.0 + 10.0);
}

/// Ein Vorbeiflug zählt nicht: Die Bahn um den Mond ist offen, und es endet
/// kein Brennen auf ihr.
#[test]
fn flyby_does_not_count() {
    let mut world = demo();
    // Mondfähre-4 mit dem Transfer aus der Demo: knapp an Zoon vorbei.
    world.add_node(3, prograde(600.0, 836.0)).unwrap();
    world.set_armed(3, true).unwrap();
    world.advance_to(200_000.0);
    assert!(
        world
            .log
            .iter()
            .any(|e| matches!(e.kind, LogKind::SoiEntry { body: ZOON, .. }))
    );
    assert!(done(&world).iter().all(|&(body, _)| body != ZOON));
}

/// Landung: heil aufsetzen zählt, ein Absturz nicht.
#[test]
fn landing_counts_but_a_crash_does_not() {
    let descend = |fuel: f64| {
        let mut world = demo();
        let moon = &world.bodies[ZOON];
        let (gm, r) = (moon.gm, moon.radius + 30_000.0);
        let orbit = state_from_apsides(gm, r, r, 0.0, 0.0, 0.0, false);
        let lander = world.spawn_vessel("Lander", FERRY, fuel, ZOON, orbit);
        world.add_node(lander, prograde(200.0, -60.0)).unwrap();
        world.set_armed(lander, true).unwrap();
        let touchdown = world.touchdown(lander).unwrap();
        world.advance_to(touchdown.time + 1.0);
        (world, touchdown.landing.is_ok())
    };
    let (world, landed) = descend(2_000.0);
    assert!(landed);
    assert_eq!(done(&world), vec![(ZOON, "Landung")]);
    assert_eq!(world.data, 10.0);

    // Mit zu wenig Treibstoff für das Landebrennen.
    let (world, landed) = descend(60.0);
    assert!(!landed);
    assert!(world.data == 0.0 && done(&world).is_empty());
}

/// Basis: Sie zählt, wenn sie fertig ist, nicht beim Gründen.
#[test]
fn base_counts_when_it_is_complete() {
    let mut world = demo();
    let site = SurfacePoint::from_degrees(10.0, 30.0);
    let ship = world.spawn_landed("Basisschiff", BASE_SHIP, 1_000.0, ZOON, site);
    let base = world.found_base(ship, BaseKind::Mine).unwrap();
    assert_eq!(world.data, 0.0);
    for _ in 0..2 {
        let freighter = world.spawn_landed("Frachter", FREIGHTER, 1_000.0, ZOON, site);
        world.load_cargo(freighter, 2_000.0);
        world.unload(freighter).unwrap();
    }
    assert!(world.bases[base].complete());
    assert_eq!(done(&world), vec![(ZOON, "Basis")]);
    assert_eq!(world.data, 15.0);
}

/// Wie viele Schritte die Zeit braucht, ändert nichts, und die Aufgaben
/// stehen im Spielstand.
#[test]
fn tasks_are_independent_of_step_size_and_saved() {
    let run = |step: f64| {
        let mut world = demo();
        let ship = in_orbit(&mut world, ZORB, 150_000.0, 88.0);
        world.add_node(ship, prograde(300.0, 1.0)).unwrap();
        world.set_armed(ship, true).unwrap();
        while world.time < 900.0 {
            world.advance(step);
        }
        world.advance_to(900.0);
        world
    };
    let coarse = run(900.0);
    assert_eq!(run(7.0), coarse);
    assert_eq!(coarse.data, 15.0);
    let loaded = sim::save::from_json(&sim::save::to_json(&coarse)).unwrap();
    assert_eq!(loaded, coarse);
}
