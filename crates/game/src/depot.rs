//! Karte "Startbasis" in der Seitenleiste: erscheint, wenn ein Schiff gewählt
//! ist, das an der Startbasis steht, oder ihr Zielgebiet (Klick auf seine
//! Raute; das geht nur, wenn dort kein Schiff steht, denn Schiffe haben beim
//! Klicken Vorrang). Sie zeigt das Zorit im Lager und bestellt Schiffe (siehe
//! `sim::base`).

use crate::Sim;
use crate::fracht::{price, tons_fine};
use crate::hud::UiFont;
use crate::node_ui::{SelectedZone, Selection, UiMessage};
use crate::panel::{
    BORDER_COLOR, BUTTON_COLOR, CARD_COLOR, Panel, PanelButton, button_node, physical_cursor, text,
};
use crate::status_bar::{StatusBar, over_status};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::base::PARTS_PRICE;
use sim::{BaseKind, ShipKind, World};

#[derive(Component)]
pub struct DepotCard;

/// Texte der Karte: die Zeilen und die Beschriftungen der Schaltflächen.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum DepotText {
    Lines,
    Button(ShipKind),
}

/// Schaltfläche: bestellt ein Schiff dieser Art.
#[derive(Component)]
pub struct DepotButton(ShipKind);

const KINDS: [ShipKind; 2] = [ShipKind::BaseShip, ShipKind::Freighter];

/// Ist die Startbasis gewählt: ihr Zielgebiet oder ein Schiff, das dort
/// steht?
fn depot_selected(world: &World, zone: &SelectedZone, selection: &Selection) -> bool {
    let by_zone = zone
        .0
        .and_then(|k| world.zones.get(k))
        .and_then(|z| world.base_at(z.body, &z.center));
    let by_vessel = selection.0.and_then(|i| world.base_of(i));
    by_zone
        .or(by_vessel)
        .is_some_and(|base| world.bases[base].kind == BaseKind::Depot)
}

pub fn setup(mut commands: Commands, font: Res<UiFont>, panel: Single<Entity, With<Panel>>) {
    let font = font.into_inner();
    let card = commands
        .spawn((
            DepotCard,
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
            card.spawn(text(font, "Startbasis", 16.0));
            card.spawn((DepotText::Lines, text(font, "", 14.0)));
            for kind in KINDS {
                card.spawn((
                    PanelButton,
                    DepotButton(kind),
                    button_node(percent(100)),
                    BackgroundColor(BUTTON_COLOR),
                    BorderColor::all(BORDER_COLOR),
                ))
                .with_child((DepotText::Button(kind), text(font, "", 14.0)));
            }
        })
        .id();
    commands.entity(*panel).add_child(card);
}

/// Karte zeigen oder verstecken und die Texte setzen.
pub fn sync(
    sim: Res<Sim>,
    zone: Res<SelectedZone>,
    selection: Res<Selection>,
    mut card: Single<&mut Node, With<DepotCard>>,
    mut texts: Query<(&DepotText, &mut Text)>,
) {
    let world = &sim.0;
    if !depot_selected(world, &zone, &selection) {
        card.display = Display::None;
        return;
    }
    card.display = Display::Flex;
    for (which, mut text) in &mut texts {
        let value = match which {
            DepotText::Lines => {
                let parts = if world.mine_ready() {
                    format!("Bauteile: {} je Tonne", tons_fine(PARTS_PRICE * 1_000.0))
                } else {
                    "Bauteile frei (noch keine Mine)".into()
                };
                format!(
                    "Zorit im Lager: {}\nPreise in Zorit:\n{parts}",
                    tons_fine(world.zorit)
                )
            }
            DepotText::Button(kind) => format!(
                "{} bestellen ({})",
                kind.label(),
                price(world.ship_price(*kind))
            ),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Klick auf eine Schaltfläche der Karte: Schiff bestellen und auswählen.
#[allow(clippy::too_many_arguments)]
pub fn buttons(
    mut sim: ResMut<Sim>,
    zone: Res<SelectedZone>,
    mut selection: ResMut<Selection>,
    mouse: Res<ButtonInput<MouseButton>>,
    real_time: Res<Time<Real>>,
    mut message: ResMut<UiMessage>,
    window: Single<&Window, With<PrimaryWindow>>,
    status: Res<StatusBar>,
    buttons: Query<(&DepotButton, &ComputedNode, &UiGlobalTransform)>,
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
    let Some(cursor) = physical_cursor(&window) else {
        return;
    };
    if !depot_selected(world, &zone, &selection) {
        return;
    }
    let Some(kind) = buttons
        .iter()
        .find(|(_, node, transform)| node.contains_point(**transform, cursor))
        .map(|(button, ..)| button.0)
    else {
        return;
    };
    match world.order_ship(kind) {
        // Die Bestellung steht im Protokoll.
        Ok(vessel) => selection.0 = Some(vessel),
        Err(e) => message.say(e.to_string(), world.time, real_time.elapsed_secs_f64()),
    }
}
