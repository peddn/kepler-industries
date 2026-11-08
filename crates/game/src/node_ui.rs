//! Bedienung des Manöver-Nodes: Schiff wählen, Node setzen und verschieben,
//! Δv über sechs Griffe einstellen (wie in KSP).
//!
//! Alles läuft über die Simulation: Jede Änderung ruft `World::set_node` auf,
//! die Zeitleiste wird neu berechnet und ist sofort die Vorschau.
//!
//! Treffer werden im Bildschirm geprüft (Abstand in Pixeln). Punkte hinter dem
//! Planeten zählen nicht. Annahme bis zum Mond-PR: Der Planet liegt im Fokus.

use crate::Sim;
use crate::camera::{CameraRig, MainCamera, OVERLAY_LAYER, dir_to_view, to_view};
use crate::hud::UiFont;
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
/// Node-Farbe: scharf orange, sonst hellblau.
const ARMED_COLOR: Color = Color::srgb(1.0, 0.55, 0.1);
const UNARMED_COLOR: Color = Color::srgb(0.6, 0.85, 1.0);
/// Lage und Größe der Schaltfläche "scharf" relativ zum Node (Bildschirm).
/// Rechts außerhalb der Griffe (die reichen höchstens ~77 px weit), damit sie
/// sich nie überdecken.
const BUTTON_OFFSET: Vec2 = Vec2::new(85.0, -13.0);
const BUTTON_SIZE: Vec2 = Vec2::new(170.0, 26.0);

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

/// Kurze Meldung in der Anzeige, z. B. warum ein Node nicht geht.
#[derive(Resource, Default)]
pub struct UiMessage {
    pub text: String,
    /// Echtzeit (s), bis zu der die Meldung sichtbar bleibt.
    pub until: f64,
}

impl UiMessage {
    fn show(&mut self, error: PlanError, now: f64) {
        self.text = error.to_string();
        self.until = now + 3.0;
    }
}

/// Was die linke Maustaste gerade tut.
#[derive(Resource, Default)]
pub enum Drag {
    #[default]
    None,
    /// Gedrückt, noch nicht entschieden: Klick oder nichts.
    Pending(Vec2),
    /// Griff wird gezogen.
    Handle { handle: usize, press: Vec2 },
    /// Node wird entlang der Bahn (ohne Manöver) verschoben.
    Node(Vec<(f64, DVec3)>),
}

/// Eigene Gizmo-Gruppe für Node und Griffe. Sie wird von der Overlay-Kamera
/// gezeichnet und liegt damit immer über der Welt, sonst verschwände z. B. der
/// Griff "radial nach innen" bei niedrigen Bahnen im Planeten.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct NodeGizmos;

pub fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<NodeGizmos>();
    config.render_layers = RenderLayers::layer(OVERLAY_LAYER);
    config.line.width = 2.5;
}

/// Schaltfläche neben dem Node, mit der er scharf geschaltet wird.
#[derive(Component)]
pub struct ArmButton;

#[derive(Component)]
pub struct ArmButtonText;

pub fn setup_button(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            ArmButton,
            Node {
                position_type: PositionType::Absolute,
                width: px(BUTTON_SIZE.x),
                height: px(BUTTON_SIZE.y),
                border: UiRect::all(px(1)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            BorderColor::all(Color::WHITE),
        ))
        .with_child((
            ArmButtonText,
            Text::new(""),
            TextFont {
                font: font.0.clone().into(),
                font_size: FontSize::Px(14.0),
                ..default()
            },
        ));
}

/// Bildschirmbereich der Schaltfläche, wenn der Node bei `node` liegt.
fn button_rect(node: Vec2) -> Rect {
    Rect::from_corners(node + BUTTON_OFFSET, node + BUTTON_OFFSET + BUTTON_SIZE)
}

