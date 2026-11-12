//! Fenster "Entwürfe": der Schiffseditor. Links die Liste der Entwürfe der
//! Welt (anlegen als Kopie, löschen), rechts der gewählte Entwurf: Name,
//! Chassis, Triebwerke, Fundament und der Regler, der den freien Innenraum
//! in Tank und Fracht teilt. Darunter stehen die Werte, die daraus folgen,
//! und Warnungen; bestellen lässt sich der Entwurf gleich hier.
//!
//! Der Editor ändert den Entwurf in der Liste direkt (siehe `sim::design`).
//! Schon gebaute Schiffe behalten ihre Kopie. Einstellen lässt sich nur, was
//! sich bauen lässt: Der Regler endet am Innenraum, die Zahl der Triebwerke
//! an den Plätzen, gesperrte Teile sind nicht wählbar. Ob der Entwurf taugt,
//! ist Sache der Warnungen, nicht von Verboten.

use crate::fracht::{tons, tons_fine};
use crate::ui::{Notes, stepper, wide_button};
use bevy::prelude::*;
use bevy_egui::egui;
use sim::design::SPACE_STEP;
use sim::{Chassis, Design, Engine, LaunchEstimate, Part, PlanError, World};

/// Breite der Liste und des Entwurfs daneben.
const LIST_WIDTH: f32 = 170.0;
const EDIT_WIDTH: f32 = 400.0;

/// Start auf dem Booster: ohne Fracht und mit vollem Frachtraum.
type Estimates = [Result<LaunchEstimate, PlanError>; 2];

/// Zustand des Editors (Bedienung, nicht im Spielstand).
#[derive(Resource, Default)]
pub struct Editor {
    /// Der gewählte Entwurf (Stelle in `World::designs`).
    selected: usize,
    /// Die Probeplanung des Starts für den Entwurf, für den sie gerechnet
    /// wurde: neu nur, wenn er sich ändert.
    estimates: Option<(Design, Estimates)>,
}

/// Liste der Entwürfe: wählen, als Kopie anlegen, löschen.
fn list(ui: &mut egui::Ui, world: &mut World, editor: &mut Editor) {
    for (i, design) in world.designs.iter().enumerate() {
        let name = if design.name.is_empty() {
            "(ohne Namen)"
        } else {
            &design.name
        };
        if ui.selectable_label(editor.selected == i, name).clicked() {
            editor.selected = i;
        }
    }
    ui.add_space(6.0);
    if wide_button(ui, "Neu (Kopie)") {
        let copy = match world.designs.get(editor.selected) {
            Some(design) => Design {
                name: format!("{} Kopie", design.name),
                ..design.clone()
            },
            None => Design::freighter(),
        };
        world.designs.push(copy);
        editor.selected = world.designs.len() - 1;
    }
    if editor.selected < world.designs.len() && wide_button(ui, "Löschen") {
        world.designs.remove(editor.selected);
        editor.selected = editor.selected.min(world.designs.len().saturating_sub(1));
    }
}

/// Passt Triebwerke und Tank an, wenn Chassis oder Fundament weniger Platz
/// lassen.
fn fit(design: &mut Design) {
    design.engines = design.engines.min(design.chassis.spec().slots);
    let free = design.free_space().max(0.0);
    design.tank = (design.tank.min(free) / SPACE_STEP).floor() * SPACE_STEP;
}

/// Eine Zeile: Beschriftung in fester Breite, dann die Bedienung.
fn row(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(format!("{label:<12}"));
        contents(ui);
    });
}

/// Auswahl aus dem Katalog; gesperrte Teile sind sichtbar, aber nicht
/// wählbar.
fn choose<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    all: &[T],
    unlocked: &[Part],
    part: impl Fn(T) -> Part,
) {
    for &option in all {
        let part = part(option);
        ui.add_enabled_ui(unlocked.contains(&part), |ui| {
            ui.selectable_value(value, option, part.name())
                .on_disabled_hover_text("noch nicht freigeschaltet");
        });
    }
}

/// Die Bedienung des Entwurfs.
fn edit(ui: &mut egui::Ui, design: &mut Design, unlocked: &[Part]) {
    row(ui, "Name", |ui| {
        ui.text_edit_singleline(&mut design.name);
    });
    row(ui, "Chassis", |ui| {
        choose(
            ui,
            &mut design.chassis,
            &Chassis::ALL,
            unlocked,
            Part::Chassis,
        );
    });
    row(ui, "Triebwerk", |ui| {
        choose(ui, &mut design.engine, &Engine::ALL, unlocked, Part::Engine);
    });
    fit(design);
    let slots = design.chassis.spec().slots;
    row(ui, "Anzahl", |ui| {
        let text = format!("{} von {slots}", design.engines);
        let step = stepper(ui, ["−", "+"], text.into());
        design.engines = design.engines.saturating_add_signed(step).min(slots);
    });
    row(ui, "Einbauten", |ui| {
        ui.checkbox(&mut design.foundation, "Fundament (gründet eine Basis)");
    });
    fit(design);

    // Der freie Innenraum: links Tank, rechts Fracht.
    let free = design.free_space().max(0.0);
    let mut tank = design.tank / 1_000.0;
    ui.add_space(4.0);
    ui.spacing_mut().slider_width = ui.available_width();
    ui.add(
        egui::Slider::new(&mut tank, 0.0..=free / 1_000.0)
            .step_by(SPACE_STEP / 1_000.0)
            .show_value(false),
    )
    .on_hover_text("Teilt den freien Innenraum: links mehr Fracht, rechts mehr Tank");
    design.tank = (tank * 1_000.0 / SPACE_STEP).round() * SPACE_STEP;
    ui.horizontal(|ui| {
        ui.label(format!("Tank {}", tons(design.tank)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(format!("Fracht {}", tons(design.cargo())));
        });
    });
}

