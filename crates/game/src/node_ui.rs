//! Bedienung der Manöverkette: Schiff wählen, Nodes setzen, wählen und
//! verschieben, Δv über sechs Griffe einstellen (wie in KSP).
//!
//! Alles läuft über die Simulation: Jede Änderung ruft `World::add_node` oder
//! `World::set_node` auf, die Zeitleiste wird neu berechnet und ist sofort die
//! Vorschau. Spätere Nodes wandern dabei mit.
//!
//! Vorerst ist jede neue Kette sofort scharf. Die Simulation kennt auch nicht
//! scharfe Ketten; eine Schaltfläche dafür kommt später an anderer Stelle in
//! die Oberfläche.
//!
//! Treffer werden im Bildschirm geprüft (Abstand in Pixeln). Punkte hinter dem
//! Planeten zählen nicht. Zielgebiete sind über ihre Markierung in der Mitte
//! wählbar.

use crate::Sim;
use crate::camera::{CameraRig, Focus, MainCamera, OVERLAY_LAYER, dir_to_view, to_view};
use crate::dv_bar::{BarShown, over_bar};
use crate::hud::UiFont;
use crate::panel::{PanelShown, over_panel};
use crate::status_bar::{StatusBar, over_status};
use crate::view::orbit_points;
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::window::PrimaryWindow;
use sim::{ManeuverNode, Plan, PlanError, Segment, Trajectory, World, kepler, maneuver};

/// Trefferradius in Pixeln.
const PICK_RADIUS: f32 = 14.0;
/// Bis zu dieser Mausbewegung in Pixeln gilt Drücken + Loslassen als Klick.
const CLICK_TOLERANCE: f32 = 5.0;
/// Länge der Griffe relativ zum Kameraabstand (gleich groß auf dem Bildschirm).
const HANDLE_LENGTH: f32 = 0.08;
/// Radius der Griff- und Node-Ringe relativ zum Kameraabstand.
const HANDLE_RADIUS: f32 = 0.008;
const NODE_RADIUS: f32 = 0.012;
/// Nicht gewählte Nodes der Kette sind kleiner.
const OTHER_NODE_SCALE: f32 = 0.65;
/// Node-Farbe: scharfe Kette orange, sonst hellblau.
const ARMED_COLOR: Color = Color::srgb(1.0, 0.55, 0.1);
const UNARMED_COLOR: Color = Color::srgb(0.6, 0.85, 1.0);
/// Lage des Etiketts "+N Umläufe" relativ zum Node (Bildschirm).
const LABEL_OFFSET: Vec2 = Vec2::new(18.0, 8.0);

/// Die sechs Griffe: Achse (0 prograd, 1 normal, 2 radial), Richtung, Farbe.
const HANDLES: [(usize, f64, Color); 6] = [
    (0, 1.0, Color::srgb(0.85, 0.95, 0.2)),
    (0, -1.0, Color::srgb(0.85, 0.95, 0.2)),
    (1, 1.0, Color::srgb(0.85, 0.35, 1.0)),
    (1, -1.0, Color::srgb(0.85, 0.35, 1.0)),
    (2, 1.0, Color::srgb(0.3, 0.9, 1.0)),
    (2, -1.0, Color::srgb(0.3, 0.9, 1.0)),
];

/// Ausgewähltes Schiff.
#[derive(Resource, Default)]
pub struct Selection(pub Option<usize>);

/// Gewähltes Zielgebiet. Es schließt ein gewähltes Schiff aus: Es ist immer
/// nur eins gewählt.
#[derive(Resource, Default)]
pub struct SelectedZone(pub Option<usize>);

/// Gewählter Node des gewählten Schiffs, erkannt an seinem gesetzten
/// Zeitpunkt. So bleibt die Wahl erhalten, wenn ein früherer Node ausgeführt
/// wird und die Stellen in der Kette sich verschieben.
#[derive(Resource, Default)]
pub struct SelectedNode(pub Option<f64>);

