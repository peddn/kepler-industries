//! Für mehrere Testdateien: Prüfschiffe für Tests, die ihre eigene kleine
//! Welt bauen, und ein Hop in der Demo.
//!
//! Zu den Prüfschiffen: Die Regeln der
//! Simulation gelten für beliebige Werte; diese beiden decken Bereiche ab,
//! die der Katalog (`sim::design`) nicht hat: sehr wenig Schub und ein Schiff
//! ohne Fallschirm.
#![allow(dead_code)]

use sim::design::Design;
use sim::scenario::{ZORB, demo};
use sim::world::VesselParams;
use sim::{Launch, LaunchProfile, Modules, World};

/// Kleiner Satellit: 400 kg leer, 2 kN Schub (lange Brennzeiten), rund
/// 1,2 km/s Δv bei 200 kg Treibstoff, kein Fallschirm.
pub const SATELLITE: VesselParams = VesselParams {
    dry_mass: 400.0,
    drag_area: 2.0,
    thrust: 2_000.0,
    exhaust_velocity: 3_000.0,
    fuel_capacity: 200.0,
    parachute_area: 0.0,
    cargo_capacity: 0.0,
    modules: Modules::NONE,
};

/// Fähre, die selbst vom Boden startet: 1,5 t leer, Schub für das 1,5-fache
/// des Startgewichts, mit 4 t Treibstoff rund 4,2 km/s Δv, mit Fallschirm.
pub const FERRY: VesselParams = VesselParams {
    dry_mass: 1_500.0,
    drag_area: 2.0,
    thrust: 80_000.0,
    exhaust_velocity: 3_200.0,
    fuel_capacity: 4_000.0,
    parachute_area: 400.0,
    cargo_capacity: 0.0,
    modules: Modules::NONE,
};

/// Der Frachter und das Basisschiff aus den Startentwürfen.
pub fn freighter_params() -> VesselParams {
    Design::freighter().params()
}

pub fn base_ship_params() -> VesselParams {
    Design::base_ship().params()
}

/// Ein vollgetankter Frachter steht im Übungsfeld der Demo, 60 km
/// östlich der Startbasis. Von dort startet er aus eigener Kraft (der
/// Booster gehört zur Startbasis). Liefert die Welt und seine Nummer.
pub fn at_practice_field() -> (World, usize) {
    let mut world = demo();
    let field = world
        .zones
        .iter()
        .find(|z| z.name == "Übungsfeld")
        .unwrap()
        .center;
    let ship = world.spawn_landed("Frachter", freighter_params(), 14_000.0, ZORB, field);
    (world, ship)
}

/// Hop mit Kurs und Ziel-Apoapsis (km), Start nach einer Minute. Flach
/// genug (Wendewinkel 20°), um 60 km weit zu kommen.
pub fn hop(heading: f64, apoapsis_km: f64) -> (World, usize) {
    let (mut world, ship) = at_practice_field();
    let profile = LaunchProfile {
        heading,
        target_apoapsis: apoapsis_km * 1000.0,
        kick: 20.0,
        ..LaunchProfile::default()
    };
    let launch = Launch {
        time: 60.0,
        profile,
    };
    world.set_launch(ship, launch).unwrap();
    world.set_armed(ship, true).unwrap();
    (world, ship)
}
