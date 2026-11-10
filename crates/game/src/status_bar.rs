//! Statusleiste ganz unten: die letzte Meldung mit Spielzeit. Ein Klick auf ▲
//! rechts klappt darüber einen Bereich mit den letzten Meldungen auf, mit
//! Mausrad und ziehbarer Scrollleiste; ▼ klappt ihn wieder zu.
//!
//! Die Meldungen sind das Protokoll der Simulation (`World::log`, gehört zum
//! Spielstand) und, gelb, die Hinweise der Bedienung (`UiMessage`, nicht
//! gespeichert), zusammen nach Spielzeit geordnet. Wer ganz unten steht, sieht
//! neue Meldungen sofort; wer hochgescrollt hat, bleibt an seiner Stelle.

use crate::Sim;
use crate::hud::{UiFont, format_time, km, place};
use crate::node_ui::UiMessage;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::window::PrimaryWindow;
use sim::{Crash, LogEntry, LogKind, World};

/// Höhe der Leiste in Pixeln.
pub const STATUS_HEIGHT: f32 = 26.0;
/// Sichtbare Zeilen im aufgeklappten Bereich.
const LINES: usize = 12;
const LINE_HEIGHT: f32 = 18.0;
const PADDING: f32 = 8.0;
/// Höhe des aufgeklappten Bereichs.
const AREA_HEIGHT: f32 = LINES as f32 * LINE_HEIGHT + 2.0 * PADDING;
/// Breite des Knopfs ▲/▼ und der Scrollleiste.
const ICON_WIDTH: f32 = 36.0;
const SCROLLBAR_WIDTH: f32 = 10.0;
const FONT_SIZE: f32 = 14.0;

const BAR_COLOR: Color = Color::srgb(0.03, 0.04, 0.07);
const BORDER_COLOR: Color = Color::srgb(0.25, 0.3, 0.4);
const TRACK_COLOR: Color = Color::srgb(0.1, 0.12, 0.17);
const THUMB_COLOR: Color = Color::srgb(0.4, 0.46, 0.6);
const TEXT_COLOR: Color = Color::srgb(0.9, 0.92, 0.95);
const NOTE_COLOR: Color = Color::srgb(1.0, 0.85, 0.35);
const CRASH_COLOR: Color = Color::srgb(1.0, 0.4, 0.35);
const LANDED_COLOR: Color = Color::srgb(0.45, 1.0, 0.55);
const ATMOSPHERE_COLOR: Color = Color::srgb(1.0, 0.7, 0.4);
const HINT_COLOR: Color = Color::srgb(0.6, 0.65, 0.75);

/// Zustand der Statusleiste.
#[derive(Resource, Default)]
pub struct StatusBar {
    /// Bereich aufgeklappt?
    pub open: bool,
    /// Zeilen über der untersten (0 = ganz unten, neue Meldungen erscheinen).
    scroll: usize,
    /// Zahl der Meldungen im letzten Bild (um beim Hochscrollen die Stelle zu
    /// halten).
    total: usize,
    /// Griff der Scrollleiste wird gezogen: Abstand der Maus zur Griffoberkante.
    grab: Option<f32>,
}

/// Liegt der Mauszeiger auf der Leiste oder dem aufgeklappten Bereich?
pub fn over_status(bar: &StatusBar, window: &Window, cursor: Vec2) -> bool {
    let height = STATUS_HEIGHT + if bar.open { AREA_HEIGHT } else { 0.0 };
    cursor.y >= window.height() - height
}

/// Eine Zeile der Anzeige.
struct Line {
    time: f64,
    text: String,
    color: Color,
}

/// Grund eines Absturzes als Text.
fn crash_reason(reason: &Crash) -> String {
    match reason {
        Crash::NoFuel { available } => format!("Δv an Bord nur {available:.0} m/s"),
        Crash::TooWeak { thrust_to_weight } => {
            format!("Schub zu schwach zum Bremsen (Schub/Gewicht {thrust_to_weight:.2})")
        }
    }
}

