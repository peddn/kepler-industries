//! Karte "Booster" der Seitenleiste: wo der Booster der Startbasis gerade
//! ist. Er lässt sich nicht steuern (siehe `sim::booster`), deshalb gibt es
//! für ihn keine der Karten eines Schiffs.

use crate::hud::{self, format_span};
use crate::ui::card;
use bevy_egui::egui;
use sim::{BoosterStatus, World};

/// Wo der Booster ist, in einem Satz. `None` ohne Booster.
pub fn status(world: &World) -> Option<String> {
    let booster = world.booster?;
    let span = |until: f64| format_span((until - world.time).max(0.0), hud::day(world));
    Some(match world.booster_status()? {
        BoosterStatus::Ready => "Steht an der Startbasis bereit.".into(),
        BoosterStatus::Reserved { ship } => {
            format!("Vorgesehen für den Start von {}.", world.vessels[ship].name)
        }
        BoosterStatus::Ascending { ship } => {
            let end = world.vessels[ship]
                .trajectory
                .launch()
                .map_or(world.time, |plan| plan.burn.end);
            format!(
                "Trägt {} nach oben, Trennung in {}.",
                world.vessels[ship].name,
                span(end)
            )
        }
        BoosterStatus::Returning => match world.touchdown(booster) {
            Some(touchdown) => format!("Fliegt zurück, Landung in {}.", span(touchdown.time)),
            None => "Nicht an der Startbasis.".into(),
        },
    })
}

pub fn show(ui: &mut egui::Ui, world: &World) {
    let Some(status) = status(world) else { return };
    card(ui, "Booster", |ui| {
        ui.label(status);
        ui.weak(
            "Jeder Start von der Startbasis läuft mit ihm. Nach der Trennung fliegt er \
             selbst zurück und landet mit einem Landebrennen; steuern lässt er sich nicht.",
        );
    });
}
