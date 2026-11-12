//! Statusleiste ganz unten: die letzte Meldung mit Spielzeit. ▲ rechts klappt
//! darüber die letzten Meldungen auf (zum Scrollen), ▼ klappt sie wieder zu.
//!
//! Die Meldungen sind das Protokoll der Simulation (`World::log`, gehört zum
//! Spielstand) und, gelb, die Hinweise der Bedienung (`UiMessage`, nicht
//! gespeichert), zusammen nach Spielzeit geordnet. Wer ganz unten steht, sieht
//! neue Meldungen sofort; wer hochgescrollt hat, bleibt an seiner Stelle.

use crate::hud::{self, format_time, km, place};
use crate::node_ui::UiMessage;
use bevy::prelude::*;
use bevy_egui::egui;
use sim::{Crash, LogEntry, LogKind, World};

/// Sichtbare Zeilen im aufgeklappten Bereich.
const LINES: usize = 12;

const TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 235, 242);
const NOTE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 217, 89);
const CRASH_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 102, 89);
const LANDED_COLOR: egui::Color32 = egui::Color32::from_rgb(115, 255, 140);
const ATMOSPHERE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 179, 102);

/// Ist der Bereich mit den letzten Meldungen aufgeklappt?
#[derive(Resource, Default)]
pub struct StatusBar {
    pub open: bool,
}

/// Eine Zeile der Anzeige.
struct Line {
    time: f64,
    text: String,
    color: egui::Color32,
}

/// Grund eines Absturzes als Text.
fn crash_reason(reason: &Crash) -> String {
    match reason {
        Crash::NoFuel { available, .. } => format!("Δv an Bord nur {available:.0} m/s"),
        Crash::TooWeak { thrust_to_weight } => {
            format!("Schub zu schwach zum Bremsen (Schub/Gewicht {thrust_to_weight:.2})")
        }
        Crash::TooLate => "keine Zeit zum Bremsen".into(),
    }
}

/// Text und Farbe zu einem Eintrag des Protokolls.
fn entry_line(world: &World, entry: &LogEntry) -> Line {
    let name = &world.vessels[entry.vessel].name;
    // Auf dem Heimatplaneten ohne Namen, sonst " auf Zoon".
    let on = |body: usize| match body {
        0 => String::new(),
        b => format!(" auf {}", world.bodies[b].name),
    };
    let zone = |zone: Option<usize>, point| match zone {
        Some(k) => format!("im Zielgebiet {}", world.zones[k].name),
        None => format!("bei {}", place(point)),
    };
    let (what, color) = match &entry.kind {
        LogKind::Liftoff => ("Start".into(), TEXT_COLOR),
        LogKind::BurnStart => ("Brennbeginn".into(), TEXT_COLOR),
        LogKind::LandingBurn => ("Landebrennen".into(), TEXT_COLOR),
        LogKind::BaseFounded { base } => {
            (format!("gründet {}", world.bases[*base].name), LANDED_COLOR)
        }
        LogKind::Ordered => ("steht an der Startbasis bereit".into(), LANDED_COLOR),
        LogKind::BaseCompleted { base } => (
            format!(
                "liefert die letzten Bauteile, {} ist fertig",
                world.bases[*base].name
            ),
            LANDED_COLOR,
        ),
        LogKind::BurnEnd {
            apoapsis,
            periapsis,
        } => (
            format!(
                "Brennschluss, Ap {}  Pe {}",
                apoapsis.map_or("–".into(), km),
                km(*periapsis)
            ),
            TEXT_COLOR,
        ),
        LogKind::AtmosphereEntry { periapsis } => (
            format!("tritt in die Atmosphäre ein (Pe {})", km(*periapsis)),
            ATMOSPHERE_COLOR,
        ),
        LogKind::Landed {
            point,
            zone: z,
            body,
        } => {
            let at = zone(*z, point);
            let outside = if z.is_none() { " (außerhalb)" } else { "" };
            (format!("gelandet{} {at}{outside}", on(*body)), LANDED_COLOR)
        }
        LogKind::Crashed {
            point,
            zone: z,
            reason,
            body,
        } => (
            format!(
                "abgestürzt{} {}: {}",
                on(*body),
                zone(*z, point),
                crash_reason(reason)
            ),
            CRASH_COLOR,
        ),
        LogKind::Docked { host } => (
            format!("angedockt an {}", world.vessels[*host].name),
            LANDED_COLOR,
        ),
        LogKind::Undocked { host, fuel } => (
            format!(
                "abgedockt von {} ({fuel:.0} kg Treibstoff)",
                world.vessels[*host].name
            ),
            TEXT_COLOR,
        ),
        LogKind::SoiEntry { body, periapsis } => (
            format!(
                "erreicht die Einflusssphäre von {} (Pe {}{})",
                world.bodies[*body].name,
                km(*periapsis),
                if *periapsis < 0.0 { ", Aufschlag" } else { "" }
            ),
            TEXT_COLOR,
        ),
        LogKind::SoiExit { body } => (
            format!(
                "verlässt die Einflusssphäre von {}",
                world.bodies[*body].name
            ),
            TEXT_COLOR,
        ),
    };
    Line {
        time: entry.time,
        text: format!("{name}: {what}"),
        color,
    }
}

/// Alle Meldungen nach Spielzeit: Protokoll und Hinweise (bei gleicher Zeit
/// das Protokoll zuerst). Nur die Zeilen `range` werden als Text gebaut.
fn lines(world: &World, notes: &UiMessage, range: std::ops::Range<usize>) -> Vec<Line> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    let mut index = 0;
    while i < world.log.len() || j < notes.notes.len() {
        let take_log = j >= notes.notes.len()
            || (i < world.log.len() && world.log[i].time <= notes.notes[j].0);
        if range.contains(&index) {
            out.push(if take_log {
                entry_line(world, &world.log[i])
            } else {
                let (time, text) = &notes.notes[j];
                Line {
                    time: *time,
                    text: text.clone(),
                    color: NOTE_COLOR,
                }
            });
        }
        if take_log {
            i += 1;
        } else {
            j += 1;
        }
        index += 1;
        if index >= range.end {
            break;
        }
    }
    out
}

fn label(ui: &mut egui::Ui, line: &Line, day: f64) {
    let text = format!("{}  {}", format_time(line.time, day), line.text);
    ui.add(egui::Label::new(egui::RichText::new(text).color(line.color)).truncate());
}

/// Zeichnet die Leiste an den unteren Rand von `root`.
pub fn show(root: &mut egui::Ui, world: &World, notes: &UiMessage, bar: &mut StatusBar) {
    let day = hud::day(world);
    let total = world.log.len() + notes.notes.len();
    egui::Panel::bottom("status").show(root, |ui| {
        if bar.open {
            let row = ui.text_style_height(&egui::TextStyle::Body);
            egui::ScrollArea::vertical()
                .max_height(LINES as f32 * (row + ui.spacing().item_spacing.y))
                .auto_shrink(false)
                .stick_to_bottom(true)
                .show_rows(ui, row, total, |ui, range| {
                    for line in lines(world, notes, range) {
                        label(ui, &line, day);
                    }
                });
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(if bar.open { "▼" } else { "▲" }).clicked() {
                    bar.open = !bar.open;
                }
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| match lines(world, notes, total.saturating_sub(1)..total).first() {
                        Some(line) => label(ui, line, day),
                        None => {
                            ui.weak("Noch keine Meldungen.");
                        }
                    },
                );
            });
        });
    });
}
