//! Die Zeitleiste eines Schiffs: eine Folge von Abschnitten.
//!
//! Jeder Abschnitt beschreibt lückenlos, wo das Schiff in einem Zeitraum ist:
//! - `Kepler`: Freiflug ohne Luft und ohne Schub, analytisch.
//! - `Integrated`: vorausberechnet, wenn Luft oder Schub im Spiel ist.
//! - `Surface`: am Boden, fest mit dem Körper verbunden.
//! - `Approach`: Anflug zum Andocken, gerade Linie im mitbewegten System des
//!   Ziels (siehe `docking`); danach endet die Zeitleiste im Verbund.
//!
//! "Wo ist das Schiff zum Zeitpunkt t?" heißt immer: passenden Abschnitt suchen.
//! Ein Abschnitt hängt nur von seinem Anfangszustand und dem geplanten Manöver
//! ab. Deshalb ist das Ergebnis unabhängig davon, wann und in wie großen
//! Zeitschritten die Simulation vorangetrieben wird.
//!
//! Geplante Manöver bilden eine Kette. Jeder Node schneidet die Zeitleiste an
//! seinem Brennbeginn ab; dort folgt ein `Integrated`-Abschnitt mit Schub. Die
//! Vorschau ist einfach die vorausberechnete Zeitleiste.
//!
//! An der Grenze einer Einflusssphäre (siehe `soi`) endet ein Abschnitt; der
//! nächste läuft um den anderen Körper weiter, ein Brennen ebenso. Jeder
//! Abschnitt rechnet relativ zu seinem Körper.
//!
//! Endet ein Freiflug zu schnell am Boden, endet sein Abschnitt stattdessen
//! am Beginn des Landebrennens (siehe `landing`); das ist ein `Integrated`-
//! Abschnitt mit Schub wie jedes Brennen, danach fällt das Schiff die letzten
//! Zentimeter.

use crate::body::{Body, BodyId, SurfacePoint};
use crate::booster::{self, BoosterPlan};
use crate::docking::{self, DockError, DockPlan, DockRequest, Docking};
use crate::integrator::{self, Forces, Sample};
use crate::kepler::{self, StateVector};
use crate::landing::{self, Crash, Outcome};
use crate::launch::{self, Launch, LaunchPlan};
use crate::log::LogKind;
use crate::maneuver::{self, Burn, ManeuverNode, Plan, PlanError};
use crate::soi;
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
    /// Bodenberührung: aufgesetzt oder Wrack.
    GroundContact,
    /// Beginn des automatischen Landebrennens (siehe `landing`).
    LandingStart,
    /// Beginn des geplanten Brennmanövers.
    BurnStart,
    /// Ende des Brennmanövers.
    BurnEnd,
    /// Maximale Stücklänge erreicht; gleiche Kräfte im nächsten Stück.
    Continue,
    /// Abheben vom Boden: Beginn des Aufstiegs.
    Liftoff,
    /// Kontakt beim Andocken: Das Schiff wird Teil eines Verbunds.
    Docking,
    /// Eintritt in die Einflusssphäre eines Körpers (Mond oder Planet):
    /// weiter um ihn.
    SoiEntry(BodyId),
    /// Verlassen der Einflusssphäre: weiter um den Mutterkörper.
    SoiExit,
}

impl Event {
    /// Derselbe Flug geht im nächsten Abschnitt weiter, ein Brennen ebenso:
    /// nur ein neues Stück oder ein anderer Körper.
    pub fn continues(self) -> bool {
        matches!(self, Event::Continue | Event::SoiEntry(_) | Event::SoiExit)
    }

    /// Die Bahn führt in die Luft oder auf den Boden.
    pub fn is_descent(self) -> bool {
        matches!(
            self,
            Event::AtmosphereEntry | Event::GroundContact | Event::LandingStart
        )
    }
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
        /// Endet der Freiflug mit einem Absturz: warum nicht gebremst wird.
        #[serde(default)]
        crash: Option<Crash>,
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
        /// Endet der Freiflug mit einem Absturz: warum nicht gebremst wird.
        #[serde(default)]
        crash: Option<Crash>,
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
    /// Anflug zum Andocken (siehe `docking`).
    Approach {
        body: BodyId,
        start: f64,
        /// Bahn des Ziels: sein Zustand zu Beginn (eine Kopie, damit der
        /// Abschnitt von keinem anderen Schiff abhängt).
        target: StateVector,
        /// Lage relativ zum Ziel zu Beginn, im mitbewegten System des Ziels.
        offset: glam::DVec3,
        mass: f64,
        /// Masse beim Kontakt.
        end_mass: f64,
    },
}

