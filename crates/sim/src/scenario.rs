//! Startszenario für die Vorführung.

use crate::body::{Atmosphere, Body, BodyOrbit, SurfacePoint, circular_period};
use crate::kepler::state_from_apsides;
use crate::landing::LandingZone;
use crate::world::{Base, VesselParams, World};
use std::f64::consts::PI;

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
        orbit: None,
    }
}

/// Mond wie der Mun in KSP: 200 km Radius, g = 1,63 m/s², ohne Luft, auf
/// einer Kreisbahn 12.000 km über dem Mittelpunkt des Planeten (Umlauf etwa
/// 38,6 h, Einflusssphäre etwa 2.430 km). Gebundene Rotation: Er zeigt dem
/// Planeten immer dieselbe Seite, Länge 0° zum Planeten hin (deshalb beginnt
/// er bei 180°).
pub fn moon(planet: &Body) -> Body {
    let radius = 200_000.0;
    let distance = 12_000_000.0;
    Body {
        name: "Mond".into(),
        gm: 1.63 * radius * radius,
        radius,
        rotation_period: circular_period(planet.gm, distance),
        atmosphere: Atmosphere::NONE,
        orbit: Some(BodyOrbit {
            parent: 0,
            radius: distance,
            phase: PI,
        }),
    }
}

/// Kleiner Satellit mit Triebwerk: 400 kg leer, rund 1,2 km/s Δv bei 200 kg
/// Treibstoff.
pub const SATELLITE: VesselParams = VesselParams {
    dry_mass: 400.0,
    drag_area: 2.0,
    thrust: 2_000.0,
    exhaust_velocity: 3_000.0,
    fuel_capacity: 200.0,
    parachute_area: 0.0,
};

/// Radius der Zielgebiete in m.
pub const ZONE_RADIUS: f64 = 5_000.0;

/// Fähre, die selbst vom Boden startet: 1,5 t leer, Schub reicht für das
/// 1,5-fache des Startgewichts. Mit 4 t Treibstoff rund 4,2 km/s Δv: genug
/// für 100 km und das Zirkularisieren, mit etwas Rest. Am Fallschirm kommt sie
/// mit rund 10 m/s am Boden an.
pub const FERRY: VesselParams = VesselParams {
    dry_mass: 1_500.0,
    drag_area: 2.0,
    thrust: 80_000.0,
    exhaust_velocity: 3_200.0,
    fuel_capacity: 4_000.0,
    parachute_area: 400.0,
};

/// Ein Planet mit Mond, eine Startbasis, zwei Satelliten: einer stabil oberhalb der
/// Atmosphäre, einer zu tief abgesetzt. Ohne Eingriff bremst die Luft ihn bei
/// jedem Durchgang ab, bis er abstürzt. Mit einem Manöver an der Apoapsis
/// lässt er sich retten. Dazu eine Fähre an der Startbasis, die starten kann.
pub fn demo() -> World {
    let planet = home_planet();
    let moon = moon(&planet);
    let mut world = World::new(vec![planet, moon]);
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

    // Zielgebiete: an der Startbasis, eines etwa 60 km östlich zum Üben eines
    // Hops (Kurs 90°, Ziel-Apoapsis rund 27 km) und eines weit weg im Osten,
    // zum Anfliegen aus dem Orbit.
    for (name, point) in [
        ("Startbasis", pad),
        ("Übungsfeld", SurfacePoint::from_degrees(4.94, -14.2)),
        ("Messfeld Ost", SurfacePoint::from_degrees(2.0, 60.0)),
    ] {
        world.zones.push(LandingZone {
            name: name.into(),
            body: 0,
            center: point,
            radius: ZONE_RADIUS,
        });
    }

    world
}
