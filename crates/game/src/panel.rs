//! Seitenleiste rechts: alles, was man bedienen kann, je nach Lage als Karte.
//! Die Anzeige links bleibt reine Information.
//!
//! Erste Karte: "Start" für ein gelandetes Schiff. Später kommen hier z. B.
//! der Node-Manager, "scharf schalten" und "+1 Umlauf" dazu.
//!
//! Klicks auf die Leiste gehen nicht an die Karte dahinter (`over_panel`).
//! Schaltflächen prüfen Treffer selbst (`ComputedNode::contains_point`), ohne
//! Bevys Picking; das hält die Abhängigkeiten klein.

use crate::Sim;
use crate::hud::{UiFont, format_span, km};
use crate::node_ui::{Selection, UiMessage};
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use sim::{Launch, LaunchProfile, PlanError, Segment, World, kepler};

/// Breite der Leiste in Pixeln.
pub const PANEL_WIDTH: f32 = 330.0;
/// Vorgabe für einen neuen Start: so lange bis zum Abheben.
const DEFAULT_COUNTDOWN: f64 = 120.0;

const PANEL_COLOR: Color = Color::srgba(0.03, 0.04, 0.07, 0.88);
const CARD_COLOR: Color = Color::srgb(0.07, 0.08, 0.12);
const BORDER_COLOR: Color = Color::srgb(0.25, 0.3, 0.4);
const BUTTON_COLOR: Color = Color::srgb(0.12, 0.14, 0.2);
const BUTTON_HOVER: Color = Color::srgb(0.2, 0.24, 0.33);
const BUTTON_PRESSED: Color = Color::srgb(0.35, 0.4, 0.55);
const HINT_COLOR: Color = Color::srgb(0.6, 0.65, 0.75);

/// Ist die Leiste gerade zu sehen?
#[derive(Resource, Default)]
pub struct PanelShown(pub bool);

/// Liegt der Mauszeiger auf der Leiste?
pub fn over_panel(shown: &PanelShown, window: &Window, cursor: Vec2) -> bool {
    shown.0 && cursor.x >= window.width() - PANEL_WIDTH
}

#[derive(Component)]
pub struct Panel;

/// Karte "Start" mit Titel, Zeilen und Ergebnis.
#[derive(Component)]
pub struct LaunchCard;

#[derive(Component)]
pub struct LaunchTitle;

/// Die einstellbaren Zeilen; erst sichtbar, wenn ein Start geplant ist.
#[derive(Component)]
pub struct LaunchRows;

#[derive(Component)]
pub struct LaunchResult;

/// Schaltfläche "Start planen" / "Start abbrechen" und ihre Beschriftung.
#[derive(Component)]
pub struct PlanButton;

#[derive(Component)]
pub struct PlanButtonText;

/// Einstellbare Werte des Starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Param {
    StartIn,
    Heading,
    Turn,
    Apoapsis,
}

impl Param {
    const ALL: [Param; 4] = [Param::StartIn, Param::Heading, Param::Turn, Param::Apoapsis];

    fn label(self) -> &'static str {
        match self {
            Param::StartIn => "Start in",
            Param::Heading => "Kurs",
            Param::Turn => "Wendebeginn",
            Param::Apoapsis => "Ziel-Apoapsis",
        }
    }

    /// Schrittweite: normal und fein (mit Umschalt).
    fn step(self, fine: bool) -> f64 {
        match (self, fine) {
            (Param::StartIn, false) => 60.0,
            (Param::StartIn, true) => 10.0,
            (Param::Heading, false) => 5.0,
            (Param::Heading, true) => 1.0,
            (Param::Turn, false) => 500.0,
            (Param::Turn, true) => 100.0,
            (Param::Apoapsis, false) => 10_000.0,
            (Param::Apoapsis, true) => 1_000.0,
        }
    }

    fn value(self, launch: &Launch, now: f64) -> String {
        let p = &launch.profile;
        match self {
            Param::StartIn => format_span((launch.time - now).max(0.0)),
            Param::Heading => format!("{:.0}°", p.heading),
            Param::Turn => km(p.turn_altitude),
            Param::Apoapsis => km(p.target_apoapsis),
        }
    }
}