impl Segment {
    pub fn body(&self) -> BodyId {
        match self {
            Segment::Kepler { body, .. }
            | Segment::Integrated { body, .. }
            | Segment::Surface { body, .. }
            | Segment::Approach { body, .. } => *body,
        }
    }

    pub fn start(&self) -> f64 {
        match self {
            Segment::Kepler { start, .. }
            | Segment::Surface { start, .. }
            | Segment::Approach { start, .. } => *start,
            Segment::Integrated { samples, .. } => samples[0].time,
        }
    }

    /// Endzeitpunkt, `f64::INFINITY` für offene Abschnitte.
    pub fn end(&self) -> f64 {
        match self {
            Segment::Kepler { end, .. } => end.map_or(f64::INFINITY, |(t, _)| t),
            Segment::Integrated { samples, .. } => samples[samples.len() - 1].time,
            Segment::Surface { liftoff, .. } => liftoff.unwrap_or(f64::INFINITY),
            Segment::Approach { start, offset, .. } => {
                start + docking::Profile::new(offset.length()).duration()
            }
        }
    }

    pub fn end_event(&self) -> Option<Event> {
        match self {
            Segment::Kepler { end, .. } => end.map(|(_, e)| e),
            Segment::Integrated { end_event, .. } => Some(*end_event),
            Segment::Surface { liftoff, .. } => liftoff.map(|_| Event::Liftoff),
            Segment::Approach { .. } => Some(Event::Docking),
        }
    }

    /// Läuft in diesem Abschnitt das Triebwerk?
    pub fn is_burn(&self) -> bool {
        matches!(self, Segment::Integrated { burn: Some(_), .. })
    }

    /// Das Landebrennen, falls dieser Abschnitt eins ist.
    pub fn landing_burn(&self) -> Option<&Burn> {
        match self {
            Segment::Integrated {
                burn: Some(burn), ..
            } if burn.is_landing() => Some(burn),
            _ => None,
        }
    }

    /// Endet der Abschnitt mit einem Absturz: warum (ohne Angabe: ein Brennen
    /// führt in den Boden).
    pub fn crash(&self) -> Option<Crash> {
        match self {
            Segment::Kepler { crash, .. } | Segment::Integrated { crash, .. } => *crash,
            _ => None,
        }
    }

    pub fn mass_at(&self, t: f64) -> f64 {
        match self {
            // Auf dem Booster zählt die Masse des Schiffs: Es verbraucht
            // nichts und fliegt nach der Trennung mit ihr weiter.
            Segment::Integrated {
                burn: Some(burn), ..
            } => burn.payload().unwrap_or(burn.mass_at(t)),
            Segment::Approach { mass, end_mass, .. } => {
                // Gleichmäßig über den Anflug verbraucht.
                let (start, end) = (self.start(), self.end());
                let f = ((t - start) / (end - start)).clamp(0.0, 1.0);
                mass + (end_mass - mass) * f
            }
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
                // Erster Stützpunkt mit Zeit >= t, dann zwischen Vorgänger und
                // ihm interpolieren (genau auf ihm: sein Zustand).
                let i = samples.partition_point(|s| s.time < t);
                if i == 0 {
                    samples[0].state
                } else if i >= samples.len() {
                    samples[samples.len() - 1].state
                } else if samples[i].time == t {
                    samples[i].state
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
            Segment::Approach {
                start,
                target,
                offset,
                ..
            } => docking::approach_state(body.gm, *start, target, *offset, t),
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
            Event::AtmosphereExit => kepler_segment(bodies, params, body, t, state, mass, pending),
            Event::GroundContact => touchdown(bodies, body, t, state, mass),
            Event::LandingStart => landing_burn(bodies, params, body, t, state, mass),
            Event::BurnStart | Event::Liftoff => {
                let burn = *pending?;
                integrated(bodies, params, body, t, state, mass, Some(burn), None)
            }
            Event::BurnEnd => coast(bodies, params, body, t, state, mass, pending),
            Event::Continue => match self {
                Segment::Integrated {
                    burn: Some(burn), ..
                } => integrated(bodies, params, body, t, state, mass, Some(*burn), None),
                Segment::Kepler { .. } => {
                    kepler_segment(bodies, params, body, t, state, mass, pending)
                }
                _ => integrated(bodies, params, body, t, state, mass, None, pending),
            },
            Event::SoiEntry(_) | Event::SoiExit => {
                let to = soi::next_body(bodies, body, event).expect("Wechsel");
                let state = soi::convert(bodies, body, to, t, state);
                match self {
                    Segment::Integrated {
                        burn: Some(burn), ..
                    } => integrated(bodies, params, to, t, state, mass, Some(*burn), None),
                    _ => coast(bodies, params, to, t, state, mass, pending),
                }
            }
            // Danach geht es im Verbund weiter (siehe `World`).
            Event::Docking => return None,
        })
    }
}

