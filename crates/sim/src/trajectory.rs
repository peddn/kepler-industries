//! Die Zeitleiste eines Schiffs: eine Folge von Abschnitten.
//!
//! Jeder Abschnitt beschreibt lückenlos, wo das Schiff in einem Zeitraum ist:
//! - `Kepler`: Freiflug ohne Luft und ohne Schub, analytisch.
//! - `Integrated`: vorausberechnet, wenn Luft oder Schub im Spiel ist.
//! - `Surface`: am Boden, fest mit dem Körper verbunden.
//!
//! "Wo ist das Schiff zum Zeitpunkt t?" heißt immer: passenden Abschnitt suchen.
//! Ein Abschnitt hängt nur von seinem Anfangszustand und dem geplanten Manöver
//! ab. Deshalb ist das Ergebnis unabhängig davon, wann und in wie großen
//! Zeitschritten die Simulation vorangetrieben wird.
//!
//! Geplante Manöver bilden eine Kette. Jeder Node schneidet die Zeitleiste an
//! seinem Brennbeginn ab; dort folgt ein `Integrated`-Abschnitt mit Schub. Die
//! Vorschau ist einfach die vorausberechnete Zeitleiste.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::integrator::{self, Forces, Sample};
use crate::kepler::{self, StateVector};
use crate::launch::{self, Launch, LaunchPlan};
use crate::maneuver::{self, Burn, ManeuverNode, Plan, PlanError};
use crate::world::VesselParams;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Wie viele Abschnitte nach dem letzten geplanten Brennen (ohne Manöver: ab
/// jetzt) im Voraus bekannt sind. Reicht, um nach einem Manöver die neue Bahn
/// oder den Aufschlag zu sehen. Auf einer Bahn, die bei jedem Umlauf durch
/// die Luft führt, sind das vier Umläufe.
pub const LOOKAHEAD: usize = 8;

/// Womit ein Abschnitt endet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// Eintritt in die Atmosphäre: weiter schrittweise.
    AtmosphereEntry,
    /// Verlassen der Atmosphäre: weiter auf einer Keplerbahn.
    AtmosphereExit,
    /// Bodenberührung: Wrack.
    GroundContact,
    /// Beginn des geplanten Brennmanövers.
    BurnStart,
    /// Ende des Brennmanövers.
    BurnEnd,
    /// Maximale Stücklänge erreicht; gleiche Kräfte im nächsten Stück.
    Continue,
    /// Abheben vom Boden: Beginn des Aufstiegs.
    Liftoff,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Kepler {
        body: BodyId,
        start: f64,
        /// Zustand zu Beginn (die "Epoche"); wird nie verändert.
        state: StateVector,
        mass: f64,
        /// Zeitpunkt und Art des Endes; `None`, wenn die Bahn weder Luft noch
        /// Boden berührt und kein Manöver geplant ist.
        end: Option<(f64, Event)>,
    },
    Integrated {
        body: BodyId,
        /// Stützpunkte; der erste ist der Anfang, der letzte das Ende.
        samples: Vec<Sample>,
        /// Masse zu Beginn (ohne Schub konstant).
        mass: f64,
        /// Brennmanöver, falls in diesem Abschnitt das Triebwerk läuft.
        burn: Option<Burn>,
        end_event: Event,
    },
    Surface {
        body: BodyId,
        start: f64,
        point: SurfacePoint,
        mass: f64,
        /// Durch einen Aufschlag entstanden.
        wreck: bool,
        /// Zeitpunkt des geplanten Abhebens; sonst bleibt das Schiff stehen.
        #[serde(default)]
        liftoff: Option<f64>,
    },
}

impl Segment {
    pub fn body(&self) -> BodyId {
        match self {
            Segment::Kepler { body, .. }
            | Segment::Integrated { body, .. }
            | Segment::Surface { body, .. } => *body,
        }
    }

    pub fn start(&self) -> f64 {
        match self {
            Segment::Kepler { start, .. } | Segment::Surface { start, .. } => *start,
            Segment::Integrated { samples, .. } => samples[0].time,
        }
    }

