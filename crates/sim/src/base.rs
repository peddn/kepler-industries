//! Fracht und Basen: der Kreislauf der Wirtschaft (siehe ARCHITEKTUR,
//! „Wirtschaft“).
//!
//! - **Stoffe:** Bauteile und Zorit sind Fracht (kg) und teilen sich den
//!   Frachtraum; sie zählen beim Fliegen als Masse. Treibstoff ist der dritte
//!   Stoff und liegt im Tank.
//! - **Startbasis** (auf dem Heimatplaneten): Schiffe in ihrem Umkreis laden
//!   Bauteile, tanken und laden Zorit ab. Das Zorit kommt ins Lager der
//!   Startbasis (`World::zorit`).
//! - **Preise:** Mit Zorit aus dem Lager bezahlt man Bauteile
//!   (`PARTS_PRICE`) und bestellt neue Schiffe nach einem Entwurf
//!   (`World::order_ship`, Preis siehe `design`), die dann vollgetankt an
//!   der Startbasis stehen. Treibstoff ist dort frei.
//!   Das Spiel beginnt mit einem Vorrat, der genau für eine Basis reicht
//!   (`scenario::START_ZORIT`). Es gibt keine Absicherung: Wer damit keine
//!   Mine fertig bekommt, steckt fest.
//! - **Gründen:** Ein Basisschiff, das auf einem Körper außer dem
//!   Heimatplaneten steht, gründet dort eine Tankstelle oder eine Mine und
//!   wird ihr Fundament; es fliegt dann nicht mehr. Der Landepunkt wird zur
//!   Zone (ein Zielgebiet mit `SITE_RADIUS`, damit Vorschau und Karte ihn wie
//!   jedes Zielgebiet zeigen).
//! - **Baustelle:** Die Basis braucht `BASE_KIT` kg Bauteile. Schiffe, die im
//!   Umkreis stehen, laden ab; ist alles da, ist die Basis fertig.
//! - **Herstellung:** Eine fertige Basis stellt genau einen Stoff her (vorerst
//!   aus dem Nichts): die Tankstelle Treibstoff, die Mine Zorit, gleichmäßig
//!   bis das Lager voll ist. Schiffe im Umkreis tanken oder laden dort. Der
//!   Lagerstand wird für jeden Zeitpunkt direkt berechnet; Warten füllt das
//!   Lager nur einmal (Regel 14).
//! - **Häufigkeit:** Weil Vorspulen nichts kostet, wäre eine Förderrate
//!   wirkungslos. Die Häufigkeit von Zorit bestimmt deshalb die Größe des
//!   Lagers einer Mine (`Body::zorit`), also die Menge pro Abholung: Was
//!   zählt, ist der Ertrag pro Flug.
//!
//! Laden, Abladen und Tanken gehen nur am Boden, ohne geplanten Start und
//! ohne angedockte Schiffe: Dann ändert sich nur die Masse des stehenden
//! Schiffs.

use crate::body::{BodyId, SurfacePoint};
use crate::design::DesignError;
use crate::landing::LandingZone;
use crate::log::{LogEntry, LogKind};
use crate::trajectory::Segment;
use crate::world::{Base, BaseKind, Store, World};

/// Bauteile für eine fertige Basis in kg.
pub const BASE_KIT: f64 = 4_000.0;
/// Umkreis einer Basis in m (entlang der Oberfläche), in dem Schiffe laden,
/// abladen und tanken.
pub const SITE_RADIUS: f64 = 5_000.0;
/// Eine Tankstelle stellt so viel Treibstoff her (kg/s; 180 kg pro Stunde).
pub const FUEL_RATE: f64 = 0.05;
/// Ihr Lager fasst so viel Treibstoff (kg).
pub const FUEL_STORAGE: f64 = 8_000.0;
/// Eine Mine baut so viel Zorit ab (kg/s; 100 kg pro Stunde). Wie viel ihr
/// Lager fasst, hängt vom Körper ab (`Body::zorit`).
pub const ZORIT_RATE: f64 = 100.0 / 3_600.0;

/// Ein kg Bauteile kostet so viel Zorit (kg).
pub const PARTS_PRICE: f64 = 0.25;

