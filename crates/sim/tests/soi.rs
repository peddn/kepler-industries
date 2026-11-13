//! Tests für den Wechsel der Einflusssphäre: Mond-Vorbeiflug, Brennen über
//! die Grenze, Einfang, Bahn um die Sonne, Flug zu Nura.

mod common;

use common::FERRY;
use glam::DVec3;
use sim::body::{body_state, sphere_of_influence};
use sim::kepler::{StateVector, orbit_info, state_from_apsides};
use sim::log::LogKind;
use sim::scenario::{NURA, ZOON, ZORB, ZUNN, home_planet, moon, solar_system};
use sim::trajectory::{Event, Segment};
use sim::{ManeuverNode, VesselStatus, World};
use std::f64::consts::PI;

const MOON: usize = 1;
const LOW: f64 = 100_000.0;
/// Brennen für den Transfer.
const BURN: f64 = 1_000.0;

/// Planet und Mond, ohne Schiffe.
fn system() -> World {
    let planet = home_planet();
    let moon = moon(0, &planet);
    World::new(vec![planet, moon])
}

/// Fähre in 100 km Höhe, zum Zeitpunkt 0 beim Winkel `angle`.
fn ferry_at(world: &mut World, angle: f64) {
    let (gm, r) = (world.bodies[0].gm, world.bodies[0].radius + LOW);
    let state = state_from_apsides(gm, r, r, 0.0, 0.0, angle, false);
    world.spawn_vessel("Fähre", FERRY, 4_000.0, 0, state);
}