/// Eine Schaltfläche der Leiste (Farbe wechselt unter der Maus).
#[derive(Component)]
pub struct PanelButton;

/// Wert einer Zeile.
#[derive(Component)]
pub struct ParamValue(Param);

/// −/+ an einer Zeile.
#[derive(Component)]
pub struct Adjust {
    param: Param,
    sign: f64,
}

fn text(font: &UiFont, value: &str, size: f32) -> (Text, TextFont) {
    (
        Text::new(value),
        TextFont {
            font: font.0.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
    )
}

fn button_node(width: Val) -> Node {
    Node {
        width,
        height: px(24),
        border: UiRect::all(px(1)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

pub fn setup(mut commands: Commands, font: Res<UiFont>) {
    let font = font.into_inner();
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                right: px(0),
                top: px(0),
                bottom: px(0),
                width: px(PANEL_WIDTH),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(12),
                border: UiRect::left(px(1)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL_COLOR),
            BorderColor::all(BORDER_COLOR),
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    LaunchCard,
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(10)),
                        row_gap: px(8),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(CARD_COLOR),
                    BorderColor::all(BORDER_COLOR),
                ))
                .with_children(|card| {
                    card.spawn((LaunchTitle, text(font, "Start", 16.0)));
                    card.spawn((
                        LaunchRows,
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(6),
                            ..default()
                        },
                    ))
                    .with_children(|rows| {
                        for param in Param::ALL {
                            rows.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: px(6),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    text(font, param.label(), 14.0),
                                    Node {
                                        width: px(118),
                                        ..default()
                                    },
                                ));
                                row.spawn((
                                    ParamValue(param),
                                    text(font, "", 14.0),
                                    Node {
                                        flex_grow: 1.0,
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                                for (sign, label) in [(-1.0, "−"), (1.0, "+")] {
                                    row.spawn((
                                        PanelButton,
                                        Adjust { param, sign },
                                        button_node(px(26)),
                                        BackgroundColor(BUTTON_COLOR),
                                        BorderColor::all(BORDER_COLOR),
                                    ))
                                    .with_child(text(font, label, 14.0));
                                }
                            });
                        }
                        rows.spawn((
                            text(font, "Umschalt: feine Schritte", 12.0),
                            TextColor(HINT_COLOR),
                        ));
                    });
                    card.spawn((
                        PanelButton,
                        PlanButton,
                        button_node(percent(100)),
                        BackgroundColor(BUTTON_COLOR),
                        BorderColor::all(BORDER_COLOR),
                    ))
                    .with_child((PlanButtonText, text(font, "Start planen", 14.0)));
                    card.spawn((LaunchResult, text(font, "", 14.0)));
                });
        });
}

/// Ein gelandetes Schiff (kein Wrack) ist gewählt: dessen Nummer.
fn landed_selection(world: &World, selection: &Selection) -> Option<usize> {
    let i = selection.0?;
    matches!(
        world.vessels[i].trajectory.current(),
        Segment::Surface { wreck: false, .. }
    )
    .then_some(i)
}

