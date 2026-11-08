//! Startszenario für die Vorführung.

use crate::body::{Atmosphere, Body, SurfacePoint};
use crate::kepler::state_from_apsides;
use crate::world::{Base, VesselParams, World};

/// Fiktiver Heimatplanet in Kerbin-Größe: kleine Welt, kurze Umlaufzeiten,
/// ein Orbit ist mit einer einzigen Stufe erreichbar.
pub fn home_planet() -> Body {
    let radius = 600_000.0;
    let surface_gravity = 9.81;
    Body {
        name: "Heimat".into(),
        gm: surface_gravity * radius * radius,
        radius,
        rotation_period: 6.0 * 3600.0,
        atmosphere: Atmosphere {
            surface_density: 1.225,
            scale_height: 5_000.0,
            top_altitude: 70_000.0,
        },
    }
}

/// Kleiner Satellit mit Triebwerk: 400 kg leer, rund 1,2 km/s Δv bei 200 kg
/// Treibstoff.
pub const SATELLITE: VesselParams = VesselParams {
    dry_mass: 400.0,
    drag_area: 2.0,
    thrust: 2_000.0,
    exhaust_velocity: 3_000.0,
};

/// Fähre, die selbst vom Boden startet: 1,5 t leer, Schub reicht für das
/// 1,5-fache des Startgewichts. Mit 4 t Treibstoff rund 4,2 km/s Δv: genug
/// für 100 km und das Zirkularisieren, mit etwas Rest.
pub const FERRY: VesselParams = VesselParams {
    dry_mass: 1_500.0,
    drag_area: 2.0,
    thrust: 80_000.0,
    exhaust_velocity: 3_200.0,
};

/// Ein Planet, eine Startbasis, zwei Satelliten: einer stabil oberhalb der
/// Atmosphäre, einer zu tief abgesetzt. Ohne Eingriff bremst die Luft ihn bei
/// jedem Durchgang ab, bis er abstürzt. Mit einem Manöver an der Apoapsis
/// lässt er sich retten. Dazu eine Fähre an der Startbasis, die starten kann.
pub fn demo() -> World {
    let mut world = World::new(vec![home_planet()]);
    let planet = &world.bodies[0];
    let (gm, r) = (planet.gm, planet.radius);

    let pad = SurfacePoint::from_degrees(5.0, -20.0);
    world.bases.push(Base {
        name: "Startbasis".into(),
        body: 0,
        location: pad,
    });

    let stable = state_from_apsides(
        gm,
        r + 100_000.0,
        r + 100_000.0,
        28f64.to_radians(),
        0.0,
        0.0,
        false,
    );
    world.spawn_vessel("Relais-1", SATELLITE, 100.0, 0, stable);

    // Periapsis 62 km liegt in der Luft.
    let too_low = state_from_apsides(
        gm,
        r + 62_000.0,
        r + 150_000.0,
        10f64.to_radians(),
        120f64.to_radians(),
        0.0,
        true,
    );
    world.spawn_vessel("Sonde-2", SATELLITE, 200.0, 0, too_low);

    world.spawn_landed("Fähre-3", FERRY, 4_000.0, 0, pad);

    world
}
