//! Speichern und Laden des Spielstands (`sim::save`): `F5` speichert, `F9`
//! lädt, `--load DATEI` lädt beim Start.
//!
//! Es gibt einen Spielstand, `SAVE_FILE` im Ordner, aus dem das Spiel
//! gestartet wurde. Beim Speichern bleibt der vorherige als `BACKUP_FILE`
//! erhalten, falls man einen guten Stand überschreibt. Gespeichert wird die
//! Welt; Kamera, Auswahl und Zeitraffer gehören nicht dazu. Nach dem Laden
//! ist das Spiel pausiert und nichts ausgewählt.

use crate::camera::{CameraRig, Focus};
use crate::node_ui::{Drag, SelectedBody, SelectedNode, SelectedZone, Selection, UiMessage};
use crate::rendezvous::Rendezvous;
use crate::verbund::UndockChoice;
use crate::{Sim, TimeWarp};
use bevy::prelude::*;
use sim::World;
use std::fs;
use std::path::Path;

pub const SAVE_FILE: &str = "spielstand.json";
pub const BACKUP_FILE: &str = "spielstand.vorher.json";
/// Hierhin wird zuerst geschrieben, damit ein Abbruch mitten im Schreiben
/// den Spielstand nicht zerstört.
const NEW_FILE: &str = "spielstand.neu.json";

/// Liest einen Spielstand aus einer Datei.
pub fn read(path: &Path) -> Result<World, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    sim::save::from_json(&text).map_err(|e| e.to_string())
}

/// Schreibt den Spielstand; der vorherige wird zur Sicherung.
fn write(world: &World) -> Result<(), String> {
    let failed = |e: std::io::Error| e.to_string();
    fs::write(NEW_FILE, sim::save::to_json(world)).map_err(failed)?;
    if Path::new(SAVE_FILE).exists() {
        fs::rename(SAVE_FILE, BACKUP_FILE).map_err(failed)?;
    }
    fs::rename(NEW_FILE, SAVE_FILE).map_err(failed)
}

/// Welches Zielgebiet zu Beginn gewählt ist: ohne Schiffe das der
/// Startbasis, damit ihre Karte (Schiffe bestellen) gleich offen ist.
pub fn initial_zone(world: &World) -> SelectedZone {
    SelectedZone((world.vessels.is_empty() && !world.zones.is_empty()).then_some(0))
}

/// `F5` speichert, `F9` lädt.
#[allow(clippy::too_many_arguments)]
pub fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut sim: ResMut<Sim>,
    mut warp: ResMut<TimeWarp>,
    mut rig: ResMut<CameraRig>,
    mut message: ResMut<UiMessage>,
    mut selected: (
        ResMut<Selection>,
        ResMut<SelectedNode>,
        ResMut<SelectedZone>,
        ResMut<SelectedBody>,
    ),
    mut ui: (ResMut<Rendezvous>, ResMut<UndockChoice>, ResMut<Drag>),
) {
    let real = real_time.elapsed_secs_f64();
    if keys.just_pressed(KeyCode::F5) {
        let text = match write(&sim.0) {
            Ok(()) => format!("Gespeichert ({SAVE_FILE})"),
            Err(e) => format!("Speichern fehlgeschlagen: {e}"),
        };
        message.say(text, sim.0.time, real);
    }
    if keys.just_pressed(KeyCode::F9) {
        match read(Path::new(SAVE_FILE)) {
            Ok(world) => {
                // Alles, was auf Schiffe, Nodes oder Zonen der alten Welt
                // zeigt, wird zurückgesetzt.
                *selected.0 = Selection::default();
                *selected.1 = SelectedNode::default();
                *selected.2 = initial_zone(&world);
                *selected.3 = SelectedBody::default();
                *ui.0 = Rendezvous::default();
                *ui.1 = UndockChoice::default();
                *ui.2 = Drag::default();
                if !matches!(rig.target, Focus::Body(_)) {
                    rig.set_target(Focus::Body(world.home));
                }
                warp.paused = true;
                // Hinweise tragen Zeiten der alten Welt.
                *message = UiMessage::default();
                message.say(format!("Geladen ({SAVE_FILE})"), world.time, real);
                sim.0 = world;
            }
            Err(e) => message.say(format!("Laden fehlgeschlagen: {e}"), sim.0.time, real),
        }
    }
}
