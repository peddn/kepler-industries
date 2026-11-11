//! Karte "Fracht & Basis" in der Seitenleiste (siehe `sim::base`).
//!
//! Sie erscheint für Schiffe mit Frachtraum, für Basisschiffe und für jedes
//! Schiff, das im Umkreis einer Basis steht. Sie zeigt Fracht, Baustelle und
//! Lager; die Schaltflächen darunter tun, was dort gerade geht: an der
//! Startbasis laden, tanken und Zorit abladen, mit dem Basisschiff gründen,
//! an der Baustelle abladen, an der Tankstelle tanken, an der Mine laden.

use crate::Sim;
use crate::hud::UiFont;
use crate::node_ui::{Selection, UiMessage};
use crate::panel::{
    BORDER_COLOR, BUTTON_COLOR, CARD_COLOR, Panel, PanelButton, button_node, physical_cursor, text,
};
use crate::status_bar::{StatusBar, over_status};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::base::SITE_RADIUS;
use sim::{BaseKind, Segment, World};

#[derive(Component)]
pub struct FrachtCard;

/// Texte der Karte: die Zeilen und die Beschriftungen der Schaltflächen.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum FrachtText {
    Lines,
    Button(usize),
}

/// Schaltfläche der Karte mit ihrer Nummer; sie tut, was `actions` an
/// dieser Stelle nennt.
#[derive(Component)]
pub struct FrachtButton(usize);

/// So viele Schaltflächen hat die Karte höchstens.
const BUTTONS: usize = 3;

/// Was eine Schaltfläche tut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    FoundMine,
    FoundStation,
    Unload,
    Refuel,
    LoadZorit,
    LoadParts,
    DeliverZorit,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::FoundMine => "Mine gründen",
            Action::FoundStation => "Tankstelle gründen",
            Action::Unload => "Bauteile abladen",
            Action::Refuel => "Tanken",
            Action::LoadZorit => "Zorit laden",
            Action::LoadParts => "Bauteile laden",
            Action::DeliverZorit => "Zorit abladen",
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

/// Was für das Schiff gerade geht (höchstens `BUTTONS`).
fn actions(world: &World, vessel: usize) -> Vec<Action> {
    let v = &world.vessels[vessel];
    if !landed(world, vessel) || v.docked_to.is_some() || v.foundation_of.is_some() {
        return Vec::new();
    }
    let base = world.base_of(vessel).map(|k| &world.bases[k]);
    let mut actions = Vec::new();
    match base {
        None if v.params.base_kit => {
            actions.extend([Action::FoundMine, Action::FoundStation]);
        }
        None => {}
        Some(base) if !base.complete() => {
            if v.cargo.parts > 0.0 {
                actions.push(Action::Unload);
            }
        }
        Some(base) => match base.kind {
            BaseKind::Depot => {
                if v.cargo.zorit > 0.0 {
                    actions.push(Action::DeliverZorit);
                }
                if v.cargo.total() < v.params.cargo_capacity {
                    actions.push(Action::LoadParts);
                }
                if world.vessel_fuel(vessel) < v.params.fuel_capacity {
                    actions.push(Action::Refuel);
                }
            }
            BaseKind::FuelStation => actions.push(Action::Refuel),
            BaseKind::Mine => {
                if v.params.cargo_capacity > 0.0 {
                    actions.push(Action::LoadZorit);
                }
            }
        },
    }
    actions
}

