//! Tests fürs Protokoll: Einträge, Reihenfolge, Schrittgröße, Obergrenze.

use glam::DVec3;
use sim::kepler::time_to_apoapsis;
use sim::scenario::demo;
use sim::{
    Crash, LOG_LIMIT, Launch, LaunchProfile, LogEntry, LogKind, ManeuverNode, Segment, World,
};

const FERRY: usize = 2;

fn kinds(world: &World, vessel: usize) -> Vec<LogKind> {
    world
        .log
        .iter()
        .filter(|e| e.vessel == vessel)
        .map(|e| e.kind)
        .collect()
}

/// Start in den Orbit mit Zirkularisieren an der Apoapsis: Abheben,
/// Brennschluss des Aufstiegs, Brennbeginn und Brennschluss des Nodes; danach
/// eine runde Bahn.
#[test]
fn launch_and_circularize() {
    let mut world = demo();
    let launch = Launch {
        time: 60.0,
        profile: LaunchProfile::default(),
    };
    world.set_launch(FERRY, launch).unwrap();
    world.set_armed(FERRY, true).unwrap();
    // Node an der Apoapsis der ersten Keplerbahn nach dem Aufstieg.
    let gm = world.bodies[0].gm;
    let burn_end = world.vessels[FERRY].trajectory.launch().unwrap().burn.end;
    let (start, state) = world.vessels[FERRY]
        .trajectory
        .segments()
        .find_map(|s| match s {
            Segment::Kepler { start, state, .. } if *start >= burn_end => Some((*start, *state)),
            _ => None,
        })
        .unwrap();
    let dt = time_to_apoapsis(gm, &state).unwrap();
    let apoapsis = sim::kepler::propagate(gm, state, dt);
    let dv = (gm / apoapsis.position.length()).sqrt() - apoapsis.velocity.length();
    world
        .add_node(
            FERRY,
            ManeuverNode {
                time: start + dt,
                delta_v: DVec3::new(dv, 0.0, 0.0),
            },
        )
        .unwrap();

    world.advance_to(start + dt + 600.0);
    let kinds = kinds(&world, FERRY);
    assert!(
        matches!(
            kinds.as_slice(),
            [
                LogKind::Liftoff,
                LogKind::BurnEnd { .. },
                LogKind::BurnStart,
                LogKind::BurnEnd { .. }
            ]
        ),
        "{kinds:?}"
    );
    let LogKind::BurnEnd {
        apoapsis: Some(ap),
        periapsis: pe,
    } = kinds[3]
    else {
        panic!("Bahn nach dem Zirkularisieren");
    };
    assert!(ap > 95_000.0 && pe > 95_000.0, "Ap {ap} Pe {pe}");
    // Die Zeiten sind die der Ereignisse, aufsteigend.
    let times: Vec<f64> = world.log.iter().map(|e| e.time).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(world.log[0].time, 60.0);
}

/// Sonde-2 taucht bei jedem Umlauf in die Luft; jeder Eintritt steht mit der
/// Periapsis im Protokoll, bis sie abstürzt (Satellit: zu schwach zum Bremsen).
#[test]
fn decay_and_crash() {
    let mut world = demo();
    world.advance_to(40_000.0);
    let kinds = kinds(&world, 1);
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
    let Some(LogKind::Crashed { reason, .. }) = kinds.last() else {
        panic!("Absturz erwartet: {kinds:?}");
    };
    assert!(matches!(reason, Crash::TooWeak { .. }));
}

/// Hop zum Übungsfeld: Landung mit Zielgebiet im Protokoll.
#[test]
fn landing_names_the_zone() {
    let field = demo()
        .zones
        .iter()
        .position(|z| z.name == "Übungsfeld")
        .unwrap();
    let mut world = (20..35)
        .map(|km| {
            let mut world = demo();
            let profile = LaunchProfile {
                target_apoapsis: km as f64 * 1000.0,
                ..LaunchProfile::default()
            };
            world
                .set_launch(
                    FERRY,
                    Launch {
                        time: 60.0,
                        profile,
                    },
                )
                .unwrap();
            world.set_armed(FERRY, true).unwrap();
            world
        })
        .find(|w| w.touchdown(FERRY).is_some_and(|t| t.zone == Some(field)))
        .unwrap();
    let touchdown = world.touchdown(FERRY).unwrap();
    world.advance_to(touchdown.time + 1.0);
    let entry = world.log.last().unwrap();
    assert_eq!(entry.time, touchdown.time);
    assert_eq!(
        entry.kind,
        LogKind::Landed {
            point: touchdown.point,
            zone: Some(field),
            body: 0,
        }
    );
}

/// Regel 6 auch fürs Protokoll: 1 Schritt == 10 000 Schritte, bitgenau.
#[test]
fn log_is_independent_of_step_size() {
    let setup = || {
        let mut world = demo();
        let profile = LaunchProfile {
            target_apoapsis: 25_000.0,
            ..LaunchProfile::default()
        };
        world
            .set_launch(
                FERRY,
                Launch {
                    time: 60.0,
                    profile,
                },
            )
            .unwrap();
        world.set_armed(FERRY, true).unwrap();
        world
    };
    let target = 20_000.0;
    let mut one_step = setup();
    one_step.advance_to(target);
    let mut many_steps = setup();
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
        LogKind::Crashed { .. }
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
    let moon = &world.bodies[1];
    let (gm, r) = (moon.gm, moon.radius + 30_000.0);
    let near_side = world
        .zones
        .iter()
        .position(|z| z.name == "Nahseite")
        .unwrap();
    let orbit = sim::kepler::state_from_apsides(gm, r, r, 0.0, 0.0, 0.0, false);
    world.spawn_vessel("Lander", sim::scenario::FERRY, 2_000.0, 1, orbit);
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
    assert_eq!(touchdown.body, 1);
    world.advance_to(touchdown.time + 1.0);
    let entry = world.log.last().unwrap();
    assert_eq!(
        entry.kind,
        LogKind::Landed {
            point: touchdown.point,
            zone: Some(near_side),
            body: 1,
        }
    );
}
