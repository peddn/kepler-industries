//! Techtree: Technologien, die man mit Daten freischaltet (siehe `task`).
//!
//! Vorerst eine Liste mit Voraussetzungen. Eine Technologie gibt entweder ein
//! Teil für Entwürfe frei (`World::unlocked`, siehe `design`) oder ersetzt den
//! Booster der Startbasis durch ein stärkeres Modell. Freischalten kostet
//! Daten, beim Booster dazu Zorit für den Bau; es geschieht sofort (eine
//! Forschungszeit wäre wirkungslos, Regel 14) und gilt für immer. Im
//! Protokoll steht es nicht: Das gehört den Schiffen, und was freigeschaltet
//! ist, sagt `World::techs`.

use crate::design::{Chassis, Engine, Part};
use crate::scenario::BOOSTER_B2;
use crate::world::{BoosterStatus, World};
use serde::{Deserialize, Serialize};

/// Eine Technologie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tech {
    EngineZ2,
    ChassisLarge,
    BoosterB2,
    EngineZ3,
}

/// Was eine Technologie kostet und voraussetzt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TechSpec {
    pub name: &'static str,
    /// Preis in Daten (GB).
    pub data: f64,
    /// Zusätzlich Zorit aus dem Lager der Startbasis (kg).
    pub zorit: f64,
    /// Muss vorher freigeschaltet sein.
    pub requires: Option<Tech>,
}

impl Tech {
    pub const ALL: [Tech; 4] = [
        Tech::EngineZ2,
        Tech::ChassisLarge,
        Tech::BoosterB2,
        Tech::EngineZ3,
    ];

    pub fn spec(self) -> TechSpec {
        let (name, data, zorit, requires) = match self {
            Tech::EngineZ2 => ("Triebwerk Z-2", 30.0, 0.0, None),
            Tech::ChassisLarge => ("Chassis Groß", 30.0, 0.0, None),
            Tech::BoosterB2 => ("Booster B-2", 50.0, 3_000.0, None),
            Tech::EngineZ3 => ("Triebwerk Z-3", 100.0, 0.0, Some(Tech::EngineZ2)),
        };
        TechSpec {
            name,
            data,
            zorit,
            requires,
        }
    }

    /// Das Teil, das sie für Entwürfe freigibt (der Booster gibt keins frei).
    pub fn part(self) -> Option<Part> {
        match self {
            Tech::EngineZ2 => Some(Part::Engine(Engine::Z2)),
            Tech::EngineZ3 => Some(Part::Engine(Engine::Z3)),
            Tech::ChassisLarge => Some(Part::Chassis(Chassis::Large)),
            Tech::BoosterB2 => None,
        }
    }
}

/// Warum sich eine Technologie nicht freischalten lässt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechError {
    /// Schon freigeschaltet.
    Done,
    /// Diese Technologie fehlt noch.
    Requires(Tech),
    NotEnoughData,
    NotEnoughZorit,
    /// Der Booster lässt sich nur tauschen, wenn er frei an der Startbasis
    /// steht.
    BoosterBusy,
}

impl std::fmt::Display for TechError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TechError::Done => f.write_str("schon freigeschaltet"),
            TechError::Requires(tech) => write!(f, "braucht {}", tech.spec().name),
            TechError::NotEnoughData => f.write_str("zu wenig Daten"),
            TechError::NotEnoughZorit => f.write_str("zu wenig Zorit im Lager der Startbasis"),
            TechError::BoosterBusy => f.write_str("der Booster muss frei an der Startbasis stehen"),
        }
    }
}

impl World {
    /// Lässt sich die Technologie jetzt freischalten?
    pub fn can_unlock(&self, tech: Tech) -> Result<(), TechError> {
        let spec = tech.spec();
        if self.techs.contains(&tech) {
            return Err(TechError::Done);
        }
        if let Some(needed) = spec.requires.filter(|t| !self.techs.contains(t)) {
            return Err(TechError::Requires(needed));
        }
        if spec.data > self.data {
            return Err(TechError::NotEnoughData);
        }
        if spec.zorit > self.zorit {
            return Err(TechError::NotEnoughZorit);
        }
        if tech == Tech::BoosterB2 && self.booster_status() != Some(BoosterStatus::Ready) {
            return Err(TechError::BoosterBusy);
        }
        Ok(())
    }

    /// Schaltet die Technologie frei: bucht Daten und Zorit ab, gibt das Teil
    /// frei oder stellt den neuen Booster vollgetankt an die Startbasis.
    pub fn unlock(&mut self, tech: Tech) -> Result<(), TechError> {
        self.can_unlock(tech)?;
        let spec = tech.spec();
        self.data -= spec.data;
        self.zorit -= spec.zorit;
        self.techs.push(tech);
        if let Some(part) = tech.part() {
            self.unlocked.push(part);
        }
        if tech == Tech::BoosterB2
            && let Some(booster) = self.booster
        {
            let vessel = &mut self.vessels[booster];
            vessel.params = BOOSTER_B2;
            vessel
                .trajectory
                .set_landed_mass(BOOSTER_B2.dry_mass + BOOSTER_B2.fuel_capacity);
        }
        Ok(())
    }
}
