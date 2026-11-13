//! Der Booster der Startbasis.
//!
//! Jeder Start von der Startbasis läuft mit ihm: Schiff und Booster heben als
//! Stapel ab, nur der Booster brennt. Bei Brennschluss trennen sie sich. Das
//! Schiff fliegt mit seinen eigenen Werten weiter, der Booster schiebt sich
//! waagerecht zurück (Boostback) und landet an der Startbasis: ohne
//! Fallschirm, mit einem Landebrennen kurz über dem Boden (siehe `landing`).
//!
//! Der Rückflug wird beim Planen des Starts berechnet (`plan_return`); kommt
//! der Booster nicht zurück, gibt es keinen Start. Bei der Trennung bekommt
//! er genau diesen Node, und seine Zeitleiste ist dieselbe wie beim Planen:
//! Vorschau = Ausführung.
//!
//! Für den Rückflug hält der Booster immer Treibstoff zurück (`reserve`). Der
//! Aufstieg endet an der Ziel-Apoapsis oder wenn nur noch diese Reserve im
//! Tank ist.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::design::Design;
use crate::kepler::{self, StateVector};
use crate::launch::{Launch, LaunchProfile};
use crate::maneuver::{self, ManeuverNode, PlanError};
use crate::trajectory::{Segment, Trajectory};
use crate::world::{BaseKind, VesselParams, World};
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Der erste Booster: hebt Schiffe bis etwa 19 t so hoch und schnell, dass
/// sie mit rund 1,5 km/s eigenem Δv die Umlaufbahn erreichen, und kommt
/// zurück: Er landet ohne Fallschirm mit einem Landebrennen kurz über dem
/// Boden. Schub: das 1,5-fache des Startgewichts mit einem vollen Frachter.
pub const B1: VesselParams = VesselParams {
    dry_mass: 3_000.0,
    drag_area: 8.0,
    thrust: 760_000.0,
    exhaust_velocity: 3_000.0,
    fuel_capacity: 30_000.0,
    parachute_area: 0.0,
    cargo_capacity: 0.0,
    base_kit: false,
};

/// Booster B-2 (Techtree, siehe `tech`): doppelt so groß, für Schiffe bis
/// etwa 38 t, also ein volles großes Chassis.
pub const B2: VesselParams = VesselParams {
    dry_mass: 6_000.0,
    drag_area: 12.0,
    thrust: 1_520_000.0,
    exhaust_velocity: 3_000.0,
    fuel_capacity: 60_000.0,
    parachute_area: 0.0,
    cargo_capacity: 0.0,
    base_kit: false,
};

/// So viel Δv hält der Booster für den Boostback zurück (m/s). Fliegt der
/// Stapel bei der Trennung schneller waagerecht, als sich damit umkehren
/// lässt, gibt es keinen Rückweg.
pub const RETURN_DELTA_V: f64 = 1_000.0;

/// So lange nach der Trennung liegt der Node des Boostbacks (s): Zeit, sich
/// zu drehen.
pub const BOOSTBACK_DELAY: f64 = 10.0;

/// So nah an der Startbasis muss der Booster aufsetzen (m).
pub const RETURN_RADIUS: f64 = 2_000.0;

/// So genau wird der Aufsetzpunkt entlang des Kurses gesucht (m).
const PRECISION: f64 = 10.0;

/// So genau wird er quer zum Kurs gesucht (m).
const ASIDE_PRECISION: f64 = 200.0;

/// Weiter als so wird der Boostback nicht zur Seite gedreht (Radiant).
const MAX_TURN: f64 = 0.6;

/// So viel Δv hält er zusätzlich für das Landebrennen zurück (m/s): Er fällt
/// ohne Fallschirm, die Luft bremst ihn auf rund 85 m/s.
pub const LANDING_DELTA_V: f64 = 200.0;

/// Treibstoff, den der Booster für Rückflug und Landung zurückhält (kg).
pub fn reserve(booster: &VesselParams) -> f64 {
    let delta_v = RETURN_DELTA_V + LANDING_DELTA_V;
    booster.dry_mass * ((delta_v / booster.exhaust_velocity).exp() - 1.0)
}

