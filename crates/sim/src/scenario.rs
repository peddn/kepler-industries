//! Startszenario für die Vorführung.

use crate::body::{Atmosphere, Body, BodyId, BodyOrbit, SurfacePoint, circular_period};
use crate::design::Design;
use crate::kepler::state_from_apsides;
use crate::landing::LandingZone;
use crate::task;
use crate::world::{Base, BaseKind, VesselParams, World};
use std::f64::consts::PI;

/// Nummern der Körper in der Demo: die Sonne zuerst, dann der Heimatplanet
/// mit seinen Monden, dann die anderen Planeten (wie in KSP).
pub const ZUNN: BodyId = 0;
pub const ZORB: BodyId = 1;
pub const ZOON: BodyId = 2;
pub const ZILL: BodyId = 3;
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
        zorit: 0.0,
        data: 0.0,
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
        zorit: 0.0,
        data: 5.0,
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
        zorit: 1_000.0,
        data: 5.0,
    }
}

/// Zill, der zweite, äußere Mond von Zorb (`parent`): klein, aber extrem
/// dicht (100 km Radius, g = 5,75 m/s², ohne Luft; Fluchtgeschwindigkeit am
/// Boden rund 1.070 m/s, mehr als bei Zoon), weit draußen, schräg und
/// deutlich elliptisch: 35.250 bis 58.750 km vom Mittelpunkt des Planeten
/// (große Halbachse 47.000 km, Exzentrizität 0,25, Umlauf 1.077.311 s, knapp
/// 50 Tage des Planeten), Neigung 15°, Einflusssphäre etwa 9.050 km. Dreht
/// sich frei (in 40.400 s). Das anspruchsvollere Ziel nach Zoon: teurer zu
/// landen und zu verlassen, und man muss den richtigen Punkt seiner Bahn
/// treffen.
pub fn outer_moon(parent: BodyId) -> Body {
    let radius = 100_000.0;
    Body {
        name: "Zill".into(),
        gm: 5.75 * radius * radius,
        radius,
        rotation_period: 40_400.0,
        atmosphere: Atmosphere::NONE,
        orbit: Some(BodyOrbit {
            parent,
            semi_major_axis: 47_000_000.0,
            eccentricity: 0.25,
            inclination: 15f64.to_radians(),
            ascending_node: 78f64.to_radians(),
            argument_of_periapsis: 38f64.to_radians(),
            mean_anomaly: 2.2,
        }),
        zorit: 2_000.0,
        data: 10.0,
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
        zorit: 4_000.0,
        data: 15.0,
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
    cargo_capacity: 0.0,
    base_kit: false,
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
    cargo_capacity: 0.0,
    base_kit: false,
};

/// Booster der Startbasis (siehe `booster`): hebt Schiffe bis etwa 19 t so
/// hoch und schnell, dass sie mit rund 1,5 km/s eigenem Δv die Umlaufbahn
/// erreichen, und kommt zurück: Er landet ohne Fallschirm mit einem
/// Landebrennen kurz über dem Boden. Schub: das 1,5-fache des
/// Startgewichts mit einem vollen Frachter.
pub const BOOSTER: VesselParams = VesselParams {
    dry_mass: 3_000.0,
    drag_area: 8.0,
    thrust: 760_000.0,
    exhaust_velocity: 3_000.0,
    fuel_capacity: 30_000.0,
    parachute_area: 0.0,
    cargo_capacity: 0.0,
    base_kit: false,
};

/// Frachter der Demo (feste Werte; im Spiel kommen Schiffe aus Entwürfen,
/// siehe `design`, der Frachter dort hat zwei Z-1 mit zusammen 500 kN):
/// 2 t Frachtraum. So groß, dass er voll beladen auf dem Booster
/// von Zorb startet, auf Zoon landet und ohne Tanken mit Zorit zurückkommt;
/// auf Zorb bremsen Luft und Fallschirm.
pub const FREIGHTER: VesselParams = VesselParams {
    dry_mass: 2_500.0,
    drag_area: 5.0,
    thrust: 440_000.0,
    exhaust_velocity: 3_200.0,
    fuel_capacity: 14_000.0,
    parachute_area: 1_000.0,
    cargo_capacity: 2_000.0,
    base_kit: false,
};

/// Basisschiff der Demo (feste Werte, dieselben wie der Startentwurf in
/// `design`): startet auf dem Booster von Zorb, landet einmal und wird zum
/// Fundament einer Basis (siehe `base`). Kein Frachtraum.
pub const BASE_SHIP: VesselParams = VesselParams {
    dry_mass: 3_000.0,
    drag_area: 4.0,
    thrust: 250_000.0,
    exhaust_velocity: 3_200.0,
    fuel_capacity: 7_000.0,
    parachute_area: 400.0,
    cargo_capacity: 0.0,
    base_kit: true,
};

/// So viel Treibstoff haben die Schiffe der Startausstattung im Orbit: etwa,
/// was nach einem Start von Zorb übrig ist.
const FREIGHTER_FUEL_IN_ORBIT: f64 = 6_900.0;
const BASE_SHIP_FUEL_IN_ORBIT: f64 = 3_100.0;

/// Zorit im Lager der Startbasis zu Spielbeginn (kg): genau, was eine Basis
/// kostet. Ein Basisschiff, ein Frachter und 4 t Bauteile (der Frachter
/// fliegt dafür zweimal).
pub const START_ZORIT: f64 = 3_000.0;

/// Spielbeginn: das Sonnensystem (`solar_system`), auf Zorb die Startbasis
/// mit ihrem Zielgebiet, dem Booster und `START_ZORIT` im Lager. Keine
/// Schiffe: Man bestellt, was man braucht (siehe `base`).
pub fn start() -> World {
    let mut world = base_world();
    add_booster(&mut world);
    world
}

/// Der Booster steht vollgetankt an der Startbasis.
fn add_booster(world: &mut World) {
    let pad = world.bases[0].location;
    let booster = world.spawn_landed("Booster", BOOSTER, BOOSTER.fuel_capacity, ZORB, pad);
    world.booster = Some(booster);
}

/// Sonnensystem und Startbasis, noch ohne Booster.
fn base_world() -> World {
    let mut world = World::new(solar_system());
    world.tasks = task::standard(&world.bodies, ZORB);
    world.designs = vec![Design::base_ship(), Design::freighter()];
    world.home = ZORB;
    world.zorit = START_ZORIT;
    let pad = SurfacePoint::from_degrees(5.0, -20.0);
    world.bases.push(Base {
        name: "Startbasis".into(),
        body: ZORB,
        location: pad,
        kind: BaseKind::Depot,
        required: 0.0,
        delivered: 0.0,
        store: None,
    });
    world.zones.push(LandingZone {
        name: "Startbasis".into(),
        body: ZORB,
        center: pad,
        radius: ZONE_RADIUS,
    });
    world
}

/// Für Tests und zum Ausprobieren (`--demo`): wie `start`, aber ohne Booster
/// (die Fähre an der Startbasis startet aus eigener Kraft, wie überall
/// sonst; die Beispiele mit ihr bleiben so, wie sie sind), mit leerem
/// Lager, weiteren Zielgebieten und Schiffen. Zwei Satelliten: einer stabil
/// oberhalb der Atmosphäre, einer zu tief abgesetzt. Ohne Eingriff bremst die
/// Luft ihn bei jedem Durchgang ab, bis er abstürzt. Mit einem Manöver an der
/// Apoapsis lässt er sich retten. Dazu eine Fähre an der Startbasis, die
/// starten kann, eine vollgetankte Fähre im Orbit für den Flug zum Mond und
/// dahinter zwei Basisschiffe und zwei beladene Frachter.
pub fn demo() -> World {
    let mut world = base_world();
    world.zorit = 0.0;
    let planet = &world.bodies[ZORB];
    let (gm, r) = (planet.gm, planet.radius);
    let pad = world.bases[0].location;

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

    // Die Startausstattung (siehe `base`), auf derselben Bahn
    // hintereinander: zwei Basisschiffe (für eine Mine und eine Tankstelle)
    // und zwei Frachter mit zusammen den Bauteilen für eine Basis.
    for (name, params, fuel, cargo, ahead) in [
        (
            "Basisschiff-5",
            BASE_SHIP,
            BASE_SHIP_FUEL_IN_ORBIT,
            0.0,
            316.0,
        ),
        (
            "Basisschiff-6",
            BASE_SHIP,
            BASE_SHIP_FUEL_IN_ORBIT,
            0.0,
            318.0,
        ),
        (
            "Frachter-7",
            FREIGHTER,
            FREIGHTER_FUEL_IN_ORBIT,
            2_000.0,
            320.0,
        ),
        (
            "Frachter-8",
            FREIGHTER,
            FREIGHTER_FUEL_IN_ORBIT,
            2_000.0,
            322.0,
        ),
    ] {
        let state = state_from_apsides(
            gm,
            r + 100_000.0,
            r + 100_000.0,
            0.0,
            0.0,
            f64::to_radians(ahead),
            false,
        );
        let vessel = world.spawn_vessel(name, params, fuel, ZORB, state);
        world.load_cargo(vessel, cargo);
    }

    // Weitere Zielgebiete: eines etwa 60 km östlich der Startbasis zum Üben
    // eines Hops (Kurs 90°, Ziel-Apoapsis rund 27 km) und eines weit weg im
    // Osten, zum Anfliegen aus dem Orbit.
    // Auf Zoon: die Nahseite (Zorb im Zenit) und die Fernseite.
    for (name, body, point) in [
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
