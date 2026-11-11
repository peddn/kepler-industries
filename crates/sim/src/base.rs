//! Fracht und Basen: erster Schritt der Wirtschaft (siehe ARCHITEKTUR,
//! „Wirtschaft“).
//!
//! - **Fracht:** Ein Schiff mit Frachtraum trägt Bauteile (kg). Sie zählen
//!   beim Fliegen als Masse.
//! - **Gründen:** Ein Basisschiff, das auf einem Körper außer dem
//!   Heimatplaneten steht, gründet dort eine Basis und wird ihr Fundament; es
//!   fliegt dann nicht mehr. Der Landepunkt wird zur Zone (ein Zielgebiet mit
//!   `SITE_RADIUS`, damit Vorschau und Karte ihn wie jedes Zielgebiet zeigen).
//! - **Baustelle:** Die Basis braucht `BASE_KIT` kg Bauteile. Schiffe, die im
//!   Umkreis stehen, laden ab; ist alles da, ist die Basis fertig.
//! - **Treibstoff:** Eine fertige Basis stellt Treibstoff her (vorerst aus
//!   dem Nichts, bis es Rohstoffe gibt), gleichmäßig bis das Lager voll ist.
//!   Schiffe im Umkreis tanken dort. Der Lagerstand wird für jeden Zeitpunkt
//!   direkt berechnet; Warten füllt das Lager nur einmal (Regel 14).
//!
//! Abladen und Tanken gehen nur am Boden, ohne geplanten Start und ohne
//! angedockte Schiffe: Dann ändert sich nur die Masse des stehenden Schiffs.

use crate::body::{BodyId, SurfacePoint};
use crate::landing::LandingZone;
use crate::log::{LogEntry, LogKind};
use crate::trajectory::Segment;
use crate::world::{Base, FuelStore, World};

/// Bauteile für eine fertige Basis in kg.
pub const BASE_KIT: f64 = 4_000.0;
/// Umkreis einer Basis in m (entlang der Oberfläche), in dem Schiffe abladen
/// und tanken.
pub const SITE_RADIUS: f64 = 5_000.0;
/// Eine fertige Basis stellt so viel Treibstoff her (kg/s; 180 kg pro Stunde).
pub const FUEL_RATE: f64 = 0.05;
/// Ihr Lager fasst so viel Treibstoff (kg).
pub const FUEL_STORAGE: f64 = 8_000.0;

/// Warum ein Befehl an Fracht oder Basis nicht geht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseError {
    /// Das Schiff steht nicht heil am Boden.
    NotLanded,
    /// Ein Start ist geplant, oder Schiffe sind angedockt.
    Busy,
    /// Kein Basisschiff, oder es hat schon gegründet.
    NoBaseKit,
    /// Auf dem Heimatplaneten wird nicht gebaut.
    HomePlanet,
    /// Hier steht schon eine Basis (im Umkreis).
    Occupied,
    /// Keine Basis im Umkreis.
    NoBase,
    /// Die Basis ist noch nicht fertig.
    Unfinished,
    /// Nichts abzuladen, oder die Basis braucht nichts mehr.
    NothingToUnload,
    /// Tank voll oder Lager leer.
    NothingToRefuel,
}

impl std::fmt::Display for BaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BaseError::NotLanded => "Schiff steht nicht am Boden",
            BaseError::Busy => "erst den Start zurücknehmen und abdocken",
            BaseError::NoBaseKit => "kein Basisschiff",
            BaseError::HomePlanet => "auf dem Heimatplaneten wird nicht gebaut",
            BaseError::Occupied => "hier steht schon eine Basis",
            BaseError::NoBase => "keine Basis im Umkreis",
            BaseError::Unfinished => "die Basis ist noch nicht fertig",
            BaseError::NothingToUnload => "nichts abzuladen",
            BaseError::NothingToRefuel => "Tank voll oder Lager leer",
        })
    }
}

/// Treibstoff im Lager zum Zeitpunkt `t`.
pub fn fuel_at(store: &FuelStore, t: f64) -> f64 {
    (store.amount + FUEL_RATE * (t - store.since).max(0.0)).min(FUEL_STORAGE)
}

impl World {
    /// Wo ein Schiff heil am Boden steht und bereit ist (kein Start geplant,
    /// nichts angedockt).
    fn parked(&self, vessel: usize) -> Result<(BodyId, SurfacePoint), BaseError> {
        let v = &self.vessels[vessel];
        let Segment::Surface {
            body,
            point,
            wreck: false,
            ..
        } = v.trajectory.current()
        else {
            return Err(BaseError::NotLanded);
        };
        if v.docked_to.is_some()
            || !self.parts(vessel).is_empty()
            || v.trajectory.launch().is_some()
        {
            return Err(BaseError::Busy);
        }
        Ok((*body, *point))
    }

