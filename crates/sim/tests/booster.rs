//! Tests für den Booster der Startbasis: Aufstieg als Stapel, Trennung,
//! Boostback und Landung mit Landebrennen.

mod common;

use common::{base_ship_params, freighter_params};
use glam::DVec3;
use sim::booster::{self, B1, RETURN_RADIUS};
use sim::save::{from_json, to_json};
use sim::scenario::{ZOON, ZORB, start};
use sim::{
    BoosterStatus, Launch, LaunchProfile, LogKind, ManeuverNode, PlanError, SurfacePoint,
    VesselParams, VesselStatus, World,
};

/// Der Startentwurf, wie er in `World::designs` steht.
const BASE_SHIP_DESIGN: usize = 0;

const BOOSTER_INDEX: usize = 0;

/// Spielbeginn mit einem vollgetankten Schiff an der Startbasis (dazu `cargo`
/// kg Bauteile).
fn at_depot(params: VesselParams, cargo: f64) -> (World, usize) {
    let mut world = start();
    let pad = world.bases[0].location;
    let ship = world.spawn_landed("Schiff", params, params.fuel_capacity, ZORB, pad);
    world.load_cargo(ship, cargo);
    (world, ship)
}

fn launch(time: f64, kick: f64) -> Launch {
    Launch {
        time,
        profile: LaunchProfile {
            kick,
            ..LaunchProfile::default()
        },
    }
}

/// Der Frachter mit geplantem Start und dem Node für die Kreisbahn an der
/// Apoapsis, scharf.
fn ready_freighter_params() -> (World, usize) {
    let (mut world, ship) = at_depot(freighter_params(), 2_000.0);
    world.set_launch(ship, launch(60.0, 10.0)).unwrap();
    let (time, delta_v) = circularize(&world, ship);
    let node = ManeuverNode {
        time,
        delta_v: DVec3::new(delta_v, 0.0, 0.0),
    };
    world.add_node(ship, node).unwrap();
    world.set_armed(ship, true).unwrap();
    (world, ship)
}

/// Zeitpunkt der Apoapsis nach dem Aufstieg und das Δv dort für die
/// Kreisbahn.
fn circularize(world: &World, ship: usize) -> (f64, f64) {
    booster::to_orbit(&world.bodies, &world.vessels[ship].trajectory)
        .expect("der Aufstieg muss über die Luft führen")
}

/// Der ganze Ablauf: Der Stapel hebt ab, das Schiff verbraucht dabei nichts,
/// trennt sich bei Brennschluss und erreicht mit seinem Node die Umlaufbahn.
/// Der Booster landet heil an der Startbasis und steht wieder bereit.
#[test]
fn ship_reaches_orbit_and_booster_returns() {
    let (mut world, ship) = ready_freighter_params();
    let plan = *world.vessels[ship].trajectory.launch().unwrap();
    let booster = plan.booster.unwrap();
    let full = freighter_params().dry_mass + freighter_params().fuel_capacity + 2_000.0;
    assert_eq!(
        world.booster_status(),
        Some(BoosterStatus::Reserved { ship })
    );
    // Der Stapel wiegt beim Abheben Schiff und vollen Booster.
    assert_eq!(plan.burn.start_mass, full + B1.dry_mass + B1.fuel_capacity);
    // Der Aufstieg kostet das Schiff nichts.
    assert!(world.delta_v_budget(ship).burns.len() == 1);
    assert!(booster.mass >= B1.dry_mass + booster::reserve(&B1) - 1e-6);

    // Vor dem Start: voller Tank, verplant für Aufstieg und Boostback.
    let budget = world.delta_v_budget(BOOSTER_INDEX);
    assert_eq!(budget.available, budget.full);
    assert_eq!(budget.burns.len(), 2);
    assert!((budget.burns[1].1 - booster.back.delta_v()).abs() < 1e-9);
    assert!(
        budget.rest >= booster::LANDING_DELTA_V - 1e-6,
        "{}",
        budget.rest
    );

    world.advance_to(plan.burn.start + 30.0);
    assert_eq!(
        world.booster_status(),
        Some(BoosterStatus::Ascending { ship })
    );
    // Der Booster verbrennt dabei seinen Treibstoff, nicht den des Schiffs.
    let burned = 30.0 * plan.burn.mass_flow;
    assert!((world.vessel_fuel(BOOSTER_INDEX) - (B1.fuel_capacity - burned)).abs() < 1e-6);
    let rising = world.delta_v_budget(BOOSTER_INDEX);
    assert!(rising.available < budget.available && rising.burns[0].1 < budget.burns[0].1);
    assert!((rising.rest - budget.rest).abs() < 1e-9);
    assert_eq!(world.vessel_mass(ship), full);

    world.advance_to(plan.burn.end + 1.0);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Returning));
    assert_eq!(world.vessel_mass(ship), full);
    assert!((world.vessel_mass(BOOSTER_INDEX) - booster.mass).abs() < 1e-6);
    assert!(
        world
            .log
            .iter()
            .any(|e| e.vessel == BOOSTER_INDEX && e.kind == LogKind::Separated { ship })
    );

    world.advance_to(booster.back.landing + 1.0);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
    let VesselStatus::Landed { point } = world.vessel_status(BOOSTER_INDEX) else {
        panic!("Booster: {:?}", world.vessel_status(BOOSTER_INDEX));
    };
    assert_eq!(world.zone_at(ZORB, &point), Some(0));
    assert!(booster.back.miss < RETURN_RADIUS);
    // Ohne Fallschirm: Er hat kurz über dem Boden gebremst und ist nach
    // wenigen Minuten zurück.
    let landing_burns = |world: &World| {
        world
            .log
            .iter()
            .filter(|e| e.vessel == BOOSTER_INDEX && e.kind == LogKind::LandingBurn)
            .count()
    };
    assert_eq!(landing_burns(&world), 1);
    // An der Startbasis ist er sofort wieder vollgetankt.
    assert!((world.vessel_fuel(BOOSTER_INDEX) - B1.fuel_capacity).abs() < 1e-9);
    assert_eq!(B1.parachute_area, 0.0);
    assert!(booster.back.landing - plan.burn.end < 400.0);

    // Das Schiff ist auf einer Bahn über der Luft und hat genug für Zoon.
    let VesselStatus::Orbit {
        periapsis_altitude, ..
    } = world.vessel_status(ship)
    else {
        panic!("Schiff: {:?}", world.vessel_status(ship));
    };
    assert!(periapsis_altitude > 75_000.0, "{periapsis_altitude}");
    let left = world.vessel_delta_v(ship);
    assert!(left > 2_900.0, "{left}");
}

