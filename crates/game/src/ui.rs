//! Oberfläche mit egui: ein System zeichnet jedes Bild neu, was zur Lage
//! passt, und führt Klicks sofort aus. Es gibt keinen Aufbau vorab und
//! keinen Abgleich von Texten; eine Karte ist eine Funktion, die die Welt
//! liest, zeichnet und bei einem Klick ändert.
//!
//! Bisher hier: die Seitenleiste rechts mit allem, was man bedienen kann, je
//! nach Lage als Karte. Statusleiste, Δv-Balken und die Anzeige links folgen.
//!
//! Klicks auf die Oberfläche gehen nicht an die Karte dahinter: Die Eingabe
//! der 3D-Ansicht fragt egui (`EguiWantsInput`).

use crate::node_ui::{SelectedZone, Selection, UiMessage};
use crate::rendezvous::Rendezvous;
use crate::status_bar::{self, StatusBar};
use crate::verbund::UndockChoice;
use crate::{Sim, depot, fracht, start_card, target_card, verbund};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use sim::{Segment, World};
use std::fmt::Display;
use std::sync::Arc;

/// Breite der Seitenleiste in Pixeln.
pub const PANEL_WIDTH: f32 = 330.0;
/// Schriftgrößen.
const TEXT_SIZE: f32 = 14.0;
const HEADING_SIZE: f32 = 16.0;

/// Ist die Seitenleiste gerade zu sehen? (Der Δv-Balken weicht ihr aus.)
#[derive(Resource, Default)]
pub struct PanelShown(pub bool);

/// Hinweise an den Spieler aus der Oberfläche (erscheinen in der
/// Statusleiste).
pub struct Notes<'a> {
    message: &'a mut UiMessage,
    /// Spielzeit und Echtzeit in s.
    time: f64,
    real: f64,
}

impl Notes<'_> {
    pub fn say(&mut self, text: String) {
        self.message.say(text, self.time, self.real);
    }

    /// Meldet einen Fehler; liefert den Wert, wenn es keiner war.
    pub fn check<T, E: Display>(&mut self, result: Result<T, E>) -> Option<T> {
        result.map_err(|e| self.say(e.to_string())).ok()
    }
}

/// Farbe von Bevy für egui.
pub fn color32(color: Color) -> egui::Color32 {
    let c = color.to_srgba().to_u8_array();
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Eine Karte der Seitenleiste: Rahmen und Titel.
pub fn card<R>(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.heading(title);
            contents(ui)
        })
        .inner
}

/// Schaltfläche über die ganze Breite; wurde sie geklickt?
pub fn wide_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> bool {
    let size = [ui.available_width(), ui.spacing().interact_size.y];
    ui.add_sized(size, egui::Button::new(label)).clicked()
}

/// Wert zwischen zwei Schaltflächen (◄ ► zum Blättern, − + zum Verstellen):
/// −1, +1 oder 0.
pub fn stepper(ui: &mut egui::Ui, [less, more]: [&str; 2], value: egui::RichText) -> i32 {
    ui.horizontal(|ui| {
        let button = egui::vec2(28.0, ui.spacing().interact_size.y);
        let less = ui.add_sized(button, egui::Button::new(less)).clicked();
        let width = ui.available_width() - button.x - ui.spacing().item_spacing.x;
        ui.add_sized([width.max(0.0), button.y], egui::Label::new(value));
        let more = ui.add_sized(button, egui::Button::new(more)).clicked();
        more as i32 - less as i32
    })
    .inner
}

/// Schrift (Fira Mono, wie die übrige Anzeige: dicktengleich, mit Umlauten
/// und Pfeilen) und Größen.
fn style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Fira Mono".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/FiraMono-Regular.ttf"
        ))),
    );
    for family in fonts.families.values_mut() {
        family.insert(0, "Fira Mono".into());
    }
    ctx.set_fonts(fonts);
    let mut visuals = egui::Visuals::dark();
    // Gut lesbarer Text auf dem dunklen Grund.
    visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_gray(225);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        for (text_style, font) in &mut style.text_styles {
            font.size = match text_style {
                egui::TextStyle::Heading => HEADING_SIZE,
                _ => TEXT_SIZE,
            };
        }
    });
}

/// Das gewählte Schiff, wenn es kein Wrack ist.
fn intact(world: &World, selection: &Selection) -> Option<usize> {
    selection.0.filter(|&i| {
        !matches!(
            world.vessels[i].trajectory.current(),
            Segment::Surface { wreck: true, .. }
        )
    })
}

/// Zeichnet die Oberfläche.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut contexts: EguiContexts,
    mut styled: Local<bool>,
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    zone: Res<SelectedZone>,
    rendezvous: Res<Rendezvous>,
    status: Res<StatusBar>,
    real_time: Res<Time<Real>>,
    mut choice: ResMut<UndockChoice>,
    mut message: ResMut<UiMessage>,
    mut shown: ResMut<PanelShown>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !*styled {
        style(ctx);
        *styled = true;
    }
    let world = &mut sim.0;
    let mut notes = Notes {
        message: &mut message,
        time: world.time,
        real: real_time.elapsed_secs_f64(),
    };
    let vessel = intact(world, &selection);
    let at_depot = depot::selected(world, &zone, &selection);
    shown.0 = vessel.is_some() || at_depot;
    if !shown.0 {
        return Ok(());
    }

    // Die Statusleiste unten gehört noch nicht zu egui: Platz lassen.
    let mut screen = ctx.viewport_rect();
    screen.max.y -= status_bar::height(&status);
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(screen),
    );
    egui::Panel::right("sidebar")
        .resizable(false)
        .exact_size(PANEL_WIDTH)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(vessel) = vessel {
                    start_card::show(ui, world, vessel, &mut notes);
                    target_card::show(ui, world, vessel, &rendezvous, &mut notes);
                    verbund::show(ui, world, vessel, &mut choice, &mut notes);
                    fracht::show(ui, world, vessel, &mut notes);
                }
                if at_depot && let Some(ordered) = depot::show(ui, world, &mut notes) {
                    selection.0 = Some(ordered);
                }
            });
        });
    Ok(())
}