    /// Endzeitpunkt, `f64::INFINITY` für offene Abschnitte.
    pub fn end(&self) -> f64 {
        match self {
            Segment::Kepler { end, .. } => end.map_or(f64::INFINITY, |(t, _)| t),
            Segment::Integrated { samples, .. } => samples[samples.len() - 1].time,
            Segment::Surface { liftoff, .. } => liftoff.unwrap_or(f64::INFINITY),
        }
    }

    pub fn end_event(&self) -> Option<Event> {
        match self {
            Segment::Kepler { end, .. } => end.map(|(_, e)| e),
            Segment::Integrated { end_event, .. } => Some(*end_event),
            Segment::Surface { liftoff, .. } => liftoff.map(|_| Event::Liftoff),
        }
    }

    /// Läuft in diesem Abschnitt das Triebwerk?
    pub fn is_burn(&self) -> bool {
        matches!(self, Segment::Integrated { burn: Some(_), .. })
    }

    pub fn mass_at(&self, t: f64) -> f64 {
        match self {
            Segment::Integrated {
                burn: Some(burn), ..
            } => burn.mass_at(t),
            Segment::Kepler { mass, .. }
            | Segment::Integrated { mass, .. }
            | Segment::Surface { mass, .. } => *mass,
        }
    }

    /// Zustand relativ zum eigenen Körper zum Zeitpunkt `t`.
    pub fn state_at(&self, bodies: &[Body], t: f64) -> StateVector {
        let body = &bodies[self.body()];
        match self {
            Segment::Kepler { start, state, .. } => kepler::propagate(body.gm, *state, t - start),
            Segment::Integrated { samples, .. } => {
                // Erster Stützpunkt mit Zeit >= t, dann zwischen Vorgänger und ihm interpolieren.
                let i = samples.partition_point(|s| s.time < t);
                if i == 0 {
                    samples[0].state
                } else if i >= samples.len() {
                    samples[samples.len() - 1].state
                } else {
                    integrator::interpolate(&samples[i - 1], &samples[i], t)
                }
            }
            Segment::Surface { point, .. } => {
                let position = body.surface_to_inertial(*point, 0.0, t);
                StateVector {
                    position,
                    velocity: body.co_rotating_velocity(position),
                }
            }
        }
    }

    /// Berechnet den Abschnitt, der auf diesen folgt (`None` bei offenen Abschnitten).
    /// `pending` ist das geplante, noch nicht begonnene Brennmanöver.
    fn next(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        pending: Option<&Burn>,
    ) -> Option<Segment> {
        let event = self.end_event()?;
        let (body, t) = (self.body(), self.end());
        let state = self.state_at(bodies, t);
        let mass = self.mass_at(t);
        Some(match event {
            Event::AtmosphereEntry => {
                integrated(bodies, params, body, t, state, mass, None, pending)
            }
            Event::AtmosphereExit => kepler_segment(bodies, body, t, state, mass, pending),
            Event::GroundContact => wreck(bodies, body, t, state, mass),
            Event::BurnStart | Event::Liftoff => {
                let burn = *pending?;
                integrated(bodies, params, body, t, state, mass, Some(burn), None)
            }
            Event::BurnEnd => coast(bodies, params, body, t, state, mass, pending),
            Event::Continue => match self {
                Segment::Integrated {
                    burn: Some(burn), ..
                } => integrated(bodies, params, body, t, state, mass, Some(*burn), None),
                _ => integrated(bodies, params, body, t, state, mass, None, pending),
            },
        })
    }
}

/// Beginn des geplanten Brennmanövers, falls es nicht vor `t` liegt.
fn burn_start_after(pending: Option<&Burn>, t: f64) -> Option<f64> {
    pending.map(|b| b.start).filter(|&s| s >= t)
}

/// Keplerabschnitt ab `t`, inklusive seines Endereignisses.
fn kepler_segment(
    bodies: &[Body],
    body_id: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
    pending: Option<&Burn>,
) -> Segment {
    let body = &bodies[body_id];
    let natural_event = if body.atmosphere.exists() {
        Event::AtmosphereEntry
    } else {
        Event::GroundContact
    };
    let natural = kepler::time_to_radius_inbound(body.gm, &state, body.boundary_radius())
        .map(|dt| (t + dt, natural_event));
    let end = match (natural, burn_start_after(pending, t)) {
        (Some((t_natural, event)), Some(t_burn)) if t_natural <= t_burn => Some((t_natural, event)),
        (_, Some(t_burn)) => Some((t_burn, Event::BurnStart)),
        (natural, None) => natural,
    };
    Segment::Kepler {
        body: body_id,
        start: t,
        state,
        mass,
        end,
    }
}