/// Was nach dem Aufstieg folgt, als Textzeilen.
fn launch_result(world: &World, i: usize) -> String {
    let trajectory = &world.vessels[i].trajectory;
    let Some(plan) = trajectory.launch() else {
        let (body, point) = match trajectory.current() {
            Segment::Surface { body, point, .. } => (*body, *point),
            _ => return String::new(),
        };
        let place = match world.zone_at(body, &point) {
            Some(k) => format!("Gelandet in {}.", world.zones[k].name),
            None => "Außerhalb eines Zielgebiets: mit einem Hop (Start mit niedriger \
                     Ziel-Apoapsis) ins Ziel."
                .into(),
        };
        return format!(
            "{place} Kurs 90° ist Osten. In den Orbit: danach mit einem Node an der \
             Apoapsis zirkularisieren."
        );
    };
    let burn = &plan.burn;
    let params = &world.vessels[i].params;
    let ascent = trajectory.segments().find(|s| s.is_burn());
    let altitude = ascent.map_or(0.0, |s| {
        s.state_at(&world.bodies, burn.end).position.length() - world.bodies[s.body()].radius
    });
    // Feste Spalten (die Schrift ist dicktengleich).
    let row = |label: &str, value: String| format!("{label:<14}{value}");
    let mut lines = vec![
        row(
            "Brennschluss",
            format!("in {}", format_span((burn.end - world.time).max(0.0))),
        ),
        row("  in Höhe", km(altitude)),
        row(
            "Treibstoff",
            format!("{:.0} kg", burn.end_mass() - params.dry_mass),
        ),
        row(
            "  reicht für",
            format!(
                "{:.0} m/s",
                params.exhaust_velocity * (burn.end_mass() / params.dry_mass).ln()
            ),
        ),
    ];
    let orbit = trajectory
        .segments()
        .find(|s| s.start() >= burn.end && matches!(s, Segment::Kepler { .. }));
    if let Some(Segment::Kepler { state, body, .. }) = orbit {
        let body = &world.bodies[*body];
        let info = kepler::orbit_info(body.gm, state);
        let h = state.position.cross(state.velocity);
        lines.push(row(
            "Bahn danach",
            format!(
                "Ap {}",
                info.apoapsis.map_or("–".into(), |r| km(r - body.radius))
            ),
        ));
        lines.push(row("", format!("Pe {}", km(info.periapsis - body.radius))));
        lines.push(row(
            "Bahnneigung",
            format!("{:.1}°", (h.z / h.length()).acos().to_degrees()),
        ));
    }
    // Kommt die Fähre wieder herunter (Hop), wo und heil?
    if let Some(t) = world.touchdown(i) {
        let label = if t.braking.is_ok() {
            "Aufsetzen"
        } else {
            "Absturz!"
        };
        lines.push(row(
            label,
            match (t.zone, t.nearest) {
                (Some(k), _) => format!("in {}", world.zones[k].name),
                (None, Some((k, d))) => format!("{} von {}", km(d), world.zones[k].name),
                (None, None) => "außerhalb".into(),
            },
        ));
    }
    lines.join("\n")
}

/// Leiste und Karten passend zur Auswahl zeigen und beschriften.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn sync(
    sim: Res<Sim>,
    selection: Res<Selection>,
    mut shown: ResMut<PanelShown>,
    mut panel: Single<&mut Node, With<Panel>>,
    mut rows: Single<&mut Node, (With<LaunchRows>, Without<Panel>)>,
    mut title: Single<&mut Text, With<LaunchTitle>>,
    mut plan_text: Single<&mut Text, (With<PlanButtonText>, Without<LaunchTitle>)>,
    mut result: Single<
        &mut Text,
        (
            With<LaunchResult>,
            Without<LaunchTitle>,
            Without<PlanButtonText>,
        ),
    >,
    mut values: Query<
        (&ParamValue, &mut Text),
        (
            Without<LaunchTitle>,
            Without<PlanButtonText>,
            Without<LaunchResult>,
        ),
    >,
) {
    let world = &sim.0;
    let landed = landed_selection(world, &selection);
    shown.0 = landed.is_some();
    panel.display = if shown.0 {
        Display::Flex
    } else {
        Display::None
    };
    let Some(i) = landed else { return };

    let launch = world.vessels[i].trajectory.launch().map(|p| p.launch);
    rows.display = if launch.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    let set = |text: &mut Text, value: String| {
        if text.0 != value {
            text.0 = value;
        }
    };
    set(&mut title, format!("Start: {}", world.vessels[i].name));
    set(
        &mut plan_text,
        if launch.is_some() {
            "Start abbrechen"
        } else {
            "Start planen"
        }
        .into(),
    );
    set(&mut result, launch_result(world, i));
    if let Some(launch) = launch {
        for (ParamValue(param), mut text) in &mut values {
            set(&mut text, param.value(&launch, world.time));
        }
    }
}