/// Der Booster eines geplanten Starts: seine Werte und sein Rückflug.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoosterPlan {
    pub params: VesselParams,
    /// Masse des Boosters bei der Trennung (kg).
    pub mass: f64,
    pub back: Return,
}

/// Der Rückflug des Boosters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Return {
    /// Der Boostback; `None`, wenn der Booster auch ohne ihn an der
    /// Startbasis aufsetzt (sehr steiler Aufstieg).
    pub node: Option<ManeuverNode>,
    /// Zeitpunkt des Aufsetzens.
    pub landing: f64,
    /// Abstand des Aufsetzpunkts von der Startbasis (m).
    pub miss: f64,
}

impl Return {
    /// Δv des Boostbacks (m/s); 0 ohne.
    pub fn delta_v(&self) -> f64 {
        self.node.map_or(0.0, |node| node.delta_v.length())
    }
}

/// Zeitleiste des Boosters ab der Trennung (Zustand `state`, Masse `mass`
/// zum Zeitpunkt `t`) mit dem Boostback `node` (ohne: freier Fall bis zum
/// Landebrennen).
pub fn flight(
    bodies: &[Body],
    params: &VesselParams,
    body: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
    node: Option<ManeuverNode>,
) -> Option<Trajectory> {
    let mut trajectory = Trajectory::new_flying(bodies, params, body, t, state, mass);
    if let Some(node) = node {
        trajectory.add_node(bodies, params, t, node).ok()?;
        trajectory.set_armed(true).ok()?;
    }
    Some(trajectory)
}

