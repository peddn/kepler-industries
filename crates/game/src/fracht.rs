//! Karte "Fracht & Basis" in der Seitenleiste (siehe `sim::base`).
//!
//! Sie erscheint für Schiffe mit Frachtraum, für Basisschiffe und für jedes
//! Schiff, das im Umkreis einer Basis steht. Sie zeigt Fracht, Baustelle und
//! Treibstofflager; die eine Schaltfläche darunter tut, was gerade dran ist:
//! "Basis gründen", "Bauteile abladen" oder "Tanken".

use crate::Sim;
use crate::hud::UiFont;
use crate::node_ui::{Selection, UiMessage};
use crate::panel::{
    BORDER_COLOR, BUTTON_COLOR, CARD_COLOR, Panel, PanelButton, button_node, physical_cursor, text,
};
use crate::status_bar::{StatusBar, over_status};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::base::{FUEL_RATE, FUEL_STORAGE, SITE_RADIUS};
use sim::{Segment, World};

#[derive(Component)]
pub struct FrachtCard;

/// Texte der Karte: die Zeilen und die Beschriftung der Schaltfläche.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum FrachtText {
    Lines,
    Button,
}

#[derive(Component)]
pub struct FrachtButton;

/// Was die Schaltfläche gerade tut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Found,
    Unload,
    Refuel,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Found => "Basis gründen",
            Action::Unload => "Bauteile abladen",
            Action::Refuel => "Tanken",
        }
    }
}

/// Tonnen mit einer Nachkommastelle.
fn tons(kg: f64) -> String {
    format!("{:.1} t", kg / 1_000.0).replace('.', ",")
}

/// Steht das Schiff heil am Boden?
fn landed(world: &World, vessel: usize) -> bool {
    matches!(
        world.vessels[vessel].trajectory.current(),
        Segment::Surface { wreck: false, .. }
    )
}

/// Was für das Schiff gerade dran ist.
fn action(world: &World, vessel: usize) -> Option<Action> {
    let v = &world.vessels[vessel];
    if !landed(world, vessel) || v.docked_to.is_some() || v.foundation_of.is_some() {
        return None;
    }
    if v.params.base_kit {
        return Some(Action::Found);
    }
    let base = &world.bases[world.base_of(vessel)?];
    if base.fuel.is_some() {
        Some(Action::Refuel)
    } else {
        (!base.complete() && v.cargo > 0.0).then_some(Action::Unload)
    }
}

/// Die Zeilen der Karte; leer: keine Karte.
fn lines(world: &World, vessel: usize) -> Vec<String> {
    let v = &world.vessels[vessel];
    let mut lines = Vec::new();
    if v.params.cargo_capacity > 0.0 {
        lines.push(format!(
            "Fracht: {} von {} Bauteile",
            tons(v.cargo),
            tons(v.params.cargo_capacity)
        ));
    }
    if let Some(base) = v.foundation_of {
        lines.push(format!("Fundament von {}", world.bases[base].name));
    } else if v.params.base_kit {
        lines.push("Basisschiff: wird am Landeplatz zum Fundament einer Basis.".into());
        if !landed(world, vessel) {
            lines.push(format!(
                "Dazu landen (nicht auf {}).",
                world.bodies[world.home].name
            ));
        }
    }
    if !landed(world, vessel) {
        return lines;
    }
    match world.base_of(vessel).map(|k| (k, &world.bases[k])) {
        Some((_, base)) if !base.complete() => lines.push(format!(
            "{} (Baustelle):\n  {} von {} Bauteile",
            base.name,
            tons(base.delivered),
            tons(base.required)
        )),
        Some((k, base)) if base.fuel.is_some() => {
            lines.push(format!(
                "{}:\n  Treibstoff {:.0} von {:.0} kg\n  (+{:.0} kg pro Stunde)",
                base.name,
                world.base_fuel(k).floor(),
                FUEL_STORAGE,
                FUEL_RATE * 3_600.0
            ));
            if v.foundation_of.is_none() {
                lines.push(format!(
                    "Tank: {:.0} von {:.0} kg",
                    world.vessel_fuel(vessel).floor(),
                    world.params(world.host(vessel)).fuel_capacity
                ));
            }
        }
        Some(_) => {}
        None if v.cargo > 0.0 => lines.push(format!(
            "Keine Basis im Umkreis ({:.0} km).",
            SITE_RADIUS / 1_000.0
        )),
        None => {}
    }
    lines
}

