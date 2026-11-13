//! Schiffe aus Komponenten: der Katalog und Entwürfe daraus.
//!
//! Ein Entwurf ist ein Chassis mit Triebwerken einer Sorte, Modulen und
//! einer Aufteilung des Innenraums:
//!
//! - Das **Chassis** bringt Masse, Innenraum, Plätze für Triebwerke und
//!   Module, Fallschirm und Luftwiderstand mit.
//! - **Triebwerke:** eine Sorte je Schiff, so viele wie Plätze da sind
//!   (gemischte Sorten würfen die Frage auf, welches gerade brennt).
//! - **Module** geben dem Schiff Fähigkeiten (`Module`): bisher das
//!   Fundament, mit dem es eine Basis gründet (siehe `base`). Sie belegen
//!   Modulplätze und haben Masse und Preis, brauchen aber keinen Innenraum.
//! - Der **Innenraum** teilt sich in Tank und Frachtraum; eine
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
    pub engine_slots: u32,
    /// Plätze für Module.
    pub module_slots: u32,
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
                interior: 7_000.0,
                engine_slots: 1,
                module_slots: 2,
                drag_area: 4.0,
                parachute_area: 400.0,
                price: 400.0,
            },
            Chassis::Medium => ChassisSpec {
                name: "Mittel",
                mass: 1_900.0,
                interior: 16_000.0,
                engine_slots: 2,
                module_slots: 4,
                drag_area: 5.0,
                parachute_area: 1_000.0,
                price: 600.0,
            },
            Chassis::Large => ChassisSpec {
                name: "Groß",
                mass: 3_800.0,
                interior: 32_000.0,
                engine_slots: 4,
                module_slots: 8,
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

/// Ein Modul: ein Einbau, der dem Schiff eine Fähigkeit gibt. Module belegen
/// Modulplätze des Chassis und haben Masse und Preis; Innenraum brauchen sie
/// nicht (der gehört Tank und Fracht). Ein neues Modul ist ein Eintrag hier
/// und die Regel, die nach ihm fragt (`Modules::has`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Module {
    /// Fundament einer Basis: Das Schiff gründet dort, wo es landet, eine
    /// Basis und wird ihr Fundament (siehe `base`).
    Foundation,
}

/// Werte eines Moduls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModuleSpec {
    pub name: &'static str,
    /// Wozu es dient, für den Editor.
    pub purpose: &'static str,
    /// Masse in kg.
    pub mass: f64,
    /// So viele Modulplätze belegt es.
    pub slots: u32,
    /// So oft passt es höchstens in ein Schiff.
    pub limit: u32,
    /// Preis in Zorit (kg).
    pub price: f64,
}

impl Module {
    pub const ALL: [Module; 1] = [Module::Foundation];

    pub fn spec(self) -> ModuleSpec {
        match self {
            Module::Foundation => ModuleSpec {
                name: "Fundament",
                purpose: "gründet eine Basis",
                mass: 1_500.0,
                slots: 2,
                limit: 1,
                price: 400.0,
            },
        }
    }
}

/// Welche Module ein Schiff eingebaut hat: seine Fähigkeiten. Eine Menge,
/// damit die Flugwerte (`VesselParams`) einfach kopierbar bleiben; wie viele
/// es von einem Modul sind, steht im Entwurf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modules(u32);

impl Modules {
    /// Kein Modul.
    pub const NONE: Modules = Modules(0);

    fn bit(module: Module) -> u32 {
        1 << module as u32
    }

    pub fn has(self, module: Module) -> bool {
        self.0 & Self::bit(module) != 0
    }

    /// Dieselbe Menge mit diesem Modul dazu.
    pub fn with(self, module: Module) -> Modules {
        Modules(self.0 | Self::bit(module))
    }

    /// Beide Mengen zusammen (ein Verbund hat die Module aller Schiffe).
    pub fn union(self, other: Modules) -> Modules {
        Modules(self.0 | other.0)
    }
}