/// Schrittweise berechneter Abschnitt: mit Schub, wenn `burn` gesetzt ist,
/// sonst Freiflug durch die Luft (endet spätestens am Beginn von `pending`).
#[allow(clippy::too_many_arguments)]
fn integrated(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
    burn: Option<Burn>,
    pending: Option<&Burn>,
) -> Segment {
    let forces = Forces {
        body: &bodies[body_id],
        drag_area: params.drag_area,
        mass,
        burn: burn.as_ref(),
    };
    let stop = match &burn {
        Some(b) => Some((b.end, Event::BurnEnd)),
        None => burn_start_after(pending, t).map(|s| (s, Event::BurnStart)),
    };
    let (samples, end_event) = integrator::integrate_arc(&forces, Sample { time: t, state }, stop);
    Segment::Integrated {
        body: body_id,
        samples,
        mass,
        burn,
        end_event,
    }
}

/// Freiflug ab `t`: schrittweise, wenn dort Luft ist, sonst Kepler.
fn coast(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
    pending: Option<&Burn>,
) -> Segment {
    let body = &bodies[body_id];
    let altitude = state.position.length() - body.radius;
    if body.atmosphere.density(altitude) > 0.0 {
        integrated(bodies, params, body_id, t, state, mass, None, pending)
    } else {
        kepler_segment(bodies, body_id, t, state, mass, pending)
    }
}

/// Derselbe Plan einen Umlauf später. `segment` ist der Keplerabschnitt, in
/// dem der Node liegt. Nur möglich, wenn die Bahn geschlossen ist und bis zum
/// Ende des verschobenen Brennens weder Luft noch Boden berührt.
fn next_orbit(bodies: &[Body], segment: &Segment, plan: &Plan) -> Option<Plan> {
    let Segment::Kepler { start, state, .. } = segment else {
        return None;
    };
    let body = &bodies[segment.body()];
    let period = kepler::period(body.gm, state)?;
    let natural_end = kepler::time_to_radius_inbound(body.gm, state, body.boundary_radius())
        .map_or(f64::INFINITY, |dt| start + dt);
    let later = plan.one_orbit_later(period);
    (later.burn.end <= natural_end).then_some(later)
}

fn wreck(bodies: &[Body], body_id: BodyId, t: f64, state: StateVector, mass: f64) -> Segment {
    let (point, _) = bodies[body_id].inertial_to_surface(state.position, t);
    Segment::Surface {
        body: body_id,
        start: t,
        point,
        mass,
        wreck: true,
        liftoff: None,
    }
}

/// Das nächste geplante Brennen nach dem Abschnitt `back`. Nach einem Brennen
/// beginnt das nächste echt später (siehe `locate`); so wird ein Brennen ohne
/// Dauer nicht zweimal gefunden.
fn pending_after(burns: &[Burn], back: &Segment) -> Option<Burn> {
    let t = back.end();
    let after_burn = back.is_burn();
    burns.iter().copied().find(|b| {
        if after_burn {
            b.start > t
        } else {
            b.start >= t
        }
    })
}

/// Berechnet Abschnitte voraus, bis `LOOKAHEAD` Abschnitte nach dem Ende des
/// letzten geplanten Brennens liegen (ohne Brennen: insgesamt) oder die
/// Zeitleiste offen endet (stabile Bahn oder am Boden).
fn extend(
    segments: &mut VecDeque<Segment>,
    bodies: &[Body],
    params: &VesselParams,
    burns: &[Burn],
) {
    let horizon = burns.last().map_or(f64::NEG_INFINITY, |b| b.end);
    loop {
        let ahead = segments
            .iter()
            .filter(|s| s.start() >= horizon && !s.is_burn())
            .count();
        let Some(back) = segments.back() else { return };
        if ahead >= LOOKAHEAD {
            return;
        }
        let pending = pending_after(burns, back);
        match back.next(bodies, params, pending.as_ref()) {
            Some(segment) => segments.push_back(segment),
            None => return,
        }
    }
}