/// Hohmann-Transfer auf die Apoapsis `apoapsis` (Radius), so gestartet, dass
/// der Mond bei der Ankunft gegenüber dem Brennpunkt steht.
fn transfer(apoapsis: f64) -> World {
    let mut world = system();
    let gm = world.bodies[0].gm;
    let r1 = world.bodies[0].radius + LOW;
    let half = PI * ((0.5 * (r1 + apoapsis)).powi(3) / gm).sqrt();
    let (w1, w_moon) = (
        (gm / r1.powi(3)).sqrt(),
        (gm / 12_000_000f64.powi(3)).sqrt(),
    );
    ferry_at(&mut world, w_moon * (BURN + half) - w1 * BURN);
    let dv = (gm / r1).sqrt() * ((2.0 * apoapsis / (r1 + apoapsis)).sqrt() - 1.0);
    let node = ManeuverNode {
        time: BURN,
        delta_v: DVec3::new(dv, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
    world.set_armed(0, true).unwrap();
    world
}

/// Ein Vorbeiflug: Apoapsis 1.000 km innerhalb der Mondbahn.
fn flyby() -> World {
    transfer(11_000_000.0)
}

fn segments(world: &World) -> Vec<Segment> {
    world.vessels[0].trajectory.segments().cloned().collect()
}

/// Lage relativ zum Planeten.
fn absolute(world: &World, segment: &Segment, t: f64) -> StateVector {
    let s = segment.state_at(&world.bodies, t);
    let b = body_state(&world.bodies, segment.body(), t);
    StateVector {
        position: s.position + b.position,
        velocity: s.velocity + b.velocity,
    }
}

/// Der Wechsel zwischen `a` und dem folgenden `b` ist stetig, und er liegt
/// auf der Grenze der Einflusssphäre des Monds.
fn assert_switch(world: &World, a: &Segment, b: &Segment) {
    let t = a.end();
    assert_eq!(b.start(), t);
    let (sa, sb) = (absolute(world, a, t), absolute(world, b, t));
    assert!(sa.position.distance(sb.position) < 1e-6, "{sa:?} {sb:?}");
    assert!(sa.velocity.distance(sb.velocity) < 1e-9, "{sa:?} {sb:?}");
    let to_moon = sa.position - body_state(&world.bodies, MOON, t).position;
    let soi = sphere_of_influence(&world.bodies, MOON);
    assert!(
        (to_moon.length() - soi).abs() < 1.0,
        "{} statt {soi}",
        to_moon.length()
    );
}

/// Vorbeiflug: Eintritt in die Sphäre des Monds, Bahn um ihn mit
/// Periapsis über dem Boden, Austritt zurück zum Planeten. An beiden Grenzen
/// stetig; das Protokoll nennt beides.
#[test]
fn flyby_enters_and_leaves_the_moon() {
    let mut world = flyby();
    let s = segments(&world);
    let entry = s
        .iter()
        .position(|x| matches!(x.end_event(), Some(Event::SoiEntry(MOON))))
        .expect("Eintritt");
    assert_eq!(s[entry].body(), 0);
    let around = &s[entry + 1];
    assert_eq!(around.body(), MOON);
    assert_eq!(around.end_event(), Some(Event::SoiExit));
    assert_eq!(s[entry + 2].body(), 0);
    assert_switch(&world, &s[entry], around);
    assert_switch(&world, around, &s[entry + 2]);

    let moon = &world.bodies[MOON];
    let periapsis = orbit_info(moon.gm, &around.state_at(&world.bodies, around.start())).periapsis
        - moon.radius;
    assert!(periapsis > 0.0 && periapsis < 1_000_000.0, "{periapsis}");

    world.advance_to(around.end() + 10.0);
    assert_eq!(world.vessels[0].trajectory.current().body(), 0);
    let kinds: Vec<LogKind> = world.log.iter().map(|e| e.kind).collect();
    let i = kinds
        .iter()
        .position(|k| matches!(k, LogKind::SoiEntry { body: MOON, .. }))
        .expect("Eintrag");
    let LogKind::SoiEntry {
        periapsis: logged, ..
    } = kinds[i]
    else {
        unreachable!()
    };
    assert!((logged - periapsis).abs() < 1e-3, "{logged} {periapsis}");
    assert_eq!(kinds[i + 1], LogKind::SoiExit { body: MOON });
}

/// Vorschau gleich Ausführung: In einem Schritt oder in vielen krummen
/// Schritten über den Vorbeiflug hinweg kommt bitgenau dasselbe heraus.
#[test]
fn same_result_with_any_time_steps() {
    let end = 40_000.0;
    let mut one = flyby();
    one.advance_to(end);
    let mut many = flyby();
    let mut t = 0.0;
    while t < end {
        t = (t + 777.7).min(end);
        many.advance_to(t);
    }
    assert_eq!(one.vessel_state(0), many.vessel_state(0));
    assert_eq!(one.log, many.log);
}

/// Bahnen, die nicht in die Nähe der Mondbahn kommen, bleiben, wie sie
/// waren: ein offener Keplerabschnitt, keine Suche.
#[test]
fn low_orbit_does_not_search() {
    let mut world = system();
    ferry_at(&mut world, 0.0);
    let s = segments(&world);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].end_event(), None);
}