/// Gewählter Node: (Schiff, Stelle in der Kette). Ohne ausdrückliche Wahl
/// (oder wenn der gewählte nicht mehr da ist) der erste.
pub fn selected_node(
    world: &World,
    selection: &Selection,
    node: &SelectedNode,
) -> Option<(usize, usize)> {
    let i = selection.0?;
    let nodes = world.vessels[i].trajectory.nodes();
    if nodes.is_empty() {
        return None;
    }
    let k = node
        .0
        .and_then(|t| nodes.iter().position(|n| n.node.time == t))
        .unwrap_or(0);
    Some((i, k))
}

/// Hinweise der Bedienung, z. B. warum ein Node nicht geht. Sie erscheinen in
/// der Statusleiste zwischen den Einträgen des Protokolls, gehören aber nicht
/// zum Spielstand.
#[derive(Resource, Default)]
pub struct UiMessage {
    /// Spielzeit und Text, älteste zuerst.
    pub notes: Vec<(f64, String)>,
    /// Echtzeit (s) des letzten Hinweises.
    last: f64,
}

/// So viele Hinweise bleiben erhalten.
const NOTE_LIMIT: usize = 200;

impl UiMessage {
    /// Hinweis zur Spielzeit `time`. Derselbe Text innerhalb von 2 s
    /// (`real`: Echtzeit) zählt nur einmal, z. B. beim Ziehen eines Griffs am
    /// Anschlag.
    pub fn say(&mut self, text: String, time: f64, real: f64) {
        let repeated = self.notes.last().is_some_and(|(_, last)| *last == text);
        if !(repeated && real - self.last < 2.0) {
            self.notes.push((time, text));
            if self.notes.len() > NOTE_LIMIT {
                self.notes.remove(0);
            }
        }
        self.last = real;
    }

    pub fn show(&mut self, error: PlanError, time: f64, real: f64) {
        self.say(error.to_string(), time, real);
    }
}

/// Wie weit (Pixel) die Maus seit dem Drücken entlang eines Griffs gezogen
/// wurde; `a` und `b` sind Node und Griff auf dem Bildschirm.
fn pull_along(cursor: Vec2, press: Vec2, a: Vec2, b: Vec2) -> f32 {
    (cursor - press).dot((b - a).normalize_or_zero())
}

/// Begrenzt eine Änderung an der Achse `axis`, damit |Δv| den Vorrat
/// `available` nicht übersteigt. Verkleinern geht immer. Liefert das erlaubte
/// Δv und ob begrenzt wurde.
///
/// Nur die Bedienung begrenzt; die Simulation erlaubt mehr Δv als im Tank ist
/// (das Brennen endet dann leer). Das braucht es, sobald vor einem Node noch
/// nachgetankt werden kann.
fn limit_to_tank(old: DVec3, mut new: DVec3, axis: usize, available: f64) -> (DVec3, bool) {
    if new.length() <= available || new.length() <= old.length() {
        return (new, false);
    }
    if old.length() >= available {
        return (old, true);
    }
    let others = new.length_squared() - new[axis] * new[axis];
    new[axis] = new[axis].signum() * (available * available - others).max(0.0).sqrt();
    (new, true)
}

/// Bahnstücke (Zeit, Position), auf die ein Node gesetzt werden kann.
type Path = Vec<Vec<(f64, DVec3)>>;

/// Was die linke Maustaste gerade tut.
#[derive(Resource, Default)]
pub enum Drag {
    #[default]
    None,
    /// Gedrückt, noch nicht entschieden: Klick oder nichts.
    Pending(Vec2),
    /// Griff des Nodes `index` wird gezogen.
    Handle {
        index: usize,
        handle: usize,
        press: Vec2,
    },
    /// Node `index` wird entlang seiner Bahn verschoben (der Bahn nach den
    /// früheren Manövern). Erst ab `CLICK_TOLERANCE` Bewegung, sonst ist es
    /// nur ein Klick zum Wählen.
    Node {
        index: usize,
        path: Path,
        press: Vec2,
        moved: bool,
    },
}

/// Eigene Gizmo-Gruppe für Nodes und Griffe. Sie wird von der Overlay-Kamera
/// gezeichnet und liegt damit immer über der Welt, sonst verschwände z. B. der
/// Griff "radial nach innen" bei niedrigen Bahnen im Planeten.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct NodeGizmos;