/// Beginn des geplanten Brennmanövers, falls es nicht vor `t` liegt.
fn burn_start_after(pending: Option<&Burn>, t: f64) -> Option<f64> {
    pending.map(|b| b.start).filter(|&s| s >= t)
}

/// Keplerabschnitt ab `t`, inklusive seines Endereignisses: Luft oder Boden
/// (davor eventuell das Landebrennen), Verlassen der Einflusssphäre,
/// Brennbeginn; kann die Bahn die Sphäre eines Monds erreichen, außerdem
/// deren Eintritt (gesucht bis zum Ende des Suchfensters, sonst endet der
/// Abschnitt dort mit `Continue`).
fn kepler_segment(
    bodies: &[Body],
    params: &VesselParams,
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
    let exit = soi::exit_time(bodies, body_id, t, &state).map(|t_exit| (t_exit, Event::SoiExit));
    let burn = burn_start_after(pending, t).map(|t_burn| (t_burn, Event::BurnStart));
    // Bei Gleichstand gewinnt das erste: Luft oder Boden vor allem anderen.
    let mut end = [natural, exit, burn]
        .into_iter()
        .flatten()
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if soi::reaches_moon(bodies, body_id, &state) {
        let window = soi::window_end(bodies, body_id, t, &state);
        let until = end.map_or(window, |(t_end, _)| t_end.min(window));
        if let Some((t_entry, moon)) = soi::entry_time(bodies, body_id, t, &state, until) {
            end = Some((t_entry, Event::SoiEntry(moon)));
        } else if end.is_none_or(|(t_end, _)| window < t_end) {
            end = Some((window, Event::Continue));
        }
    }
    let mut crash = None;
    if let Some((contact, Event::GroundContact)) = end {
        let at = |time: f64| kepler::propagate(body.gm, state, time - t);
        match plan_landing(bodies, params, body_id, t, (contact, at(contact)), mass, at) {
            Some(Ok(ignition)) => end = Some((ignition, Event::LandingStart)),
            Some(Err(reason)) => crash = Some(reason),
            None => {}
        }
    }
    Segment::Kepler {
        body: body_id,
        start: t,
        state,
        mass,
        end,
        crash,
    }
}