/// Sucht den Boostback, mit dem der Booster nach der Trennung an `pad`
/// aufsetzt. Der Schub geht waagerecht gegen den Kurs.
/// Gesucht wird sein Betrag (mehr davon trägt den Aufsetzpunkt entlang des
/// Kurses `heading`, in Grad, weiter zurück) und, wo die Drehung des Planeten
/// den Booster zur Seite trägt, ein Winkel, um den er dagegen gedreht ist.
#[allow(clippy::too_many_arguments)]
pub fn plan_return(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    pad: SurfacePoint,
    heading: f64,
    t: f64,
    state: StateVector,
    mass: f64,
) -> Result<Return, PlanError> {
    let body = &bodies[body_id];
    let time = t + BOOSTBACK_DELAY;
    // Lage am Node ohne Boostback: legt die Richtungen fest.
    let coast = Trajectory::new_flying(bodies, params, body_id, t, state, mass);
    let (_, at_node) = coast
        .predicted_state(bodies, time)
        .ok_or(PlanError::BoosterNoReturn)?;
    // Kurs am Startplatz, im mitdrehenden System.
    let pad_up = pad.body_fixed_direction();
    let east = DVec3::Z.cross(pad_up).try_normalize().unwrap_or(DVec3::Y);
    let north = pad_up.cross(east);
    let heading = heading.to_radians();
    let along = north * heading.cos() + east * heading.sin();
    // Der Stoß geht waagerecht gegen den Kurs: dessen Richtung dort, wo der
    // Booster gerade ist. (Nicht gegen seine Bewegung über dem Boden: Nach
    // einem steilen Aufstieg ist die fast null und ihre Richtung Zufall.)
    let up = at_node.position.normalize();
    let course = body.body_fixed_to_inertial(along, time);
    let Some(back) = (up * course.dot(up) - course).try_normalize() else {
        return Err(PlanError::BoosterNoReturn);
    };
    let side = up.cross(back);
    let [prograde, normal, radial] = maneuver::node_frame(&at_node);
    // Boostback mit dem Betrag `amount` (negativ: nach vorn), um den Winkel
    // `turn` zur Seite gedreht.
    let node = |amount: f64, turn: f64| {
        let push = (back * turn.cos() + side * turn.sin()) * amount;
        ManeuverNode {
            time,
            delta_v: DVec3::new(push.dot(prograde), push.dot(normal), push.dot(radial)),
        }
    };

    let left = pad_up.cross(along);
    // Wo der Booster heil aufsetzt, vom Startplatz aus gesehen.
    let touchdown = |node: Option<ManeuverNode>| -> Option<Landing> {
        let trajectory = flight(bodies, params, body_id, t, state, mass, node)?;
        let (time, point) = trajectory.segments().find_map(|s| match s {
            Segment::Surface {
                start,
                point,
                wreck: false,
                ..
            } => Some((*start, *point)),
            _ => None,
        })?;
        let at = point.body_fixed_direction();
        Some(Landing {
            time,
            ahead: body.radius * at.dot(along),
            aside: body.radius * at.dot(left),
        })
    };
    let landing = |amount: f64, turn: f64| touchdown(Some(node(amount, turn)));

    // Mehr als die Reserve hergibt, wird nicht versucht (bleibt vom Aufstieg
    // Treibstoff übrig, wäre mehr im Tank): Weit stärkere Stöße trügen den
    // Booster um den halben Planeten. Was für das Landebrennen zurückbleiben
    // muss, steht nicht zur Verfügung.
    let on_board = params.exhaust_velocity * (mass / params.dry_mass).ln();
    let available = (on_board - LANDING_DELTA_V).clamp(0.0, RETURN_DELTA_V);
    // Ohne Boostback landet er so weit vor dem Startplatz; liegt das schon
    // an der Startbasis, braucht es keinen.
    let free = touchdown(None).ok_or(PlanError::BoosterNoReturn)?;
    let range = free.ahead;
    if range.hypot(free.aside) <= ASIDE_PRECISION {
        return Ok(Return {
            node: None,
            landing: free.time,
            miss: range.hypot(free.aside),
        });
    }
    // Der Betrag für die Richtung `turn`: Mehr davon trägt den Aufsetzpunkt
    // stetig weiter zurück, mit allem Verfügbaren bis hinter den Startplatz.
    // Nach einem sehr steilen Aufstieg fiele der Booster schon ohne Stoß
    // hinter den Startplatz (der Planet dreht sich unter ihm weiter): Dann
    // geht der Stoß nach vorn, der Betrag ist negativ. Regula falsi
    // (Illinois), wenige Schritte genügen.
    let solve = |turn: f64| -> Result<(f64, Landing), PlanError> {
        let failed = PlanError::BoosterNoReturn;
        let far = if range > 0.0 { available } else { -available };
        // `low` landet vor dem Startplatz, `high` dahinter.
        let ahead_far = landing(far, turn).ok_or(failed)?.ahead;
        if range * ahead_far > 0.0 {
            return Err(failed);
        }
        let ((mut low, mut ahead_low), (mut high, mut ahead_high)) = if range > 0.0 {
            ((0.0, range), (far, ahead_far))
        } else {
            ((far, ahead_far), (0.0, range))
        };
        let mut found = Err(failed);
        for _ in 0..30 {
            let amount = (low * ahead_high - high * ahead_low) / (ahead_high - ahead_low);
            let at = landing(amount, turn).ok_or(failed)?;
            found = Ok((amount, at));
            if at.ahead.abs() < PRECISION {
                break;
            }
            if at.ahead > 0.0 {
                (low, ahead_low) = (amount, at.ahead);
                ahead_high *= 0.5;
            } else {
                (high, ahead_high) = (amount, at.ahead);
                ahead_low *= 0.5;
            }
        }
        found
    };

    // Die Drehung des Planeten trägt den Aufsetzpunkt zur Seite, je nach
    // Kurs verschieden weit: die Richtung nachdrehen, bis es passt
    // (Sekantenverfahren; der erste Schritt nach der Geometrie).
    let mut turn = 0.0;
    let mut best = solve(turn)?;
    let mut previous: Option<(f64, f64)> = None;
    for _ in 0..5 {
        let aside = best.1.aside;
        if aside.abs() < ASIDE_PRECISION {
            break;
        }
        let next = match previous {
            Some((turn_before, aside_before)) if aside != aside_before => {
                turn - aside * (turn - turn_before) / (aside - aside_before)
            }
            _ => turn + aside / range,
        };
        previous = Some((turn, aside));
        turn = next.clamp(-MAX_TURN, MAX_TURN);
        match solve(turn) {
            Ok(better) => best = better,
            Err(_) => break,
        }
    }
    let (amount, at) = best;
    let miss = at.ahead.hypot(at.aside);
    if miss > RETURN_RADIUS {
        return Err(PlanError::BoosterNoReturn);
    }
    Ok(Return {
        node: Some(node(amount, turn)),
        landing: at.time,
        miss,
    })
}

