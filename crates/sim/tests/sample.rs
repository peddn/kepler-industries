//! Tests für Proben: der Probenbohrer, Proben vom Boden nehmen und an der
//! Startbasis für Daten abgeben.

use sim::body::SurfacePoint;
use sim::design::{Design, DesignError, Module, Part};
use sim::goods::{LOT, SAMPLE_LIMIT};
use sim::scenario::{ZILL, ZOON, ZORB, start};
use sim::tech::Tech;
use sim::{BaseError, Good, LogKind, World};

/// Frachter mit Probenbohrer.
fn sampler() -> Design {
    let mut design = Design::freighter();
    design.name = "Sammler".into();
    design.modules.push(Module::Sampler);
    design
}

/// Ein Schiff nach `design` mit 1 t Treibstoff, gelandet auf `body`.
fn landed(world: &mut World, design: &Design, body: usize) -> usize {
    let at = SurfacePoint::from_degrees(10.0, 30.0);
    world.spawn_landed("Schiff", design.params(), 1_000.0, body, at)
}

/// Ein Frachter an der Startbasis mit `amount` kg Proben von `body` an Bord.
fn back_home(world: &mut World, body: usize, amount: f64) -> usize {
    let pad = world.bases[0].location;
    let params = Design::freighter().params();
    let ship = world.spawn_landed("Zurück", params, 1_000.0, ZORB, pad);
    world.vessels[ship].cargo.set(Good::Sample(body), amount);
    world.vessels[ship]
        .trajectory
        .set_landed_mass(params.dry_mass + 1_000.0 + amount);
    ship
}

/// Der Probenbohrer ist ein Modul: ein Platz, 300 kg, 0,3 t Zorit, kein
/// Innenraum. Bauen lässt er sich erst mit der Technik für 20 GB.
#[test]
fn sampler_is_a_module_behind_a_tech() {
    let mut world = start();
    let (plain, design) = (Design::freighter(), sampler());
    assert_eq!(
        design.check(&world.unlocked()),
        Err(DesignError::Locked(Part::Module(Module::Sampler)))
    );
    world.data = 20.0;
    world.unlock(Tech::Sampler).unwrap();
    assert_eq!(world.data, 0.0);
    assert_eq!(design.check(&world.unlocked()), Ok(()));

    assert_eq!(design.module_slots(), 1);
    assert_eq!(design.params().dry_mass, plain.params().dry_mass + 300.0);
    assert_eq!(design.price(), plain.price() + 300.0);
    assert_eq!(design.cargo(), plain.cargo());
    assert!(design.params().modules.has(Module::Sampler));
    assert!(!plain.params().modules.has(Module::Sampler));
    // Neben dem Fundament ist im mittleren Chassis noch Platz dafür.
    let mut both = design.clone();
    both.modules.push(Module::Foundation);
    assert_eq!(both.check(&world.unlocked()), Ok(()));
}

/// Proben kommen vom Boden: nur mit Probenbohrer, nur die des Körpers, auf
/// dem das Schiff steht, nicht auf Zorb, und so viel, wie hineinpasst.
/// Auskippen geht überall und bringt nichts.
#[test]
fn samples_come_from_the_ground() {
    let mut world = start();
    let ship = landed(&mut world, &sampler(), ZOON);
    let plain = landed(&mut world, &Design::freighter(), ZOON);
    let at_home = landed(&mut world, &sampler(), ZORB);
    let zoon = Good::Sample(ZOON);
    let empty = world.vessel_mass(ship);

    assert_eq!(world.load(plain, zoon, LOT), Err(BaseError::NoSampler));
    assert_eq!(
        world.load(ship, Good::Sample(ZILL), LOT),
        Err(BaseError::NoSamplesHere)
    );
    assert_eq!(
        world.load(at_home, Good::Sample(ZORB), LOT),
        Err(BaseError::NoSamplesHere)
    );

    assert_eq!(world.load(ship, zoon, LOT), Ok(500.0));
    assert_eq!(world.vessel_mass(ship), empty + 500.0);
    // Der Boden ist unbegrenzt, der Frachtraum (2 t) nicht.
    assert_eq!(world.load(ship, zoon, 5_000.0), Ok(1_500.0));
    assert_eq!(world.load(ship, zoon, LOT), Err(BaseError::NothingToLoad));
    assert_eq!(world.vessels[ship].cargo.get(zoon), 2_000.0);

    // Auskippen: weg damit, ohne Daten und ohne Eintrag.
    assert_eq!(world.unload(ship, zoon, LOT), Ok(500.0));
    assert_eq!(world.vessels[ship].cargo.get(zoon), 1_500.0);
    assert_eq!(world.vessel_mass(ship), empty + 1_500.0);
    assert_eq!(world.data, 0.0);
    assert!(world.log.is_empty());
    assert_eq!(world.in_depot(zoon), 0.0);
}

/// An der Startbasis abgeladen, bringen Proben Daten: je Tonne das Doppelte
/// des Grundwerts ihres Körpers, bis zur Obergrenze. Sie bleiben im Lager
/// und lassen sich nicht wieder laden.
#[test]
fn samples_bring_data_up_to_the_limit() {
    let mut world = start();
    world.tasks.clear();
    let zoon = Good::Sample(ZOON);
    let ship = back_home(&mut world, ZOON, 2_000.0);

    // Zoon hat den Grundwert 5 GB: 10 GB je Tonne.
    assert_eq!(world.unload(ship, zoon, 1_500.0), Ok(1_500.0));
    assert_eq!(world.data, 15.0);
    assert_eq!(world.in_depot(zoon), 1_500.0);
    assert_eq!(
        world.log.last().unwrap().kind,
        LogKind::Samples {
            body: ZOON,
            amount: 1_500.0,
            data: 15.0
        }
    );
    // Abgegeben ist abgegeben.
    assert_eq!(world.load(ship, zoon, LOT), Err(BaseError::NoSamplesHere));

    // Über die Obergrenze hinaus zählt nichts mehr.
    let more = back_home(&mut world, ZOON, 2_000.0);
    assert_eq!(world.unload(more, zoon, 2_000.0), Ok(2_000.0));
    assert_eq!(world.data, 20.0);
    assert_eq!(world.sample_data(ZOON), 20.0);
    assert_eq!(world.in_depot(zoon), 3_500.0);
    assert_eq!(world.unload(ship, zoon, LOT), Ok(500.0));
    assert_eq!(world.data, 20.0);
    assert!(matches!(
        world.log.last().unwrap().kind,
        LogKind::Samples { data, .. } if data == 0.0
    ));

    // Jeder Körper zählt für sich: Zill hat den Grundwert 10 GB.
    let zill = back_home(&mut world, ZILL, SAMPLE_LIMIT);
    assert_eq!(
        world.unload(zill, Good::Sample(ZILL), f64::INFINITY),
        Ok(2_000.0)
    );
    assert_eq!(world.data, 60.0);

    // Der Spielstand behält, was abgegeben ist.
    let loaded = sim::save::from_json(&sim::save::to_json(&world)).unwrap();
    assert_eq!(loaded.sample_data(ZOON), 20.0);
    assert_eq!(loaded.data, 60.0);
}
