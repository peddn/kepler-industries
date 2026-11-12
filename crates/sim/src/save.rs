//! Spielstand als Text: Die Welt (`World`) ist der ganze Spielstand.
//!
//! JSON, mit bitgenau erhaltenen Zahlen (`float_roundtrip`): Ein geladener
//! Flug geht exakt so weiter wie ohne Speichern (Regel 6). Alte Spielstände
//! werden nicht umgerechnet: Passt `World::save_version` nicht zu
//! `SAVE_VERSION`, wird nicht geladen.

use crate::world::{SAVE_VERSION, World};
use serde::Deserialize;
use std::fmt;

/// Warum ein Spielstand nicht geladen wird.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Von einer anderen Version des Spiels geschrieben (diese Nummer).
    Version(u32),
    /// Kein lesbarer Spielstand (Meldung des Lesers).
    Invalid(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Version(found) => write!(
                f,
                "Spielstand hat Version {found}, das Spiel braucht {SAVE_VERSION}"
            ),
            LoadError::Invalid(reason) => write!(f, "kein lesbarer Spielstand ({reason})"),
        }
    }
}

/// Nur die Versionsnummer: Sie wird zuerst gelesen, damit ein alter
/// Spielstand als solcher erkannt wird und nicht an fehlenden Feldern
/// scheitert.
#[derive(Deserialize)]
struct Header {
    save_version: u32,
}

/// Die Welt als Spielstand.
pub fn to_json(world: &World) -> String {
    serde_json::to_string(world).expect("die Welt lässt sich immer schreiben")
}

/// Liest einen Spielstand.
pub fn from_json(text: &str) -> Result<World, LoadError> {
    let invalid = |e: serde_json::Error| LoadError::Invalid(e.to_string());
    let header: Header = serde_json::from_str(text).map_err(invalid)?;
    if header.save_version != SAVE_VERSION {
        return Err(LoadError::Version(header.save_version));
    }
    serde_json::from_str(text).map_err(invalid)
}