/// Der Abschnitt `first`, neu berechnet ab seinem Anfangszustand und bis zum
/// ersten geplanten Brennen (am Boden: bis zum Abheben). `None` während eines
/// Brennens.
fn restart(
    first: &Segment,
    bodies: &[Body],
    params: &VesselParams,
    burns: &[Burn],
) -> Option<Segment> {
    let pending = burns.first().copied();
    match first {
        Segment::Surface {
            body,
            start,
            point,
            mass,
            wreck,
            ..
        } => Some(Segment::Surface {
            body: *body,
            start: *start,
            point: *point,
            mass: *mass,
            wreck: *wreck,
            liftoff: pending
                .filter(|b| !*wreck && b.ascent.is_some())
                .map(|b| b.start),
        }),
        Segment::Kepler {
            body,
            start,
            state,
            mass,
            ..
        } => Some(kepler_segment(
            bodies,
            *body,
            *start,
            *state,
            *mass,
            pending.as_ref(),
        )),
        Segment::Integrated {
            body,
            samples,
            mass,
            burn: None,
            ..
        } => Some(integrated(
            bodies,
            params,
            *body,
            samples[0].time,
            samples[0].state,
            *mass,
            None,
            pending.as_ref(),
        )),
        _ => None,
    }
}

/// Der Plan für `node` auf der Zeitleiste `segments`, die alle früheren
/// Brennvorgänge der Kette enthält und danach frei weiterläuft. `previous`
/// ist das Brennen davor.
///
/// Regel: Das Brennen beginnt nicht vor jetzt und echt nach dem Ende des
/// vorherigen. Sonst rückt der Node auf seiner Bahn einen Umlauf weiter (an
/// dieselbe Stelle der Bahn), so oft wie nötig. Geht das nicht, ist er
/// ungültig.
fn locate(
    bodies: &[Body],
    params: &VesselParams,
    segments: &VecDeque<Segment>,
    now: f64,
    previous: Option<&Burn>,
    node: ManeuverNode,
) -> Result<Plan, PlanError> {
    if node.time <= now {
        return Err(PlanError::InPast);
    }
    let too_early = |burn: &Burn| match previous {
        None => burn.start < now,
        Some(p) => burn.start <= p.end,
    };
    let no_next_orbit = if previous.is_some() {
        PlanError::Overlaps
    } else {
        PlanError::TooLate
    };

    // Die Bahn, auf der der Node liegt: ab dem Ende des vorherigen Brennens.
    let leg_start = previous.map_or(f64::NEG_INFINITY, |b| b.end);
    let mut leg = segments
        .iter()
        .filter(|s| s.start() >= leg_start && !s.is_burn());
    let segment = if node.time < leg_start {
        // Vor dem Ende des vorherigen Brennens: Nur auf einer geschlossenen
        // Bahn gibt es dieselbe Stelle einen Umlauf später.
        match leg.next() {
            Some(s @ Segment::Kepler { state, .. })
                if kepler::period(bodies[s.body()].gm, state).is_some() =>
            {
                s
            }
            _ => return Err(no_next_orbit),
        }
    } else {
        leg.find(|s| s.start() <= node.time && node.time < s.end())
            .ok_or(PlanError::BeyondPrediction)?
    };
    if matches!(segment, Segment::Surface { .. }) {
        return Err(PlanError::OnGround);
    }

    let node_state = segment.state_at(bodies, node.time);
    let mass = segment.mass_at(node.time);
    let delta_v = maneuver::to_inertial(&node_state, node.delta_v);
    let direction = delta_v
        .try_normalize()
        .unwrap_or_else(|| node_state.velocity.normalize());
    let burn = Burn::centered(node.time, mass, params, direction, delta_v.length());
    let mut plan = Plan {
        node,
        body: segment.body(),
        node_state,
        burn,
    };
    while too_early(&plan.burn) {
        plan = next_orbit(bodies, segment, &plan).ok_or(no_next_orbit)?;
    }
    Ok(plan)
}

/// Winkel in Flugrichtung von `from` nach `to`, 0 bis 2π.
fn forward_angle(from: &StateVector, to: glam::DVec3) -> f64 {
    let normal = from.position.cross(from.velocity).normalize();
    let angle = normal
        .dot(from.position.cross(to))
        .atan2(from.position.dot(to));
    if angle < 0.0 {
        angle + std::f64::consts::TAU
    } else {
        angle
    }
}

