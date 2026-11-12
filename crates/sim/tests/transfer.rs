//! Tests für den Flug zu einem Körper als Ziel: Startfenster und Annäherung.

use glam::DVec3;
use sim::ManeuverNode;
use sim::scenario::{NURA, ZILL, ZOON, ZORB, demo};

const FERRY: usize = 3;

fn node(time: f64, prograde: f64) -> ManeuverNode {
    ManeuverNode {
        time,
        delta_v: DVec3::new(prograde, 0.0, 0.0),
    }
}

/// Zorb → Nura: Start von Zorb aus (nicht vom Schiff), nötig sind wie bei
/// Kerbin → Duna etwa 44°. Das erste Fenster liegt nahe am bekannten
/// Transfer der Demo (163.400 s), der Richtwert trifft ihn auf wenige Tage.
#[test]
fn window_to_another_planet() {
    let mut world = demo();
    world.set_target_body(FERRY, Some(NURA));
    let window = world.transfer_window(FERRY).unwrap();
    assert_eq!(window.origin, Some(ZORB));
    assert!(
        (window.needed.to_degrees() - 44.4).abs() < 0.5,
        "{window:?}"
    );
    assert!(window.phase > window.needed, "{window:?}");
    let day = 21_600.0;
    assert!((window.wait - 163_400.0).abs() < 4.0 * day, "{window:?}");
    // Zorb holt auf: Der Phasenwinkel schrumpft, das Fenster rückt näher.
    world.advance_to(day);
    let later = world.transfer_window(FERRY).unwrap();
    assert!(later.phase < window.phase);
    assert!((window.wait - later.wait - day).abs() < 100.0, "{later:?}");
}

/// Zorb-Orbit → Zoon: Hier zählt die Stellung des Schiffs selbst; das
/// Fenster liegt beim Transfer der Demo (836 m/s nach 600 s).
#[test]
fn window_to_a_moon() {
    let mut world = demo();
    world.set_target_body(FERRY, Some(ZOON));
    let window = world.transfer_window(FERRY).unwrap();
    assert_eq!(window.origin, None);
    assert!((window.wait - 600.0).abs() < 120.0, "{window:?}");
    world.set_target_body(FERRY, Some(ZILL));
    assert!(world.transfer_window(FERRY).is_some());
}

/// Kein Fenster: zum Körper, um den das Schiff kreist, und am Boden.
#[test]
fn no_window_without_a_transfer() {
    let mut world = demo();
    world.set_target_body(FERRY, Some(ZORB));
    assert_eq!(world.transfer_window(FERRY), None);
    assert_eq!(world.body_approach(FERRY), None);
    // Fähre-3 steht an der Startbasis.
    world.set_target_body(2, Some(NURA));
    assert_eq!(world.transfer_window(2), None);
    assert_eq!(world.body_approach(2), None);
}

/// Die Annäherung zeigt, wie nah die geplante Bahn dem Ziel kommt, auch ohne
/// Begegnung; führt die Bahn in die Einflusssphäre, zählen Eintritt und
/// Periapsis.
#[test]
fn approach_guides_to_the_encounter() {
    let mut world = demo();
    world.set_target_body(FERRY, Some(NURA));
    let soi = sim::body::sphere_of_influence(&world.bodies, NURA);
    let parked = world.body_approach(FERRY).unwrap();
    assert!(!parked.encounter && parked.distance > 1.0e10, "{parked:?}");

    let with = |dv: f64| {
        let mut w = world.clone();
        w.add_node(FERRY, node(163_400.0, dv)).unwrap();
        w.body_approach(FERRY).unwrap()
    };
    let (near, hit) = (with(1_080.0), with(1_090.0));
    assert!(!near.encounter);
    assert!(
        near.distance > soi && near.distance < 1.0e9 && near.distance < parked.distance,
        "{near:?}"
    );
    // Abstand = Abstand der beiden Lagen.
    assert!((near.position.distance(near.body_position) - near.distance).abs() < 1.0);

    assert!(hit.encounter);
    let periapsis = hit.distance - world.bodies[NURA].radius;
    assert!((periapsis - 9_871_100.0).abs() < 1_000.0, "{periapsis}");
    assert!((hit.position.distance(hit.body_position) - soi).abs() < 1.0);
    let days = hit.time / 21_600.0;
    assert!(days > 250.0 && days < 290.0, "{days}");
}

/// Ein Schiff hat höchstens ein Ziel: Schiff oder Körper.
#[test]
fn one_target_at_a_time() {
    let mut world = demo();
    world.set_target(FERRY, Some(0));
    world.set_target_body(FERRY, Some(NURA));
    assert_eq!(world.vessels[FERRY].target, None);
    assert_eq!(world.vessels[FERRY].target_body, Some(NURA));
    world.set_target(FERRY, Some(0));
    assert_eq!(world.vessels[FERRY].target_body, None);
    world.set_target_body(FERRY, None);
    assert_eq!(world.vessels[FERRY].target, Some(0));
    let json = serde_json::to_string(&world).unwrap();
    let loaded: sim::World = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, world);
}
