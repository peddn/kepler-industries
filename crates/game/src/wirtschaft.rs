//! Fenster "Wirtschaft" mit drei Reitern: die Übersicht über alle Basen
//! (Vorrat, wann das Lager voll ist) und die Fracht an Bord der Schiffe, die
//! Aufgaben, die Daten bringen (siehe `sim::task`), und die Technik, die
//! Daten kostet (siehe `sim::tech`). Reine Anzeige;
//! ein Klick auf eine Zeile wählt die Basis oder das Schiff. Preise und
//! Bestellen stehen in der Karte "Startbasis".

use crate::fracht::{tons, tons_fine};
use crate::hud;
use crate::ui::{Notes, Selected};
use crate::zahl::De;
use bevy_egui::egui;
use sim::body::sphere_of_influence;
use sim::goods::{SAMPLE_DATA_PER_TON, SAMPLE_LIMIT};
use sim::{BaseKind, Goal, Good, Task, Tech, TechError, VesselStatus, World};

/// Vorrat einer Basis und der Stand dazu: bei der Startbasis Zorit und
/// Bauteile im Lager, auf einer Baustelle die Bauteile, sonst der hergestellte Stoff und
/// wann das Lager voll ist.
fn stock(world: &World, k: usize) -> (String, String) {
    let base = &world.bases[k];
    if base.kind == BaseKind::Depot {
        return (crate::depot::stock(world).join(", "), String::new());
    }
    if !base.complete() {
        return (
            format!(
                "Bauteile {} von {}",
                tons(base.delivered),
                tons(base.required)
            ),
            "im Bau".into(),
        );
    }
    let (rate, capacity) = world.production(k);
    let amount = world.base_stock(k);
    let stuff = match base.kind {
        BaseKind::FuelStation => "Treibstoff",
        _ => "Zorit",
    };
    let state = if amount >= capacity {
        "voll".into()
    } else {
        let span = hud::format_span((capacity - amount) / rate, hud::day(world));
        format!("voll in {span}")
    };
    (
        format!("{stuff} {} von {}", tons(amount), tons(capacity)),
        state,
    )
}

/// Wo ein Schiff gerade ist: an welcher Basis, sonst auf oder bei welchem
/// Körper.
fn place(world: &World, i: usize) -> String {
    let body = hud::ship_body(world, i);
    match (world.vessel_status(i), world.base_of(i)) {
        (VesselStatus::Landed { .. }, Some(k)) => world.bases[k].name.clone(),
        (VesselStatus::Landed { .. }, None) => format!("gelandet auf {body}"),
        _ => format!("unterwegs bei {body}"),
    }
}

fn header(ui: &mut egui::Ui, titles: &[&str]) {
    for title in titles {
        ui.weak(*title);
    }
    ui.end_row();
}

fn bases(ui: &mut egui::Ui, world: &World, selected: &mut Selected) {
    ui.heading("Basen");
    egui::Grid::new("bases").striped(true).show(ui, |ui| {
        header(ui, &["Name", "Körper", "Art", "Vorrat", "Stand"]);
        for (k, base) in world.bases.iter().enumerate() {
            let zone = world.zone_at(base.body, &base.location);
            let chosen = zone.is_some() && selected.zone.0 == zone;
            if ui.selectable_label(chosen, &base.name).clicked()
                && let Some(zone) = zone
            {
                selected.select_zone(zone);
            }
            ui.label(&world.bodies[base.body].name);
            ui.label(match base.kind {
                BaseKind::Depot => "Startbasis",
                BaseKind::FuelStation => "Tankstelle",
                BaseKind::Mine => "Mine",
            });
            let (amount, state) = stock(world, k);
            ui.label(amount);
            ui.label(state);
            ui.end_row();
        }
    });
}

fn cargo(ui: &mut egui::Ui, world: &World, selected: &mut Selected) {
    ui.heading("Fracht an Bord");
    let loaded: Vec<usize> = (0..world.vessels.len())
        .filter(|&i| world.vessels[i].cargo.total() > 0.0)
        .collect();
    if loaded.is_empty() {
        ui.weak("Kein Schiff hat Fracht geladen.");
        return;
    }
    egui::Grid::new("cargo").striped(true).show(ui, |ui| {
        header(ui, &["Schiff", "Fracht", "Ort"]);
        for i in loaded {
            let v = &world.vessels[i];
            // Ein angedocktes Schiff wird mit seinem Verbund gewählt.
            let host = world.host(i);
            if ui
                .selectable_label(selected.vessel.0 == Some(host), &v.name)
                .clicked()
            {
                selected.select_vessel(host);
            }
            let goods: Vec<String> = v
                .cargo
                .iter()
                .map(|(good, kg)| format!("{} {}", good.name(&world.bodies), tons(kg)))
                .collect();
            ui.label(goods.join(", "));
            ui.label(place(world, host));
            ui.end_row();
        }
    });
}