/// Was aus dem Entwurf folgt, als Zeilen in festen Spalten.
fn values(world: &World, design: &Design, estimates: &Estimates) -> String {
    let params = design.params();
    let cargo = design.cargo();
    let full = params.dry_mass + params.fuel_capacity + cargo;
    let delta_v = |load: f64| {
        if params.thrust > 0.0 {
            let dry = params.dry_mass + load;
            params.exhaust_velocity * ((dry + params.fuel_capacity) / dry).ln()
        } else {
            0.0
        }
    };
    let home = &world.bodies[world.home];
    let weight = full * home.gm / (home.radius * home.radius);
    let in_orbit = |estimate: &Result<LaunchEstimate, PlanError>| match estimate {
        Ok(estimate) => match estimate.left() {
            Some(left) => format!("{left:.0} m/s"),
            None => "–".into(),
        },
        Err(_) => "–".into(),
    };
    let line = |label: &str, value: String| format!("{label:<22}{value}");
    [
        line("Preis", tons_fine(design.price())),
        line("Leermasse", tons_fine(params.dry_mass)),
        line("Startmasse (voll)", tons_fine(full)),
        line(
            "Schub",
            format!(
                "{:.0} kN, {}× Gewicht",
                params.thrust / 1_000.0,
                format!("{:.1}", params.thrust / weight).replace('.', ",")
            ),
        ),
        line(
            "Δv leer / beladen",
            format!("{:.0} / {:.0} m/s", delta_v(0.0), delta_v(cargo)),
        ),
        line(
            "Im Orbit leer / bel.",
            format!("{} / {}", in_orbit(&estimates[0]), in_orbit(&estimates[1])),
        ),
    ]
    .join("\n")
}

/// Was am Entwurf nicht taugt.
fn warnings(design: &Design, unlocked: &[Part], estimates: &Estimates) -> Vec<String> {
    let mut lines = Vec::new();
    if let Err(error) = design.check(unlocked) {
        lines.push(format!("Lässt sich nicht bauen: {error}."));
    }
    if design.engines == 0 {
        lines.push("Kein Triebwerk: Das Schiff kann nicht manövrieren.".into());
    }
    if design.tank == 0.0 {
        lines.push("Kein Tank.".into());
    }
    // Voll beladen ist der schwerere Fall; nur wenn es leer anders ausgeht,
    // steht es dabei.
    let verdict = |estimate: &Result<LaunchEstimate, PlanError>| match estimate {
        Err(error) => Some(format!("{error}")),
        Ok(estimate) => match estimate.left() {
            None => Some("Der Aufstieg auf dem Booster führt nicht über die Luft".into()),
            Some(left) if left < 0.0 => Some("Reicht nicht bis in die Umlaufbahn".into()),
            Some(_) => None,
        },
    };
    match (verdict(&estimates[0]), verdict(&estimates[1])) {
        (Some(empty), Some(_)) => lines.push(format!("{empty}.")),
        (None, Some(loaded)) => lines.push(format!("Voll beladen: {loaded}.")),
        (Some(empty), None) => lines.push(format!("Ohne Fracht: {empty}.")),
        (None, None) => {}
    }
    lines
}

/// Zeichnet das Fenster; liefert ein eben bestelltes Schiff.
pub fn show(
    ctx: &egui::Context,
    world: &mut World,
    editor: &mut Editor,
    open: &mut bool,
    notes: &mut Notes,
) -> Option<usize> {
    let mut ordered = None;
    egui::Window::new("Entwürfe")
        .open(open)
        .resizable(false)
        .collapsible(false)
        .default_pos([320.0, 60.0])
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(LIST_WIDTH);
                    list(ui, world, editor);
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(EDIT_WIDTH);
                    let Some(mut design) = world.designs.get(editor.selected).cloned() else {
                        ui.weak("Kein Entwurf. Mit \"Neu\" einen anlegen.");
                        return;
                    };
                    let unlocked = world.unlocked.clone();
                    edit(ui, &mut design, &unlocked);
                    world.designs[editor.selected] = design.clone();

                    // Der Name ändert an der Rechnung nichts.
                    let key = Design {
                        name: String::new(),
                        ..design.clone()
                    };
                    if editor.estimates.as_ref().map(|(d, _)| d) != Some(&key) {
                        let estimates = [0.0, key.cargo()].map(|c| world.launch_estimate(&key, c));
                        editor.estimates = Some((key, estimates));
                    }
                    let Some((_, estimates)) = &editor.estimates else {
                        return;
                    };
                    ui.separator();
                    ui.label(values(world, &design, estimates));
                    for line in warnings(&design, &unlocked, estimates) {
                        ui.colored_label(ui.visuals().warn_fg_color, line);
                    }
                    ui.add_space(4.0);
                    let label = format!("Bestellen ({})", tons_fine(design.price()));
                    if wide_button(ui, label) {
                        ordered = notes.check(world.order_ship(editor.selected));
                    }
                });
            });
        });
    ordered
}
