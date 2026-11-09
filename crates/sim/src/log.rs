//! Protokoll: was beim Vorrücken der Zeit passiert ist.
//!
//! Es gehört zum Spielzustand und wird mit ihm gespeichert. Gespeichert sind
//! Daten, kein Text: Die Anzeige formuliert selbst. Die Einträge entstehen,
//! wenn ein Abschnitt der Zeitleiste endet, mit dessen genauem Zeitpunkt.
//! Deshalb ist das Protokoll unabhängig davon, in wie vielen Schritten die Zeit
//! vorrückt.

use crate::body::SurfacePoint;
use crate::landing::Crash;
use serde::{Deserialize, Serialize};

/// So viele Einträge bleiben erhalten (die ältesten fallen heraus).
pub const LOG_LIMIT: usize = 1000;

/// Ein Eintrag im Protokoll.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Spielzeit in s.
    pub time: f64,
    /// Nummer des Schiffs.
    pub vessel: usize,
    pub kind: LogKind,
}

/// Was passiert ist. Höhen in m über dem Boden.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LogKind {
    /// Abheben vom Boden (Beginn des Aufstiegs).
    Liftoff,
    /// Ein geplantes Brennen beginnt.
    BurnStart,
    /// Brennschluss und die Bahn danach (Apoapsis `None` bei offener Bahn).
    BurnEnd {
        apoapsis: Option<f64>,
        periapsis: f64,
    },
    /// Eintritt in die Atmosphäre.
    AtmosphereEntry { periapsis: f64 },
    /// Heil aufgesetzt, im Zielgebiet `zone` oder außerhalb.
    Landed {
        point: SurfacePoint,
        zone: Option<usize>,
    },
    /// Abgestürzt und warum.
    Crashed {
        point: SurfacePoint,
        zone: Option<usize>,
        reason: Crash,
    },
}
