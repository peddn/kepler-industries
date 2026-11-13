//! Schiffe aus Komponenten: der Katalog und Entwürfe daraus.
//!
//! Ein Entwurf ist ein Chassis mit Triebwerken einer Sorte, optional dem
//! Fundament einer Basis und einer Aufteilung des Innenraums:
//!
//! - Das **Chassis** bringt Masse, Innenraum, Triebwerksplätze, Fallschirm
//!   und Luftwiderstand mit.
//! - **Triebwerke:** eine Sorte je Schiff, so viele wie Plätze da sind
//!   (gemischte Sorten würfen die Frage auf, welches gerade brennt).
//! - **Feste Einbauten** belegen zuerst Platz im Innenraum: bisher nur das
//!   Fundament, mit dem das Schiff eine Basis gründet (siehe `base`).
//! - Der **Rest des Innenraums** teilt sich in Tank und Frachtraum; eine
//!   Tonne Platz fasst eine Tonne Treibstoff oder eine Tonne Fracht. Es gibt
//!   einen Treibstoff und keine Tanksorten: Reichweite kommt von den
//!   Triebwerken.
//!
//! Aus dem Entwurf folgen die Flugwerte (`Design::params`) und der Preis in
//! Zorit (`Design::price`). Bestellt wird an der Startbasis
//! (`World::order_ship`); das Schiff merkt sich seinen Entwurf.
//!
//! Was man bauen darf, sagt `World::unlocked`: die Ausstattung vom Spielbeginn
//! (`Part::from_start`) und was die Technik freigeschaltet hat (siehe `tech`).

use crate::world::VesselParams;
use serde::{Deserialize, Serialize};

/// Der Innenraum teilt sich in Schritten dieser Größe (kg).
pub const SPACE_STEP: f64 = 500.0;

/// Fundament einer Basis: Masse (kg), belegter Innenraum (kg) und Preis in
/// Zorit (kg).
pub const FOUNDATION_MASS: f64 = 1_500.0;
pub const FOUNDATION_SPACE: f64 = 3_000.0;
pub const FOUNDATION_PRICE: f64 = 400.0;

/// Rumpf eines Schiffs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chassis {
    Small,
    Medium,
    Large,
}

/// Werte eines Chassis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChassisSpec {
    pub name: &'static str,
    /// Masse in kg, mit Fallschirm.
    pub mass: f64,
    /// Innenraum in kg: so viel Treibstoff und Fracht passen zusammen hinein.
    pub interior: f64,
    /// Plätze für Triebwerke.
    pub slots: u32,
    /// Widerstandsbeiwert × Querschnittsfläche in m².
    pub drag_area: f64,
    /// Widerstandsbeiwert × Fläche des offenen Fallschirms in m².
    pub parachute_area: f64,
    /// Preis in Zorit (kg).
    pub price: f64,
}

impl Chassis {
    pub const ALL: [Chassis; 3] = [Chassis::Small, Chassis::Medium, Chassis::Large];

    pub fn spec(self) -> ChassisSpec {
        match self {
            Chassis::Small => ChassisSpec {
                name: "Klein",
                mass: 1_200.0,
                interior: 10_000.0,
                slots: 1,
                drag_area: 4.0,
                parachute_area: 400.0,
                price: 400.0,
            },
            Chassis::Medium => ChassisSpec {
                name: "Mittel",
                mass: 1_900.0,
                interior: 16_000.0,
                slots: 2,
                drag_area: 5.0,
                parachute_area: 1_000.0,
                price: 600.0,
            },
            Chassis::Large => ChassisSpec {
                name: "Groß",
                mass: 3_800.0,
                interior: 32_000.0,
                slots: 4,
                drag_area: 8.0,
                parachute_area: 2_000.0,
                price: 1_200.0,
            },
        }
    }
}

/// Sorte der Triebwerke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    Z1,
    Z2,
    Z3,
}

/// Werte eines Triebwerks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineSpec {
    pub name: &'static str,
    /// Schub in N.
    pub thrust: f64,
    /// Ausströmgeschwindigkeit in m/s.
    pub exhaust_velocity: f64,
    /// Masse in kg.
    pub mass: f64,
    /// Preis in Zorit (kg).
    pub price: f64,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Z1, Engine::Z2, Engine::Z3];

    pub fn spec(self) -> EngineSpec {
        let (name, exhaust_velocity, mass, price) = match self {
            Engine::Z1 => ("Z-1", 3_200.0, 300.0, 200.0),
            Engine::Z2 => ("Z-2", 3_600.0, 350.0, 400.0),
            Engine::Z3 => ("Z-3", 4_000.0, 400.0, 800.0),
        };
        EngineSpec {
            name,
            thrust: 250_000.0,
            exhaust_velocity,
            mass,
            price,
        }
    }
}

/// Etwas, das man freischalten kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Part {
    Chassis(Chassis),
    Engine(Engine),
}

impl Part {
    /// Die Ausstattung vom Spielbeginn.
    pub fn from_start() -> Vec<Part> {
        vec![
            Part::Chassis(Chassis::Small),
            Part::Chassis(Chassis::Medium),
            Part::Engine(Engine::Z1),
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            Part::Chassis(chassis) => chassis.spec().name,
            Part::Engine(engine) => engine.spec().name,
        }
    }
}