pub fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<NodeGizmos>();
    config.render_layers = RenderLayers::layer(OVERLAY_LAYER);
    config.line.width = 2.5;
}

/// Etikett neben dem gewählten Node: nach wie vielen Umläufen er kommt.
#[derive(Component)]
pub struct NodeLabel;

pub fn setup_label(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        NodeLabel,
        Text::new(""),
        TextFont {
            font: font.0.clone().into(),
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.9, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            ..default()
        },
    ));
}

/// "1 Umlauf", "3 Umläufe".
pub fn revolutions_text(n: u32) -> String {
    if n == 1 {
        "1 Umlauf".into()
    } else {
        format!("{n} Umläufe")
    }
}

/// Etikett an den gewählten Node schieben. Nur wenn er mindestens einen
/// Umlauf nach dem vorherigen Brennen (oder nach jetzt) kommt.
pub fn sync_label(
    sim: Res<Sim>,
    selection: Res<Selection>,
    selected: Res<SelectedNode>,
    rig: Res<CameraRig>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
    mut label: Single<(&mut Node, &mut Text), With<NodeLabel>>,
) {
    let world = &sim.0;
    let (camera, transform) = *camera;
    let shown = selected_node(world, &selection, &selected).and_then(|(i, k)| {
        let plan = world.vessels[i].trajectory.nodes()[k].plan.ok()?;
        let n = world.revolutions_before_node(i, k).filter(|&n| n > 0)?;
        let node = to_view(plan.node_state.position, rig.focus);
        let screen = camera
            .world_to_viewport(&GlobalTransform::from(*transform), node)
            .ok()?;
        Some((screen, n))
    });
    let (node, text) = &mut *label;
    let Some((screen, n)) = shown else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;
    node.left = px(screen.x + LABEL_OFFSET.x);
    node.top = px(screen.y + LABEL_OFFSET.y);
    let wanted = format!("+{}", revolutions_text(n));
    if text.0 != wanted {
        text.0 = wanted;
    }
}

/// Bildschirmprojektion mit Verdeckung durch den Planeten.
pub(crate) struct Projector<'a> {
    camera: &'a Camera,
    transform: GlobalTransform,
    eye: Vec3,
    /// Planetenmittelpunkt in Bevy-Koordinaten (relativ zum Fokus).
    planet_center: Vec3,
    planet_radius: f32,
}

impl<'a> Projector<'a> {
    /// Für den Planeten im Ursprung der Simulation (`focus`: Fokuspunkt).
    pub(crate) fn new(
        camera: &'a Camera,
        transform: &Transform,
        focus: DVec3,
        planet_radius: f64,
    ) -> Self {
        Self {
            camera,
            transform: GlobalTransform::from(*transform),
            eye: transform.translation,
            planet_center: to_view(DVec3::ZERO, focus),
            planet_radius: planet_radius as f32,
        }
    }

    /// Auf den Bildschirm; `None`, wenn der Planet davor liegt.
    pub(crate) fn project(&self, p: Vec3) -> Option<Vec2> {
        if self.occluded(p) {
            return None;
        }
        self.camera.world_to_viewport(&self.transform, p).ok()
    }

    /// Strahl von der Kamera durch den Bildschirmpunkt.
    fn ray(&self, cursor: Vec2) -> Option<Ray3d> {
        self.camera.viewport_to_world(&self.transform, cursor).ok()
    }

    /// Auf den Bildschirm, auch wenn der Planet davor liegt (für Nodes und
    /// Griffe, die immer obenauf gezeichnet werden).
    fn screen(&self, p: Vec3) -> Option<Vec2> {
        self.camera.world_to_viewport(&self.transform, p).ok()
    }

    /// Liegt der Planet zwischen Kamera und Punkt?
    fn occluded(&self, p: Vec3) -> bool {
        let d = p - self.eye;
        let len = d.length();
        let dir = d / len;
        let eye = self.eye - self.planet_center;
        let b = eye.dot(dir);
        // Etwas kleiner, damit Punkte auf der Oberfläche sich nicht selbst verdecken.
        let r = self.planet_radius * 0.999;
        let disc = b * b - (eye.length_squared() - r * r);
        disc > 0.0 && {
            let hit = -b - disc.sqrt();
            hit > 0.0 && hit < len
        }
    }
}

