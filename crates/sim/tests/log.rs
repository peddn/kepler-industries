//! Tests fürs Protokoll: Einträge, Reihenfolge, Schrittgröße, Obergrenze.

mod common;

use common::hop;

use glam::DVec3;
use sim::scenario::{ZOON, ZORB, demo};
use sim::{LOG_LIMIT, Launch, LaunchProfile, LogEntry, LogKind, ManeuverNode, World};

/// Frachter-3 der Demo steht an der Startbasis.
const SHIP: usize = 3;

fn kinds(world: &World, vessel: usize) -> Vec<LogKind> {
    world
        .log
        .iter()
        .filter(|e| e.vessel == vessel)
        .map(|e| e.kind)
        .collect()
}

/// Start auf dem Booster und Zirkularisieren an der Apoapsis: Abheben,
/// Brennschluss des Aufstiegs, Brennbeginn und Brennschluss des Nodes; danach
/// eine runde Bahn. Die Trennung und der Rückflug stehen beim Booster.
#[test]
fn launch_and_circularize() {
    let mut world = demo();
    // Hier geht es um die Ereignisse des Flugs, nicht um Aufgaben.
    world.tasks.clear();
    let launch = Launch {
        time: 60.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(SHIP, launch).unwrap();
    world.set_armed(SHIP, true).unwrap();
    let (time, dv) =
        sim::booster::to_orbit(&world.bodies, &world.vessels[SHIP].trajectory).unwrap();
    let node = ManeuverNode {
        time,
        delta_v: DVec3::new(dv, 0.0, 0.0),
    };
    world.add_node(SHIP, node).unwrap();

    world.advance_to(time + 600.0);
    let ship = kinds(&world, SHIP);
    assert!(
        matches!(
            ship.as_slice(),
            [
                LogKind::Liftoff,
                LogKind::BurnEnd { .. },
                LogKind::BurnStart,
                LogKind::BurnEnd { .. }
            ]
        ),
        "{ship:?}"
    );
    let LogKind::BurnEnd {
        apoapsis: Some(ap),
        periapsis: pe,
        ..
    } = ship[3]
    else {
        panic!("Bahn nach dem Zirkularisieren");
    };
    assert!(ap > 95_000.0 && pe > 95_000.0, "Ap {ap} Pe {pe}");
    let booster = kinds(&world, world.booster.unwrap());
    assert_eq!(booster[0], LogKind::Separated { ship: SHIP });
    assert!(
        matches!(booster.last(), Some(LogKind::Landed { zone: Some(0), .. })),
        "{booster:?}"
    );
    // Die Zeiten sind die der Ereignisse, aufsteigend.
    let times: Vec<f64> = world.log.iter().map(|e| e.time).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(world.log[0].time, 60.0);
}

/// Frachter-2 taucht bei jedem Umlauf in die Luft; jeder Eintritt steht mit
/// der Periapsis im Protokoll, bis er herunterkommt und am Fallschirm landet.
#[test]
fn decay_and_landing() {
    let mut world = demo();
    world.advance_to(40_000.0);
    let kinds = kinds(&world, 2);
    let entries: Vec<f64> = kinds
        .iter()
        .filter_map(|k| match k {
            LogKind::AtmosphereEntry { periapsis } => Some(*periapsis),
            _ => None,
        })
        .collect();
    assert!(!entries.is_empty());
    assert!(
        (entries[0] - 62_000.0).abs() < 100.0,
        "erste Periapsis {}",
        entries[0]
    );
    // Die Bahn sinkt von Umlauf zu Umlauf.
    assert!(entries.windows(2).all(|w| w[1] < w[0]), "{entries:?}");
    assert!(
        matches!(kinds.last(), Some(LogKind::Landed { body: ZORB, .. })),
        "Landung erwartet: {kinds:?}"
    );
}

/// Hop vom Übungsfeld zur Startbasis: Landung mit Zielgebiet im Protokoll.
#[test]
fn landing_names_the_zone() {
    let (mut world, ship) = hop(270.0, 49.0);
    let depot = world
        .zones
        .iter()
        .position(|z| z.name == "Startbasis")
        .unwrap();
    let touchdown = world.touchdown(ship).unwrap();
    world.advance_to(touchdown.time + 1.0);
    let entry = world.log.last().unwrap();
    assert_eq!(entry.time, touchdown.time);
    assert_eq!(
        entry.kind,
        LogKind::Landed {
            point: touchdown.point,
            zone: Some(depot),
            body: ZORB,
        }
    );
}

/// Regel 6 auch fürs Protokoll: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn log_is_independent_of_step_size() {
    let target = 20_000.0;
    let (mut one_step, _) = hop(90.0, 25.0);
    one_step.advance_to(target);
    let (mut many_steps, _) = hop(90.0, 25.0);
    for i in 1..=10_000 {
        many_steps.advance_to(target * i as f64 / 10_000.0);
    }
    assert!(one_step.log.len() > 5, "{:?}", one_step.log);
    assert_eq!(one_step.log, many_steps.log);
}