/// Die Zeilen der Karte; leer: keine Karte.
fn lines(world: &World, vessel: usize) -> Vec<String> {
    let v = &world.vessels[vessel];
    let mut lines = Vec::new();
    if v.params.cargo_capacity > 0.0 {
        lines.push(format!(
            "Fracht: {} von {}",
            tons(v.cargo.total()),
            tons(v.params.cargo_capacity)
        ));
        if v.cargo.parts > 0.0 {
            lines.push(format!("  Bauteile {}", tons(v.cargo.parts)));
        }
        if v.cargo.zorit > 0.0 {
            lines.push(format!("  Zorit {}", tons(v.cargo.zorit)));
        }
    }
    if let Some(base) = v.foundation_of {
        lines.push(format!("Fundament von {}", world.bases[base].name));
    } else if v.params.base_kit {
        lines.push(
            "Basisschiff: wird am Landeplatz zum Fundament einer Mine oder Tankstelle.".into(),
        );
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
    let tank = || {
        format!(
            "Tank: {:.0} von {:.0} kg",
            world.vessel_fuel(vessel).floor(),
            world.params(world.host(vessel)).fuel_capacity
        )
    };
    match world.base_of(vessel).map(|k| (k, &world.bases[k])) {
        Some((_, base)) if !base.complete() => lines.push(format!(
            "{} (Baustelle):\n  {} von {} Bauteile",
            base.name,
            tons(base.delivered),
            tons(base.required)
        )),
        Some((k, base)) => {
            let (rate, capacity) = world.production(k);
            let stock = world.base_stock(k).floor();
            let per_hour = rate * 3_600.0;
            match base.kind {
                BaseKind::Depot => {
                    lines.push(format!(
                        "{}:\n  Zorit im Lager {}\n  Bauteile und Treibstoff frei",
                        base.name,
                        tons(world.zorit)
                    ));
                    lines.push(tank());
                }
                BaseKind::FuelStation => {
                    lines.push(format!(
                        "{}:\n  Treibstoff {stock:.0} von {capacity:.0} kg\n  (+{per_hour:.0} kg pro Stunde)",
                        base.name
                    ));
                    if v.foundation_of.is_none() {
                        lines.push(tank());
                    }
                }
                BaseKind::Mine => lines.push(format!(
                    "{}:\n  Zorit {stock:.0} von {capacity:.0} kg\n  (+{per_hour:.0} kg pro Stunde)",
                    base.name
                )),
            }
        }
        None if v.cargo.total() > 0.0 => lines.push(format!(
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
            for k in 0..BUTTONS {
                card.spawn((
                    PanelButton,
                    FrachtButton(k),
                    button_node(percent(100)),
                    BackgroundColor(BUTTON_COLOR),
                    BorderColor::all(BORDER_COLOR),
                ))
                .with_child((FrachtText::Button(k), text(font, "", 14.0)));
            }
        })
        .id();
    commands.entity(*panel).add_child(card);
}

/// Karte zeigen oder verstecken und die Texte setzen.
pub fn sync(
    sim: Res<Sim>,
    selection: Res<Selection>,
    mut card: Single<&mut Node, With<FrachtCard>>,
    mut buttons: Query<(&FrachtButton, &mut Node), Without<FrachtCard>>,
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
    let actions = actions(world, vessel);
    for (button, mut node) in &mut buttons {
        node.display = if button.0 < actions.len() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (which, mut text) in &mut texts {
        let value = match which {
            FrachtText::Lines => lines.join("\n"),
            FrachtText::Button(k) => actions.get(*k).map_or("", |a| a.label()).into(),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Klick auf eine Schaltfläche der Karte.
#[allow(clippy::too_many_arguments)]
pub fn buttons(
    mut sim: ResMut<Sim>,
    selection: Res<Selection>,
    mouse: Res<ButtonInput<MouseButton>>,
    real_time: Res<Time<Real>>,
    mut message: ResMut<UiMessage>,
    window: Single<&Window, With<PrimaryWindow>>,
    status: Res<StatusBar>,
    buttons: Query<(&FrachtButton, &ComputedNode, &UiGlobalTransform)>,
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
    let actions = actions(world, vessel);
    let Some(action) = buttons
        .iter()
        .find(|(_, node, transform)| node.contains_point(**transform, cursor))
        .and_then(|(button, ..)| actions.get(button.0).copied())
    else {
        return;
    };
    let name = world.vessels[vessel].name.clone();
    let done = match action {
        // Gründen steht schon im Protokoll.
        Action::FoundMine => world
            .found_base(vessel, BaseKind::Mine)
            .map(|_| String::new()),
        Action::FoundStation => world
            .found_base(vessel, BaseKind::FuelStation)
            .map(|_| String::new()),
        Action::Unload => world
            .unload(vessel)
            .map(|kg| format!("{name}: {} Bauteile abgeladen", tons(kg))),
        Action::Refuel => world
            .refuel(vessel)
            .map(|kg| format!("{name}: {kg:.0} kg getankt")),
        Action::LoadZorit => world
            .load_zorit(vessel)
            .map(|kg| format!("{name}: {} Zorit geladen", tons(kg))),
        Action::LoadParts => world
            .load_parts(vessel)
            .map(|kg| format!("{name}: {} Bauteile geladen", tons(kg))),
        Action::DeliverZorit => world.deliver_zorit(vessel).map(|kg| {
            format!(
                "{name}: {} Zorit abgeladen, im Lager {}",
                tons(kg),
                tons(world.zorit)
            )
        }),
    };
    let text = done.unwrap_or_else(|e| e.to_string());
    if !text.is_empty() {
        message.say(text, world.time, real_time.elapsed_secs_f64());
    }
}