/// Wie viele Schritte die Zeit braucht, ändert nichts: Trennung und Landung
/// des Boosters fallen bitgenau gleich aus.
#[test]
fn booster_is_independent_of_step_size() {
    let run = |step: f64| {
        let (mut world, _) = ready_freighter_params();
        while world.time < 1_500.0 {
            world.advance(step);
        }
        world.advance_to(1_500.0);
        world
    };
    let coarse = run(1_500.0);
    assert_eq!(run(0.7), coarse);
    assert_eq!(run(33.0), coarse);
    assert_eq!(coarse.booster_status(), Some(BoosterStatus::Ready));
}

/// Mitten im Rückflug gespeichert und geladen: Es geht genau gleich weiter.
#[test]
fn return_flight_is_saved() {
    let (mut world, ship) = ready_freighter_params();
    let end = world.vessels[ship].trajectory.launch().unwrap().burn.end;
    world.advance_to(end + 200.0);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Returning));
    let mut loaded = from_json(&to_json(&world)).unwrap();
    assert_eq!(loaded, world);
    world.advance(2_000.0);
    loaded.advance(2_000.0);
    assert_eq!(loaded, world);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
}

/// Kein Start ohne Booster: nicht, solange er für ein anderes Schiff
/// verplant ist, nicht, solange er unterwegs ist, und er selbst lässt sich
/// nicht steuern.
#[test]
fn launch_needs_the_booster() {
    let (mut world, ship) = ready_freighter_params();
    let other = world.order_ship(BASE_SHIP_DESIGN).unwrap();
    let later = launch(5_000.0, 10.0);
    assert_eq!(world.set_launch(other, later), Err(PlanError::BoosterBusy));
    assert_eq!(
        world.set_launch(BOOSTER_INDEX, later),
        Err(PlanError::Booster)
    );

    let end = world.vessels[ship].trajectory.launch().unwrap().burn.end;
    world.advance_to(end + 100.0);
    assert_eq!(world.set_launch(other, later), Err(PlanError::BoosterAway));
    let node = ManeuverNode {
        time: world.time + 60.0,
        delta_v: DVec3::X,
    };
    assert_eq!(world.add_node(BOOSTER_INDEX, node), Err(PlanError::Booster));

    // Zurück an der Startbasis trägt er das nächste Schiff.
    world.advance_to(3_000.0);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
    world.set_launch(other, later).unwrap();
    assert_eq!(
        world.booster_status(),
        Some(BoosterStatus::Reserved { ship: other })
    );
    // Abgesagt ist er wieder frei.
    world.clear_launch(other).unwrap();
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
}

/// Verweigert, wenn der Booster nicht zurückkäme: mit einem zu schweren
/// Schiff und bei zu flachem Aufstieg.
#[test]
fn refused_when_the_booster_cannot_return() {
    let heavy = VesselParams {
        fuel_capacity: 26_000.0,
        ..freighter_params()
    };
    let (mut world, ship) = at_depot(heavy, 2_000.0);
    assert_eq!(
        world.set_launch(ship, launch(60.0, 10.0)),
        Err(PlanError::BoosterNoReturn)
    );

    let (mut world, ship) = at_depot(freighter_params(), 2_000.0);
    assert_eq!(
        world.set_launch(ship, launch(60.0, 20.0)),
        Err(PlanError::BoosterNoReturn)
    );
    // Steiler geht es.
    world.set_launch(ship, launch(60.0, 8.0)).unwrap();
}

