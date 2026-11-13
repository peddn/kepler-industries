//! Güter und Inventare: was Schiffe als Fracht tragen und Basen lagern.
//!
//! Ein Gut ist eine Sorte Fracht, ein Inventar eine Liste aus Gut und Menge
//! (kg). Es gibt es an zwei Stellen: als Frachtraum eines Schiffs
//! (`Vessel::cargo`, begrenzt durch `VesselParams::cargo_capacity`; alle
//! Güter teilen ihn sich und zählen beim Fliegen als Masse) und als Lager
//! einer Basis (`Base::stock`). Dazwischen gibt es genau zwei Handlungen,
//! Einladen und Ausladen (`World::load`, `World::unload`, siehe `base`); die
//! Basis bestimmt, was sie herausgibt und annimmt. Ein neues Gut braucht
//! deshalb nur einen Eintrag hier.
//!
//! Treibstoff ist im Schiff kein Frachtgut: Er liegt im Tank und gehört zum
//! Flug (Masse, Δv). Im Lager einer Tankstelle ist er ein Gut wie jedes
//! andere; in den Tank kommt er mit `World::refuel`.

use serde::{Deserialize, Serialize};

/// Laden und Abladen in der Oberfläche gehen in Schritten dieser Größe (kg).
pub const LOT: f64 = 500.0;

/// Eine Sorte Fracht.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Good {
    /// Bauteile: von der Startbasis zu den Baustellen.
    Parts,
    /// Zorit: von den Minen zur Startbasis; bezahlt dort Schiffe und
    /// Bauteile.
    Zorit,
    /// Treibstoff im Lager einer Tankstelle.
    Fuel,
}

impl Good {
    pub fn name(self) -> &'static str {
        match self {
            Good::Parts => "Bauteile",
            Good::Zorit => "Zorit",
            Good::Fuel => "Treibstoff",
        }
    }
}

/// Güter mit ihren Mengen in kg. Was nicht da ist, steht nicht drin; die
/// Reihenfolge ist die, in der die Güter dazukamen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventory(Vec<(Good, f64)>);

impl Inventory {
    /// Menge eines Guts (0, wenn keins da ist).
    pub fn get(&self, good: Good) -> f64 {
        self.0
            .iter()
            .find(|(g, _)| *g == good)
            .map_or(0.0, |(_, amount)| *amount)
    }

    /// Setzt die Menge eines Guts; 0 oder weniger nimmt es heraus.
    pub fn set(&mut self, good: Good, amount: f64) {
        match self.0.iter().position(|(g, _)| *g == good) {
            Some(k) if amount > 0.0 => self.0[k].1 = amount,
            Some(k) => {
                self.0.remove(k);
            }
            None if amount > 0.0 => self.0.push((good, amount)),
            None => {}
        }
    }

    /// Ändert die Menge eines Guts um `change` kg (nie unter 0).
    pub fn add(&mut self, good: Good, change: f64) {
        self.set(good, self.get(good) + change);
    }

    /// Alles zusammen in kg.
    pub fn total(&self) -> f64 {
        // Nicht `sum`: Das ergäbe für ein leeres Inventar -0.
        self.0.iter().fold(0.0, |total, (_, amount)| total + amount)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Die Güter mit ihren Mengen.
    pub fn iter(&self) -> impl Iterator<Item = (Good, f64)> + '_ {
        self.0.iter().copied()
    }
}