/// Warum sich ein Entwurf nicht bauen lässt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DesignError {
    /// Mehr Triebwerke als das Chassis Plätze hat.
    TooManyEngines { slots: u32 },
    /// Der Tank ist größer als der freie Innenraum (kg), oder kleiner als
    /// null.
    TankOutOfRange { free: f64 },
    /// Der Tank liegt nicht auf einem Schritt von `SPACE_STEP`.
    TankOffStep,
    /// Das Fundament passt nicht in den Innenraum.
    NoRoomForFoundation,
    /// Dieses Teil ist noch nicht freigeschaltet.
    Locked(Part),
}

impl std::fmt::Display for DesignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DesignError::TooManyEngines { slots } => {
                write!(f, "das Chassis hat nur {slots} Triebwerksplätze")
            }
            DesignError::TankOutOfRange { free } => write!(
                f,
                "der Tank passt nicht in den freien Innenraum ({:.1} t)",
                free / 1_000.0
            ),
            DesignError::TankOffStep => write!(
                f,
                "der Tank geht nur in Schritten von {:.1} t",
                SPACE_STEP / 1_000.0
            ),
            DesignError::NoRoomForFoundation => {
                f.write_str("das Fundament passt nicht in dieses Chassis")
            }
            DesignError::Locked(part) => write!(f, "{} ist noch nicht freigeschaltet", part.name()),
        }
    }
}

/// Ein Schiffsentwurf.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub name: String,
    pub chassis: Chassis,
    pub engine: Engine,
    /// Zahl der Triebwerke (0: keins).
    pub engines: u32,
    /// Mit Fundament: Das Schiff gründet eine Basis.
    pub foundation: bool,
    /// Tank in kg; der Rest des freien Innenraums ist Frachtraum.
    pub tank: f64,
}

impl Design {
    /// Startentwurf: mittleres Chassis, zwei Z-1, 14 t Tank und 2 t Fracht.
    pub fn freighter() -> Self {
        Self {
            name: "Frachter".into(),
            chassis: Chassis::Medium,
            engine: Engine::Z1,
            engines: 2,
            foundation: false,
            tank: 14_000.0,
        }
    }

    /// Startentwurf: kleines Chassis, ein Z-1, Fundament und 7 t Tank.
    pub fn base_ship() -> Self {
        Self {
            name: "Basisschiff".into(),
            chassis: Chassis::Small,
            engine: Engine::Z1,
            engines: 1,
            foundation: true,
            tank: 7_000.0,
        }
    }

    /// Innenraum, den die festen Einbauten übrig lassen (kg).
    pub fn free_space(&self) -> f64 {
        let fixed = if self.foundation {
            FOUNDATION_SPACE
        } else {
            0.0
        };
        self.chassis.spec().interior - fixed
    }

    /// Frachtraum in kg: was der Tank vom freien Innenraum übrig lässt.
    pub fn cargo(&self) -> f64 {
        (self.free_space() - self.tank).max(0.0)
    }

    /// Die Teile, die freigeschaltet sein müssen.
    pub fn parts(&self) -> [Part; 2] {
        [Part::Chassis(self.chassis), Part::Engine(self.engine)]
    }

    /// Lässt sich der Entwurf mit den freigeschalteten Teilen bauen?
    pub fn check(&self, unlocked: &[Part]) -> Result<(), DesignError> {
        let chassis = self.chassis.spec();
        if self.engines > chassis.slots {
            return Err(DesignError::TooManyEngines {
                slots: chassis.slots,
            });
        }
        let free = self.free_space();
        if free < 0.0 {
            return Err(DesignError::NoRoomForFoundation);
        }
        if !(0.0..=free).contains(&self.tank) {
            return Err(DesignError::TankOutOfRange { free });
        }
        if self.tank % SPACE_STEP != 0.0 {
            return Err(DesignError::TankOffStep);
        }
        match self.parts().into_iter().find(|p| !unlocked.contains(p)) {
            Some(part) => Err(DesignError::Locked(part)),
            None => Ok(()),
        }
    }

    /// Flugwerte des Schiffs.
    pub fn params(&self) -> VesselParams {
        let chassis = self.chassis.spec();
        let engine = self.engine.spec();
        let engines = f64::from(self.engines);
        let foundation = if self.foundation {
            FOUNDATION_MASS
        } else {
            0.0
        };
        VesselParams {
            dry_mass: chassis.mass + engines * engine.mass + foundation,
            drag_area: chassis.drag_area,
            thrust: engines * engine.thrust,
            exhaust_velocity: engine.exhaust_velocity,
            fuel_capacity: self.tank,
            parachute_area: chassis.parachute_area,
            cargo_capacity: self.cargo(),
            base_kit: self.foundation,
        }
    }

    /// Preis in Zorit (kg).
    pub fn price(&self) -> f64 {
        let foundation = if self.foundation {
            FOUNDATION_PRICE
        } else {
            0.0
        };
        self.chassis.spec().price + f64::from(self.engines) * self.engine.spec().price + foundation
    }
}