/// Lage von Node und Griffen in Bevy-Koordinaten.
fn node_geometry(plan: &Plan, focus: DVec3, eye: Vec3) -> (Vec3, [Vec3; 6]) {
    let node = to_view(plan.node_state.position, focus);
    let frame = maneuver::node_frame(&plan.node_state);
    let length = node.distance(eye) * HANDLE_LENGTH;
    let handles = HANDLES.map(|(axis, sign, _)| node + dir_to_view(frame[axis] * sign) * length);
    (node, handles)
}

/// Bahnstücke ab `from` (frühestens jetzt), auf die ein Node gesetzt werden
/// kann: aller Freiflug bis zum Aufschlag, ohne die Brennvorgänge. `points`
/// Stützpunkte pro Umlauf, so fein wie die gezeichnete Linie.
fn coast_path(world: &World, trajectory: &Trajectory, from: f64, points: usize) -> Path {
    let from = from.max(world.time);
    let mut pieces = Vec::new();
    for segment in trajectory.segments() {
        if segment.end() <= from {
            continue;
        }
        match segment {
            Segment::Kepler { state, .. } => {
                let gm = world.bodies[segment.body()].gm;
                let a = segment.start().max(from);
                let span = kepler::period(gm, state).unwrap_or(3.0 * 3600.0);
                let b = segment.end().min(a + span);
                let n = ((points as f64 * (b - a) / span).ceil() as usize).max(16);
                pieces.push(
                    (0..=n)
                        .map(|k| {
                            let t = a + (b - a) * k as f64 / n as f64;
                            (t, segment.state_at(&world.bodies, t).position)
                        })
                        .collect(),
                );
            }
            Segment::Integrated {
                burn: None,
                samples,
                ..
            } => pieces.push(
                samples
                    .iter()
                    .filter(|s| s.time > from)
                    .map(|s| (s.time, s.state.position))
                    .collect(),
            ),
            // Brennen (auch der Aufstieg) und das Warten am Boden vor dem Start.
            Segment::Integrated { .. }
            | Segment::Surface {
                liftoff: Some(_), ..
            } => {}
            // Am Boden und im Anflug zum Andocken geht kein Node.
            Segment::Surface { .. } | Segment::Approach { .. } => break,
        }
    }
    pieces
}

/// Nächster Punkt der Bahn zum Mauszeiger: (Abstand in Pixeln, Zeitpunkt).
/// Zwischen den Stützpunkten wird im Bildschirm interpoliert, nie über ein
/// Brennen hinweg.
fn nearest_on_path(
    projector: &Projector,
    path: &Path,
    rig: &CameraRig,
    now: f64,
    cursor: Vec2,
) -> Option<(f32, f64)> {
    // Gesucht wird in 3D: der Punkt jedes Bahnstücks, der dem Strahl unter der
    // Maus am nächsten liegt. So zählen auch Stücke, deren Enden außerhalb des
    // Bildes oder hinter der Kamera liegen (Nahansicht).
    let ray = projector.ray(cursor)?;
    let mut best: Option<(f32, f64)> = None;
    for piece in path {
        for pair in piece.windows(2) {
            let [(t0, p0), (t1, p1)] = [pair[0], pair[1]];
            if t1 <= now {
                continue;
            }
            let (a, b) = (to_view(p0, rig.focus), to_view(p1, rig.focus));
            let u = closest_on_segment(&ray, a, b);
            let t = t0 + (t1 - t0) * u as f64;
            let point = a + (b - a) * u;
            // Ausgeblendete Stücke (Nahansicht) lassen sich nicht anklicken.
            if t <= now || rig.fade(point) == 0.0 {
                continue;
            }
            let Some(screen) = projector.project(point) else {
                continue;
            };
            let distance = screen.distance(cursor);
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, t));
            }
        }
    }
    best
}