pub fn setup(mut commands: Commands, font: Res<UiFont>, panel: Single<Entity, With<Panel>>) {
    let font = font.into_inner();
    let card = commands
        .spawn((
            FrachtCard,
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10)),
                row_gap: px(8),
                border: UiRect::all(px(1)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(CARD_COLOR),
            BorderColor::all(BORDER_COLOR),
        ))
        .with_children(|card| {
            card.spawn(text(font, "Fracht & Basis", 16.0));
            card.spawn((FrachtText::Lines, text(font, "", 14.0)));
            card.spawn((
                PanelButton,
                FrachtButton,
                button_node(percent(100)),
                BackgroundColor(BUTTON_COLOR),
                BorderColor::all(BORDER_COLOR),
            ))
            .with_child((FrachtText::Button, text(font, "", 14.0)));
        })
        .id();
    commands.entity(*panel).add_child(card);
}

/// Karte zeigen oder verstecken und die Texte setzen.
pub fn sync(
    sim: Res<Sim>,
    selection: Res<Selection>,
    mut card: Single<&mut Node, With<FrachtCard>>,
    mut button: Single<&mut Node, (With<FrachtButton>, Without<FrachtCard>)>,
    mut texts: Query<(&FrachtText, &mut Text)>,
) {
    let world = &sim.0;
    let shown = selection
        .0
        .map(|i| (i, lines(world, i)))
        .filter(|(_, lines)| !lines.is_empty());
    let Some((vessel, lines)) = shown else {
        card.display = Display::None;
        return;
    };
    card.display = Display::Flex;
    let action = action(world, vessel);
    button.display = if action.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for (which, mut text) in &mut texts {
        let value = match which {
            FrachtText::Lines => lines.join("\n"),
            FrachtText::Button => action.map_or("", Action::label).into(),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Klick auf die Schaltfläche der Karte.
#[allow(clippy::too_many_arguments)]
pub fn buttons(
    mut sim: ResMut<Sim>,
    selection: Res<Selection>,
    mouse: Res<ButtonInput<MouseButton>>,
    real_time: Res<Time<Real>>,
    mut message: ResMut<UiMessage>,
    window: Single<&Window, With<PrimaryWindow>>,
    status: Res<StatusBar>,
    button: Single<(&ComputedNode, &UiGlobalTransform), With<FrachtButton>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if window
        .cursor_position()
        .is_some_and(|c| over_status(&status, &window, c))
    {
        return;
    }
    let world = &mut sim.0;
    let (Some(vessel), Some(cursor)) = (selection.0, physical_cursor(&window)) else {
        return;
    };
    let Some(action) = action(world, vessel) else {
        return;
    };
    if !button.0.contains_point(*button.1, cursor) {
        return;
    }
    let name = world.vessels[vessel].name.clone();
    let done = match action {
        // Gründen steht schon im Protokoll.
        Action::Found => world.found_base(vessel).map(|_| String::new()),
        Action::Unload => world
            .unload(vessel)
            .map(|kg| format!("{name}: {} Bauteile abgeladen", tons(kg))),
        Action::Refuel => world
            .refuel(vessel)
            .map(|kg| format!("{name}: {kg:.0} kg getankt")),
    };
    let text = done.unwrap_or_else(|e| e.to_string());
    if !text.is_empty() {
        message.say(text, world.time, real_time.elapsed_secs_f64());
    }
}
