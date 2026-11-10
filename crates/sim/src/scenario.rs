//! Startszenario für die Vorführung.

use crate::body::{Atmosphere, Body, BodyOrbit, SurfacePoint, circular_period};
use crate::kepler::state_from_apsides;
use crate::landing::LandingZone;
use crate::world::{Base, VesselParams, World};
use std::f64::consts::PI;

/// Zorb, der Heimatplanet der Zorbels, in Kerbin-Größe: kleine Welt, kurze
/// Umlaufzeiten, ein Orbit ist mit einer einzigen Stufe erreichbar.
pub fn home_planet() -> Body {
    let radius = 600_000.0;
    let surface_gravity = 9.81;
    let gm = surface_gravity * radius * radius;
    Body {
        name: "Zorb".into(),
        gm,
        radius,
        rotation_period: 6.0 * 3600.0,
        atmosphere: Atmosphere {
            surface_density: 1.225,
            scale_height: 5_000.0,
            top_altitude: 70_000.0,
        },
        orbit: None,
        soi_radius: Some(home_system_radius(gm)),
    }
}

/// Rand des Heimatsystems: die Einflusssphäre des Planeten gegenüber der
/// Sonne (Laplace, a · (m/M)^0,4), mit Masse und Abstand wie bei Kerbin und
/// Kerbol. Etwa 84.159 km. Die Sonne selbst wird noch nicht simuliert.
fn home_system_radius(gm: f64) -> f64 {
    const SUN_GM: f64 = 1.172_332_8e18;
    const DISTANCE: f64 = 13_599_840_256.0;
    DISTANCE * (gm / SUN_GM).powf(0.4)
}

/// Zoon, der Mond von Zorb, wie der Mun in KSP: 200 km Radius, g = 1,63 m/s², ohne Luft, auf
/// einer Kreisbahn 12.000 km über dem Mittelpunkt des Planeten (Umlauf etwa
/// 38,6 h, Einflusssphäre etwa 2.430 km). Gebundene Rotation: Er zeigt dem
/// Planeten immer dieselbe Seite, Länge 0° zum Planeten hin (deshalb beginnt
/// er bei 180°).
pub fn moon(planet: &Body) -> Body {
    let radius = 200_000.0;
    let distance = 12_000_000.0;
    Body {
        name: "Zoon".into(),
        gm: 1.63 * radius * radius,
        radius,
        rotation_period: circular_period(planet.gm, distance),
        atmosphere: Atmosphere::NONE,
        orbit: Some(BodyOrbit::circular(0, distance, PI)),
        soi_radius: None,
    }
}

/// Zimm, der zweite, äußere Mond von Zorb, ähnlich Minmus in KSP: klein
/// (60 km Radius, g = 0,49 m/s², ohne Luft), weit draußen und schräg. Anders
/// als Minmus leicht elliptisch: 42.300 bis 51.700 km vom Mittelpunkt des
/// Planeten (große Halbachse 47.000 km, Exzentrizität 0,1, Umlauf wie
/// Minmus 1.077.311 s, knapp 50 Tage des Planeten), Neigung 6°,
/// Einflusssphäre etwa 2.250 km. Dreht sich frei (wie Minmus in 40.400 s).
pub fn outer_moon() -> Body {
    let radius = 60_000.0;
    Body {
        name: "Zimm".into(),
        gm: 0.491 * radius * radius,
        radius,
        rotation_period: 40_400.0,
        atmosphere: Atmosphere::NONE,
        orbit: Some(BodyOrbit {
            parent: 0,
            semi_major_axis: 47_000_000.0,
            eccentricity: 0.1,
            inclination: 6f64.to_radians(),
            ascending_node: 78f64.to_radians(),
            argument_of_periapsis: 38f64.to_radians(),
            mean_anomaly: 0.9,
        }),
        soi_radius: None,
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
/// lässt er sich retten. Dazu eine Fähre an der Startbasis, die starten kann,
/// und eine vollgetankte Fähre im Orbit für den Flug zum Mond.
pub fn demo() -> World {
    let planet = home_planet();
    let moon = moon(&planet);
    let mut world = World::new(vec![planet, moon, outer_moon()]);
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

    // Vollgetankt in 100 km Höhe über dem Äquator, so gestellt, dass ein
    // Transfer nach 10 Minuten (836 m/s prograd) knapp am Mond vorbeiführt.
    let parked = state_from_apsides(
        gm,
        r + 100_000.0,
        r + 100_000.0,
        0.0,
        0.0,
        314f64.to_radians(),
        false,
    );
    world.spawn_vessel("Mondfähre-4", FERRY, 4_000.0, 0, parked);

    // Zielgebiete: an der Startbasis, eines etwa 60 km östlich zum Üben eines
    // Hops (Kurs 90°, Ziel-Apoapsis rund 27 km) und eines weit weg im Osten,
    // zum Anfliegen aus dem Orbit.
    // Auf Zoon: die Nahseite (Zorb im Zenit) und die Fernseite.
    for (name, body, point) in [
        ("Startbasis", 0, pad),
        ("Übungsfeld", 0, SurfacePoint::from_degrees(4.94, -14.2)),
        ("Messfeld Ost", 0, SurfacePoint::from_degrees(2.0, 60.0)),
        ("Nahseite", 1, SurfacePoint::from_degrees(0.0, 0.0)),
        ("Fernseite", 1, SurfacePoint::from_degrees(0.0, 180.0)),
    ] {
        world.zones.push(LandingZone {
            name: name.into(),
            body,
            center: point,
            radius: ZONE_RADIUS,
        });
    }

    world
}
