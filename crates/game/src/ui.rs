//! Oberfläche mit egui: ein System zeichnet jedes Bild neu, was zur Lage
//! passt, und führt Klicks sofort aus. Es gibt keinen Aufbau vorab und
//! keinen Abgleich von Texten; eine Karte ist eine Funktion, die die Welt
//! liest, zeichnet und bei einem Klick ändert.
//!
//! Aufbau des Bildes: oben eine Leiste mit Zeit, Zeitraffer, Kamera und den
//! Schaltern für die Bereiche; unten die Statusleiste mit dem Protokoll; links
//! die Leiste mit reiner Information (Schiffe, Auswahl); rechts die
//! Seitenleiste mit allem, was man bedienen kann (je nach Lage als Karte);
//! unten über die ganze Breite der Δv-Balken. Die Seitenleisten sind
//! durchscheinend (Regler an ihrem unteren Ende). Über der Karte selbst stehen nur
//! Warnungen zum gewählten Schiff und die Beschriftungen an ihren Punkten
//! (`WorldLabels`).
//!
//! Klicks auf die Oberfläche gehen nicht an die Karte dahinter: Die Eingabe
//! der 3D-Ansicht fragt egui (`EguiWantsInput`).

use crate::camera::CameraRig;
use crate::node_ui::{
    SelectedBody, SelectedNode, SelectedZone, Selection, UiMessage, selected_node,
};
use crate::rendezvous::Rendezvous;
use crate::status_bar::{self, StatusBar};
use crate::top_bar::{self, Panels};
use crate::verbund::UndockChoice;
use crate::{
    Sim, TimeWarp, depot, dv_bar, fracht, hud, left_bar, maneuver_card, start_card, target_card,
    verbund,
};
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

/// Beschriftungen an Punkten der Karte (Bildschirmkoordinaten) für das
/// nächste Bild: Die 3D-Ansicht trägt sie ein, `draw` zeichnet und leert sie.
#[derive(Resource, Default)]
pub struct WorldLabels(Vec<(Vec2, String, Color)>);

impl WorldLabels {
    pub fn add(&mut self, at: Vec2, text: String, color: Color) {
        self.0.push((at, text, color));
    }
}

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

/// Was gerade gewählt ist.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Selected<'w> {
    pub vessel: ResMut<'w, Selection>,
    pub node: ResMut<'w, SelectedNode>,
    pub zone: ResMut<'w, SelectedZone>,
    pub body: ResMut<'w, SelectedBody>,
}

impl Selected<'_> {
    /// Wählt ein Schiff (und nichts sonst).
    pub fn select_vessel(&mut self, vessel: usize) {
        self.vessel.0 = Some(vessel);
        self.node.0 = None;
        self.zone.0 = None;
        self.body.0 = None;
    }
}

/// Zeichnet die Oberfläche.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut contexts: EguiContexts,
    mut styled: Local<bool>,
    mut sim: ResMut<Sim>,
    mut selected: Selected,
    rendezvous: Res<Rendezvous>,
    warp: Res<TimeWarp>,
    rig: Res<CameraRig>,
    real_time: Res<Time<Real>>,
    mut status: ResMut<StatusBar>,
    mut choice: ResMut<UndockChoice>,
    mut message: ResMut<UiMessage>,
    mut labels: ResMut<WorldLabels>,
    mut panels: ResMut<Panels>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !*styled {
        style(ctx);
        *styled = true;
    }
    let world = &mut sim.0;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    top_bar::show(&mut root, world, &warp, &rig, &mut panels);
    status_bar::show(&mut root, world, &message, &mut status);
    // Der Δv-Balken geht über die ganze Breite, die Seitenleisten stehen
    // darüber.
    if let Some(vessel) = selected.vessel.0 {
        let chosen = selected_node(world, &selected.vessel, &selected.node).map(|(_, k)| k);
        dv_bar::show(&mut root, world, vessel, chosen);
    }
    left_bar::show(&mut root, world, &mut selected, &mut panels);

    let mut notes = Notes {
        message: &mut message,
        time: world.time,
        real: real_time.elapsed_secs_f64(),
    };
    let vessel = intact(world, &selected.vessel);
    let at_depot = depot::selected(world, &selected.zone, &selected.vessel);
    if vessel.is_some() || at_depot {
        egui::Panel::right("sidebar")
            .resizable(false)
            .exact_size(PANEL_WIDTH)
            .frame(panels.right.frame(root.style()))
            .show(&mut root, |ui| {
                panels.right.slider(ui, "right_opacity");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(vessel) = vessel {
                        start_card::show(ui, world, vessel, &mut notes);
                        let chosen = selected_node(world, &selected.vessel, &selected.node);
                        let chosen = chosen.map(|(_, k)| k);
                        if let Some(time) = maneuver_card::show(ui, world, vessel, chosen) {
                            selected.node.0 = Some(time);
                        }
                        target_card::show(ui, world, vessel, &rendezvous, &mut notes);
                        verbund::show(ui, world, vessel, &mut choice, &mut notes);
                        fracht::show(ui, world, vessel, &mut notes);
                    }
                    if at_depot && let Some(ordered) = depot::show(ui, world, &mut notes) {
                        selected.vessel.0 = Some(ordered);
                    }
                });
            });
    }
    // Was die Leisten frei lassen, gehört der Karte: Warnungen und
    // Beschriftungen werden darauf begrenzt gezeichnet.
    let free = root.available_rect_before_wrap();
    let painter = root.painter_at(free);
    let font = egui::FontId::monospace(TEXT_SIZE);
    if let Some(vessel) = selected.vessel.0 {
        hud::show_warnings(&painter, free, &font, hud::warnings(world, vessel));
    }
    for (at, text, color) in labels.0.drain(..) {
        painter.text(
            egui::pos2(at.x, at.y),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            color32(color),
        );
    }
    Ok(())
}