/// Schaltfläche an den Node des gewählten Schiffs schieben und beschriften.
/// Während des Brennens ausgeblendet, dann ist nichts mehr zu schalten.
pub fn sync_button(
    sim: Res<Sim>,
    selection: Res<Selection>,
    rig: Res<CameraRig>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
    mut button: Single<(&mut Node, &mut BackgroundColor, &mut BorderColor), With<ArmButton>>,
    mut label: Single<&mut Text, With<ArmButtonText>>,
) {
    let world = &sim.0;
    let (camera, transform) = *camera;
    let screen = selection.0.and_then(|i| {
        let trajectory = &world.vessels[i].trajectory;
        let plan = trajectory
            .plan()
            .filter(|_| !trajectory.current().is_burn())?;
        let node = to_view(plan.node_state.position, rig.focus);
        let screen = camera
            .world_to_viewport(&GlobalTransform::from(*transform), node)
            .ok()?;
        Some((screen, plan.armed))
    });
    let (node, background, border) = &mut *button;
    let Some((screen, armed)) = screen else {
        node.display = Display::None;
        return;
    };
    let rect = button_rect(screen);
    node.display = Display::Flex;
    node.left = px(rect.min.x);
    node.top = px(rect.min.y);
    let (text, fill) = if armed {
        ("● Scharf", ARMED_COLOR.darker(0.25))
    } else {
        ("○ Scharf schalten", Color::srgb(0.12, 0.14, 0.18))
    };
    background.0 = fill;
    **border = BorderColor::all(if armed { ARMED_COLOR } else { UNARMED_COLOR });
    if label.0 != text {
        label.0 = text.into();
    }
}

/// Bildschirmprojektion mit Verdeckung durch den Planeten.
struct Projector<'a> {
    camera: &'a Camera,
    transform: GlobalTransform,
    eye: Vec3,
    planet_radius: f32,
}