/// So hoch ist die Liste der Aufgaben höchstens; darüber scrollt sie.
const TASKS_HEIGHT: f32 = 420.0;

/// Was eine Aufgabe verlangt, in Worten.
fn goal(world: &World, task: &Task) -> String {
    let spec = match task.goal {
        Goal::Landing => return "heil aufsetzen".into(),
        Goal::Base => return "eine Basis fertig bauen".into(),
        Goal::Orbit(spec) => spec,
    };
    // Genannt wird nur, was die Vorgabe einschränkt: Über Luft und Boden
    // und in der Einflusssphäre liegt jede Umlaufbahn.
    let body = &world.bodies[task.body];
    let floor = body.atmosphere.boundary_altitude();
    let ceiling = sphere_of_influence(&world.bodies, task.body) - body.radius;
    let (low, high) = (
        spec.periapsis.0.max(spec.apoapsis.0),
        spec.periapsis.1.min(spec.apoapsis.1),
    );
    let mut parts = Vec::new();
    match (low > floor, high < ceiling) {
        (true, true) => parts.push(format!("Höhe {} bis {}", hud::km(low), hud::km(high))),
        (true, false) => parts.push(format!("Höhe ab {}", hud::km(low))),
        (false, true) => parts.push(format!("Höhe bis {}", hud::km(high))),
        (false, false) => {}
    }
    if spec.inclination != (0.0, 180.0) {
        parts.push(format!(
            "Neigung {:.0}° bis {:.0}°",
            De(spec.inclination.0),
            De(spec.inclination.1)
        ));
    }
    if parts.is_empty() {
        "eine Umlaufbahn".into()
    } else {
        parts.join(", ")
    }
}

/// Die Aufgaben: was sie verlangen und bringen, und welche erfüllt sind.
fn tasks(ui: &mut egui::Ui, world: &World) {
    // Nicht `sum`: Das ergäbe ohne erfüllte Aufgabe -0.
    let earned = world
        .tasks
        .iter()
        .filter(|t| t.done.is_some())
        .fold(0.0, |earned, t| earned + t.data);
    let total: f64 = world.tasks.iter().map(|t| t.data).sum();
    ui.heading(format!(
        "Aufgaben ({earned:.0} von {total:.0} GB)",
        earned = De(earned),
        total = De(total)
    ));
    ui.weak(
        "Eine Umlaufbahn zählt, wenn ein Brennen auf ihr endet; die Karte \"Manöver\" \
         zeigt beim Node vorab, was er erfüllt.",
    );
    // Die Liste wird lang: Sie scrollt in ihrer eigenen Fläche.
    egui::ScrollArea::vertical()
        .max_height(TASKS_HEIGHT)
        .min_scrolled_height(TASKS_HEIGHT)
        .show(ui, |ui| {
            egui::Grid::new("tasks").striped(true).show(ui, |ui| {
                header(ui, &["Körper", "Aufgabe", "Vorgabe", "Daten", "Stand"]);
                for task in &world.tasks {
                    ui.label(&world.bodies[task.body].name);
                    ui.label(&task.title);
                    ui.label(goal(world, task));
                    ui.label(format!("{:.0} GB", De(task.data)));
                    match task.done {
                        Some(time) => ui.label(format!(
                            "erfüllt ({})",
                            hud::format_time(time, hud::day(world))
                        )),
                        None => ui.weak("offen"),
                    };
                    ui.end_row();
                }
            });
        });
    samples(ui, world);
}

