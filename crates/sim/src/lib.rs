//! Simulation von Kepler Industries.
//!
//! Bewusst ohne Bevy: schnell zu kompilieren, vollständig testbar, und der
//! Zustand (`World`) ist zugleich der Spielstand. Die Regeln dahinter stehen in
//! `docs/ARCHITEKTUR.md`.
//!
//! Einheiten: SI (m, s, kg), alles in `f64`.

pub mod body;
pub mod integrator;
pub mod kepler;
pub mod launch;
pub mod maneuver;
pub mod scenario;
pub mod trajectory;
pub mod world;

pub use body::{Atmosphere, Body, BodyId, SurfacePoint};
pub use kepler::StateVector;
pub use launch::{Launch, LaunchPlan, LaunchProfile};
pub use maneuver::{Burn, ManeuverNode, Plan, PlanError};
pub use trajectory::{ChainNode, Event, Segment, Trajectory};
pub use world::{Base, Vessel, VesselParams, VesselStatus, World};