impl Projector<'_> {
    fn project(&self, p: Vec3) -> Option<Vec2> {
        if self.occluded(p) {
            return None;
        }
        self.camera.world_to_viewport(&self.transform, p).ok()
    }

    /// Liegt der Planet (im Ursprung) zwischen Kamera und Punkt?
    fn occluded(&self, p: Vec3) -> bool {
        let d = p - self.eye;
        let len = d.length();
        let dir = d / len;
        let b = self.eye.dot(dir);
        // Etwas kleiner, damit Punkte auf der Oberfläche sich nicht selbst verdecken.
        let r = self.planet_radius * 0.999;
        let disc = b * b - (self.eye.length_squared() - r * r);
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

/// Punkte (Zeit, Position) der Bahn ab jetzt, auf die ein Node gesetzt werden
/// kann: Freiflug bis zum ersten Brennen oder Aufschlag.
fn coast_path(world: &World, trajectory: &Trajectory) -> Vec<(f64, DVec3)> {
    let now = world.time;
    let mut points = Vec::new();
    for segment in trajectory.segments() {
        match segment {
            Segment::Kepler { state, .. } => {
                let gm = world.bodies[segment.body()].gm;
                let from = segment.start().max(now);
                let span = kepler::period(gm, state).unwrap_or(3.0 * 3600.0);
                let to = segment.end().min(from + span);
                for k in 0..=256 {
                    let t = from + (to - from) * k as f64 / 256.0;
                    points.push((t, segment.state_at(&world.bodies, t).position));
                }
            }
            Segment::Integrated {
                burn: None,
                samples,
                ..
            } => points.extend(
                samples
                    .iter()
                    .filter(|s| s.time > now)
                    .map(|s| (s.time, s.state.position)),
            ),
            _ => break,
        }
    }
    points
}

/// Nächster Punkt der Bahn zum Mauszeiger: (Abstand in Pixeln, Zeitpunkt).
/// Zwischen den Stützpunkten wird im Bildschirm interpoliert.
fn nearest_on_path(
    projector: &Projector,
    path: &[(f64, DVec3)],
    focus: DVec3,
    now: f64,
    cursor: Vec2,
) -> Option<(f32, f64)> {
    let projected: Vec<Option<(f64, Vec2)>> = path
        .iter()
        .map(|(t, p)| {
            (*t > now)
                .then(|| projector.project(to_view(*p, focus)).map(|s| (*t, s)))
                .flatten()
        })
        .collect();
    let mut best: Option<(f32, f64)> = None;
    for pair in projected.windows(2) {
        let [Some((t0, a)), Some((t1, b))] = pair else {
            continue;
        };
        let ab = *b - *a;
        let u = if ab.length_squared() > 0.0 {
            ((cursor - *a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let distance = (*a + ab * u).distance(cursor);
        if best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, t0 + (t1 - t0) * u as f64));
        }
    }
    best
}

/// Maus und Tastatur für Auswahl und Node.
#[allow(clippy::too_many_arguments)]
pub fn input(
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    mut drag: ResMut<Drag>,
    mut message: ResMut<UiMessage>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    rig: Res<CameraRig>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
) {
    let world = &mut sim.0;
    let mut failure: Option<PlanError> = None;
    let count = world.vessels.len();
    if keys.just_pressed(KeyCode::Tab) && count > 0 {
        selection.0 = Some(selection.0.map_or(0, |i| (i + 1) % count));
    }
    if keys.just_pressed(KeyCode::Escape) {
        selection.0 = None;
    }
    if (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace))
        && let Some(i) = selection.0
    {
        failure = world.clear_node(i).err();
    }
    if keys.just_pressed(KeyCode::KeyH)
        && let Some(i) = selection.0
    {
        let vessel = &mut world.vessels[i];
        vessel.halt_warp_at_burn = !vessel.halt_warp_at_burn;
    }
    if (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter))
        && let Some(i) = selection.0
        && let Some(armed) = world.vessels[i].trajectory.plan().map(|p| p.armed)
    {
        failure = world.set_node_armed(i, !armed).err();
    }

    let Some(cursor) = window.cursor_position() else {
        if let Some(e) = failure {
            message.show(e, real_time.elapsed_secs_f64());
        }
        return;
    };
    let (camera, transform) = *camera;
    let projector = Projector {
        camera,
        transform: GlobalTransform::from(*transform),
        eye: transform.translation,
        planet_radius: world.bodies[0].radius as f32,
    };
    let editable_plan = |world: &World, i: usize| {
        let trajectory = &world.vessels[i].trajectory;
        trajectory
            .plan()
            .copied()
            .filter(|_| !trajectory.current().is_burn())
    };

    if buttons.just_pressed(MouseButton::Left) {
        *drag = Drag::Pending(cursor);
        if let Some(i) = selection.0
            && let Some(plan) = editable_plan(world, i)
        {
            let (node, handles) = node_geometry(&plan, rig.focus, projector.eye);
            let near = |p: Vec3| {
                projector
                    .camera
                    .world_to_viewport(&projector.transform, p)
                    .is_ok_and(|s| s.distance(cursor) < PICK_RADIUS)
            };
            if let Some(handle) = (0..6).find(|&h| near(handles[h])) {
                *drag = Drag::Handle {
                    handle,
                    press: cursor,
                };
            } else if near(node) {
                // Bahn ohne Manöver: Auf ihr wird der Node verschoben.
                let vessel = &world.vessels[i];
                let mut free = vessel.trajectory.clone();
                let _ = free.clear_node(&world.bodies, &vessel.params);
                *drag = Drag::Node(coast_path(world, &free));
            } else if let Ok(screen) = camera.world_to_viewport(&projector.transform, node)
                && button_rect(screen).contains(cursor)
            {
                failure = world.set_node_armed(i, !plan.armed).err();
                *drag = Drag::None;
            }
        }
    }

    if buttons.pressed(MouseButton::Left)
        && let Some(i) = selection.0
    {
        let plan = editable_plan(world, i);
        match (&*drag, plan) {
            (Drag::Handle { handle, press }, Some(plan)) => {
                let (node, handles) = node_geometry(&plan, rig.focus, projector.eye);
                let to_screen = |p| camera.world_to_viewport(&projector.transform, p).ok();
                if let (Some(a), Some(b)) = (to_screen(node), to_screen(handles[*handle])) {
                    // Je weiter man vom Druckpunkt wegzieht, desto schneller ändert
                    // sich das Δv (wie in KSP). Umschalt: zehnmal feiner.
                    let pull = (cursor - *press).dot((b - a).normalize_or_zero()) as f64;
                    let fine =
                        keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                    let rate = pull.signum() * 0.02 * pull * pull * if fine { 0.1 } else { 1.0 };
                    let (axis, sign, _) = HANDLES[*handle];
                    let mut node = plan.node;
                    node.delta_v[axis] += sign * rate * real_time.delta_secs_f64();
                    if let Err(e) = world.set_node(i, node) {
                        failure = Some(e);
                        *drag = Drag::None;
                    }
                }
            }
            (Drag::Node(path), Some(plan)) => {
                if let Some((_, time)) =
                    nearest_on_path(&projector, path, rig.focus, world.time, cursor)
                {
                    let node = ManeuverNode { time, ..plan.node };
                    if let Err(e) = world.set_node(i, node) {
                        failure = Some(e);
                        *drag = Drag::None;
                    }
                }
            }
            (Drag::Handle { .. } | Drag::Node(_), None) => *drag = Drag::None,
            _ => {}
        }
    }

    if buttons.just_released(MouseButton::Left) {
        if let Drag::Pending(press) = *drag
            && press.distance(cursor) < CLICK_TOLERANCE
        {
            failure = click(world, &mut selection, &projector, rig.focus, cursor).err();
        }
        *drag = Drag::None;
    }
    if let Some(e) = failure {
        message.show(e, real_time.elapsed_secs_f64());
    }
}

