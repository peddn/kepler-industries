//! Δv-Balken unten: der Tank des gewählten Schiffs als Balken. Die volle
//! Breite ist der volle Tank; gefüllt ist, was an Bord ist. Darin liegen die
//! geplanten Brennvorgänge der Kette (Start gelb, Nodes violett, Bremsen beim
//! Landen türkis, Andocken rosa), dahinter der Rest (grün). Verlangt die Kette mehr, als an Bord ist, steht der Fehlbetrag
//! rot dahinter.
//!
//! Die Zahlen kommen aus `World::delta_v_budget`; hier wird nur gezeichnet.
//! Alle Knoten entstehen beim Start und werden nur ein- und ausgeblendet.

use crate::Sim;
use crate::hud::UiFont;
use crate::node_ui::{SelectedNode, Selection, selected_node};
use crate::panel::{PANEL_WIDTH, PanelShown};
use crate::rendezvous::TARGET_COLOR;
use crate::status_bar::STATUS_HEIGHT;
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::window::PrimaryWindow;
use sim::BudgetItem;

const MARGIN: f32 = 12.0;
const BAR_HEIGHT: f32 = 36.0;
/// Höhe der Zeile über dem Balken.
const CAPTION_HEIGHT: f32 = 22.0;
const FONT_SIZE: f32 = 15.0;
/// Ungefähre Zeichenbreite der dicktengleichen Schrift (für "passt die
/// Beschriftung?").
const CHAR_WIDTH: f32 = 9.0;
/// Höchstzahl der Abschnitte (Start, Nodes, Rest, fehlt) und Teilstriche.
const SEGMENTS: usize = 16;
const TICKS: usize = 40;
/// Abstand der Teilstriche in m/s.
const TICK_STEP: f64 = 500.0;

const EMPTY_COLOR: Color = Color::srgba(0.05, 0.06, 0.09, 0.9);
const LAUNCH_COLOR: Color = Color::srgb(0.95, 0.8, 0.25);
const NODE_COLORS: [Color; 2] = [Color::srgb(0.6, 0.4, 0.95), Color::srgb(0.43, 0.28, 0.75)];
const LANDING_COLOR: Color = Color::srgb(0.2, 0.7, 0.8);
const REST_COLOR: Color = Color::srgb(0.22, 0.62, 0.33);
const MISSING_COLOR: Color = Color::srgb(0.85, 0.22, 0.18);
const SEPARATOR_COLOR: Color = Color::srgb(0.03, 0.03, 0.05);
const SELECTED_COLOR: Color = Color::WHITE;
const TICK_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.45);
const BORDER_COLOR: Color = Color::srgb(0.25, 0.3, 0.4);

/// Ist der Balken gerade zu sehen?
#[derive(Resource, Default)]
pub struct BarShown(pub bool);

/// Liegt der Mauszeiger auf dem Balken (samt Zeile darüber)?
pub fn over_bar(shown: &BarShown, panel: &PanelShown, window: &Window, cursor: Vec2) -> bool {
    let right = window.width() - MARGIN - if panel.0 { PANEL_WIDTH } else { 0.0 };
    shown.0
        && cursor.y >= window.height() - STATUS_HEIGHT - MARGIN - BAR_HEIGHT - CAPTION_HEIGHT
        && cursor.y < window.height() - STATUS_HEIGHT
        && cursor.x >= MARGIN
        && cursor.x <= right
}

#[derive(Component)]
pub struct Bar;

#[derive(Component)]
pub struct BarCaption;

#[derive(Component)]
pub struct BarSegment(usize);

#[derive(Component)]
pub struct BarSegmentText(usize);

#[derive(Component)]
pub struct BarTick(usize);