/// Ein Brennen über die Grenze wird geteilt und läuft um den Mond weiter;
/// erst danach ist der Node erledigt.
#[test]
fn burn_continues_across_the_boundary() {
    let mut world = system();
    // 300 m vor der Grenze, mit 30 m/s auf den Mond zu: Eintritt nach 10 s.
    let moon = body_state(&world.bodies, MOON, 0.0);
    let soi = sphere_of_influence(&world.bodies, MOON);
    let out = moon.position.normalize();
    let state = StateVector {
        position: moon.position + out * (soi + 300.0),
        velocity: moon.velocity - out * 30.0,
    };
    world.spawn_vessel("Fähre", FERRY, 4_000.0, 0, state);
    let node = ManeuverNode {
        time: 25.0,
        delta_v: DVec3::new(500.0, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
    world.set_armed(0, true).unwrap();

    let s = segments(&world);
    let split = s
        .iter()
        .position(|x| matches!(x.end_event(), Some(Event::SoiEntry(MOON))))
        .expect("Eintritt");
    assert!(s[split].is_burn() && s[split + 1].is_burn());
    assert_eq!(s[split + 1].body(), MOON);
    let burn = |x: &Segment| match x {
        Segment::Integrated { burn: Some(b), .. } => *b,
        _ => unreachable!(),
    };
    assert_eq!(burn(&s[split]), burn(&s[split + 1]));
    assert!(s[split].start() < s[split].end() && s[split].end() < burn(&s[split]).end);
    assert_switch(&world, &s[split], &s[split + 1]);

    // Mitten im Brennen, schon um den Mond: Der Node gilt noch.
    world.advance_to(s[split].end() + 1.0);
    assert_eq!(world.vessels[0].trajectory.current().body(), MOON);
    assert_eq!(world.vessels[0].trajectory.plans().count(), 1);
    world.advance_to(burn(&s[split]).end + 1.0);
    assert_eq!(world.vessels[0].trajectory.plans().count(), 0);
    let kinds: Vec<LogKind> = world.log.iter().map(|e| e.kind).collect();
    assert!(matches!(
        kinds[..],
        [
            LogKind::BurnStart,
            LogKind::SoiEntry { body: MOON, .. },
            LogKind::BurnEnd { .. }
        ]
    ));
}

/// Einfang: Ein Node an der Periapsis über dem Mond, gegen die Flugrichtung,
/// macht aus dem Vorbeiflug eine geschlossene Bahn um den Mond.
#[test]
fn node_at_the_moon_captures() {
    let mut world = flyby();
    let s = segments(&world);
    let around = s.iter().find(|x| x.body() == MOON).expect("Vorbeiflug");
    // Periapsis: tiefster Punkt des Abschnitts um den Mond.
    let (a, b) = (around.start(), around.end());
    let periapsis = (0..=10_000)
        .map(|i| a + (b - a) * i as f64 / 10_000.0)
        .min_by(|x, y| {
            let r = |t: f64| around.state_at(&world.bodies, t).position.length();
            r(*x).total_cmp(&r(*y))
        })
        .unwrap();
    let node = ManeuverNode {
        time: periapsis,
        delta_v: DVec3::new(-250.0, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
    world.set_armed(0, true).unwrap();
    let last = world.vessels[0].trajectory.segments().last().unwrap();
    assert_eq!(last.body(), MOON);
    assert_eq!(last.end_event(), None, "geschlossene Bahn um den Mond");
    let moon = &world.bodies[MOON];
    let info = orbit_info(moon.gm, &last.state_at(&world.bodies, last.start()));
    assert!(info.periapsis > moon.radius);
    assert!(info.apoapsis.unwrap() < sphere_of_influence(&world.bodies, MOON));
}

/// In der Demo: Frachter-4 fliegt mit 836 m/s nach 10 Minuten knapp am
/// Mond vorbei.
#[test]
fn demo_ferry_reaches_the_moon() {
    let mut world = sim::scenario::demo();
    let ferry = world
        .vessels
        .iter()
        .position(|v| v.name == "Frachter-4")
        .unwrap();
    let node = ManeuverNode {
        time: 600.0,
        delta_v: DVec3::new(836.0, 0.0, 0.0),
    };
    world.add_node(ferry, node).unwrap();
    let s: Vec<Segment> = world.vessels[ferry]
        .trajectory
        .segments()
        .cloned()
        .collect();
    let entry = s
        .iter()
        .position(|x| matches!(x.end_event(), Some(Event::SoiEntry(ZOON))))
        .expect("Begegnung");
    let moon = &world.bodies[ZOON];
    let state = s[entry + 1].state_at(&world.bodies, s[entry].end());
    let periapsis = orbit_info(moon.gm, &state).periapsis - moon.radius;
    assert!(periapsis > 50_000.0 && periapsis < 500_000.0, "{periapsis}");
}

/// Schnell genug hinaus: Am Rand von Zorbs Einflusssphäre (84.159 km)
/// wechselt die Bahn zu Zunn und geht dort weiter; das Protokoll trägt den
/// Wechsel ein, auf der Bahn um die Sonne lassen sich Nodes setzen.
#[test]
fn escape_goes_around_the_sun() {
    let mut world = World::new(solar_system());
    world.home = ZORB;
    let (gm, r) = (world.bodies[ZORB].gm, world.bodies[ZORB].radius + LOW);
    let state = state_from_apsides(gm, r, r, 0.0, 0.0, 0.0, false);
    world.spawn_vessel("Fähre", FERRY, 4_000.0, ZORB, state);
    let node = ManeuverNode {
        time: BURN,
        delta_v: DVec3::new(1_500.0, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
    world.set_armed(0, true).unwrap();
    let s = segments(&world);
    let exit = s
        .iter()
        .position(|x| x.end_event() == Some(Event::SoiExit))
        .expect("verlässt Zorb");
    assert_eq!(s[exit].body(), ZORB);
    assert_eq!(s[exit + 1].body(), ZUNN);
    let t = s[exit].end();
    let r_exit = s[exit].state_at(&world.bodies, t).position.length();
    let soi = sphere_of_influence(&world.bodies, ZORB);
    assert!((soi / 84_159_286.0 - 1.0).abs() < 1e-6, "{soi}");
    assert!((r_exit - soi).abs() < 1.0, "{r_exit}");
    // Um die Sonne: gebunden, fast auf Zorbs Bahn.
    let sun = &world.bodies[ZUNN];
    let around_sun = s[exit + 1].state_at(&world.bodies, t);
    let info = orbit_info(sun.gm, &around_sun);
    assert!(info.apoapsis.is_some());
    let distance = around_sun.position.length();
    assert!(
        (distance / 13_599_840_256.0 - 1.0).abs() < 0.01,
        "{distance}"
    );

    world.advance_to(t + 100.0);
    assert!(matches!(world.vessel_status(0), VesselStatus::Orbit { .. }));
    assert_eq!(world.vessel_state(0).0, ZUNN);
    assert!(
        world
            .log
            .iter()
            .any(|e| e.kind == LogKind::SoiExit { body: ZORB } && e.time == t)
    );
    let node = ManeuverNode {
        time: world.time + 1_000_000.0,
        delta_v: DVec3::new(10.0, 0.0, 0.0),
    };
    world.add_node(0, node).unwrap();
}

/// In der Demo: Frachter-4 fliegt mit 1.088,5 m/s prograd nach 163.400 s
/// (gut 7 Tagen, im ersten Startfenster) aus Zorbs Sphäre zu Zunn und nach
/// etwa 270 Tagen in Nuras Sphäre.
#[test]
fn demo_ferry_reaches_nura() {
    let mut world = sim::scenario::demo();
    let node = ManeuverNode {
        time: 163_400.0,
        delta_v: DVec3::new(1_088.5, 0.0, 0.0),
    };
    world.add_node(4, node).unwrap();
    let s: Vec<Segment> = world.vessels[4].trajectory.segments().cloned().collect();
    let entry = s
        .iter()
        .position(|x| x.end_event() == Some(Event::SoiEntry(NURA)))
        .expect("Begegnung mit Nura");
    assert_eq!(s[entry].body(), ZUNN);
    assert!(
        s[..entry]
            .iter()
            .any(|x| x.body() == ZORB && x.end_event() == Some(Event::SoiExit))
    );
    let days = s[entry].end() / world.bodies[ZORB].rotation_period;
    assert!(days > 250.0 && days < 290.0, "{days}");
    let nura = &world.bodies[NURA];
    let state = s[entry + 1].state_at(&world.bodies, s[entry].end());
    let periapsis = orbit_info(nura.gm, &state).periapsis - nura.radius;
    assert!(periapsis > 0.0 && periapsis < 20_000_000.0, "{periapsis}");
}
