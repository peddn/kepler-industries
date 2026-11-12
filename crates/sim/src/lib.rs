//! Simulation von Kepler Industries.
//!
//! Bewusst ohne Bevy: schnell zu kompilieren, vollständig testbar, und der
//! Zustand (`World`) ist zugleich der Spielstand. Die Regeln dahinter stehen in
//! `docs/ARCHITEKTUR.md`.
//!
//! Einheiten: SI (m, s, kg), alles in `f64`.

pub mod base;
pub mod body;
pub mod booster;
pub mod docking;
pub mod integrator;
pub mod kepler;
pub mod landing;
pub mod launch;
pub mod log;
pub mod maneuver;
pub mod rendezvous;
pub mod save;
pub mod scenario;
pub mod soi;
pub mod trajectory;
pub mod transfer;
pub mod world;

pub use base::{BaseError, ShipKind};
pub use body::{Atmosphere, Body, BodyId, BodyOrbit, SurfacePoint};
pub use booster::{BoosterPlan, Return};
pub use docking::{DockError, DockPlan, DockRequest, Docking, UndockError, UndockFuel};
pub use kepler::StateVector;
pub use landing::{Crash, LandingZone};
pub use launch::{Launch, LaunchPlan, LaunchProfile};
pub use log::{LOG_LIMIT, LogEntry, LogKind};
pub use maneuver::{Burn, ManeuverNode, Plan, PlanError};
pub use rendezvous::{Approach, PlaneCrossing};
pub use trajectory::{ChainNode, Event, Segment, Trajectory};
pub use transfer::{BodyApproach, Window};
pub use world::{
    Base, BaseKind, BoosterStatus, BudgetItem, Cargo, DeltaVBudget, LandingBurn, Store, Touchdown,
    Vessel, VesselParams, VesselStatus, World,
};