/// Text und Farbe zu einem Eintrag des Protokolls.
fn entry_line(world: &World, entry: &LogEntry) -> Line {
    let name = &world.vessels[entry.vessel].name;
    let zone = |zone: Option<usize>, point| match zone {
        Some(k) => format!("im Zielgebiet {}", world.zones[k].name),
        None => format!("bei {}", place(point)),
    };
    let (what, color) = match &entry.kind {
        LogKind::Liftoff => ("Start".into(), TEXT_COLOR),
        LogKind::BurnStart => ("Brennbeginn".into(), TEXT_COLOR),
        LogKind::BurnEnd {
            apoapsis,
            periapsis,
        } => (
            format!(
                "Brennschluss, Ap {}  Pe {}",
                apoapsis.map_or("–".into(), km),
                km(*periapsis)
            ),
            TEXT_COLOR,
        ),
        LogKind::AtmosphereEntry { periapsis } => (
            format!("tritt in die Atmosphäre ein (Pe {})", km(*periapsis)),
            ATMOSPHERE_COLOR,
        ),
        LogKind::Landed { point, zone: z } => {
            let at = zone(*z, point);
            let outside = if z.is_none() { " (außerhalb)" } else { "" };
            (format!("gelandet {at}{outside}"), LANDED_COLOR)
        }
        LogKind::Crashed {
            point,
            zone: z,
            reason,
        } => (
            format!("abgestürzt {}: {}", zone(*z, point), crash_reason(reason)),
            CRASH_COLOR,
        ),
        LogKind::Docked { host } => (
            format!("angedockt an {}", world.vessels[*host].name),
            LANDED_COLOR,
        ),
        LogKind::Undocked { host, fuel } => (
            format!(
                "abgedockt von {} ({fuel:.0} kg Treibstoff)",
                world.vessels[*host].name
            ),
            TEXT_COLOR,
        ),
    };
    Line {
        time: entry.time,
        text: format!("{name}: {what}"),
        color,
    }
}

/// Alle Meldungen nach Spielzeit: Protokoll und Hinweise (bei gleicher Zeit
/// das Protokoll zuerst). Nur die Zeilen `range` werden als Text gebaut.
fn lines(world: &World, notes: &UiMessage, range: std::ops::Range<usize>) -> Vec<Line> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    let mut index = 0;
    while i < world.log.len() || j < notes.notes.len() {
        let take_log = j >= notes.notes.len()
            || (i < world.log.len() && world.log[i].time <= notes.notes[j].0);
        if range.contains(&index) {
            out.push(if take_log {
                entry_line(world, &world.log[i])
            } else {
                let (time, text) = &notes.notes[j];
                Line {
                    time: *time,
                    text: text.clone(),
                    color: NOTE_COLOR,
                }
            });
        }
        if take_log {
            i += 1;
        } else {
            j += 1;
        }
        index += 1;
        if index >= range.end {
            break;
        }
    }
    out
}

fn line_text(line: &Line) -> String {
    format!("{}  {}", format_time(line.time), line.text)
}

#[derive(Component)]
pub struct Bar;

#[derive(Component)]
pub struct LastLine;

#[derive(Component)]
pub struct Icon;

#[derive(Component)]
pub struct Area;

/// Zeile `n` (von oben) im aufgeklappten Bereich.
#[derive(Component)]
pub struct AreaLine(usize);

#[derive(Component)]
pub struct Thumb;