/// Schrittweise berechneter Abschnitt: mit Schub, wenn `burn` gesetzt ist,
/// sonst Freiflug durch die Luft (endet spätestens am Beginn von `pending`,
/// vor dem Boden eventuell mit dem Landebrennen).
#[allow(clippy::too_many_arguments)]
fn integrated(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
    mut burn: Option<Burn>,
    pending: Option<&Burn>,
) -> Segment {
    // Auf dem Booster zählt die Fläche des Stapels.
    let drag_area = burn
        .and_then(|b| b.ascent)
        .and_then(|a| a.stack)
        .map_or(params.drag_area, |s| s.drag_area);
    let forces = Forces {
        body: &bodies[body_id],
        drag_area,
        parachute_area: params.parachute_area,
        mass,
        burn: burn.as_ref(),
    };
    let stop = match &burn {
        Some(b) => Some((b.end, Event::BurnEnd)),
        None => burn_start_after(pending, t).map(|s| (s, Event::BurnStart)),
    };
    let watch = soi::Watch::new(bodies, body_id);
    let (mut samples, mut end_event) =
        integrator::integrate_arc(&forces, Sample { time: t, state }, stop, &watch);
    let mut crash = None;
    let last = samples[samples.len() - 1];
    match burn {
        // Das Landebrennen endet, sobald das Schiff steht.
        Some(b) if b.is_landing() && end_event == Event::BurnEnd => {
            burn = Some(Burn {
                end: last.time,
                ..b
            });
        }
        None if end_event == Event::GroundContact => {
            let contact = (last.time, last.state);
            let at = |time: f64| integrator::state_on_arc(&forces, &samples, time);
            match plan_landing(bodies, params, body_id, t, contact, mass, at) {
                Some(Ok(ignition)) => {
                    let state = at(ignition);
                    samples.retain(|s| s.time < ignition);
                    samples.push(Sample {
                        time: ignition,
                        state,
                    });
                    end_event = Event::LandingStart;
                }
                Some(Err(reason)) => crash = Some(reason),
                None => {}
            }
        }
        _ => {}
    }
    Segment::Integrated {
        body: body_id,
        samples,
        mass,
        burn,
        end_event,
        crash,
    }
}

/// Landebrennen ab `t` (siehe `landing`): voller Schub gegen die Bewegung
/// zum Boden, bis das Schiff steht.
fn landing_burn(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    t: f64,
    state: StateVector,
    mass: f64,
) -> Segment {
    let burn = Burn::landing(t, mass, params, &bodies[body_id], &state);
    integrated(bodies, params, body_id, t, state, mass, Some(burn), None)
}

/// Ein Freiflug ab `from` trifft auf den Boden (`contact`: Zeitpunkt und
/// Zustand); `at(t)` ist sein Zustand davor (genau wie in der Zeitleiste). Zu
/// schnell dafür: Beginn des Landebrennens, oder warum nicht gebremst wird.
/// `None`: langsam genug.
fn plan_landing(
    bodies: &[Body],
    params: &VesselParams,
    body_id: BodyId,
    from: f64,
    (contact, contact_state): (f64, StateVector),
    mass: f64,
    at: impl Fn(f64) -> StateVector,
) -> Option<Result<f64, Crash>> {
    let body = &bodies[body_id];
    let speed = landing::ground_speed(body, &contact_state);
    if speed <= landing::SAFE_SPEED {
        return None;
    }
    Some(landing::latest_start(
        body,
        params,
        mass,
        from,
        contact,
        speed,
        |t| {
            let Segment::Integrated {
                samples, end_event, ..
            } = landing_burn(bodies, params, body_id, t, at(t), mass)
            else {
                unreachable!("Brennen wird schrittweise berechnet");
            };
            let end = samples[samples.len() - 1].state;
            Outcome::of(body, end_event == Event::GroundContact, &end)
        },
    ))
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
        kepler_segment(bodies, params, body_id, t, state, mass, pending)
    }
}

/// Derselbe Plan einen Umlauf später. `segment` ist der Keplerabschnitt, in
/// dem der Node liegt. Nur möglich, wenn die Bahn geschlossen ist und bis zum
/// Ende des verschobenen Brennens weder Luft noch Boden berührt und keine
/// Einflusssphäre wechselt.
fn next_orbit(bodies: &[Body], segment: &Segment, plan: &Plan) -> Option<Plan> {
    let Segment::Kepler { start, state, .. } = segment else {
        return None;
    };
    let period = kepler::period(bodies[segment.body()].gm, state)?;
    let later = plan.one_orbit_later(period);
    let end = orbit_end(bodies, segment.body(), *start, state, later.burn.end);
    (later.burn.end <= end).then_some(later)
}