/// Proben: was an der Startbasis abgegeben ist und was es gebracht hat, je
/// Körper bis zur Obergrenze.
fn samples(ui: &mut egui::Ui, world: &World) {
    let bodies: Vec<usize> = (0..world.bodies.len())
        .filter(|&body| body != world.home && world.bodies[body].data > 0.0)
        .collect();
    let per_ton = |body: usize| SAMPLE_DATA_PER_TON * world.bodies[body].data;
    let full = |body: usize| per_ton(body) * SAMPLE_LIMIT / 1_000.0;
    let earned: f64 = bodies.iter().map(|&body| world.sample_data(body)).sum();
    let total: f64 = bodies.iter().map(|&body| full(body)).sum();
    ui.add_space(6.0);
    ui.heading(format!(
        "Proben ({:.0} von {:.0} GB)",
        De(earned),
        De(total)
    ));
    ui.weak(
        "Ein Schiff mit Probenbohrer nimmt sie vom Boden; an der Startbasis abgeladen, \
         bringen sie Daten, bis zur Obergrenze je Körper.",
    );
    egui::Grid::new("samples").striped(true).show(ui, |ui| {
        header(ui, &["Körper", "Daten je Tonne", "abgegeben", "Daten"]);
        for body in bodies {
            let delivered = world.in_depot(Good::Sample(body));
            ui.label(&world.bodies[body].name);
            ui.label(format!("{:.0} GB", De(per_ton(body))));
            ui.label(format!(
                "{} von {}",
                tons(delivered.min(SAMPLE_LIMIT)),
                tons(SAMPLE_LIMIT)
            ));
            ui.label(format!(
                "{:.0} von {:.0} GB",
                De(world.sample_data(body)),
                De(full(body))
            ));
            ui.end_row();
        }
    });
}

/// Was eine Technologie bringt, in Worten.
fn effect(tech: Tech) -> &'static str {
    match tech {
        Tech::Sampler => "Modul: nimmt Proben vom Boden",
        Tech::EngineZ2 => "3600 m/s Ausströmgeschw. (Z-1: 3200)",
        Tech::EngineZ3 => "4000 m/s Ausströmgeschwindigkeit",
        Tech::ChassisLarge => "32 t Innenraum, 4 Triebwerksplätze",
        Tech::BoosterB2 => "neuer Booster: trägt 38 t statt 19 t",
    }
}

/// Der Techtree als Liste: was es gibt, was es bringt und kostet, und der
/// Knopf zum Freischalten (oder warum es gerade nicht geht).
fn techs(ui: &mut egui::Ui, world: &mut World, notes: &mut Notes) {
    ui.heading(format!("Technik (Daten: {:.0} GB)", De(world.data)));
    ui.weak("Freischalten kostet Daten und gilt sofort und für immer.");
    egui::Grid::new("techs").striped(true).show(ui, |ui| {
        header(ui, &["Technologie", "bringt", "Preis", ""]);
        for tech in Tech::ALL {
            let spec = tech.spec();
            ui.label(spec.name);
            ui.label(effect(tech));
            ui.label(if spec.zorit > 0.0 {
                format!("{:.0} GB + {} Zorit", De(spec.data), tons_fine(spec.zorit))
            } else {
                format!("{:.0} GB", De(spec.data))
            });
            match world.can_unlock(tech) {
                Ok(()) => {
                    if ui.button("Freischalten").clicked()
                        && notes.check(world.unlock(tech)).is_some()
                    {
                        notes.say(format!("Freigeschaltet: {}", spec.name));
                    }
                }
                Err(TechError::Done) => {
                    ui.label("freigeschaltet");
                }
                Err(reason) => {
                    ui.weak(reason.to_string());
                }
            }
            ui.end_row();
        }
    });
}

/// Die Reiter des Fensters.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// Basen und Fracht an Bord.
    #[default]
    Overview,
    /// Was Daten bringt.
    Tasks,
    /// Was Daten kostet.
    Tech,
}

pub fn show(
    ctx: &egui::Context,
    world: &mut World,
    selected: &mut Selected,
    open: &mut bool,
    tab: &mut Tab,
    notes: &mut Notes,
) {
    egui::Window::new("Wirtschaft")
        .open(open)
        .resizable(false)
        .collapsible(false)
        .default_pos(crate::ui::window_pos(2))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(tab, Tab::Overview, "Übersicht");
                ui.selectable_value(tab, Tab::Tasks, "Aufgaben");
                ui.selectable_value(tab, Tab::Tech, "Technik");
            });
            ui.separator();
            match tab {
                Tab::Overview => {
                    bases(ui, world, selected);
                    ui.add_space(8.0);
                    cargo(ui, world, selected);
                }
                Tab::Tasks => tasks(ui, world),
                Tab::Tech => techs(ui, world, notes),
            }
        });
}