impl FromIterator<Module> for Modules {
    fn from_iter<I: IntoIterator<Item = Module>>(modules: I) -> Self {
        modules.into_iter().fold(Modules::NONE, Modules::with)
    }
}

/// Etwas, das man freischalten kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Part {
    Chassis(Chassis),
    Engine(Engine),
    Module(Module),
}

impl Part {
    /// Die Ausstattung vom Spielbeginn.
    pub fn from_start() -> Vec<Part> {
        vec![
            Part::Chassis(Chassis::Small),
            Part::Chassis(Chassis::Medium),
            Part::Engine(Engine::Z1),
            Part::Module(Module::Foundation),
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            Part::Chassis(chassis) => chassis.spec().name,
            Part::Engine(engine) => engine.spec().name,
            Part::Module(module) => module.spec().name,
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
    /// Die Module belegen mehr Plätze, als das Chassis hat.
    TooManyModules { slots: u32 },
    /// Dieses Modul ist öfter eingebaut, als es geht.
    ModuleLimit(Module),
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
            DesignError::TooManyModules { slots } => {
                write!(f, "das Chassis hat nur {slots} Modulplätze")
            }
            DesignError::ModuleLimit(module) => {
                let spec = module.spec();
                write!(f, "{} geht höchstens {}-mal", spec.name, spec.limit)
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
    /// Die eingebauten Module.
    pub modules: Vec<Module>,
    /// Tank in kg; der Rest des Innenraums ist Frachtraum.
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
            modules: Vec::new(),
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
            modules: vec![Module::Foundation],
            tank: 7_000.0,
        }
    }

    /// Innenraum in kg: Ihn teilen sich Tank und Fracht.
    pub fn free_space(&self) -> f64 {
        self.chassis.spec().interior
    }

    /// Frachtraum in kg: was der Tank vom Innenraum übrig lässt.
    pub fn cargo(&self) -> f64 {
        (self.free_space() - self.tank).max(0.0)
    }

    /// So viele Modulplätze belegen die Module.
    pub fn module_slots(&self) -> u32 {
        self.modules.iter().map(|m| m.spec().slots).sum()
    }

    /// Ist dieses Modul eingebaut?
    pub fn has(&self, module: Module) -> bool {
        self.modules.contains(&module)
    }

    /// Die Teile, die freigeschaltet sein müssen.
    pub fn parts(&self) -> Vec<Part> {
        let mut parts = vec![Part::Chassis(self.chassis), Part::Engine(self.engine)];
        parts.extend(self.modules.iter().map(|&m| Part::Module(m)));
        parts
    }

    /// Lässt sich der Entwurf mit den freigeschalteten Teilen bauen?
    pub fn check(&self, unlocked: &[Part]) -> Result<(), DesignError> {
        let chassis = self.chassis.spec();
        if self.engines > chassis.engine_slots {
            return Err(DesignError::TooManyEngines {
                slots: chassis.engine_slots,
            });
        }
        if self.module_slots() > chassis.module_slots {
            return Err(DesignError::TooManyModules {
                slots: chassis.module_slots,
            });
        }
        let count = |module| self.modules.iter().filter(|&&m| m == module).count() as u32;
        if let Some(&module) = self.modules.iter().find(|&&m| count(m) > m.spec().limit) {
            return Err(DesignError::ModuleLimit(module));
        }
        let free = self.free_space();
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
        let modules: f64 = self.modules.iter().map(|m| m.spec().mass).sum();
        VesselParams {
            dry_mass: chassis.mass + engines * engine.mass + modules,
            drag_area: chassis.drag_area,
            thrust: engines * engine.thrust,
            exhaust_velocity: engine.exhaust_velocity,
            fuel_capacity: self.tank,
            parachute_area: chassis.parachute_area,
            cargo_capacity: self.cargo(),
            modules: self.modules.iter().copied().collect(),
        }
    }

    /// Preis in Zorit (kg).
    pub fn price(&self) -> f64 {
        let modules: f64 = self.modules.iter().map(|m| m.spec().price).sum();
        self.chassis.spec().price + f64::from(self.engines) * self.engine.spec().price + modules
    }
}