pub fn setup(mut commands: Commands, font: Res<UiFont>) {
    let text_font = TextFont {
        font: font.0.clone().into(),
        font_size: FontSize::Px(FONT_SIZE),
        ..default()
    };
    commands
        .spawn((
            Bar,
            Node {
                position_type: PositionType::Absolute,
                left: px(MARGIN),
                right: px(MARGIN),
                bottom: px(STATUS_HEIGHT + MARGIN),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|bar| {
            bar.spawn((BarCaption, Text::new(""), text_font.clone()));
            bar.spawn((
                Node {
                    width: percent(100),
                    height: px(BAR_HEIGHT),
                    flex_direction: FlexDirection::Row,
                    border: UiRect::all(px(1)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(EMPTY_COLOR),
                BorderColor::all(BORDER_COLOR),
            ))
            .with_children(|track| {
                for k in 0..SEGMENTS {
                    track
                        .spawn((
                            BarSegment(k),
                            Node {
                                height: percent(100),
                                flex_shrink: 0.0,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                overflow: Overflow::clip(),
                                display: Display::None,
                                ..default()
                            },
                            BackgroundColor(REST_COLOR),
                            BorderColor::all(SEPARATOR_COLOR),
                        ))
                        .with_child((BarSegmentText(k), Text::new(""), text_font.clone()));
                }
                for k in 0..TICKS {
                    track.spawn((
                        BarTick(k),
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: px(0),
                            width: px(1),
                            height: percent(30),
                            display: Display::None,
                            ..default()
                        },
                        BackgroundColor(TICK_COLOR),
                    ));
                }
            });
        });
}

/// Ein Abschnitt des Balkens.
struct Piece {
    delta_v: f64,
    color: Color,
    label: String,
    selected: bool,
}

/// Balken zum gewählten Schiff füllen.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn sync(
    sim: Res<Sim>,
    selection: Res<Selection>,
    selected: Res<SelectedNode>,
    panel: Res<PanelShown>,
    mut shown: ResMut<BarShown>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut bar: Single<&mut Node, With<Bar>>,
    mut caption: Single<&mut Text, With<BarCaption>>,
    mut segments: Query<
        (
            &BarSegment,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        Without<Bar>,
    >,
    mut labels: Query<(&BarSegmentText, &mut Text, &mut TextColor), Without<BarCaption>>,
    mut ticks: Query<(&BarTick, &mut Node), (Without<Bar>, Without<BarSegment>)>,
) {
    let world = &sim.0;
    let vessel = selection.0.filter(|&i| {
        let params = world.params(i);
        params.thrust > 0.0 && params.fuel_capacity > 0.0
    });
    shown.0 = vessel.is_some();
    bar.display = if shown.0 {
        Display::Flex
    } else {
        Display::None
    };
    bar.right = px(MARGIN + if panel.0 { PANEL_WIDTH } else { 0.0 });
    let Some(i) = vessel else { return };

    let budget = world.delta_v_budget(i);
    let chosen = selected_node(world, &selection, &selected).map(|(_, k)| k);
    let mut pieces: Vec<Piece> = budget
        .burns
        .iter()
        .map(|&(item, delta_v)| match item {
            BudgetItem::Launch => Piece {
                delta_v,
                color: LAUNCH_COLOR,
                label: format!("Start {delta_v:.0}"),
                selected: false,
            },
            BudgetItem::Node(k) => Piece {
                delta_v,
                color: NODE_COLORS[k % 2],
                label: format!("{}: {delta_v:.0}", k + 1),
                selected: chosen == Some(k),
            },
            BudgetItem::Landing => Piece {
                delta_v,
                color: LANDING_COLOR,
                label: format!("Landen {delta_v:.0}"),
                selected: false,
            },
            BudgetItem::Docking => Piece {
                delta_v,
                color: TARGET_COLOR,
                label: format!("Andocken {delta_v:.0}"),
                selected: false,
            },
        })
        .collect();
    for (delta_v, color, name) in [
        (budget.rest, REST_COLOR, "Rest"),
        (budget.missing, MISSING_COLOR, "fehlt"),
    ] {
        if delta_v > 0.5 {
            pieces.push(Piece {
                delta_v,
                color,
                label: format!("{name} {delta_v:.0}"),
                selected: false,
            });
        }
    }
    pieces.truncate(SEGMENTS);

    // Volle Breite = voller Tank; fehlt etwas, wird der Balken so lang, dass
    // auch das hineinpasst.
    let scale = budget.full.max(budget.available + budget.missing).max(1e-9);
    let width_px =
        (window.width() - 2.0 * MARGIN - if panel.0 { PANEL_WIDTH } else { 0.0 }).max(1.0);

    let wanted = format!(
        "Δv {:.0} von {:.0} m/s (voller Tank)",
        budget.available, budget.full
    );
    if caption.0 != wanted {
        caption.0 = wanted;
    }
    for (BarSegment(k), mut node, mut background, mut border) in &mut segments {
        let Some(piece) = pieces.get(*k) else {
            node.display = Display::None;
            continue;
        };
        let share = (piece.delta_v / scale) as f32;
        node.display = Display::Flex;
        node.width = percent(100.0 * share);
        background.0 = piece.color;
        if piece.selected {
            node.border = UiRect::all(px(2));
            *border = BorderColor::all(SELECTED_COLOR);
        } else {
            node.border = UiRect::right(px(1));
            *border = BorderColor::all(SEPARATOR_COLOR);
        }
    }
    for (BarSegmentText(k), mut text, mut color) in &mut labels {
        let label = pieces.get(*k).map_or(String::new(), |piece| {
            let fits = piece.label.chars().count() as f32 * CHAR_WIDTH + 10.0
                <= width_px * (piece.delta_v / scale) as f32;
            if fits {
                piece.label.clone()
            } else {
                String::new()
            }
        });
        if text.0 != label {
            text.0 = label;
        }
        color.0 = match pieces.get(*k) {
            Some(piece) if piece.color == LAUNCH_COLOR => Color::BLACK,
            _ => Color::WHITE,
        };
    }
    for (BarTick(k), mut node) in &mut ticks {
        let at = (*k + 1) as f64 * TICK_STEP;
        if at < scale {
            node.display = Display::Flex;
            node.left = percent(100.0 * (at / scale) as f32);
        } else {
            node.display = Display::None;
        }
    }
}