/// Auch das leichte Basisschiff kommt mit dem Booster in die Bahn.
#[test]
fn base_ship_launches_on_the_booster() {
    let (mut world, ship) = at_depot(base_ship_params(), 0.0);
    world.set_launch(ship, launch(60.0, 12.0)).unwrap();
    let (_, delta_v) = circularize(&world, ship);
    let left = world.vessel_delta_v(ship) - delta_v;
    assert!(left > 2_100.0, "{left}");
}

/// Nur an der Startbasis: Anderswo startet ein Schiff aus eigener Kraft,
/// auch auf Zorb.
#[test]
fn elsewhere_ships_launch_on_their_own() {
    let mut world = start();
    let field = SurfacePoint::from_degrees(2.0, 60.0);
    let on_zorb = world.spawn_landed("Feld", freighter_params(), 14_000.0, ZORB, field);
    let on_zoon = world.spawn_landed(
        "Zoon",
        freighter_params(),
        14_000.0,
        ZOON,
        SurfacePoint::from_degrees(0.0, 0.0),
    );
    world.set_launch(on_zorb, launch(60.0, 10.0)).unwrap();
    let zoon_profile = Launch {
        time: 60.0,
        profile: LaunchProfile::for_body(&world.bodies[ZOON]),
    };
    world.set_launch(on_zoon, zoon_profile).unwrap();
    for ship in [on_zorb, on_zoon] {
        let plan = world.vessels[ship].trajectory.launch().unwrap();
        assert!(plan.booster.is_none());
        assert_eq!(plan.burn.start_mass, 2_500.0 + 14_000.0);
    }
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
}

/// Eine nicht scharfe Kette rückt beim Abheben einen Tag weiter: Der Booster
/// bleibt stehen und ist weiter für dieses Schiff vorgesehen.
#[test]
fn unarmed_launch_keeps_the_booster_waiting() {
    let (mut world, ship) = at_depot(freighter_params(), 2_000.0);
    world.set_launch(ship, launch(60.0, 10.0)).unwrap();
    let first = *world.vessels[ship].trajectory.launch().unwrap();
    world.advance_to(first.burn.end + 500.0);
    assert_eq!(
        world.booster_status(),
        Some(BoosterStatus::Reserved { ship })
    );
    assert!(matches!(
        world.vessel_status(BOOSTER_INDEX),
        VesselStatus::Landed { .. }
    ));
    let day = world.bodies[ZORB].rotation_period;
    let shifted = world.vessels[ship].trajectory.launch().unwrap();
    assert!((shifted.launch.time - first.launch.time - day).abs() < 1e-9);
}

/// Der Booster findet von jedem Kurs zwischen Nord und Süd über Ost zurück:
/// Die Drehung des Planeten trägt ihn je nach Kurs zur Seite, der Boostback
/// ist entsprechend gedreht.
#[test]
fn booster_returns_from_any_eastward_heading() {
    for heading in [0.0, 45.0, 75.0, 90.0, 135.0, 180.0] {
        let (mut world, ship) = at_depot(freighter_params(), 2_000.0);
        let start = Launch {
            time: 60.0,
            profile: LaunchProfile {
                heading,
                ..LaunchProfile::default()
            },
        };
        world.set_launch(ship, start).unwrap();
        let plan = world.vessels[ship].trajectory.launch().unwrap();
        let back = plan.booster.unwrap().back;
        assert!(back.miss < 500.0, "Kurs {heading}: {} m", back.miss);
    }
}

/// Sehr steil geht auch: Nach einem senkrechten Aufstieg hat sich der Planet
/// weitergedreht, der Booster fiele hinter den Startplatz. Die Suche findet
/// auch dafür den Stoß (oder keinen, wenn er ohnehin trifft).
#[test]
fn steep_launch_returns_too() {
    for kick in [0.0, 1.0, 2.0] {
        let (mut world, ship) = at_depot(freighter_params(), 2_000.0);
        world.set_launch(ship, launch(60.0, kick)).unwrap();
        world.set_armed(ship, true).unwrap();
        let plan = *world.vessels[ship].trajectory.launch().unwrap();
        let back = plan.booster.unwrap().back;
        assert!(back.miss < 500.0, "{kick}°: {} m", back.miss);
        assert!(back.delta_v() < 200.0, "{kick}°: {}", back.delta_v());
        world.advance_to(back.landing + 1.0);
        assert_eq!(
            world.booster_status(),
            Some(BoosterStatus::Ready),
            "{kick}°"
        );
    }
}

/// Mitten im Aufstieg gespeichert und geladen: Trennung und Rückflug laufen
/// genau gleich weiter.
#[test]
fn ascent_is_saved() {
    let (mut world, ship) = ready_freighter_params();
    let plan = *world.vessels[ship].trajectory.launch().unwrap();
    world.advance_to(plan.burn.start + 40.0);
    assert_eq!(
        world.booster_status(),
        Some(BoosterStatus::Ascending { ship })
    );
    let mut loaded = from_json(&to_json(&world)).unwrap();
    assert_eq!(loaded, world);
    world.advance(2_000.0);
    loaded.advance(2_000.0);
    assert_eq!(loaded, world);
    assert_eq!(world.booster_status(), Some(BoosterStatus::Ready));
}