/// Wann die Keplerbahn (Zustand `state` bei `t0`) endet, ohne geplante
/// Brennen: Luft oder Boden, Verlassen der Einflusssphäre oder Eintritt in
/// die eines Monds. Gesucht wird in denselben Fenstern wie in der
/// Zeitleiste, aber höchstens bis `until`.
fn orbit_end(bodies: &[Body], body_id: BodyId, t0: f64, state: &StateVector, until: f64) -> f64 {
    let body = &bodies[body_id];
    let mut end = kepler::time_to_radius_inbound(body.gm, state, body.boundary_radius())
        .map_or(f64::INFINITY, |dt| t0 + dt);
    if let Some(exit) = soi::exit_time(bodies, body_id, t0, state) {
        end = end.min(exit);
    }
    if !soi::reaches_moon(bodies, body_id, state) {
        return end;
    }
    let (mut t, mut s) = (t0, *state);
    while t < end.min(until) {
        let window = soi::window_end(bodies, body_id, t, &s);
        if let Some((entry, _)) = soi::entry_time(bodies, body_id, t, &s, window.min(end)) {
            return entry;
        }
        s = kepler::propagate(body.gm, s, window - t);
        t = window;
    }
    end
}

/// Protokolleintrag für das Ende des Abschnitts `done` (auf ihn folgt `next`).
/// Zielgebiete kennt die Zeitleiste nicht, die trägt `World` nach.
fn log_entry(bodies: &[Body], done: &Segment, next: &Segment) -> Option<LogKind> {
    let at = done.end();
    let body = &bodies[done.body()];
    let altitude = |r: f64| r - body.radius;
    Some(match done.end_event()? {
        Event::Liftoff => LogKind::Liftoff,
        Event::BurnStart => LogKind::BurnStart,
        Event::LandingStart => LogKind::LandingBurn,
        // Nach dem Landebrennen folgt gleich das Aufsetzen.
        Event::BurnEnd if done.landing_burn().is_some() => return None,
        Event::BurnEnd => {
            let info = kepler::orbit_info(body.gm, &done.state_at(bodies, at));
            LogKind::BurnEnd {
                apoapsis: info.apoapsis.map(altitude),
                periapsis: altitude(info.periapsis),
            }
        }
        Event::AtmosphereEntry => {
            let info = kepler::orbit_info(body.gm, &done.state_at(bodies, at));
            LogKind::AtmosphereEntry {
                periapsis: altitude(info.periapsis),
            }
        }
        Event::GroundContact => {
            let Segment::Surface { point, wreck, .. } = next else {
                return None;
            };
            if *wreck {
                LogKind::Crashed {
                    point: *point,
                    zone: None,
                    reason: done.crash().unwrap_or(Crash::TooLate),
                    body: done.body(),
                }
            } else {
                LogKind::Landed {
                    point: *point,
                    zone: None,
                    body: done.body(),
                }
            }
        }
        Event::SoiEntry(moon) => {
            let moon_body = &bodies[moon];
            let info = kepler::orbit_info(moon_body.gm, &next.state_at(bodies, at));
            LogKind::SoiEntry {
                body: moon,
                periapsis: info.periapsis - moon_body.radius,
            }
        }
        Event::SoiExit => LogKind::SoiExit { body: done.body() },
        // Andocken trägt `World` ein (sie kennt das Hauptschiff).
        Event::AtmosphereExit | Event::Continue | Event::Docking => return None,
    })
}

