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
//! Ein gesetzter Manöver-Node schneidet die Zeitleiste am Brennbeginn ab; dort
//! folgt ein `Integrated`-Abschnitt mit Schub. Die Vorschau ist einfach die
//! vorausberechnete Zeitleiste.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::integrator::{self, Forces, Sample};
use crate::kepler::{self, StateVector};
use crate::maneuver::{self, Burn, ManeuverNode, Plan, PlanError};
use crate::world::VesselParams;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Wie viele Abschnitte (inklusive des aktuellen) im Voraus bekannt sind.
/// Reicht, um nach einem Manöver die neue Bahn oder den Aufschlag zu sehen.
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
            Segment::Surface { .. } => f64::INFINITY,
        }
    }

    pub fn end_event(&self) -> Option<Event> {
        match self {
            Segment::Kepler { end, .. } => end.map(|(_, e)| e),
            Segment::Integrated { end_event, .. } => Some(*end_event),
            Segment::Surface { .. } => None,
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
            Event::BurnStart => {
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
    }
}

/// Zeitleiste eines Schiffs: der aktuelle Abschnitt und bis zu `LOOKAHEAD - 1`
/// folgende, dazu der geplante Node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    segments: VecDeque<Segment>,
    plan: Option<Plan>,
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
            plan: None,
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
            }]),
            plan: None,
        }
    }

    /// Das geplante Brennmanöver, solange es noch nicht begonnen hat.
    fn pending_burn(&self) -> Option<Burn> {
        if self.segments.iter().any(Segment::is_burn) {
            None
        } else {
            self.plan.map(|p| p.burn)
        }
    }

    /// Berechnet Abschnitte voraus, bis `LOOKAHEAD` erreicht ist oder die
    /// Zeitleiste offen endet (stabile Bahn oder am Boden).
    fn extend(&mut self, bodies: &[Body], params: &VesselParams) {
        while self.segments.len() < LOOKAHEAD {
            let pending = self.pending_burn();
            let next = self
                .segments
                .back()
                .and_then(|s| s.next(bodies, params, pending.as_ref()));
            match next {
                Some(segment) => self.segments.push_back(segment),
                None => break,
            }
        }
    }

    /// Berechnet alles ab dem Beginn des aktuellen Abschnitts neu (nach einer
    /// Änderung am Node). Der aktuelle Abschnitt behält seinen Anfangszustand,
    /// das Ergebnis hängt also nicht davon ab, wann neu berechnet wird.
    fn rebuild(&mut self, bodies: &[Body], params: &VesselParams) {
        let pending = self.plan.map(|p| p.burn);
        let first = match &self.segments[0] {
            Segment::Kepler {
                body,
                start,
                state,
                mass,
                ..
            } => kepler_segment(bodies, *body, *start, *state, *mass, pending.as_ref()),
            Segment::Integrated {
                body,
                samples,
                mass,
                burn: None,
                ..
            } => integrated(
                bodies,
                params,
                *body,
                samples[0].time,
                samples[0].state,
                *mass,
                None,
                pending.as_ref(),
            ),
            _ => return,
        };
        self.segments.clear();
        self.segments.push_back(first);
        self.extend(bodies, params);
    }

    /// Rückt bis zum Zeitpunkt `t` vor: Vergangene Abschnitte fallen weg,
    /// neue werden bei Bedarf berechnet. Ein ausgeführter Node verschwindet.
    /// Ein nicht scharfer Node rückt bei Brennbeginn einen Umlauf weiter oder
    /// verfällt, wenn es keinen nächsten Umlauf gibt.
    pub fn advance_to(&mut self, bodies: &[Body], t: f64, params: &VesselParams) {
        loop {
            self.extend(bodies, params);
            if self.segments.len() > 1 && self.segments[0].end() <= t {
                if self.segments[0].end_event() == Some(Event::BurnStart)
                    && let Some(plan) = self.plan.filter(|p| !p.armed)
                {
                    self.plan = next_orbit(bodies, &self.segments[0], &plan);
                    self.rebuild(bodies, params);
                    continue;
                }
                let done = self
                    .segments
                    .pop_front()
                    .expect("mindestens zwei Abschnitte");
                if done.is_burn() && done.end_event() != Some(Event::Continue) {
                    self.plan = None;
                }
            } else {
                break;
            }
        }
        if matches!(self.segments[0], Segment::Surface { .. }) {
            self.plan = None;
        }
    }

    /// Setzt (oder ändert) den Manöver-Node und berechnet die Zeitleiste neu.
    /// Ein neuer Node ist nicht scharf, ein geänderter behält seinen Zustand.
    ///
    /// Das Brennen muss ganz in der Zukunft liegen. Läge sein Beginn in der
    /// Vergangenheit, rückt der Node einen Umlauf weiter (an dieselbe Stelle
    /// der Bahn). Gibt es keinen nächsten Umlauf ohne Luft oder Boden dazwischen,
    /// wird er abgelehnt. Bei einem Fehler bleibt alles unverändert.
    pub fn set_node(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        node: ManeuverNode,
    ) -> Result<(), PlanError> {
        match self.current() {
            Segment::Surface { .. } => return Err(PlanError::NotFlying),
            segment if segment.is_burn() => return Err(PlanError::Burning),
            _ => {}
        }
        if params.thrust <= 0.0 || params.exhaust_velocity <= 0.0 {
            return Err(PlanError::NoEngine);
        }
        if node.time <= now {
            return Err(PlanError::InPast);
        }

        // Bahn ohne Manöver: Dort liegt der Node, sie legt seine Richtungen fest.
        let mut free = self.clone();
        free.plan = None;
        free.rebuild(bodies, params);
        let segment = free
            .segments
            .iter()
            .find(|s| s.start() <= node.time && node.time < s.end())
            .ok_or(PlanError::BeyondPrediction)?;
        if matches!(segment, Segment::Surface { .. }) {
            return Err(PlanError::BeyondPrediction);
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
            armed: self.plan.is_some_and(|p| p.armed),
        };
        while plan.burn.start < now {
            plan = next_orbit(bodies, segment, &plan).ok_or(PlanError::TooLate)?;
        }
        self.plan = Some(plan);
        self.rebuild(bodies, params);
        Ok(())
    }

    /// Schaltet den Node scharf (oder wieder ab). Während des Brennens nicht möglich.
    pub fn set_armed(&mut self, armed: bool) -> Result<(), PlanError> {
        if self.current().is_burn() {
            return Err(PlanError::Burning);
        }
        let plan = self.plan.as_mut().ok_or(PlanError::NoNode)?;
        plan.armed = armed;
        Ok(())
    }

    /// Löscht den Node. Während des Brennens nicht möglich.
    pub fn clear_node(&mut self, bodies: &[Body], params: &VesselParams) -> Result<(), PlanError> {
        if self.current().is_burn() {
            return Err(PlanError::Burning);
        }
        if self.plan.take().is_some() {
            self.rebuild(bodies, params);
        }
        Ok(())
    }

    pub fn plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    /// Der Abschnitt, in dem sich das Schiff gerade befindet.
    pub fn current(&self) -> &Segment {
        &self.segments[0]
    }

    /// Aktueller und alle vorausberechneten Abschnitte.
    pub fn segments(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter()
    }

    /// Erster Abschnitt nach dem geplanten Brennmanöver.
    pub fn segment_after_burn(&self) -> Option<&Segment> {
        let i = self
            .segments
            .iter()
            .position(|s| s.end_event() == Some(Event::BurnEnd))?;
        self.segments.get(i + 1)
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