/// Stelle (0 bis 1) auf der Strecke von `a` nach `b`, die dem Strahl am
/// nächsten liegt.
fn closest_on_segment(ray: &Ray3d, a: Vec3, b: Vec3) -> f32 {
    let d1 = *ray.direction;
    let d2 = b - a;
    let r = ray.origin - a;
    let (a12, a22) = (d1.dot(d2), d2.dot(d2));
    let (b1, b2) = (d1.dot(r), d2.dot(r));
    // |d1| = 1; Nenner 0 bei parallelen Geraden oder leerer Strecke.
    let denominator = a22 - a12 * a12;
    if denominator <= f32::EPSILON * a22.max(1.0) {
        return 0.0;
    }
    ((b2 - a12 * b1) / denominator).clamp(0.0, 1.0)
}

/// Lässt sich die Kette des Schiffs gerade ändern? Nicht während des
/// Brennens, am Boden nur mit geplantem Start.
fn editable(world: &World, i: usize) -> bool {
    let trajectory = &world.vessels[i].trajectory;
    let current = trajectory.current();
    !current.is_burn()
        && !matches!(current, Segment::Approach { .. })
        && (trajectory.launch().is_some() || !matches!(current, Segment::Surface { .. }))
}

/// Maus und Tastatur für Auswahl und Nodes.
#[allow(clippy::too_many_arguments)]
pub fn input(
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    mut selected: ResMut<SelectedNode>,
    mut zone: ResMut<SelectedZone>,
    mut drag: ResMut<Drag>,
    mut message: ResMut<UiMessage>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut rig: ResMut<CameraRig>,
    shown: Res<PanelShown>,
    bar: Res<BarShown>,
    status: Res<StatusBar>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
) {
    let world = &mut sim.0;
    let mut failure: Option<PlanError> = None;
    let count = world.vessels.len();
    // Ein gerade angedocktes Schiff steht für seinen Verbund.
    if let Some(i) = selection.0
        && world.vessels[i].docked_to.is_some()
    {
        selection.0 = Some(world.host(i));
        selected.0 = None;
    }
    if keys.just_pressed(KeyCode::Tab) && count > 0 {
        // Angedockte Schiffe überspringen: Gewählt wird der Verbund.
        let start = selection.0.unwrap_or(count - 1);
        selection.0 = (1..=count)
            .map(|k| (start + k) % count)
            .find(|&i| world.vessels[i].docked_to.is_none());
        selected.0 = None;
        zone.0 = None;
    }
    if keys.just_pressed(KeyCode::Escape) {
        selection.0 = None;
        selected.0 = None;
        zone.0 = None;
    }
    if (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace))
        && let Some((i, k)) = selected_node(world, &selection, &selected)
    {
        failure = world.remove_node(i, k).err();
        if failure.is_none() {
            // Danach ist der nächste gewählt (oder der erste).
            selected.0 = world.vessels[i]
                .trajectory
                .nodes()
                .get(k)
                .map(|n| n.node.time);
        }
    }
    // Pfeiltasten: Node der Kette wählen, auch ungültige (die nicht auf der
    // Bahn erscheinen und sich so trotzdem löschen lassen).
    let step = keys.just_pressed(KeyCode::ArrowDown) as isize
        - keys.just_pressed(KeyCode::ArrowUp) as isize;
    if step != 0
        && let Some((i, k)) = selected_node(world, &selection, &selected)
    {
        let nodes = world.vessels[i].trajectory.nodes();
        let k = (k as isize + step).clamp(0, nodes.len() as isize - 1) as usize;
        selected.0 = Some(nodes[k].node.time);
    }
    if keys.just_pressed(KeyCode::KeyH)
        && let Some(i) = selection.0
    {
        let vessel = &mut world.vessels[i];
        vessel.halt_warp_at_burn = !vessel.halt_warp_at_burn;
    }

    let Some(cursor) = window.cursor_position() else {
        if let Some(e) = failure {
            message.show(e, world.time, real_time.elapsed_secs_f64());
        }
        return;
    };
    let (camera, transform) = *camera;
    let projector = Projector::new(camera, transform, rig.focus, world.bodies[0].radius);

    // Klicks auf Seitenleiste, Δv-Balken und Statusleiste gehören diesen.
    let over_ui = over_panel(&shown, &window, cursor)
        || over_bar(&bar, &shown, &window, cursor)
        || over_status(&status, &window, cursor);
    if buttons.just_pressed(MouseButton::Left) && !over_ui {
        *drag = Drag::Pending(cursor);
        if let Some(i) = selection.0
            && editable(world, i)
        {
            let trajectory = &world.vessels[i].trajectory;
            let near = |p: Vec3| {
                projector
                    .screen(p)
                    .map(|s| s.distance(cursor))
                    .filter(|&d| d < PICK_RADIUS)
            };
            // Zuerst die Griffe des gewählten Nodes, dann alle Nodes.
            let handle = selected_node(world, &selection, &selected).and_then(|(_, k)| {
                let plan = trajectory.nodes()[k].plan.ok()?;
                let (_, handles) = node_geometry(&plan, rig.focus, projector.eye);
                let handle = (0..6).find(|&h| near(handles[h]).is_some())?;
                Some((k, handle))
            });
            let node = trajectory
                .plans()
                .enumerate()
                .filter_map(|(k, plan)| {
                    near(to_view(plan.node_state.position, rig.focus)).map(|d| (d, k))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((index, handle)) = handle {
                *drag = Drag::Handle {
                    index,
                    handle,
                    press: cursor,
                };
            } else if let Some((_, index)) = node {
                selected.0 = Some(trajectory.nodes()[index].node.time);
                // Die Bahn nach den früheren Manövern: Auf ihr wird verschoben.
                let from = match index {
                    0 => world.time,
                    _ => trajectory.nodes()[index - 1]
                        .plan
                        .map_or(world.time, |p| p.burn.end),
                };
                let leg = world.trajectory_with_first_nodes(i, index);
                *drag = Drag::Node {
                    index,
                    path: coast_path(world, &leg, from, orbit_points(rig.near)),
                    press: cursor,
                    moved: false,
                };
            }
        }
    }

    if buttons.pressed(MouseButton::Left)
        && let Some(i) = selection.0
    {
        let can_edit = editable(world, i);
        match &mut *drag {
            Drag::Handle {
                index,
                handle,
                press,
            } if can_edit => {
                let index = *index;
                let chain_node = world.vessels[i].trajectory.nodes().get(index).copied();
                if let Some(chain_node) = chain_node
                    && let Ok(plan) = chain_node.plan
                {
                    let (node, handles) = node_geometry(&plan, rig.focus, projector.eye);
                    if let (Some(a), Some(b)) =
                        (projector.screen(node), projector.screen(handles[*handle]))
                    {
                        // Je weiter man vom Druckpunkt wegzieht, desto schneller
                        // ändert sich das Δv (wie in KSP). Umschalt: zehnmal feiner.
                        let pull = pull_along(cursor, *press, a, b) as f64;
                        let fine =
                            keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                        let rate =
                            pull.signum() * 0.02 * pull * pull * if fine { 0.1 } else { 1.0 };
                        let (axis, sign, _) = HANDLES[*handle];
                        let mut node = chain_node.node;
                        node.delta_v[axis] += sign * rate * real_time.delta_secs_f64();

                        // Anschlag am Tankinhalt (Masse am Brennbeginn).
                        let params = world.params(i);
                        let available =
                            params.exhaust_velocity * (plan.burn.start_mass / params.dry_mass).ln();
                        let (limited, hit) =
                            limit_to_tank(chain_node.node.delta_v, node.delta_v, axis, available);
                        node.delta_v = limited;
                        if hit {
                            message.say(
                                format!("Tank leer: höchstens {available:.0} m/s"),
                                world.time,
                                real_time.elapsed_secs_f64(),
                            );
                        }
                        if let Err(e) = world.set_node(i, index, node) {
                            failure = Some(e);
                            *drag = Drag::None;
                        }
                    }
                } else {
                    *drag = Drag::None;
                }
            }
            Drag::Node {
                index,
                path,
                press,
                moved,
            } if can_edit => {
                *moved = *moved || press.distance(cursor) >= CLICK_TOLERANCE;
                let index = *index;
                let chain_node = world.vessels[i].trajectory.nodes().get(index).copied();
                if !*moved {
                    // Noch ein Klick: nur gewählt.
                } else if let Some(chain_node) = chain_node
                    && let Some((_, time)) =
                        nearest_on_path(&projector, path, &rig, world.time, cursor)
                {
                    let node = ManeuverNode {
                        time,
                        ..chain_node.node
                    };
                    match world.set_node(i, index, node) {
                        Ok(()) => {
                            selected.0 = Some(time);
                            // Liegt die Kamera auf diesem Node, wandert sie mit.
                            let old = Focus::Node {
                                vessel: i,
                                time: chain_node.node.time,
                            };
                            if rig.target == old {
                                rig.target = Focus::Node { vessel: i, time };
                            }
                        }
                        Err(e) => {
                            failure = Some(e);
                            *drag = Drag::None;
                        }
                    }
                } else if chain_node.is_none() {
                    *drag = Drag::None;
                }
            }
            Drag::Handle { .. } | Drag::Node { .. } => *drag = Drag::None,
            _ => {}
        }
    }

    if buttons.just_released(MouseButton::Left) {
        if let Drag::Pending(press) = *drag
            && press.distance(cursor) < CLICK_TOLERANCE
        {
            failure = click(
                world,
                &mut selection,
                &mut selected,
                &mut zone,
                &projector,
                &rig,
                cursor,
            )
            .err();
        }
        *drag = Drag::None;
    }
    if let Some(e) = failure {
        message.show(e, world.time, real_time.elapsed_secs_f64());
    }
}

/// Mitte eines Zielgebiets (knapp über dem Boden), im Simulationssystem.
pub fn zone_center(world: &World, k: usize) -> DVec3 {
    let zone = &world.zones[k];
    let body = &world.bodies[zone.body];
    body.body_fixed_to_inertial(
        zone.center.body_fixed_direction() * (body.radius + crate::view::SURFACE_LIFT),
        world.time,
    )
}

/// Klick: zuerst Schiffe, dann Zielgebiete, dann die Bahn des gewählten
/// Schiffs. Ein Klick auf die Bahn fügt einen Node in die Kette ein, auch auf
/// Bahnstücke nach einem geplanten Manöver.
#[allow(clippy::too_many_arguments)]
fn click(
    world: &mut World,
    selection: &mut Selection,
    selected: &mut SelectedNode,
    zone: &mut SelectedZone,
    projector: &Projector,
    rig: &CameraRig,
    cursor: Vec2,
) -> Result<(), PlanError> {
    let (focus, points) = (rig.focus, orbit_points(rig.near));
    let vessel_hit = (0..world.vessels.len())
        .filter_map(|i| {
            let position = to_view(world.vessel_state(i).1.position, focus);
            projector.project(position).map(|s| (s.distance(cursor), i))
        })
        .filter(|(d, _)| *d < PICK_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, i)) = vessel_hit {
        // Ein angedocktes Schiff steht für seinen Verbund.
        let i = world.host(i);
        if selection.0 != Some(i) {
            selected.0 = None;
        }
        selection.0 = Some(i);
        zone.0 = None;
        return Ok(());
    }

    let zone_hit = (0..world.zones.len())
        .filter_map(|k| {
            let center = to_view(zone_center(world, k), focus);
            projector.project(center).map(|s| (s.distance(cursor), k))
        })
        .filter(|(d, _)| *d < PICK_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, k)) = zone_hit {
        selection.0 = None;
        selected.0 = None;
        zone.0 = Some(k);
        return Ok(());
    }

    if let Some(i) = selection.0 {
        let path = coast_path(world, &world.vessels[i].trajectory, world.time, points);
        if let Some((distance, time)) = nearest_on_path(projector, &path, rig, world.time, cursor)
            && distance < PICK_RADIUS
        {
            let node = ManeuverNode {
                time,
                delta_v: DVec3::ZERO,
            };
            world.add_node(i, node)?;
            selected.0 = Some(time);
            // Vorerst ist jede Kette sofort scharf.
            world.set_armed(i, true)?;
        }
    }
    Ok(())
}