/// Bodenkontakt: aufgesetzt, oder zu schnell und ein Wrack (siehe `landing`).
fn touchdown(bodies: &[Body], body_id: BodyId, t: f64, state: StateVector, mass: f64) -> Segment {
    let body = &bodies[body_id];
    let (point, _) = body.inertial_to_surface(state.position, t);
    Segment::Surface {
        body: body_id,
        start: t,
        point,
        mass,
        wreck: landing::ground_speed(body, &state) > landing::SAFE_SPEED,
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
            params,
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
    /// Andocken als letzter Schritt der Kette (siehe `docking`): Angleichen,
    /// dann Anflug. Fehlt in älteren Spielständen: keins.
    #[serde(default)]
    docking: Option<Docking>,
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
            docking: None,
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
            docking: None,
        }
    }

    /// Alle geplanten Brennvorgänge in Reihenfolge: Aufstieg, die gültigen
    /// Nodes, zuletzt das Angleichen beim Andocken.
    pub fn burns(&self) -> Vec<Burn> {
        self.launch
            .iter()
            .map(|l| l.burn)
            .chain(self.plans().map(|p| p.burn))
            .chain(self.dock_plan().map(|p| p.matching.burn))
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
        // Andocken als letzter Schritt: Angleichen, dann Anflug.
        if let Some(mut docking) = self.docking {
            docking.plan = match docking.request {
                _ if broken => Err(DockError::AfterInvalid),
                Err(e) => Err(e),
                Ok(request) => self.plan_docking(bodies, params, now, &burns, &request),
            };
            if let Ok(plan) = &docking.plan {
                burns.push(plan.matching.burn);
            }
            self.docking = Some(docking);
        }
        if let Some(mut segments) = self.compute(bodies, params, &burns) {
            if let Some(plan) = self.dock_plan() {
                // Nach dem Angleichen kein Freiflug, sondern der Anflug.
                let end = plan.matching.burn.end;
                segments.retain(|s| s.start() < end);
                segments.push_back(Segment::Approach {
                    body: plan.matching.body,
                    start: end,
                    target: plan.target,
                    offset: plan.offset,
                    mass: plan.mass,
                    end_mass: plan.end_mass,
                });
            }
            self.segments = segments;
        }
    }

    /// Angleichen und Anflug für `request` nach den Brennvorgängen `burns`.
    fn plan_docking(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        burns: &[Burn],
        request: &DockRequest,
    ) -> Result<DockPlan, DockError> {
        if params.thrust <= 0.0 || params.exhaust_velocity <= 0.0 {
            return Err(DockError::NoEngine);
        }
        let ve = params.exhaust_velocity;
        let leg = self
            .compute(bodies, params, burns)
            .ok_or(DockError::Overlaps)?;
        let matching = locate(bodies, params, &leg, now, burns.last(), request.node)
            .map_err(|_| DockError::Overlaps)?;
        // Das Angleichen gehört an genau diese Stelle, nicht einen Umlauf später.
        if matching.node.time != request.node.time {
            return Err(DockError::Overlaps);
        }
        if matching.burn.start < request.target_epoch {
            return Err(DockError::TargetNotStable);
        }
        let on_board = ve * (matching.burn.start_mass / params.dry_mass).ln();
        let wanted = request.node.delta_v.length();
        if matching.burn.delta_v() < wanted - 0.05 {
            return Err(DockError::NoFuel {
                needed: wanted,
                available: on_board,
            });
        }

        // Lage nach dem Angleichen relativ zum Ziel.
        let mut all = burns.to_vec();
        all.push(matching.burn);
        let segments = self
            .compute(bodies, params, &all)
            .ok_or(DockError::Overlaps)?;
        let end = matching.burn.end;
        let segment = segments
            .iter()
            .find(|s| s.start() < end && end <= s.end())
            .ok_or(DockError::Overlaps)?;
        let own = segment.state_at(bodies, end);
        let mass = segment.mass_at(end);
        let body = &bodies[matching.body];
        let target = kepler::propagate(body.gm, request.target_state, end - request.target_epoch);
        let (offset, residual) = docking::relative(&target, &own);
        let (cost, peak) = docking::approach_cost(body, end, &target, offset);
        if peak > params.thrust / mass {
            return Err(DockError::TooWeak);
        }
        let approach_delta_v = residual + cost;
        if approach_delta_v > ve * (mass / params.dry_mass).ln() {
            return Err(DockError::NoFuel {
                needed: wanted + approach_delta_v,
                available: on_board,
            });
        }
        Ok(DockPlan {
            matching,
            distance: offset.length(),
            approach_delta_v,
            contact: end + docking::Profile::new(offset.length()).duration(),
            target,
            offset,
            mass,
            end_mass: mass * (-approach_delta_v / ve).exp(),
        })
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
            let booster = plan.booster.map(|b| b.params);
            changed.launch = Some(self.plan_launch(bodies, params, now, launch, booster)?);
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
    pub fn advance_to(
        &mut self,
        bodies: &[Body],
        t: f64,
        params: &VesselParams,
    ) -> Vec<(f64, LogKind)> {
        let mut log = Vec::new();
        loop {
            self.extend(bodies, params);
            if self.segments.len() < 2 || self.segments[0].end() > t {
                break;
            }
            let at = self.segments[0].end();
            let first = self.plans().next().copied();
            match self.segments[0].end_event() {
                Some(Event::Liftoff) if !self.armed => {
                    self.docking = None;
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
                    // Eine verschobene Kette passt nicht mehr zur Annäherung.
                    self.docking = None;
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
            if let Some(kind) = log_entry(bodies, &done, &self.segments[0]) {
                log.push((at, kind));
            }
            if done.end_event() == Some(Event::GroundContact) {
                // Am Boden (gelandet oder Wrack): Die Kette ist erledigt.
                self.launch = None;
                self.nodes.clear();
                self.docking = None;
            } else if done.is_burn()
                && done.landing_burn().is_none()
                && !done.end_event().is_some_and(Event::continues)
            {
                // Ausgeführt: zuerst der Aufstieg, dann die Nodes der Reihe
                // nach. Das Angleichen bleibt stehen, bis angedockt ist. Das
                // Landebrennen gehört nicht zur Kette.
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
            self.docking = None;
        }
        log
    }

    /// Lässt sich die Kette gerade ändern? Am Boden nur mit geplantem Start.
    fn check_editable(&self, params: &VesselParams) -> Result<(), PlanError> {
        match self.current() {
            Segment::Surface { .. } if self.launch.is_none() => Err(PlanError::NotFlying),
            Segment::Approach { .. } => Err(PlanError::Docking),
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
    /// am Boden sein); mit `booster` als Stapel, samt dessen Rückflug (siehe
    /// `booster`).
    fn plan_launch(
        &self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        launch: Launch,
        booster: Option<VesselParams>,
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
        let burn = launch::plan(bodies, params, body, point, mass, &launch, booster.as_ref())?;
        let booster = match booster {
            None => None,
            Some(booster) => {
                // Die Trennung, genau wie sie die Zeitleiste später fliegt.
                let ascent = self
                    .compute(bodies, params, &[burn])
                    .ok_or(PlanError::NotLanded)?;
                let state = ascent
                    .iter()
                    .find(|s| s.is_burn() && s.end_event() == Some(Event::BurnEnd))
                    .map(|s| s.state_at(bodies, burn.end))
                    .ok_or(PlanError::BoosterNoReturn)?;
                let own = burn.end_mass() - mass;
                let back = booster::plan_return(
                    bodies,
                    &booster,
                    body,
                    point,
                    launch.profile.heading,
                    burn.end,
                    state,
                    own,
                )?;
                Some(BoosterPlan {
                    params: booster,
                    mass: own,
                    back,
                })
            }
        };
        Ok(LaunchPlan {
            launch,
            burn,
            booster,
        })
    }

    /// Plant oder ändert den Start (Zeitpunkt und Profil). Die Nodes danach
    /// bleiben an ihren Zeitpunkten und wandern mit der neuen Bahn. Ein neuer
    /// Start ohne Nodes ist nicht scharf. Mit `booster` (dessen Werte) startet
    /// das Schiff auf dem Booster.
    pub fn set_launch(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        launch: Launch,
        booster: Option<VesselParams>,
    ) -> Result<(), PlanError> {
        let plan = self.plan_launch(bodies, params, now, launch, booster)?;
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
        if count < trajectory.nodes.len() || trajectory.docking.is_some() {
            trajectory.nodes.truncate(count);
            trajectory.docking = None;
            trajectory.rebuild(bodies, params, now);
        }
        trajectory
    }

    /// Geplantes Andocken (auch ungültiges).
    pub fn docking(&self) -> Option<&Docking> {
        self.docking.as_ref()
    }

    /// Das berechnete Andocken, wenn es gültig ist.
    pub fn dock_plan(&self) -> Option<&DockPlan> {
        self.docking.as_ref().and_then(|d| d.plan.as_ref().ok())
    }

    /// Setzt das Andocken (von `World` berechnet) oder nimmt es zurück und
    /// berechnet die Zeitleiste neu. Während des Angleichens und des Anflugs
    /// bleibt es, wie es ist.
    pub(crate) fn set_docking(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        now: f64,
        docking: Option<(usize, Result<DockRequest, DockError>)>,
    ) {
        if self.docking_underway() {
            return;
        }
        self.docking = docking.map(|(target, request)| Docking {
            target,
            request,
            plan: Err(DockError::NoApproach),
        });
        self.rebuild(bodies, params, now);
    }

    /// Neubeginn jetzt (`t`) mit Zustand und Masse, z. B. nach dem Abdocken.
    /// Die Kette bleibt und wird mit den (neuen) Werten neu berechnet;
    /// ungültig gewordene Nodes bleiben erhalten.
    pub(crate) fn restart_flying(
        &mut self,
        bodies: &[Body],
        params: &VesselParams,
        body_id: BodyId,
        t: f64,
        state: StateVector,
        mass: f64,
    ) {
        let mut fresh = Trajectory::new_flying(bodies, params, body_id, t, state, mass);
        fresh.nodes = std::mem::take(&mut self.nodes);
        fresh.armed = self.armed;
        fresh.docking = self.docking;
        fresh.rebuild(bodies, params, t);
        *self = fresh;
    }

    /// Angedockt: Die eigene Kette ist erledigt (weiter geht es im Verbund).
    pub(crate) fn finish_docking(&mut self) {
        self.launch = None;
        self.nodes.clear();
        self.docking = None;
    }

    /// Läuft das Andocken schon (Angleichen oder Anflug)?
    pub fn docking_underway(&self) -> bool {
        match self.current() {
            Segment::Approach { .. } => true,
            segment => self
                .dock_plan()
                .is_some_and(|p| segment.is_burn() && segment.start() >= p.matching.burn.start),
        }
    }

    /// Dieselbe Zeitleiste ohne Andocken (für die Suche nach der Annäherung).
    pub fn without_docking(&self, bodies: &[Body], params: &VesselParams, now: f64) -> Trajectory {
        let mut trajectory = self.clone();
        if trajectory.docking.take().is_some() {
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
                Segment::Approach { .. } => forward_angle(&at(a), at(b).position),
            };
        }
        (angle / std::f64::consts::TAU + 1e-9).floor() as u32
    }

    /// Das Landebrennen auf der vorausberechneten Bahn (auch das laufende).
    pub fn landing(&self) -> Option<&Burn> {
        self.segments.iter().find_map(Segment::landing_burn)
    }

    /// Neue Masse für ein Schiff, das ohne geplanten Start am Boden steht
    /// (abgeladen oder getankt, siehe `base`).
    pub fn set_landed_mass(&mut self, mass: f64) {
        debug_assert!(self.launch.is_none() && self.segments.len() == 1);
        if let Some(Segment::Surface { mass: m, .. }) = self.segments.front_mut() {
            *m = mass;
        }
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

    /// Der Abschnitt, der den Zeitpunkt `t` enthält (auch in der Zukunft);
    /// `None` außerhalb der vorausberechneten Zeitleiste.
    pub fn predicted_segment(&self, t: f64) -> Option<&Segment> {
        let k = self.segments.partition_point(|s| s.start() <= t);
        let segment = self.segments.get(k.checked_sub(1)?)?;
        (t <= segment.end()).then_some(segment)
    }

    /// Körper und vorausberechneter Zustand zum Zeitpunkt `t`.
    pub fn predicted_state(&self, bodies: &[Body], t: f64) -> Option<(BodyId, StateVector)> {
        let segment = self.predicted_segment(t)?;
        Some((segment.body(), segment.state_at(bodies, t)))
    }

    /// Körper und Zustand zum Zeitpunkt `t` (muss im aktuellen Abschnitt liegen).
    pub fn state_at(&self, bodies: &[Body], t: f64) -> (BodyId, StateVector) {
        let segment = self.current();
        (segment.body(), segment.state_at(bodies, t))
    }
}