pub fn setup(mut commands: Commands, font: Res<UiFont>) {
    let text_font = TextFont {
        font: font.0.clone().into(),
        font_size: FontSize::Px(FONT_SIZE),
        ..default()
    };
    // Aufgeklappter Bereich, über der Leiste.
    commands
        .spawn((
            Area,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(STATUS_HEIGHT),
                height: px(AREA_HEIGHT),
                padding: UiRect::axes(px(PADDING), px(PADDING)),
                flex_direction: FlexDirection::Column,
                border: UiRect::top(px(1)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(BAR_COLOR),
            BorderColor::all(BORDER_COLOR),
            GlobalZIndex(10),
        ))
        .with_children(|area| {
            for n in 0..LINES {
                area.spawn((
                    AreaLine(n),
                    Text::new(""),
                    text_font.clone(),
                    TextColor(TEXT_COLOR),
                    TextLayout::no_wrap(),
                    Node {
                        height: px(LINE_HEIGHT),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                ));
            }
            // Scrollleiste rechts; der Griff wird in `sync` gesetzt.
            area.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(4),
                    top: px(PADDING),
                    bottom: px(PADDING),
                    width: px(SCROLLBAR_WIDTH),
                    ..default()
                },
                BackgroundColor(TRACK_COLOR),
            ))
            .with_child((
                Thumb,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    ..default()
                },
                BackgroundColor(THUMB_COLOR),
            ));
        });
    // Die Leiste selbst.
    commands
        .spawn((
            Bar,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(0),
                height: px(STATUS_HEIGHT),
                padding: UiRect::left(px(PADDING)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                border: UiRect::top(px(1)),
                ..default()
            },
            BackgroundColor(BAR_COLOR),
            BorderColor::all(BORDER_COLOR),
            GlobalZIndex(10),
        ))
        .with_children(|bar| {
            bar.spawn((
                LastLine,
                Text::new(""),
                text_font.clone(),
                TextColor(TEXT_COLOR),
                TextLayout::no_wrap(),
                Node {
                    flex_grow: 1.0,
                    overflow: Overflow::clip(),
                    ..default()
                },
            ));
            bar.spawn((
                Icon,
                Text::new("▲"),
                text_font,
                TextLayout::justify(Justify::Center),
                Node {
                    width: px(ICON_WIDTH),
                    ..default()
                },
            ));
        });
}

/// Lage der Scrollleiste (oben, unten) und des Griffs (oben, Höhe) in
/// Fensterpixeln, für `total` Zeilen.
fn scrollbar(bar: &StatusBar, window: &Window, total: usize) -> ((f32, f32), (f32, f32)) {
    let bottom = window.height() - STATUS_HEIGHT - PADDING;
    let top = bottom - LINES as f32 * LINE_HEIGHT;
    let track = bottom - top;
    let max_scroll = total.saturating_sub(LINES);
    let height = if total > LINES {
        (track * LINES as f32 / total as f32).max(20.0)
    } else {
        track
    };
    let from_top = max_scroll - bar.scroll.min(max_scroll);
    let offset = if max_scroll > 0 {
        (track - height) * from_top as f32 / max_scroll as f32
    } else {
        0.0
    };
    ((top, bottom), (top + offset, height))
}

