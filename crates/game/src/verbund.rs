//! Karte "Verbund" in der Seitenleiste: Abdocken eines angedockten Schiffs.
//!
//! Sie erscheint, wenn ein Verbund gewählt ist. `◄ ►` wählt das Teil (bei
//! mehreren), `− +` den Treibstoff, den es mitnimmt (Umschalt: fein),
//! vorgeschlagen ist eine Aufteilung nach Tankgröße. Darunter steht, was beide
//! danach haben. "Abdocken" führt es jetzt aus (`World::undock`).

use crate::Sim;
use crate::hud::{UiFont, km};
use crate::node_ui::{Selection, UiMessage};
use crate::panel::{
    BORDER_COLOR, BUTTON_COLOR, CARD_COLOR, Panel, PanelButton, button_node, physical_cursor, text,
};
use crate::status_bar::{StatusBar, over_status};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::docking::SEPARATION_SPEED;
use sim::{World, kepler};

/// Schritte für den Treibstoff in kg (mit Umschalt: fein).
const STEP: f64 = 10.0;
const FINE_STEP: f64 = 1.0;

/// Was in der Karte eingestellt ist (Bedienung, nicht im Spielstand).
#[derive(Resource, Default)]
pub struct UndockChoice {
    /// Hauptschiff, für das die Wahl gilt.
    host: Option<usize>,
    part: Option<usize>,
    /// Mitgenommener Treibstoff in kg; `None`: der Vorschlag.
    fuel: Option<f64>,
}

#[derive(Component)]
pub struct VerbundCard;

/// Texte der Karte.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum VerbundText {
    Part,
    Fuel,
    Result,
}

/// Schaltflächen der Karte.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum VerbundButton {
    /// Anderes Teil (−1/+1).
    Part(i32),
    /// Treibstoff (−1/+1).
    Fuel(i32),
    Undock,
}

/// Die an das gewählte Schiff angedockten Schiffe (leer: kein Verbund).
fn parts(world: &World, selection: &Selection) -> Option<(usize, Vec<usize>)> {
    let host = selection.0?;
    let parts = world.parts(host);
    (!parts.is_empty()).then_some((host, parts))
}

/// Gewähltes Teil und Treibstoff, passend zum gewählten Verbund.
fn current(world: &World, choice: &mut UndockChoice, host: usize, parts: &[usize]) -> (usize, f64) {
    if choice.host != Some(host) || !choice.part.is_some_and(|p| parts.contains(&p)) {
        *choice = UndockChoice {
            host: Some(host),
            part: Some(parts[0]),
            fuel: None,
        };
    }
    let part = choice.part.expect("gesetzt");
    let range = world.undock_fuel(part).expect("angedockt");
    let fuel = choice
        .fuel
        .map_or(range.suggested, |f| f.clamp(range.min, range.max));
    (part, fuel)
}

pub fn setup(mut commands: Commands, font: Res<UiFont>, panel: Single<Entity, With<Panel>>) {
    let font = font.into_inner();
    let button = |sign: VerbundButton, width: Val| {
        (
            PanelButton,
            sign,
            button_node(width),
            BackgroundColor(BUTTON_COLOR),
            BorderColor::all(BORDER_COLOR),
        )
    };
    let row = || Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(6),
        ..default()
    };
    let grow = || Node {
        flex_grow: 1.0,
        ..default()
    };
    let card = commands
        .spawn((
            VerbundCard,
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
            card.spawn(text(font, "Verbund", 16.0));
            card.spawn(row()).with_children(|row| {
                row.spawn(button(VerbundButton::Part(-1), px(32)))
                    .with_child(text(font, "◄", 14.0));
                row.spawn((
                    VerbundText::Part,
                    text(font, "", 14.0),
                    TextLayout::justify(Justify::Center),
                    grow(),
                ));
                row.spawn(button(VerbundButton::Part(1), px(32)))
                    .with_child(text(font, "►", 14.0));
            });
            card.spawn(text(font, "Treibstoff mitnehmen", 14.0));
            card.spawn(row()).with_children(|row| {
                row.spawn(button(VerbundButton::Fuel(-1), px(32)))
                    .with_child(text(font, "−", 14.0));
                row.spawn((
                    VerbundText::Fuel,
                    text(font, "", 14.0),
                    TextLayout::justify(Justify::Center),
                    grow(),
                ));
                row.spawn(button(VerbundButton::Fuel(1), px(32)))
                    .with_child(text(font, "+", 14.0));
            });
            card.spawn((VerbundText::Result, text(font, "", 14.0)));
            card.spawn(button(VerbundButton::Undock, percent(100)))
                .with_child(text(font, "Abdocken", 14.0));
        })
        .id();
    commands.entity(*panel).add_child(card);
}