/// Warum ein Befehl an Fracht oder Basis nicht geht.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// Diese Basis bietet das nicht (z. B. Tanken an einer Mine).
    WrongBase,
    /// Nichts abzuladen, oder die Basis braucht nichts mehr.
    NothingToUnload,
    /// Frachtraum voll oder Lager leer.
    NothingToLoad,
    /// Tank voll oder Lager leer.
    NothingToRefuel,
    /// Zu wenig Zorit im Lager der Startbasis.
    NotEnoughZorit,
    /// Diesen Entwurf gibt es nicht.
    NoDesign,
    /// Der Entwurf lässt sich nicht bauen.
    Design(DesignError),
    /// Umbau nur auf einen Entwurf mit demselben Chassis.
    OtherChassis,
    /// Die geladene Fracht passt nicht in den neuen Frachtraum.
    CargoInTheWay,
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
            BaseError::WrongBase => "das geht an dieser Basis nicht",
            BaseError::NothingToUnload => "nichts abzuladen",
            BaseError::NothingToLoad => "Frachtraum voll oder Lager leer",
            BaseError::NothingToRefuel => "Tank voll oder Lager leer",
            BaseError::NotEnoughZorit => "zu wenig Zorit im Lager der Startbasis",
            BaseError::NoDesign => "diesen Entwurf gibt es nicht",
            BaseError::OtherChassis => "Umbau nur mit demselben Chassis",
            BaseError::CargoInTheWay => {
                "die Fracht passt nicht in den neuen Frachtraum: erst abladen"
            }
            BaseError::Design(error) => return write!(f, "{error}"),
        })
    }
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

    /// Belädt ein Schiff, das noch nichts geplant hat, mit Bauteilen (für
    /// Szenarien): Die Fracht kommt zur Masse dazu, die Bahn bleibt dieselbe.
    pub fn load_cargo(&mut self, vessel: usize, parts: f64) {
        let v = &self.vessels[vessel];
        assert!(
            v.cargo.total() + parts <= v.params.cargo_capacity,
            "{}: mehr Fracht als Frachtraum",
            v.name
        );
        let (body, state) = self.vessel_state(vessel);
        let mass = self.vessel_mass(vessel) + parts;
        self.vessels[vessel].cargo.parts += parts;
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

    /// Wie schnell eine fertige Basis ihren Stoff herstellt (kg/s) und wie
    /// viel ihr Lager fasst (kg).
    pub fn production(&self, base: usize) -> (f64, f64) {
        let base = &self.bases[base];
        match base.kind {
            BaseKind::Depot => (0.0, 0.0),
            BaseKind::FuelStation => (FUEL_RATE, FUEL_STORAGE),
            BaseKind::Mine => (ZORIT_RATE, self.bodies[base.body].zorit),
        }
    }

    /// Lagerstand einer Basis jetzt: Treibstoff an der Tankstelle, Zorit an
    /// der Mine (0 ohne Lager).
    pub fn base_stock(&self, base: usize) -> f64 {
        let (rate, capacity) = self.production(base);
        self.bases[base].store.as_ref().map_or(0.0, |s| {
            (s.amount + rate * (self.time - s.since).max(0.0)).min(capacity)
        })
    }

    /// Das Schiff steht bereit an einer fertigen Basis dieser Art: ihre
    /// Nummer.
    fn parked_at(&self, vessel: usize, kind: BaseKind) -> Result<usize, BaseError> {
        self.parked(vessel)?;
        let base = self.base_of(vessel).ok_or(BaseError::NoBase)?;
        if self.bases[base].kind != kind {
            return Err(BaseError::WrongBase);
        }
        if !self.bases[base].complete() {
            return Err(BaseError::Unfinished);
        }
        Ok(base)
    }

    /// Ändert die Masse eines stehenden Schiffs um `change` kg.
    fn change_mass(&mut self, vessel: usize, change: f64) {
        let mass = self.vessel_mass(vessel) + change;
        self.vessels[vessel].trajectory.set_landed_mass(mass);
    }

    /// Nimmt `amount` kg aus dem Lager der Basis.
    fn take(&mut self, base: usize, amount: f64) {
        let stock = self.base_stock(base);
        self.bases[base].store = Some(Store {
            amount: stock - amount,
            since: self.time,
        });
    }

    /// Das Basisschiff `vessel` gründet dort, wo es steht, eine Tankstelle
    /// oder Mine (erst eine Baustelle) und wird ihr Fundament. Liefert ihre
    /// Nummer.
    pub fn found_base(&mut self, vessel: usize, kind: BaseKind) -> Result<usize, BaseError> {
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
        let label = match kind {
            BaseKind::Depot => return Err(BaseError::WrongBase),
            BaseKind::FuelStation => "Tankstelle",
            BaseKind::Mine => "Mine",
        };
        let same = |b: &&Base| b.body == body && b.kind == kind;
        let number = 1 + self.bases.iter().filter(same).count();
        let name = format!("{}-{label} {number}", self.bodies[body].name);
        let base = self.bases.len();
        self.bases.push(Base {
            name: name.clone(),
            body,
            location: point,
            kind,
            required: BASE_KIT,
            delivered: 0.0,
            store: None,
        });
        self.zones.push(LandingZone {
            name,
            body,
            center: point,
            radius: SITE_RADIUS,
        });
        self.vessels[vessel].foundation_of = Some(base);
        self.push_log([LogEntry {
            time: self.time,
            vessel,
            kind: LogKind::BaseFounded { base },
        }]);
        Ok(base)
    }

    /// Lädt Bauteile an der Baustelle ab, in deren Umkreis das Schiff steht:
    /// so viel, wie es hat und sie noch braucht. Liefert die Menge in kg.
    pub fn unload(&mut self, vessel: usize) -> Result<f64, BaseError> {
        self.parked(vessel)?;
        let base = self.base_of(vessel).ok_or(BaseError::NoBase)?;
        let needed = (self.bases[base].required - self.bases[base].delivered).max(0.0);
        let amount = self.vessels[vessel].cargo.parts.min(needed);
        if amount <= 0.0 {
            return Err(BaseError::NothingToUnload);
        }
        self.change_mass(vessel, -amount);
        self.vessels[vessel].cargo.parts -= amount;
        let site = &mut self.bases[base];
        site.delivered += amount;
        if site.complete() {
            site.store = Some(Store {
                amount: 0.0,
                since: self.time,
            });
            self.push_log([LogEntry {
                time: self.time,
                vessel,
                kind: LogKind::BaseCompleted { base },
            }]);
        }
        Ok(amount)
    }

    /// Tankt: an der Startbasis voll, an einer Tankstelle so viel, wie im
    /// Lager ist und in den Tank passt. Liefert die Menge in kg.
    pub fn refuel(&mut self, vessel: usize) -> Result<f64, BaseError> {
        self.parked(vessel)?;
        let base = self.base_of(vessel).ok_or(BaseError::NoBase)?;
        let room = self.vessels[vessel].params.fuel_capacity - self.vessel_fuel(vessel);
        let amount = match self.bases[base].kind {
            BaseKind::Depot => room,
            BaseKind::FuelStation => {
                room.min(self.base_stock(self.parked_at(vessel, BaseKind::FuelStation)?))
            }
            BaseKind::Mine => return Err(BaseError::WrongBase),
        };
        if amount <= 0.0 {
            return Err(BaseError::NothingToRefuel);
        }
        self.change_mass(vessel, amount);
        if self.bases[base].kind == BaseKind::FuelStation {
            self.take(base, amount);
        }
        Ok(amount)
    }

    /// Lädt an einer Mine Zorit: so viel, wie im Lager ist und in den
    /// Frachtraum passt. Liefert die Menge in kg.
    pub fn load_zorit(&mut self, vessel: usize) -> Result<f64, BaseError> {
        let base = self.parked_at(vessel, BaseKind::Mine)?;
        let v = &self.vessels[vessel];
        let room = v.params.cargo_capacity - v.cargo.total();
        let amount = room.min(self.base_stock(base));
        if amount <= 0.0 {
            return Err(BaseError::NothingToLoad);
        }
        self.change_mass(vessel, amount);
        self.vessels[vessel].cargo.zorit += amount;
        self.take(base, amount);
        Ok(amount)
    }

    /// Bestellt ein Schiff nach dem Entwurf `design` (Stelle in
    /// `World::designs`): Es steht sofort vollgetankt an der Startbasis (eine
    /// Bauzeit wäre wirkungslos, Regel 14) und heißt wie der Entwurf, mit
    /// laufender Nummer. Liefert seine Nummer.
    pub fn order_ship(&mut self, design: usize) -> Result<usize, BaseError> {
        let depot = self
            .bases
            .iter()
            .find(|b| b.kind == BaseKind::Depot)
            .ok_or(BaseError::NoBase)?;
        let (body, pad) = (depot.body, depot.location);
        let design = self.designs.get(design).ok_or(BaseError::NoDesign)?.clone();
        design.check(&self.unlocked()).map_err(BaseError::Design)?;
        let price = design.price();
        if price > self.zorit {
            return Err(BaseError::NotEnoughZorit);
        }
        self.zorit -= price;
        // Ein Entwurf ohne Namen ergibt trotzdem ein benanntes Schiff.
        let label = match design.name.trim() {
            "" => "Schiff",
            name => name,
        };
        let name = format!("{label}-{}", self.ship_count() + 1);
        let params = design.params();
        let vessel = self.spawn_landed(&name, params, params.fuel_capacity, body, pad);
        self.vessels[vessel].design = Some(design);
        self.push_log([LogEntry {
            time: self.time,
            vessel,
            kind: LogKind::Ordered,
        }]);
        Ok(vessel)
    }

    /// Baut das Schiff an der Startbasis auf den Entwurf `design` um (Stelle
    /// in `World::designs`; dasselbe Chassis). Bezahlt wird der
    /// Preisunterschied, ein billigerer Entwurf bringt ihn zurück; liefert
    /// ihn in kg Zorit. Das Schiff steht danach vollgetankt da (Treibstoff
    /// ist dort frei), sofort (eine Umbauzeit wäre wirkungslos, Regel 14),
    /// und behält Namen und Fracht.
    pub fn refit(&mut self, vessel: usize, design: usize) -> Result<f64, BaseError> {
        self.parked_at(vessel, BaseKind::Depot)?;
        let new = self.designs.get(design).ok_or(BaseError::NoDesign)?.clone();
        // Was nicht nach einem Entwurf gebaut ist (der Booster), baut man
        // nicht um.
        let old = self.vessels[vessel]
            .design
            .as_ref()
            .ok_or(BaseError::NoDesign)?;
        if new.chassis != old.chassis {
            return Err(BaseError::OtherChassis);
        }
        new.check(&self.unlocked()).map_err(BaseError::Design)?;
        let cargo = self.vessels[vessel].cargo.total();
        if cargo > new.cargo() {
            return Err(BaseError::CargoInTheWay);
        }
        let price = new.price() - old.price();
        if price > self.zorit {
            return Err(BaseError::NotEnoughZorit);
        }
        self.zorit -= price;
        let params = new.params();
        let v = &mut self.vessels[vessel];
        v.params = params;
        v.design = Some(new);
        v.trajectory
            .set_landed_mass(params.dry_mass + cargo + params.fuel_capacity);
        Ok(price)
    }

    /// Lädt an der Startbasis Bauteile: bis der Frachtraum voll ist, oder so
    /// viel, wie das Zorit im Lager bezahlt (`PARTS_PRICE`). Liefert die
    /// Menge in kg.
    pub fn load_parts(&mut self, vessel: usize) -> Result<f64, BaseError> {
        self.parked_at(vessel, BaseKind::Depot)?;
        let v = &self.vessels[vessel];
        let room = v.params.cargo_capacity - v.cargo.total();
        if room <= 0.0 {
            return Err(BaseError::NothingToLoad);
        }
        let amount = room.min(self.zorit / PARTS_PRICE);
        if amount <= 0.0 {
            return Err(BaseError::NotEnoughZorit);
        }
        self.zorit = (self.zorit - amount * PARTS_PRICE).max(0.0);
        self.change_mass(vessel, amount);
        self.vessels[vessel].cargo.parts += amount;
        Ok(amount)
    }

    /// Lädt an der Startbasis alles Zorit ab; es kommt in ihr Lager
    /// (`World::zorit`). Liefert die Menge in kg.
    pub fn deliver_zorit(&mut self, vessel: usize) -> Result<f64, BaseError> {
        self.parked_at(vessel, BaseKind::Depot)?;
        let amount = self.vessels[vessel].cargo.zorit;
        if amount <= 0.0 {
            return Err(BaseError::NothingToUnload);
        }
        self.change_mass(vessel, -amount);
        self.vessels[vessel].cargo.zorit = 0.0;
        self.zorit += amount;
        Ok(amount)
    }
}
