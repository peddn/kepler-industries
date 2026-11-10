//! Startszenario für die Vorführung.

use crate::body::{Atmosphere, Body, BodyId, BodyOrbit, SurfacePoint, circular_period};
use crate::kepler::state_from_apsides;
use crate::landing::LandingZone;
use crate::world::{Base, VesselParams, World};
use std::f64::consts::PI;

/// Nummern der Körper in der Demo: die Sonne zuerst, dann der Heimatplanet
/// mit seinen Monden, dann die anderen Planeten (wie in KSP).
pub const ZUNN: BodyId = 0;
pub const ZORB: BodyId = 1;
pub const ZOON: BodyId = 2;
pub const ZIMM: BodyId = 3;
pub const NURA: BodyId = 4;

/// Zunn, die Sonne, wie Kerbol in KSP: 261.600 km Radius, Masse wie Kerbol,
/// ohne Luft. Sie ruht im Ursprung, ihre Einflusssphäre ist unbegrenzt.
pub fn sun() -> Body {
    Body {
        name: "Zunn".into(),
        gm: 1.172_332_8e18,
        radius: 261_600_000.0,
        rotation_period: 432_000.0,
        atmosphere: Atmosphere::NONE,
        orbit: None,
    }
}

/// Zorb, der Heimatplanet der Zorbels, in Kerbin-Größe: kleine Welt, kurze
/// Umlaufzeiten, ein Orbit ist mit einer einzigen Stufe erreichbar. Er dreht
/// sich wie Kerbin in 21.549,425 s (5:59:09) gegenüber den Sternen, damit
/// der Sonnentag um Zunn genau 6 h dauert. Hier allein, ohne Bahn (für
/// Tests); in der Demo kreist er um Zunn (`home_orbit`).
pub fn home_planet() -> Body {
    let radius = 600_000.0;
    let surface_gravity = 9.81;
    Body {
        name: "Zorb".into(),
        gm: surface_gravity * radius * radius,
        radius,
        rotation_period: 21_549.425,
        atmosphere: Atmosphere {
            surface_density: 1.225,
            scale_height: 5_000.0,
            top_altitude: 70_000.0,
        },
        orbit: None,
    }
}

/// Zorbs Bahn um Zunn (`parent`), wie die von Kerbin: ein Kreis mit
/// 13.599.840 km Radius, ein Jahr dauert 9.203.545 s (426 Tage). Zum
/// Zeitpunkt 0 steht Zunn in +X von Zorb aus (dort liegt die Startbasis im
/// Licht). Zorbs Einflusssphäre reicht damit etwa 84.159 km weit.
pub fn home_orbit(parent: BodyId) -> BodyOrbit {
    BodyOrbit::circular(parent, 13_599_840_256.0, PI)
}

/// Zoon, der Mond von Zorb, wie der Mun in KSP: 200 km Radius, g = 1,63 m/s²,
/// ohne Luft, auf einer Kreisbahn 12.000 km über dem Mittelpunkt des Planeten
/// (`parent`, Umlauf etwa 38,6 h, Einflusssphäre etwa 2.430 km). Gebundene
/// Rotation: Er zeigt dem Planeten immer dieselbe Seite, Länge 0° zum
/// Planeten hin (deshalb beginnt er bei 180°).
pub fn moon(parent: BodyId, planet: &Body) -> Body {
    let radius = 200_000.0;
    let distance = 12_000_000.0;
    Body {
        name: "Zoon".into(),
        gm: 1.63 * radius * radius,
        radius,
        rotation_period: circular_period(planet.gm, distance),
        atmosphere: Atmosphere::NONE,
        orbit: Some(BodyOrbit::circular(parent, distance, PI)),
    }
}

/// Zimm, der zweite, äußere Mond von Zorb (`parent`), ähnlich Minmus in KSP:
/// klein (60 km Radius, g = 0,49 m/s², ohne Luft), weit draußen und schräg.
/// Anders als Minmus leicht elliptisch: 42.300 bis 51.700 km vom
/// Mittelpunkt des Planeten (große Halbachse 47.000 km, Exzentrizität 0,1,
/// Umlauf wie Minmus 1.077.311 s, knapp 50 Tage des Planeten), Neigung 6°,
/// Einflusssphäre etwa 2.250 km. Dreht sich frei (wie Minmus in 40.400 s).
pub fn outer_moon(parent: BodyId) -> Body {
    let radius = 60_000.0;
    Body {
        name: "Zimm".into(),
        gm: 0.491 * radius * radius,
        radius,
        rotation_period: 40_400.0,
        atmosphere: Atmosphere::NONE,
        orbit: Some(BodyOrbit {
            parent,
            semi_major_axis: 47_000_000.0,
            eccentricity: 0.1,
            inclination: 6f64.to_radians(),
            ascending_node: 78f64.to_radians(),
            argument_of_periapsis: 38f64.to_radians(),
            mean_anomaly: 0.9,
        }),
    }
}

