//! Tests für den Spielstand als Text (`sim::save`).

use glam::DVec3;
use sim::save::{LoadError, from_json, to_json};
use sim::scenario::{demo, start};
use sim::world::SAVE_VERSION;
use sim::{BaseKind, ManeuverNode};

/// Der Startentwurf, wie er in `World::designs` steht.
const FREIGHTER_DESIGN: usize = 1;

/// Speichern und Laden mitten im Geschehen: Die geladene Welt ist dieselbe
/// und läuft bitgenau gleich weiter, samt geplanter Manöver, Fracht, Basen
/// und Lager.
#[test]
fn loaded_game_continues_exactly() {
    let mut world = demo();
    for (vessel, time) in [(4, 647.5), (6, 620.0), (7, 610.0)] {
        let node = ManeuverNode {
            time,
            delta_v: DVec3::new(845.0, 0.0, 0.0),
        };
        world.add_node(vessel, node).unwrap();
        world.set_armed(vessel, true).unwrap();
    }
    // Mitten im Brennen der Frachter.
    world.advance_to(615.0);
    let loaded = from_json(&to_json(&world)).unwrap();
    assert_eq!(loaded, world);

    let mut worlds = [world, loaded];
    for w in &mut worlds {
        w.advance_to(28_200.0);
        let mine = w.found_base(4, BaseKind::Mine).unwrap();
        w.unload(6).unwrap();
        w.unload(7).unwrap();
        w.advance(5_000.0);
        assert!(w.base_stock(mine) > 0.0);
    }
    let [world, loaded] = worlds;
    assert_eq!(loaded, world);
    // Und noch einmal durch den Spielstand, jetzt mit Basis und Lager.
    assert_eq!(from_json(&to_json(&world)).unwrap(), world);
}

/// Der Spielbeginn samt bestellter Schiffe und Vorrat.
#[test]
fn start_is_saved() {
    let mut world = start();
    world.order_ship(FREIGHTER_DESIGN).unwrap();
    let loaded = from_json(&to_json(&world)).unwrap();
    assert_eq!(loaded, world);
    // Der Booster und das bestellte Schiff.
    assert_eq!((loaded.zorit, loaded.vessels.len()), (2_000.0, 2));
}

/// Ein Spielstand einer anderen Version wird als solcher erkannt, auch wenn
/// ihm Felder fehlen; Unlesbares wird abgelehnt.
#[test]
fn other_versions_and_garbage_are_refused() {
    let text = to_json(&start());
    let current = format!("\"save_version\":{SAVE_VERSION}");
    assert!(text.contains(&current));
    let old = text.replace(&current, "\"save_version\":7");
    assert_eq!(from_json(&old), Err(LoadError::Version(7)));
    assert_eq!(
        from_json("{\"save_version\":3,\"time\":0.0}"),
        Err(LoadError::Version(3))
    );
    let message = LoadError::Version(7).to_string();
    assert!(message.contains('7') && message.contains(&SAVE_VERSION.to_string()));

    for garbage in ["", "kein Spielstand", "{}", "[1,2,3]"] {
        assert!(
            matches!(from_json(garbage), Err(LoadError::Invalid(_))),
            "{garbage}"
        );
    }
    // Richtige Version, aber verstümmelt.
    let cut = &text[..text.len() / 2];
    assert!(matches!(from_json(cut), Err(LoadError::Invalid(_))));
}