/// Ein Node der Manöverkette: wie gesetzt und was daraus folgt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainNode {
    /// Wie gesetzt. Rückt der Node nur wegen eines früheren Manövers einen
    /// Umlauf weiter, steht das im Plan, nicht hier. So kehrt er zurück, wenn
    /// das frühere Manöver zurückgenommen wird.
    pub node: ManeuverNode,
    /// Der berechnete Plan, oder warum der Node gerade ungültig ist.
    pub plan: Result<Plan, PlanError>,
}

/// Zeitleiste eines Schiffs: der aktuelle Abschnitt und die vorausberechneten,
/// dazu der geplante Start (am Boden) und die Manöverkette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    segments: VecDeque<Segment>,
    /// Geplanter Start vom Boden; steht vor allen Nodes. Verschwindet beim
    /// Brennschluss.
    #[serde(default)]
    launch: Option<LaunchPlan>,
    /// Manöverkette in Ausführungsreihenfolge. Jeder Node liegt auf der Bahn
    /// nach den vorherigen (und nach dem Aufstieg). Die gültigen stehen vorn:
    /// Nach einem ungültigen Node ist die Kette unterbrochen, alle weiteren
    /// sind auch ungültig. Sie bleiben erhalten und werden wieder gültig,
    /// sobald es wieder passt.
    nodes: Vec<ChainNode>,
    /// Nur eine scharfe Kette (mit Start) wird ausgeführt. Eine nicht scharfe
    /// rückt bei Brennbeginn als Ganzes einen Umlauf weiter, beim Abheben eine
    /// Planetendrehung.
    armed: bool,
}

impl Trajectory {
    /// Frei fliegendes Schiff mit Zustand `state` und Masse `mass` zum Zeitpunkt `t`.
    pub fn new_flying(
        bodies: &[Body],
        params: &VesselParams,
        body_id: BodyId,
        t: f64,
        state: StateVector,
        mass: f64,
    ) -> Self {
        let first = coast(bodies, params, body_id, t, state, mass, None);
        let mut trajectory = Self {
            segments: VecDeque::from([first]),
            launch: None,
            nodes: Vec::new(),
            armed: false,
        };
        trajectory.extend(bodies, params);
        trajectory
    }

    /// Schiff am Boden.
    pub fn new_landed(body_id: BodyId, t: f64, point: SurfacePoint, mass: f64) -> Self {
        Self {
            segments: VecDeque::from([Segment::Surface {
                body: body_id,
                start: t,
                point,
                mass,
                wreck: false,
                liftoff: None,
            }]),
            launch: None,
            nodes: Vec::new(),
            armed: false,
        }
    }

    /// Alle geplanten Brennvorgänge in Reihenfolge: Aufstieg, dann die
    /// gültigen Nodes.
    pub fn burns(&self) -> Vec<Burn> {
        self.launch
            .iter()
            .map(|l| l.burn)
            .chain(self.plans().map(|p| p.burn))
            .collect()
    }

    fn extend(&mut self, bodies: &[Body], params: &VesselParams) {
        let burns = self.burns();
        extend(&mut self.segments, bodies, params, &burns);
    }

    /// Zeitleiste ab dem Anfang des aktuellen Abschnitts mit den Brennvorgängen
    /// `burns`, danach frei. `None` während eines Brennens.
    fn compute(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        burns: &[Burn],
    ) -> Option<VecDeque<Segment>> {
        let first = restart(&self.segments[0], bodies, params, burns)?;
        let mut segments = VecDeque::from([first]);
        extend(&mut segments, bodies, params, burns);
        Some(segments)
    }

    /// Berechnet die Kette und die Zeitleiste ab dem Anfang des aktuellen
    /// Abschnitts neu: nach dem Aufstieg die Nodes der Reihe nach, jeder auf
    /// der Bahn nach den vorherigen Brennvorgängen. Der aktuelle Abschnitt
    /// behält seinen Anfangszustand. Von `now` hängt nur die Regel "Brennen
    /// nicht vor jetzt" ab.
    fn rebuild(&mut self, bodies: &[Body], params: &VesselParams, now: f64) {
        let mut burns: Vec<Burn> = self.launch.iter().map(|l| l.burn).collect();
        let mut broken = false;
        for k in 0..self.nodes.len() {
            let result = if broken {
                Err(PlanError::AfterInvalid)
            } else {
                let Some(leg) = self.compute(bodies, params, &burns) else {
                    return;
                };
                locate(bodies, params, &leg, now, burns.last(), self.nodes[k].node)
            };
            match result {
                Ok(plan) => burns.push(plan.burn),
                Err(_) => broken = true,
            }
            self.nodes[k].plan = result;
        }
        if let Some(segments) = self.compute(bodies, params, &burns) {
            self.segments = segments;
        }
    }