/// Nura, der zweite Planet, Duna aus KSP nachempfunden: 320 km Radius,
/// g = 2,94 m/s², dünne Luft bis 50 km, Drehung in 65.518 s (gut drei Tage
/// von Zorb). Bahn um Zunn (`parent`) wie Duna: große Halbachse
/// 20.726.155 km, Exzentrizität 0,051, Neigung 0,06°, Umlauf 17.315.000 s
/// (gut zwei Jahre von Zorb), Einflusssphäre etwa 47.900 km. Zum Zeitpunkt 0
/// so gestellt, dass das erste Startfenster von Zorb aus nach wenigen Tagen
/// kommt (Nura etwa 44° vor Zorb); danach alle gut 900 Tage.
pub fn nura(parent: BodyId) -> Body {
    let radius = 320_000.0;
    Body {
        name: "Nura".into(),
        gm: 2.943 * radius * radius,
        radius,
        rotation_period: 65_517.859,
        atmosphere: Atmosphere {
            surface_density: 0.14,
            scale_height: 7_000.0,
            top_altitude: 50_000.0,
        },
        orbit: Some(BodyOrbit {
            parent,
            semi_major_axis: 20_726_155_264.0,
            eccentricity: 0.051,
            inclination: 0.06f64.to_radians(),
            ascending_node: 135.5f64.to_radians(),
            argument_of_periapsis: 0.0,
            mean_anomaly: 1.52,
        }),
    }
}

/// Das Sonnensystem der Demo in der Reihenfolge der Nummern oben.
pub fn solar_system() -> Vec<Body> {
    let planet = Body {
        orbit: Some(home_orbit(ZUNN)),
        ..home_planet()
    };
    let moon = moon(ZORB, &planet);
    vec![sun(), planet, moon, outer_moon(ZORB), nura(ZUNN)]
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

/// Das Sonnensystem (`solar_system`), auf Zorb eine Startbasis, zwei
/// Satelliten: einer stabil oberhalb der Atmosphäre, einer zu tief abgesetzt. Ohne Eingriff bremst die Luft ihn bei
/// jedem Durchgang ab, bis er abstürzt. Mit einem Manöver an der Apoapsis
/// lässt er sich retten. Dazu eine Fähre an der Startbasis, die starten kann,
/// und eine vollgetankte Fähre im Orbit für den Flug zum Mond.
pub fn demo() -> World {
    let mut world = World::new(solar_system());
    world.home = ZORB;
    let planet = &world.bodies[ZORB];
    let (gm, r) = (planet.gm, planet.radius);

    let pad = SurfacePoint::from_degrees(5.0, -20.0);
    world.bases.push(Base {
        name: "Startbasis".into(),
        body: ZORB,
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
    world.spawn_vessel("Relais-1", SATELLITE, 100.0, ZORB, stable);

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
    world.spawn_vessel("Sonde-2", SATELLITE, 200.0, ZORB, too_low);

    world.spawn_landed("Fähre-3", FERRY, 4_000.0, ZORB, pad);

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
    world.spawn_vessel("Mondfähre-4", FERRY, 4_000.0, ZORB, parked);

    // Zielgebiete: an der Startbasis, eines etwa 60 km östlich zum Üben eines
    // Hops (Kurs 90°, Ziel-Apoapsis rund 27 km) und eines weit weg im Osten,
    // zum Anfliegen aus dem Orbit.
    // Auf Zoon: die Nahseite (Zorb im Zenit) und die Fernseite.
    for (name, body, point) in [
        ("Startbasis", ZORB, pad),
        ("Übungsfeld", ZORB, SurfacePoint::from_degrees(4.94, -14.2)),
        ("Messfeld Ost", ZORB, SurfacePoint::from_degrees(2.0, 60.0)),
        ("Nahseite", ZOON, SurfacePoint::from_degrees(0.0, 0.0)),
        ("Fernseite", ZOON, SurfacePoint::from_degrees(0.0, 180.0)),
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
