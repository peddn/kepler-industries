//! Startszenario für die Vorführung in PR 1.

use crate::body::{Atmosphere, Body, SurfacePoint};
use crate::kepler::state_from_apsides;
use crate::world::{Base, World};

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

/// Ein Planet, eine Startbasis, zwei Satelliten: einer stabil oberhalb der
/// Atmosphäre, einer zu tief abgesetzt (Fehlschlag: er bremst sich in der Luft
/// ab und stürzt ab).
pub fn demo() -> World {
    let mut world = World::new(vec![home_planet()]);
    let planet = &world.bodies[0];
    let (gm, r) = (planet.gm, planet.radius);

    world.bases.push(Base {
        name: "Startbasis".into(),
        body: 0,
        location: SurfacePoint::from_degrees(5.0, -20.0),
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
    world.spawn_vessel("Relais-1", 500.0, 2.0, 0, stable);

    // Periapsis 62 km liegt in der Luft: rund fünf Durchgänge, dann Absturz
    // nach etwa drei Stunden Spielzeit.
    let too_low = state_from_apsides(
        gm,
        r + 62_000.0,
        r + 150_000.0,
        10f64.to_radians(),
        120f64.to_radians(),
        0.0,
        true,
    );
    world.spawn_vessel("Sonde-2", 500.0, 2.0, 0, too_low);

    world
}