/// Klick: zuerst Schiffe, dann (ohne Node) die Bahn des gewählten Schiffs.
fn click(
    world: &mut World,
    selection: &mut Selection,
    projector: &Projector,
    focus: DVec3,
    cursor: Vec2,
) -> Result<(), PlanError> {
    let vessel_hit = (0..world.vessels.len())
        .filter_map(|i| {
            let position = to_view(world.vessel_state(i).1.position, focus);
            projector.project(position).map(|s| (s.distance(cursor), i))
        })
        .filter(|(d, _)| *d < PICK_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, i)) = vessel_hit {
        selection.0 = Some(i);
        return Ok(());
    }

    if let Some(i) = selection.0
        && world.vessels[i].trajectory.plan().is_none()
    {
        let path = coast_path(world, &world.vessels[i].trajectory);
        if let Some((distance, time)) = nearest_on_path(projector, &path, focus, world.time, cursor)
            && distance < PICK_RADIUS
        {
            return world.set_node(
                i,
                ManeuverNode {
                    time,
                    delta_v: DVec3::ZERO,
                },
            );
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

/// Node und Griffe des gewählten Schiffs zeichnen. Während des Brennens nur
/// den Node, denn ändern lässt er sich dann nicht mehr.
pub fn draw(
    sim: Res<Sim>,
    selection: Res<Selection>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos<NodeGizmos>,
) {
    let world = &sim.0;
    let Some(i) = selection.0 else { return };
    let trajectory = &world.vessels[i].trajectory;
    let Some(plan) = trajectory.plan() else {
        return;
    };
    let eye = camera.translation;
    let (node, handles) = node_geometry(plan, rig.focus, eye);
    let distance = node.distance(eye);
    let color = if plan.armed {
        ARMED_COLOR
    } else {
        UNARMED_COLOR
    };
    ring(&mut gizmos, node, eye, distance * NODE_RADIUS, color);
    if trajectory.current().is_burn() {
        return;
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