/// Höchstens `LOG_LIMIT` Einträge; die ältesten fallen heraus.
#[test]
fn log_keeps_the_newest() {
    let mut world = demo();
    let old = LogEntry {
        time: 0.0,
        vessel: 0,
        kind: LogKind::BurnStart,
    };
    world.log = vec![old; LOG_LIMIT];
    world.advance_to(40_000.0);
    assert_eq!(world.log.len(), LOG_LIMIT);
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::Landed { .. }
    ));
}

/// Das Protokoll gehört zum Spielstand.
#[test]
fn log_is_saved() {
    let mut world = demo();
    world.advance_to(10_000.0);
    assert!(!world.log.is_empty());
    let json = serde_json::to_string(&world).unwrap();
    let loaded: World = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.log, world.log);
    // Ältere Spielstände ohne Protokoll laden weiter.
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value.as_object_mut().unwrap().remove("log");
    let old: World = serde_json::from_value(value).unwrap();
    assert!(old.log.is_empty());
}

/// Landung im Zielgebiet auf Zoon: Das Protokoll nennt Körper und Gebiet.
#[test]
fn landing_on_the_moon_is_logged_with_its_body() {
    let mut world = sim::scenario::demo();
    world.tasks.clear();
    let moon = &world.bodies[ZOON];
    let (gm, r) = (moon.gm, moon.radius + 30_000.0);
    let near_side = world
        .zones
        .iter()
        .position(|z| z.name == "Nahseite")
        .unwrap();
    let orbit = sim::kepler::state_from_apsides(gm, r, r, 0.0, 0.0, 0.0, false);
    world.spawn_vessel("Lander", common::FERRY, 2_000.0, ZOON, orbit);
    let lander = world.vessels.len() - 1;
    // Absenken an verschiedenen Stellen, bis das Kreuz in der Nahseite liegt.
    let world = (0..1_000)
        .map(|k| {
            let mut w = world.clone();
            let node = sim::ManeuverNode {
                time: 100.0 + 5.0 * k as f64,
                delta_v: glam::DVec3::new(-60.0, 0.0, 0.0),
            };
            w.add_node(lander, node).unwrap();
            w.set_armed(lander, true).unwrap();
            w
        })
        .find(|w| {
            w.touchdown(lander)
                .is_some_and(|t| t.zone == Some(near_side))
        })
        .expect("eine Stelle trifft die Nahseite");
    let mut world = world;
    let touchdown = world.touchdown(lander).unwrap();
    assert_eq!(touchdown.body, ZOON);
    world.advance_to(touchdown.time + 1.0);
    let entry = world.log.last().unwrap();
    assert_eq!(
        entry.kind,
        LogKind::Landed {
            point: touchdown.point,
            zone: Some(near_side),
            body: ZOON,
        }
    );
}
