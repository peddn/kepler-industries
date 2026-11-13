//! Startszenario für die Vorführung.

use crate::body::{Atmosphere, Body, BodyId, BodyOrbit, SurfacePoint, circular_period};
use crate::booster;
use crate::design::Design;
use crate::kepler::state_from_apsides;
use crate::landing::LandingZone;
use crate::task;
use crate::world::{Base, BaseKind, World};
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

/// Radius der Zielgebiete in m.
pub const ZONE_RADIUS: f64 = 5_000.0;

/// So viel Treibstoff haben die Schiffe der Demo im Orbit: etwa, was nach
/// einem Start von Zorb übrig ist.
const FREIGHTER_FUEL_IN_ORBIT: f64 = 6_900.0;
const BASE_SHIP_FUEL_IN_ORBIT: f64 = 3_100.0;
/// Frachter-1 der Demo hat einen Teil verflogen (rund 1,1 km/s übrig),
/// Frachter-2 fast alles (rund 250 m/s).
const RELAY_FUEL: f64 = 1_000.0;
const LOW_FUEL: f64 = 200.0;

/// Zorit im Lager der Startbasis zu Spielbeginn (kg): genau, was eine Basis
/// kostet. Ein Basisschiff, ein Frachter und 4 t Bauteile (der Frachter
/// fliegt dafür zweimal).
pub const START_ZORIT: f64 = 3_000.0;

/// Spielbeginn: das Sonnensystem (`solar_system`), auf Zorb die Startbasis
/// mit ihrem Zielgebiet, dem Booster und `START_ZORIT` im Lager. Keine
/// Schiffe: Man bestellt, was man braucht (siehe `base`).
pub fn start() -> World {
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
    let booster = world.spawn_landed("Booster", booster::B1, booster::B1.fuel_capacity, ZORB, pad);
    world.booster = Some(booster);
    world
}

/// Ein Schiff nach einem Entwurf, benannt wie ein bestelltes. `place` setzt
/// es mit seinen Werten aus und liefert seine Nummer.
fn ship(
    world: &mut World,
    design: &Design,
    place: impl FnOnce(&mut World, &str) -> usize,
) -> usize {
    let name = format!("{}-{}", design.name, world.ship_count() + 1);
    let vessel = place(world, &name);
    world.vessels[vessel].design = Some(design.clone());
    vessel
}

/// Für Tests und zum Ausprobieren (`--demo`): der Spielbeginn (`start`),
/// dazu weitere Zielgebiete und Schiffe aus den Startentwürfen, die schon
/// unterwegs sind. Die Nummer im Namen ist die Nummer des Schiffs (der
/// Booster ist Nummer 0):
///
/// - Frachter-1 auf einer stabilen Kreisbahn, als Ziel für ein Rendezvous.
/// - Frachter-2 zu tief abgesetzt und fast leer. Ohne Eingriff bremst die
///   Luft ihn bei jedem Durchgang ab, bis er herunterkommt. Mit einem
///   Manöver an der Apoapsis lässt er sich retten.
/// - Frachter-3 vollgetankt an der Startbasis, bereit zum Start auf dem
///   Booster.
/// - Frachter-4 auf der Parkbahn, für Flüge zu den Monden und nach Nura.
/// - dahinter zwei Basisschiffe und zwei beladene Frachter: alles für eine
///   Basis.
pub fn demo() -> World {
    let mut world = start();
    let planet = &world.bodies[ZORB];
    let (gm, r) = (planet.gm, planet.radius);
    let pad = world.bases[0].location;
    let freighter = Design::freighter();
    let base_ship = Design::base_ship();
    // Höhen in m, Winkel in Grad. Eine elliptische Bahn beginnt an der
    // Apoapsis.
    let orbit = |periapsis: f64, apoapsis: f64, inclination: f64, node: f64, anomaly: f64| {
        state_from_apsides(
            gm,
            r + periapsis,
            r + apoapsis,
            inclination.to_radians(),
            node.to_radians(),
            anomaly.to_radians(),
            periapsis < apoapsis,
        )
    };
    let in_orbit = |world: &mut World, design: &Design, fuel: f64, state| {
        ship(world, design, |w, name| {
            w.spawn_vessel(name, design.params(), fuel, ZORB, state)
        })
    };

    in_orbit(
        &mut world,
        &freighter,
        RELAY_FUEL,
        orbit(100e3, 100e3, 28.0, 0.0, 0.0),
    );
    // Periapsis 62 km liegt in der Luft.
    in_orbit(
        &mut world,
        &freighter,
        LOW_FUEL,
        orbit(62e3, 150e3, 10.0, 120.0, 0.0),
    );
    ship(&mut world, &freighter, |w, name| {
        let params = freighter.params();
        w.spawn_landed(name, params, params.fuel_capacity, ZORB, pad)
    });
    // In 100 km Höhe über dem Äquator, so gestellt, dass ein Transfer nach
    // 10 Minuten knapp am Mond vorbeiführt. Dahinter auf derselben Bahn die
    // Schiffe für eine Basis; die Frachter tragen zusammen ihre Bauteile.
    in_orbit(
        &mut world,
        &freighter,
        FREIGHTER_FUEL_IN_ORBIT,
        orbit(100e3, 100e3, 0.0, 0.0, 314.0),
    );
    for (design, fuel, cargo, ahead) in [
        (&base_ship, BASE_SHIP_FUEL_IN_ORBIT, 0.0, 316.0),
        (&base_ship, BASE_SHIP_FUEL_IN_ORBIT, 0.0, 318.0),
        (&freighter, FREIGHTER_FUEL_IN_ORBIT, 2_000.0, 320.0),
        (&freighter, FREIGHTER_FUEL_IN_ORBIT, 2_000.0, 322.0),
    ] {
        let state = orbit(100e3, 100e3, 0.0, 0.0, ahead);
        let vessel = in_orbit(&mut world, design, fuel, state);
        world.load_cargo(vessel, cargo);
    }

    // Weitere Zielgebiete: eines etwa 60 km östlich der Startbasis und eines
    // weit weg im Osten, zum Anfliegen aus dem Orbit.
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