    /// Dieselbe Kette um `dt` verschoben: Start und alle Nodes (wie gesetzt).
    /// Der Aufstieg wird neu berechnet; er liegt nur woanders über dem
    /// drehenden Planeten.
    fn shifted(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        dt: f64,
    ) -> Result<Trajectory, PlanError> {
        let mut changed = self.clone();
        if let Some(plan) = self.launch {
            let launch = Launch {
                time: plan.launch.time + dt,
                ..plan.launch
            };
            changed.launch = Some(self.plan_launch(bodies, params, now, launch)?);
        }
        for n in &mut changed.nodes {
            n.node.time += dt;
        }
        changed.rebuild(bodies, params, now);
        Ok(changed)
    }

    /// Rückt bis zum Zeitpunkt `t` vor: Vergangene Abschnitte fallen weg,
    /// neue werden bei Bedarf berechnet. Ein ausgeführter Node (oder Start)
    /// verschwindet. Eine nicht scharfe Kette rückt beim ersten Brennbeginn
    /// als Ganzes einen Umlauf weiter oder verfällt, wenn es keinen nächsten
    /// Umlauf gibt; beim Abheben rückt sie um eine Planetendrehung weiter.
    pub fn advance_to(&mut self, bodies: &[Body], t: f64, params: &VesselParams) {
        loop {
            self.extend(bodies, params);
            if self.segments.len() < 2 || self.segments[0].end() > t {
                break;
            }
            let at = self.segments[0].end();
            let first = self.plans().next().copied();
            match self.segments[0].end_event() {
                Some(Event::Liftoff) if !self.armed => {
                    let day = bodies[self.segments[0].body()].rotation_period;
                    match self.shifted(bodies, params, at, day) {
                        Ok(changed) => *self = changed,
                        Err(_) => {
                            self.launch = None;
                            self.nodes.clear();
                            self.rebuild(bodies, params, at);
                        }
                    }
                    continue;
                }
                Some(Event::BurnStart) if !self.armed && self.launch.is_none() => {
                    if let Some(first) = first {
                        match next_orbit(bodies, &self.segments[0], &first) {
                            Some(later) => {
                                let period = later.node.time - first.node.time;
                                for n in &mut self.nodes {
                                    let time = n.plan.map_or(n.node.time, |p| p.node.time);
                                    n.node.time = time + period;
                                }
                            }
                            None => self.nodes.clear(),
                        }
                        self.rebuild(bodies, params, at);
                        continue;
                    }
                }
                _ => {}
            }
            let done = self
                .segments
                .pop_front()
                .expect("mindestens zwei Abschnitte");
            if done.is_burn() && done.end_event() != Some(Event::Continue) {
                // Ausgeführt: zuerst der Aufstieg, dann die Nodes der Reihe nach.
                if self.launch.is_some() {
                    self.launch = None;
                } else if !self.nodes.is_empty() {
                    self.nodes.remove(0);
                }
            }
        }
        // Ein Wrack plant nichts mehr.
        if matches!(self.segments[0], Segment::Surface { wreck: true, .. }) {
            self.launch = None;
            self.nodes.clear();
        }
    }

    /// Lässt sich die Kette gerade ändern? Am Boden nur mit geplantem Start.
    fn check_editable(&self, params: &VesselParams) -> Result<(), PlanError> {
        match self.current() {
            Segment::Surface { .. } if self.launch.is_none() => Err(PlanError::NotFlying),
            segment if segment.is_burn() => Err(PlanError::Burning),
            _ if params.thrust <= 0.0 || params.exhaust_velocity <= 0.0 => Err(PlanError::NoEngine),
            _ => Ok(()),
        }
    }