/// Einen Wert des geplanten Starts ändern.
fn adjust(world: &mut World, i: usize, param: Param, delta: f64) -> Result<(), PlanError> {
    let mut launch = world.vessels[i]
        .trajectory
        .launch()
        .ok_or(PlanError::NoNode)?
        .launch;
    let p = &mut launch.profile;
    match param {
        // Die ganze Kette wandert mit.
        Param::StartIn => return world.shift_launch(i, delta),
        Param::Heading => p.heading = (p.heading + delta).rem_euclid(360.0),
        Param::Apoapsis => p.target_apoapsis = (p.target_apoapsis + delta).clamp(1_000.0, 5e6),
        Param::Turn => p.turn_altitude = (p.turn_altitude + delta).max(0.0),
    }
    p.turn_altitude = p.turn_altitude.min(0.5 * p.target_apoapsis);
    world.set_launch(i, launch)
}

/// Mauszeiger in physischen Pixeln (wie die Lage der UI-Knoten).
fn physical_cursor(window: &Window) -> Option<Vec2> {
    window.physical_cursor_position()
}

/// Klicks auf die Schaltflächen der Leiste.
#[allow(clippy::too_many_arguments)]
pub fn buttons(
    mut sim: ResMut<Sim>,
    selection: Res<Selection>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut message: ResMut<UiMessage>,
    window: Single<&Window, With<PrimaryWindow>>,
    adjusts: Query<(&Adjust, &ComputedNode, &UiGlobalTransform)>,
    plan: Single<(&ComputedNode, &UiGlobalTransform), With<PlanButton>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let world = &mut sim.0;
    let (Some(i), Some(cursor)) = (
        landed_selection(world, &selection),
        physical_cursor(&window),
    ) else {
        return;
    };
    let fine = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let planned = world.vessels[i].trajectory.launch().is_some();
    let hit = |node: &ComputedNode, transform: &UiGlobalTransform| {
        node.contains_point(*transform, cursor)
    };
    let mut failure = None;
    if planned && let Some((a, ..)) = adjusts.iter().find(|(_, n, t)| hit(n, t)) {
        failure = adjust(world, i, a.param, a.sign * a.param.step(fine)).err();
    } else if hit(plan.0, plan.1) {
        failure = if planned {
            world.clear_launch(i).err()
        } else {
            let launch = Launch {
                time: world.time + DEFAULT_COUNTDOWN,
                profile: LaunchProfile::default(),
            };
            // Vorerst ist jede Kette sofort scharf.
            world
                .set_launch(i, launch)
                .and_then(|()| world.set_armed(i, true))
                .err()
        };
    }
    if let Some(e) = failure {
        message.show(e, real_time.elapsed_secs_f64());
    }
}

/// Schaltflächen heller, wenn die Maus darauf liegt oder drückt.
pub fn hover(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut buttons: Query<
        (&ComputedNode, &UiGlobalTransform, &mut BackgroundColor),
        With<PanelButton>,
    >,
) {
    let cursor = physical_cursor(&window);
    for (node, transform, mut color) in &mut buttons {
        let over = cursor.is_some_and(|c| node.contains_point(*transform, c));
        let wanted = match (over, mouse.pressed(MouseButton::Left)) {
            (true, true) => BUTTON_PRESSED,
            (true, false) => BUTTON_HOVER,
            _ => BUTTON_COLOR,
        };
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}