/// Ring aus zwei Kreisen, zur Kamera gedreht.
fn ring(gizmos: &mut Gizmos<NodeGizmos>, center: Vec3, eye: Vec3, radius: f32, color: Color) {
    let rotation = Quat::from_rotation_arc(Vec3::Z, (eye - center).normalize());
    for r in [1.0, 0.5] {
        gizmos
            .circle(Isometry3d::new(center, rotation), radius * r, color)
            .resolution(16);
    }
}

/// Nodes des gewählten Schiffs zeichnen, der gewählte größer und mit Griffen.
/// Während des Brennens keine Griffe, dann lässt sich nichts ändern. Ein
/// gezogener Griff folgt der Maus entlang seiner Achse und springt beim
/// Loslassen zurück. Ungültige Nodes haben keinen Ort auf der Bahn und
/// erscheinen nur in der Liste.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    sim: Res<Sim>,
    selection: Res<Selection>,
    selected: Res<SelectedNode>,
    rig: Res<CameraRig>,
    drag: Res<Drag>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
    mut gizmos: Gizmos<NodeGizmos>,
) {
    let world = &sim.0;
    let Some(i) = selection.0 else { return };
    let trajectory = &world.vessels[i].trajectory;
    let chosen = selected_node(world, &selection, &selected).map(|(_, k)| k);
    let color = if trajectory.armed() {
        ARMED_COLOR
    } else {
        UNARMED_COLOR
    };
    let (camera, transform) = *camera;
    let eye = transform.translation;
    for (k, plan) in trajectory.plans().enumerate() {
        let (node, mut handles) = node_geometry(plan, rig.focus, eye);
        let distance = node.distance(eye);
        if chosen != Some(k) {
            ring(
                &mut gizmos,
                node,
                eye,
                distance * NODE_RADIUS * OTHER_NODE_SCALE,
                color,
            );
            continue;
        }
        ring(&mut gizmos, node, eye, distance * NODE_RADIUS, color);
        if !editable(world, i) {
            continue;
        }
        if let Drag::Handle { handle, press, .. } = &*drag
            && let Some(cursor) = window.cursor_position()
        {
            let view = GlobalTransform::from(*transform);
            if let (Ok(a), Ok(b)) = (
                camera.world_to_viewport(&view, node),
                camera.world_to_viewport(&view, handles[*handle]),
            ) && a.distance(b) > 1.0
            {
                let factor = (1.0 + pull_along(cursor, *press, a, b) / a.distance(b)).max(0.3);
                handles[*handle] = node + (handles[*handle] - node) * factor;
            }
        }
        for (h, (_, _, color)) in HANDLES.iter().enumerate() {
            gizmos.linestrip([node, handles[h]], *color);
            ring(
                &mut gizmos,
                handles[h],
                eye,
                distance * HANDLE_RADIUS,
                *color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tank_limit_caps_growth_but_allows_shrinking() {
        let old = DVec3::new(500.0, 0.0, 0.0);
        // Über den Vorrat hinaus: auf den Vorrat begrenzt.
        let (dv, hit) = limit_to_tank(old, DVec3::new(800.0, 0.0, 0.0), 0, 669.0);
        assert!(hit && (dv.length() - 669.0).abs() < 1e-9);
        // Mit Anteil auf anderer Achse: Gesamtbetrag begrenzt.
        let (dv, hit) = limit_to_tank(
            DVec3::new(500.0, 300.0, 0.0),
            DVec3::new(700.0, 300.0, 0.0),
            0,
            669.0,
        );
        assert!(hit && (dv.length() - 669.0).abs() < 1e-9 && dv.y == 300.0);
        // Verkleinern geht immer, auch oberhalb des Vorrats.
        let (dv, hit) = limit_to_tank(DVec3::X * 900.0, DVec3::X * 850.0, 0, 669.0);
        assert!(!hit && dv.x == 850.0);
        // Retrograd: Vorzeichen bleibt.
        let (dv, _) = limit_to_tank(DVec3::X * -500.0, DVec3::X * -800.0, 0, 669.0);
        assert!((dv.x + 669.0).abs() < 1e-9);
    }
}