    /// Belädt ein Schiff, das noch nichts geplant hat (für Szenarien): Die
    /// Fracht kommt zur Masse dazu, die Bahn bleibt dieselbe.
    pub fn load_cargo(&mut self, vessel: usize, cargo: f64) {
        let v = &self.vessels[vessel];
        assert!(
            v.cargo + cargo <= v.params.cargo_capacity,
            "{}: mehr Fracht als Frachtraum",
            v.name
        );
        let (body, state) = self.vessel_state(vessel);
        let mass = self.vessel_mass(vessel) + cargo;
        self.vessels[vessel].cargo += cargo;
        let params = self.params(vessel);
        if let Segment::Surface { .. } = self.vessels[vessel].trajectory.current() {
            self.vessels[vessel].trajectory.set_landed_mass(mass);
        } else {
            self.vessels[vessel].trajectory.restart_flying(
                &self.bodies,
                &params,
                body,
                self.time,
                state,
                mass,
            );
        }
    }

    /// Die Basis, in deren Umkreis der Punkt liegt (die nächste).
    pub fn base_at(&self, body: BodyId, point: &SurfacePoint) -> Option<usize> {
        let radius = self.bodies[body].radius;
        let distance = |base: &Base| {
            let (a, b) = (
                base.location.body_fixed_direction(),
                point.body_fixed_direction(),
            );
            radius * a.dot(b).clamp(-1.0, 1.0).acos()
        };
        self.bases
            .iter()
            .enumerate()
            .filter(|(_, b)| b.body == body)
            .map(|(k, b)| (k, distance(b)))
            .filter(|&(_, d)| d <= SITE_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    }

    /// Die Basis, in deren Umkreis ein Schiff am Boden steht.
    pub fn base_of(&self, vessel: usize) -> Option<usize> {
        match self.vessels[vessel].trajectory.current() {
            Segment::Surface { body, point, .. } => self.base_at(*body, point),
            _ => None,
        }
    }

    /// Treibstoff im Lager einer Basis jetzt (0 ohne Lager).
    pub fn base_fuel(&self, base: usize) -> f64 {
        self.bases[base]
            .fuel
            .as_ref()
            .map_or(0.0, |s| fuel_at(s, self.time))
    }

    /// Das Basisschiff `vessel` gründet dort, wo es steht, eine Basis (erst
    /// eine Baustelle) und wird ihr Fundament. Liefert ihre Nummer.
    pub fn found_base(&mut self, vessel: usize) -> Result<usize, BaseError> {
        let (body, point) = self.parked(vessel)?;
        let v = &self.vessels[vessel];
        if !v.params.base_kit || v.foundation_of.is_some() {
            return Err(BaseError::NoBaseKit);
        }
        if body == self.home {
            return Err(BaseError::HomePlanet);
        }
        if self.base_at(body, &point).is_some() {
            return Err(BaseError::Occupied);
        }
        let number = 1 + self.bases.iter().filter(|b| b.body == body).count();
        let name = format!("{}-Basis {number}", self.bodies[body].name);
        let base = self.bases.len();
        self.bases.push(Base {
            name: name.clone(),
            body,
            location: point,
            required: BASE_KIT,
            delivered: 0.0,
            fuel: None,
        });
        self.zones.push(LandingZone {
            name,
            body,
            center: point,
            radius: SITE_RADIUS,
        });
        self.vessels[vessel].foundation_of = Some(base);
        self.log.push(LogEntry {
            time: self.time,
            vessel,
            kind: LogKind::BaseFounded { base },
        });
        Ok(base)
    }

    /// Lädt Bauteile an der Baustelle ab, in deren Umkreis das Schiff steht:
    /// so viel, wie es hat und sie noch braucht. Liefert die Menge in kg.
    pub fn unload(&mut self, vessel: usize) -> Result<f64, BaseError> {
        self.parked(vessel)?;
        let base = self.base_of(vessel).ok_or(BaseError::NoBase)?;
        let needed = (self.bases[base].required - self.bases[base].delivered).max(0.0);
        let amount = self.vessels[vessel].cargo.min(needed);
        if amount <= 0.0 {
            return Err(BaseError::NothingToUnload);
        }
        let mass = self.vessel_mass(vessel) - amount;
        self.vessels[vessel].cargo -= amount;
        self.vessels[vessel].trajectory.set_landed_mass(mass);
        let site = &mut self.bases[base];
        site.delivered += amount;
        if site.complete() {
            site.fuel = Some(FuelStore {
                amount: 0.0,
                since: self.time,
            });
            self.log.push(LogEntry {
                time: self.time,
                vessel,
                kind: LogKind::BaseCompleted { base },
            });
        }
        Ok(amount)
    }

    /// Tankt an der fertigen Basis, in deren Umkreis das Schiff steht: so
    /// viel, wie in den Tank passt und im Lager ist. Liefert die Menge in kg.
    pub fn refuel(&mut self, vessel: usize) -> Result<f64, BaseError> {
        self.parked(vessel)?;
        let base = self.base_of(vessel).ok_or(BaseError::NoBase)?;
        let Some(store) = self.bases[base].fuel else {
            return Err(BaseError::Unfinished);
        };
        let stock = fuel_at(&store, self.time);
        let room = self.vessels[vessel].params.fuel_capacity - self.vessel_fuel(vessel);
        let amount = stock.min(room);
        if amount <= 0.0 {
            return Err(BaseError::NothingToRefuel);
        }
        let mass = self.vessel_mass(vessel) + amount;
        self.vessels[vessel].trajectory.set_landed_mass(mass);
        self.bases[base].fuel = Some(FuelStore {
            amount: stock - amount,
            since: self.time,
        });
        Ok(amount)
    }
}