    /// Übernimmt `changed`, wenn der Node `index` darin gültig ist. Spätere
    /// Nodes dürfen dabei ungültig werden, sie bleiben erhalten.
    fn accept(&mut self, changed: Trajectory, index: usize) -> Result<(), PlanError> {
        changed.nodes[index].plan?;
        *self = changed;
        Ok(())
    }

    /// Fügt einen Node in die Kette ein, und zwar dort, wo er zeitlich
    /// zwischen den gültigen Nodes liegt. Liefert seine Stelle in der Kette.
    /// Ist er ungültig, bleibt alles unverändert. Eine neue Kette ist nicht
    /// scharf.
    ///
    /// Das Brennen muss ganz in der Zukunft und nach dem vorherigen liegen.
    /// Sonst rückt der Node einen Umlauf weiter (an dieselbe Stelle der Bahn).
    pub fn add_node(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        node: ManeuverNode,
    ) -> Result<usize, PlanError> {
        self.check_editable(params)?;
        let index = self.plans().filter(|p| p.node.time < node.time).count();
        let mut changed = self.clone();
        if changed.nodes.is_empty() && changed.launch.is_none() {
            changed.armed = false;
        }
        changed.nodes.insert(
            index,
            ChainNode {
                node,
                plan: Err(PlanError::NoNode),
            },
        );
        changed.rebuild(bodies, params, now);
        self.accept(changed, index)?;
        Ok(index)
    }

    /// Ändert den Node `index` (Zeitpunkt und Δv). Spätere Nodes wandern mit.
    /// Ist er danach ungültig, bleibt alles unverändert.
    pub fn set_node(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        index: usize,
        node: ManeuverNode,
    ) -> Result<(), PlanError> {
        self.check_editable(params)?;
        if index >= self.nodes.len() {
            return Err(PlanError::NoNode);
        }
        let mut changed = self.clone();
        changed.nodes[index].node = node;
        changed.rebuild(bodies, params, now);
        self.accept(changed, index)
    }

    /// Löscht den Node `index`. Während des Brennens nicht möglich.
    pub fn remove_node(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        index: usize,
    ) -> Result<(), PlanError> {
        if self.current().is_burn() {
            return Err(PlanError::Burning);
        }
        if index >= self.nodes.len() {
            return Err(PlanError::NoNode);
        }
        self.nodes.remove(index);
        self.rebuild(bodies, params, now);
        Ok(())
    }

    /// Berechnet den Aufstieg für `launch` vom aktuellen Abschnitt aus (muss
    /// am Boden sein).
    fn plan_launch(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        launch: Launch,
    ) -> Result<LaunchPlan, PlanError> {
        let Segment::Surface {
            body,
            point,
            mass,
            wreck: false,
            ..
        } = *self.current()
        else {
            return Err(PlanError::NotLanded);
        };
        if launch.time <= now {
            return Err(PlanError::InPast);
        }
        let burn = launch::plan(bodies, params, body, point, mass, &launch)?;
        Ok(LaunchPlan { launch, burn })
    }

    /// Plant oder ändert den Start (Zeitpunkt und Profil). Die Nodes danach
    /// bleiben an ihren Zeitpunkten und wandern mit der neuen Bahn. Ein neuer
    /// Start ohne Nodes ist nicht scharf.
    pub fn set_launch(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        launch: Launch,
    ) -> Result<(), PlanError> {
        let plan = self.plan_launch(bodies, params, now, launch)?;
        if self.launch.is_none() && self.nodes.is_empty() {
            self.armed = false;
        }
        self.launch = Some(plan);
        self.rebuild(bodies, params, now);
        Ok(())
    }

    /// Verschiebt den Start um `dt`; die ganze Kette wandert mit.
    pub fn shift_launch(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        dt: f64,
    ) -> Result<(), PlanError> {
        if self.launch.is_none() {
            return Err(PlanError::NoNode);
        }
        *self = self.shifted(bodies, params, now, dt)?;
        Ok(())
    }

    /// Sagt den Start ab. Nodes danach bleiben (dann ungültig) erhalten.
    pub fn clear_launch(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
    ) -> Result<(), PlanError> {
        if !matches!(self.current(), Segment::Surface { .. }) {
            return Err(PlanError::NotLanded);
        }
        if self.launch.take().is_none() {
            return Err(PlanError::NoNode);
        }
        self.rebuild(bodies, params, now);
        Ok(())
    }

