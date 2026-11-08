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
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::{ManeuverNode, Plan, Segment, Trajectory, World, kepler, maneuver};

/// Trefferradius in Pixeln.
const PICK_RADIUS: f32 = 14.0;
/// Bis zu dieser Mausbewegung in Pixeln gilt Drücken + Loslassen als Klick.
const CLICK_TOLERANCE: f32 = 5.0;
/// Länge der Griffe relativ zum Kameraabstand (gleich groß auf dem Bildschirm).
const HANDLE_LENGTH: f32 = 0.08;
/// Radius der Griff- und Node-Ringe relativ zum Kameraabstand.
const HANDLE_RADIUS: f32 = 0.008;
const NODE_RADIUS: f32 = 0.012;

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
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    rig: Res<CameraRig>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &Transform), With<MainCamera>>,
) {
    let world = &mut sim.0;
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
        // Während des Brennens nicht möglich; dann passiert einfach nichts.
        let _ = world.clear_node(i);
    }

    let Some(cursor) = window.cursor_position() else {
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
                    if world.set_node(i, node).is_err() {
                        *drag = Drag::None;
                    }
                }
            }
            (Drag::Node(path), Some(plan)) => {
                if let Some((_, time)) =
                    nearest_on_path(&projector, path, rig.focus, world.time, cursor)
                {
                    let node = ManeuverNode { time, ..plan.node };
                    if world.set_node(i, node).is_err() {
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
            click(world, &mut selection, &projector, rig.focus, cursor);
        }
        *drag = Drag::None;
    }
}

/// Klick: zuerst Schiffe, dann (ohne Node) die Bahn des gewählten Schiffs.
fn click(
    world: &mut World,
    selection: &mut Selection,
    projector: &Projector,
    focus: DVec3,
    cursor: Vec2,
) {
    let vessel_hit = (0..world.vessels.len())
        .filter_map(|i| {
            let position = to_view(world.vessel_state(i).1.position, focus);
            projector.project(position).map(|s| (s.distance(cursor), i))
        })
        .filter(|(d, _)| *d < PICK_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, i)) = vessel_hit {
        selection.0 = Some(i);
        return;
    }

    if let Some(i) = selection.0
        && world.vessels[i].trajectory.plan().is_none()
    {
        let path = coast_path(world, &world.vessels[i].trajectory);
        if let Some((distance, time)) = nearest_on_path(projector, &path, focus, world.time, cursor)
            && distance < PICK_RADIUS
        {
            let _ = world.set_node(
                i,
                ManeuverNode {
                    time,
                    delta_v: DVec3::ZERO,
                },
            );
        }
    }
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
    ring(
        &mut gizmos,
        node,
        eye,
        distance * NODE_RADIUS,
        Color::srgb(0.6, 0.85, 1.0),
    );
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