/// Aufsetzpunkt vom Startplatz aus: wann, wie weit vor (+) oder hinter (−)
/// ihm entlang des Kurses und wie weit links (+) oder rechts davon (m).
#[derive(Clone, Copy)]
struct Landing {
    time: f64,
    ahead: f64,
    aside: f64,
}

/// Nach dem geplanten Aufstieg: wann das Schiff die Apoapsis erreicht und
/// wie viel Δv es dort für die Kreisbahn braucht (m/s). `None`, wenn der
/// Aufstieg nicht über die Luft führt.
pub fn to_orbit(bodies: &[Body], trajectory: &Trajectory) -> Option<(f64, f64)> {
    let end = trajectory.launch()?.burn.end;
    trajectory.segments().find_map(|s| match s {
        Segment::Kepler { start, state, .. } if *start >= end => {
            let gm = bodies[s.body()].gm;
            let apoapsis = kepler::orbit_info(gm, state).apoapsis?;
            let coast = kepler::time_to_radius_outbound(gm, state, apoapsis - 1.0)?;
            let speed = state.position.cross(state.velocity).length() / apoapsis;
            Some((start + coast, (gm / apoapsis).sqrt() - speed))
        }
        _ => None,
    })
}

/// Was ein Start auf dem Booster für einen Entwurf ergibt (für den Editor).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchEstimate {
    /// Δv, das das Schiff nach der Trennung noch bis in die Kreisbahn an
    /// der Apoapsis braucht (m/s); `None`, wenn der Aufstieg nicht über die
    /// Luft führt.
    pub to_orbit: Option<f64>,
    /// Δv des Schiffs mit vollem Tank und dieser Fracht (m/s).
    pub on_board: f64,
}

impl LaunchEstimate {
    /// Was im Orbit übrig bleibt (m/s; negativ: es reicht nicht).
    pub fn left(&self) -> Option<f64> {
        self.to_orbit.map(|needed| self.on_board - needed)
    }
}

impl World {
    /// Plant probeweise den Start eines Schiffs nach `design` mit `cargo` kg
    /// Fracht von der Startbasis, mit vollem Tank und dem Standardprofil:
    /// dieselbe Planung wie im Spiel, in einer eigenen kleinen Welt (was der
    /// Booster gerade tut, spielt dafür keine Rolle). Fehler: warum der
    /// Start nicht ginge.
    pub fn launch_estimate(
        &self,
        design: &Design,
        cargo: f64,
    ) -> Result<LaunchEstimate, PlanError> {
        let depot = self
            .bases
            .iter()
            .find(|b| b.kind == BaseKind::Depot)
            .ok_or(PlanError::BoosterAway)?;
        let booster = self.booster.ok_or(PlanError::BoosterAway)?;
        let booster = self.vessels[booster].params;
        let (body, pad) = (depot.body, depot.location);

        let mut world = World::new(self.bodies.clone());
        world.bases.push(depot.clone());
        let index = world.spawn_landed("Booster", booster, booster.fuel_capacity, body, pad);
        world.booster = Some(index);
        let params = design.params();
        let ship = world.spawn_landed("Entwurf", params, params.fuel_capacity, body, pad);
        world.load_cargo(ship, cargo);
        let launch = Launch {
            time: 60.0,
            profile: LaunchProfile::for_body(&world.bodies[body]),
        };
        world.set_launch(ship, launch)?;

        let to_orbit = to_orbit(&world.bodies, &world.vessels[ship].trajectory).map(|(_, dv)| dv);
        let dry = params.dry_mass + cargo;
        let on_board = if params.exhaust_velocity > 0.0 && params.thrust > 0.0 {
            params.exhaust_velocity * ((dry + params.fuel_capacity) / dry).ln()
        } else {
            0.0
        };
        Ok(LaunchEstimate { to_orbit, on_board })
    }
}