    /// Schaltet die Kette scharf (oder wieder ab). Während des Brennens nicht möglich.
    pub fn set_armed(&mut self, armed: bool) -> Result<(), PlanError> {
        if self.current().is_burn() {
            return Err(PlanError::Burning);
        }
        if self.nodes.is_empty() && self.launch.is_none() {
            return Err(PlanError::NoNode);
        }
        self.armed = armed;
        Ok(())
    }

    /// Der geplante Start, bis zum Brennschluss.
    pub fn launch(&self) -> Option<&LaunchPlan> {
        self.launch.as_ref()
    }

    /// Dieselbe Zeitleiste nur mit den ersten `count` Nodes der Kette. Auf
    /// ihr liegt der Node `count`; man verschiebt ihn entlang dieser Bahn.
    pub fn with_first_nodes(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        count: usize,
    ) -> Trajectory {
        let mut trajectory = self.clone();
        if count < trajectory.nodes.len() {
            trajectory.nodes.truncate(count);
            trajectory.rebuild(bodies, params, now);
        }
        trajectory
    }

    /// Die Manöverkette, gültige und ungültige Nodes.
    pub fn nodes(&self) -> &[ChainNode] {
        &self.nodes
    }

    /// Die Pläne der gültigen Nodes, in Ausführungsreihenfolge.
    pub fn plans(&self) -> impl Iterator<Item = &Plan> {
        self.nodes.iter().map_while(|n| n.plan.as_ref().ok())
    }

    pub fn armed(&self) -> bool {
        self.armed
    }

    /// Der Abschnitt, in dem sich das Schiff gerade befindet.
    pub fn current(&self) -> &Segment {
        &self.segments[0]
    }

    /// Aktueller und alle vorausberechneten Abschnitte.
    pub fn segments(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter()
    }

    /// Erster Abschnitt nach dem Brennen `burn`.
    pub fn segment_after(&self, burn: &Burn) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|s| s.start() >= burn.end && !s.is_burn())
    }

    /// Volle Umläufe um den Körper zwischen `from` und `to`, gezählt am
    /// überstrichenen Winkel (gilt auch für zerfallende Bahnen).
    pub fn revolutions(&self, bodies: &[Body], from: f64, to: f64) -> u32 {
        let mut angle = 0.0;
        for segment in &self.segments {
            let (a, b) = (segment.start().max(from), segment.end().min(to));
            if a >= b {
                continue;
            }
            let at = |t| segment.state_at(bodies, t);
            angle += match segment {
                Segment::Kepler { state, .. } => {
                    let gm = bodies[segment.body()].gm;
                    let whole = kepler::period(gm, state).map_or(0.0, |p| ((b - a) / p).floor());
                    whole * std::f64::consts::TAU + forward_angle(&at(a), at(b).position)
                }
                Segment::Integrated { samples, .. } => {
                    // Stützpunkte liegen 1 s auseinander: Winkel aufsummieren.
                    let inner = samples.iter().filter(|s| a < s.time && s.time < b);
                    let states: Vec<StateVector> = std::iter::once(at(a))
                        .chain(inner.map(|s| s.state))
                        .chain(std::iter::once(at(b)))
                        .collect();
                    states
                        .windows(2)
                        .map(|w| forward_angle(&w[0], w[1].position))
                        .sum()
                }
                Segment::Surface { .. } => 0.0,
            };
        }
        (angle / std::f64::consts::TAU + 1e-9).floor() as u32
    }

    /// Vorausberechneter Absturz: Zeitpunkt, Körper und Ort.
    pub fn predicted_impact(&self) -> Option<(f64, BodyId, SurfacePoint)> {
        self.segments.iter().find_map(|s| match s {
            Segment::Surface {
                start,
                body,
                point,
                wreck: true,
                ..
            } => Some((*start, *body, *point)),
            _ => None,
        })
    }

    /// Körper und Zustand zum Zeitpunkt `t` (muss im aktuellen Abschnitt liegen).
    pub fn state_at(&self, bodies: &[Body], t: f64) -> (BodyId, StateVector) {
        let segment = self.current();
        (segment.body(), segment.state_at(bodies, t))
    }
}