/// Karte zeigen oder verstecken und die Texte setzen.
pub fn sync(
    sim: Res<Sim>,
    selection: Res<Selection>,
    mut choice: ResMut<UndockChoice>,
    mut card: Single<&mut Node, With<VerbundCard>>,
    mut texts: Query<(&VerbundText, &mut Text)>,
) {
    let world = &sim.0;
    let Some((host, parts)) = parts(world, &selection) else {
        card.display = Display::None;
        return;
    };
    card.display = Display::Flex;
    let (part, fuel) = current(world, &mut choice, host, &parts);
    let range = world.undock_fuel(part).expect("angedockt");
    let (part_dv, host_dv) = world.undock_delta_v(part, fuel);
    // Ein Stoß gegen die Flugrichtung lässt das Schiff pro Umlauf etwa
    // 3 · Umlaufzeit · Stoß vorauslaufen.
    let (body, state) = world.vessel_state(host);
    let drift =
        kepler::period(world.bodies[body].gm, &state).map(|period| 3.0 * period * SEPARATION_SPEED);
    for (which, mut text) in &mut texts {
        let value = match which {
            VerbundText::Part => world.vessels[part].name.clone(),
            VerbundText::Fuel => format!(
                "{fuel:.0} kg ({:.0}–{:.0})",
                range.min.floor(),
                range.max.floor()
            ),
            VerbundText::Result => [
                format!("{} danach:", world.vessels[part].name),
                format!("  {fuel:.0} kg, Δv {part_dv:.0} m/s"),
                format!("{} danach:", world.vessels[host].name),
                format!("  {:.0} kg, Δv {host_dv:.0} m/s", range.pool - fuel),
                format!("Trennstoß {SEPARATION_SPEED:.0} m/s gegen die Flugrichtung,"),
                drift.map_or(String::new(), |d| {
                    format!("danach etwa {} mehr Abstand pro Umlauf.", km(d))
                }),
            ]
            .join("\n"),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Klicks auf die Schaltflächen der Karte.
#[allow(clippy::too_many_arguments)]
pub fn buttons(
    mut sim: ResMut<Sim>,
    selection: Res<Selection>,
    mut choice: ResMut<UndockChoice>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut message: ResMut<UiMessage>,
    window: Single<&Window, With<PrimaryWindow>>,
    status: Res<StatusBar>,
    buttons: Query<(&VerbundButton, &ComputedNode, &UiGlobalTransform)>,
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
    let (Some((host, parts)), Some(cursor)) = (parts(world, &selection), physical_cursor(&window))
    else {
        return;
    };
    let Some((button, ..)) = buttons
        .iter()
        .find(|(_, node, transform)| node.contains_point(**transform, cursor))
    else {
        return;
    };
    let (part, fuel) = current(world, &mut choice, host, &parts);
    match *button {
        VerbundButton::Part(step) => {
            let k = parts.iter().position(|&p| p == part).unwrap_or(0) as i32;
            choice.part = Some(parts[(k + step).rem_euclid(parts.len() as i32) as usize]);
            choice.fuel = None;
        }
        VerbundButton::Fuel(sign) => {
            let fine = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let step = if fine { FINE_STEP } else { STEP };
            let range = world.undock_fuel(part).expect("angedockt");
            choice.fuel = Some((fuel + sign as f64 * step).clamp(range.min, range.max));
        }
        VerbundButton::Undock => match world.undock(part, fuel) {
            Ok(()) => *choice = UndockChoice::default(),
            Err(e) => message.say(e.to_string(), world.time, real_time.elapsed_secs_f64()),
        },
    }
}