/// Klick auf ▲/▼, Mausrad und Scrollleiste. Läuft vor der Kamera: Über dem
/// aufgeklappten Bereich scrollt das Mausrad das Protokoll statt zu zoomen.
pub fn input(
    sim: Res<Sim>,
    notes: Res<UiMessage>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut scroll_input: ResMut<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut bar: ResMut<StatusBar>,
) {
    let total = sim.0.log.len() + notes.notes.len();
    // Hochgescrollt: neue Meldungen schieben die Stelle nicht weg.
    if bar.scroll > 0 && total > bar.total {
        bar.scroll += total - bar.total;
    }
    bar.total = total;
    let max_scroll = total.saturating_sub(LINES);
    bar.scroll = bar.scroll.min(max_scroll);

    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let width = window.width();
    let height = window.height();
    let on_icon = cursor.x >= width - ICON_WIDTH && cursor.y >= height - STATUS_HEIGHT;
    if mouse.just_pressed(MouseButton::Left) && on_icon {
        bar.open = !bar.open;
        bar.scroll = 0;
        return;
    }
    if !bar.open {
        bar.grab = None;
        return;
    }
    let in_area =
        cursor.y >= height - STATUS_HEIGHT - AREA_HEIGHT && cursor.y < height - STATUS_HEIGHT;
    let notches = match scroll_input.unit {
        MouseScrollUnit::Line => scroll_input.delta.y,
        MouseScrollUnit::Pixel => scroll_input.delta.y / 40.0,
    };
    if in_area && notches != 0.0 {
        let step = (notches.abs().ceil() as usize) * 3;
        bar.scroll = if notches > 0.0 {
            (bar.scroll + step).min(max_scroll)
        } else {
            bar.scroll.saturating_sub(step)
        };
        // Die Kamera soll davon nichts merken.
        scroll_input.delta = Vec2::ZERO;
    }

    // Scrollleiste: Griff ziehen, oder auf die Leiste klicken (Griff springt hin).
    let ((top, bottom), (thumb_top, thumb_height)) = scrollbar(&bar, &window, total);
    let on_track = cursor.x >= width - 4.0 - SCROLLBAR_WIDTH - 4.0
        && cursor.x <= width
        && cursor.y >= top
        && cursor.y <= bottom;
    if mouse.just_pressed(MouseButton::Left) && on_track && max_scroll > 0 {
        let inside = cursor.y >= thumb_top && cursor.y <= thumb_top + thumb_height;
        bar.grab = Some(if inside {
            cursor.y - thumb_top
        } else {
            0.5 * thumb_height
        });
    }
    if !mouse.pressed(MouseButton::Left) {
        bar.grab = None;
    }
    if let Some(grab) = bar.grab {
        let room = (bottom - top - thumb_height).max(1.0);
        let fraction = ((cursor.y - grab - top) / room).clamp(0.0, 1.0);
        let from_top = (fraction * max_scroll as f32).round() as usize;
        bar.scroll = max_scroll - from_top;
    }
}

/// Texte, Farben und Scrollleiste setzen.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn sync(
    sim: Res<Sim>,
    notes: Res<UiMessage>,
    bar: Res<StatusBar>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut area: Single<&mut Node, With<Area>>,
    mut last: Single<(&mut Text, &mut TextColor), (With<LastLine>, Without<AreaLine>)>,
    mut icon: Single<&mut Text, (With<Icon>, Without<LastLine>, Without<AreaLine>)>,
    mut rows: Query<(&AreaLine, &mut Text, &mut TextColor), (Without<LastLine>, Without<Icon>)>,
    mut thumb: Single<&mut Node, (With<Thumb>, Without<Area>)>,
) {
    let world = &sim.0;
    let total = world.log.len() + notes.notes.len();
    let set = |text: &mut Text, value: String| {
        if text.0 != value {
            text.0 = value;
        }
    };

    let (value, color) = match lines(world, &notes, total.saturating_sub(1)..total).pop() {
        Some(line) => (line_text(&line), line.color),
        None => ("Noch keine Meldungen.".into(), HINT_COLOR),
    };
    set(&mut last.0, value);
    last.1.0 = color;
    set(&mut icon, if bar.open { "▼" } else { "▲" }.into());

    area.display = if bar.open {
        Display::Flex
    } else {
        Display::None
    };
    if !bar.open {
        return;
    }
    // Sichtbar sind die Zeilen `first..first + LINES`, unten bündig.
    let max_scroll = total.saturating_sub(LINES);
    let first = max_scroll - bar.scroll.min(max_scroll);
    let shown = lines(world, &notes, first..first + LINES);
    let empty = LINES - shown.len();
    for (AreaLine(n), mut text, mut color) in &mut rows {
        match n.checked_sub(empty).and_then(|k| shown.get(k)) {
            Some(line) => {
                set(&mut text, line_text(line));
                color.0 = line.color;
            }
            None => set(&mut text, String::new()),
        }
    }
    let ((top, _), (thumb_top, thumb_height)) = scrollbar(&bar, &window, total);
    thumb.top = px(thumb_top - top);
    thumb.height = px(thumb_height);
}
