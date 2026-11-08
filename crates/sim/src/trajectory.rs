//! Die Zeitleiste eines Schiffs: eine Folge von Abschnitten.
//!
//! Jeder Abschnitt beschreibt lückenlos, wo das Schiff in einem Zeitraum ist:
//! - `Kepler`: Freiflug ohne Luft, analytisch.
//! - `Integrated`: vorausberechnet, wenn Luft (später auch Schub) im Spiel ist.
//! - `Surface`: am Boden, fest mit dem Körper verbunden.
//!
//! "Wo ist das Schiff zum Zeitpunkt t?" heißt immer: passenden Abschnitt suchen.
//! Abschnitte werden erst berechnet, wenn sie gebraucht werden, und hängen nur
//! von ihrem Anfangszustand ab. Deshalb ist das Ergebnis unabhängig davon,
//! wann und in wie großen Zeitschritten die Simulation vorangetrieben wird.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::integrator::{self, ArcEnd, Sample};
use crate::kepler::{self, StateVector};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Womit ein Keplerabschnitt endet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeplerEnd {
    /// Eintritt in die Atmosphäre.
    AtmosphereEntry,
    /// Aufschlag auf einem Körper ohne Luft.
    GroundContact,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Kepler {
        body: BodyId,
        start: f64,
        /// Zustand zu Beginn (die "Epoche"); wird nie verändert.
        state: StateVector,
        /// Zeitpunkt und Art des Endes; `None`, wenn die Bahn weder Luft noch
        /// Boden berührt.
        end: Option<(f64, KeplerEnd)>,
    },
    Integrated {
        body: BodyId,
        /// Stützpunkte; der erste ist der Anfang, der letzte das Ende.
        samples: Vec<Sample>,
        end_event: ArcEnd,
    },
    Surface {
        body: BodyId,
        start: f64,
        point: SurfacePoint,
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
    fn next(&self, bodies: &[Body], drag_per_mass: f64) -> Option<Segment> {
        let body_id = self.body();
        let body = &bodies[body_id];
        let end = self.end();
        if end.is_infinite() {
            return None;
        }
        let end_state = self.state_at(bodies, end);
        let wreck = || {
            let (point, _) = body.inertial_to_surface(end_state.position, end);
            Segment::Surface {
                body: body_id,
                start: end,
                point,
                wreck: true,
            }
        };
        Some(match self {
            Segment::Kepler { end: None, .. } => return None,
            Segment::Kepler {
                end: Some((_, event)),
                ..
            } => match event {
                KeplerEnd::AtmosphereEntry => {
                    integrated(bodies, body_id, end, end_state, drag_per_mass)
                }
                KeplerEnd::GroundContact => wreck(),
            },
            Segment::Integrated { end_event, .. } => match end_event {
                ArcEnd::AtmosphereExit => kepler_segment(bodies, body_id, end, end_state),
                ArcEnd::Continue => integrated(bodies, body_id, end, end_state, drag_per_mass),
                ArcEnd::GroundContact => wreck(),
            },
            Segment::Surface { .. } => return None,
        })
    }
}

/// Keplerabschnitt ab `t` mit Zustand `state`, inklusive seines Endereignisses.
fn kepler_segment(bodies: &[Body], body_id: BodyId, t: f64, state: StateVector) -> Segment {
    let body = &bodies[body_id];
    let event = if body.atmosphere.exists() {
        KeplerEnd::AtmosphereEntry
    } else {
        KeplerEnd::GroundContact
    };
    let end = kepler::time_to_radius_inbound(body.gm, &state, body.boundary_radius())
        .map(|dt| (t + dt, event));
    Segment::Kepler {
        body: body_id,
        start: t,
        state,
        end,
    }
}

fn integrated(
    bodies: &[Body],
    body_id: BodyId,
    t: f64,
    state: StateVector,
    drag_per_mass: f64,
) -> Segment {
    let (samples, end_event) =
        integrator::integrate_arc(&bodies[body_id], drag_per_mass, Sample { time: t, state });
    Segment::Integrated {
        body: body_id,
        samples,
        end_event,
    }
}

/// Zeitleiste eines Schiffs. Enthält immer den aktuellen Abschnitt und, falls
/// der endet, auch schon den nächsten (für die Vorschau).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    segments: VecDeque<Segment>,
}

impl Trajectory {
    /// Frei fliegendes Schiff mit Zustand `state` zum Zeitpunkt `t`.
    /// Ist dort Luft, wird sofort schrittweise gerechnet, sonst Kepler.
    pub fn new_flying(
        bodies: &[Body],
        body_id: BodyId,
        t: f64,
        state: StateVector,
        drag_per_mass: f64,
    ) -> Self {
        let body = &bodies[body_id];
        let altitude = state.position.length() - body.radius;
        let first = if body.atmosphere.density(altitude) > 0.0 {
            integrated(bodies, body_id, t, state, drag_per_mass)
        } else {
            kepler_segment(bodies, body_id, t, state)
        };
        let mut trajectory = Self {
            segments: VecDeque::from([first]),
        };
        trajectory.ensure_lookahead(bodies, drag_per_mass);
        trajectory
    }

    /// Schiff am Boden.
    pub fn new_landed(body_id: BodyId, t: f64, point: SurfacePoint) -> Self {
        Self {
            segments: VecDeque::from([Segment::Surface {
                body: body_id,
                start: t,
                point,
                wreck: false,
            }]),
        }
    }

    /// Sorgt dafür, dass hinter dem aktuellen Abschnitt der nächste bekannt ist.
    fn ensure_lookahead(&mut self, bodies: &[Body], drag_per_mass: f64) {
        if self.segments.len() < 2
            && let Some(next) = self
                .segments
                .back()
                .and_then(|s| s.next(bodies, drag_per_mass))
        {
            self.segments.push_back(next);
        }
    }

    /// Rückt bis zum Zeitpunkt `t` vor: Vergangene Abschnitte fallen weg,
    /// neue werden bei Bedarf berechnet.
    pub fn advance_to(&mut self, bodies: &[Body], t: f64, drag_per_mass: f64) {
        loop {
            self.ensure_lookahead(bodies, drag_per_mass);
            if self.segments.len() > 1 && self.segments[0].end() <= t {
                self.segments.pop_front();
            } else {
                break;
            }
        }
    }

    /// Der Abschnitt, in dem sich das Schiff gerade befindet.
    pub fn current(&self) -> &Segment {
        &self.segments[0]
    }

    /// Aktueller und (falls bekannt) folgender Abschnitt.
    pub fn segments(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter()
    }

    /// Körper und Zustand zum Zeitpunkt `t` (muss im aktuellen Abschnitt liegen).
    pub fn state_at(&self, bodies: &[Body], t: f64) -> (BodyId, StateVector) {
        let segment = self.current();
        (segment.body(), segment.state_at(bodies, t))
    }
}
