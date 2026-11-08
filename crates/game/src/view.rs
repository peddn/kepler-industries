//! Darstellung: Planet, Atmosphäre, Basen, Schiffe und ihre Bahnen.

use crate::Sim;
use crate::camera::{self, CameraRig, MainCamera, to_view};
use crate::node_ui::Selection;
use bevy::math::DVec3;
use bevy::prelude::*;
use sim::integrator::Sample;
use sim::{Event, Segment, SurfacePoint, VesselStatus, World, kepler};

/// Aktuelle Bahn.
const ORBIT_COLOR: Color = Color::srgb(0.35, 0.75, 1.0);
/// Bahnstücke durch die Luft heben sich farblich ab.
const ATMOSPHERE_PATH_COLOR: Color = Color::srgb(1.0, 0.55, 0.15);
/// Brennmanöver.
const BURN_COLOR: Color = Color::srgb(1.0, 0.95, 0.45);
/// Bahn nach einem geplanten Manöver.
const PLANNED_COLOR: Color = Color::srgb(0.8, 0.5, 1.0);
const IMPACT_COLOR: Color = Color::srgb(1.0, 0.25, 0.25);
const GRID_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
/// Größe der Markierungen relativ zum Abstand zur Kamera (bleibt auf dem
/// Bildschirm gleich groß, egal wie weit man herauszoomt).
const MARKER_SCALE: f32 = 0.007;

#[derive(Component)]
pub struct PlanetMarker;

#[derive(Component)]
pub struct BaseMarker(usize);

#[derive(Component)]
pub struct VesselMarker(usize);

#[derive(Resource)]
pub struct MarkerMaterials {
    vessel: Handle<StandardMaterial>,
    selected: Handle<StandardMaterial>,
    wreck: Handle<StandardMaterial>,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sim: Res<Sim>,
) {
    camera::spawn(&mut commands);

    // Sonne: weit entfernt, daher paralleles Licht.
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            ..default()
        },
        Transform::default().looking_to(Vec3::new(-1.0, -0.3, -0.6), Vec3::Y),
    ));
    // Etwas Grundhelligkeit, damit die Nachtseite nicht ganz schwarz ist.
    commands.insert_resource(GlobalAmbientLight {
        brightness: 300.0,
        ..default()
    });

    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(96, 48));
    let unlit = |color: Color| StandardMaterial {
        base_color: color,
        unlit: true,
        ..default()
    };

    for body in &sim.0.bodies {
        commands.spawn((
            PlanetMarker,
            Mesh3d(sphere.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.42, 0.32),
                perceptual_roughness: 0.9,
                ..default()
            })),
            Transform::from_scale(Vec3::splat(body.radius as f32)),
        ));
        if body.atmosphere.exists() {
            let outer = (body.radius + body.atmosphere.top_altitude) as f32;
            commands.spawn((
                PlanetMarker,
                Mesh3d(sphere.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(0.45, 0.65, 1.0, 0.12),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_scale(Vec3::splat(outer)),
            ));
        }
    }

    let base_material = materials.add(unlit(Color::srgb(0.3, 1.0, 0.4)));
    for i in 0..sim.0.bases.len() {
        commands.spawn((
            BaseMarker(i),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(base_material.clone()),
            Transform::default(),
        ));
    }

    let markers = MarkerMaterials {
        vessel: materials.add(unlit(Color::srgb(1.0, 1.0, 1.0))),
        selected: materials.add(unlit(Color::srgb(1.0, 0.85, 0.2))),
        wreck: materials.add(unlit(Color::srgb(1.0, 0.2, 0.2))),
    };
    for i in 0..sim.0.vessels.len() {
        commands.spawn((
            VesselMarker(i),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(markers.vessel.clone()),
            Transform::default(),
        ));
    }
    commands.insert_resource(markers);
}

fn marker_scale(position: Vec3, camera: Vec3) -> Vec3 {
    Vec3::splat(position.distance(camera) * MARKER_SCALE)
}

/// Überträgt die Positionen aus der Simulation auf die Bevy-Objekte.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn sync_markers(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    selection: Res<Selection>,
    materials: Res<MarkerMaterials>,
    camera: Single<&Transform, With<MainCamera>>,
    mut planets: Query<(&PlanetMarker, &mut Transform), Without<MainCamera>>,
    mut bases: Query<(&BaseMarker, &mut Transform), (Without<MainCamera>, Without<PlanetMarker>)>,
    mut vessels: Query<
        (
            &VesselMarker,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (
            Without<MainCamera>,
            Without<PlanetMarker>,
            Without<BaseMarker>,
        ),
    >,
) {
    let world = &sim.0;
    let camera_position = camera.translation;

    for (_, mut transform) in &mut planets {
        // In PR 1 sitzt der einzige Körper im Ursprung der Simulation.
        transform.translation = to_view(DVec3::ZERO, rig.focus);
    }

    for (BaseMarker(i), mut transform) in &mut bases {
        let base = &world.bases[*i];
        let body = &world.bodies[base.body];
        let p = to_view(
            body.surface_to_inertial(base.location, 0.0, world.time),
            rig.focus,
        );
        transform.translation = p;
        transform.scale = marker_scale(p, camera_position);
    }

    for (VesselMarker(i), mut transform, mut material) in &mut vessels {
        let (_, state) = world.vessel_state(*i);
        let p = to_view(state.position, rig.focus);
        transform.translation = p;
        transform.scale = marker_scale(p, camera_position);
        let wanted = match world.vessel_status(*i) {
            VesselStatus::Wreck { .. } => &materials.wreck,
            _ if selection.0 == Some(*i) => &materials.selected,
            _ => &materials.vessel,
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

/// Breiten- und Längenlinien, die sich mit dem Planeten drehen. Ohne sie wäre
/// die Rotation einer einfarbigen Kugel nicht zu sehen.
pub fn draw_planet_grid(sim: Res<Sim>, rig: Res<CameraRig>, mut gizmos: Gizmos) {
    let world = &sim.0;
    for body in &world.bodies {
        let radius = body.radius * 1.002;
        let point = |lat: f64, lon: f64| {
            let p = SurfacePoint::from_degrees(lat, lon);
            to_view(
                body.body_fixed_to_inertial(p.body_fixed_direction() * radius, world.time),
                rig.focus,
            )
        };
        for lat in [-60.0, -30.0, 0.0, 30.0, 60.0] {
            gizmos.linestrip((0..=120).map(|i| point(lat, i as f64 * 3.0)), GRID_COLOR);
        }
        for lon in (0..12).map(|i| i as f64 * 30.0) {
            gizmos.linestrip(
                (0..=60).map(|i| point(-90.0 + i as f64 * 3.0, lon)),
                GRID_COLOR,
            );
        }
    }
}

/// Umlaufzeit eines Keplerabschnitts, bei offenen Bahnen drei Stunden.
fn span(world: &World, segment: Option<&Segment>) -> f64 {
    match segment {
        Some(Segment::Kepler { state, .. }) => {
            kepler::period(world.bodies[segment.unwrap().body()].gm, state).unwrap_or(3.0 * 3600.0)
        }
        _ => 3.0 * 3600.0,
    }
}

/// Zeichnet für jedes Schiff die vorausberechnete Bahn ab jetzt.
///
/// Blau die aktuelle Bahn, orange Stücke durch die Luft, gelb das Brennen,
/// violett die Bahn danach. Wie in KSP wird die aktuelle Bahn bei einem
/// geplanten Manöver trotzdem ganz gezeigt (bis zum Eintritt in die Luft).
/// Gezeichnet wird ein Umlauf voraus, bei einem Manöver zusätzlich ein Umlauf
/// der neuen Bahn; sonst stapeln sich bei zerfallenden Bahnen die Ellipsen.
pub fn draw_trajectories(sim: Res<Sim>, rig: Res<CameraRig>, mut gizmos: Gizmos) {
    let world = &sim.0;
    let now = world.time;
    for vessel in &world.vessels {
        let trajectory = &vessel.trajectory;
        let first_kepler = trajectory
            .segments()
            .find(|s| matches!(s, Segment::Kepler { .. }));
        let mut horizon = now + span(world, first_kepler);
        if let Some(plan) = trajectory.plan() {
            let after = span(world, trajectory.segment_after_burn());
            horizon = horizon.max(plan.burn.end + after);
        }

        let mut after_burn = false;
        for segment in trajectory.segments() {
            if segment.start() >= horizon {
                break;
            }
            let from = segment.start().max(now);
            match segment {
                Segment::Kepler {
                    start, state, end, ..
                } => {
                    let body = &world.bodies[segment.body()];
                    // Eine geschlossene Bahn einmal ganz, eine offene drei Stunden weit.
                    let span = kepler::period(body.gm, state).unwrap_or(3.0 * 3600.0);
                    let end = match end {
                        Some((_, Event::BurnStart)) => {
                            kepler::time_to_radius_inbound(body.gm, state, body.boundary_radius())
                                .map_or(f64::INFINITY, |dt| start + dt)
                        }
                        _ => segment.end(),
                    };
                    let to = end.min(from + span).min(horizon.max(from));
                    let color = if after_burn {
                        PLANNED_COLOR
                    } else {
                        ORBIT_COLOR
                    };
                    gizmos.linestrip(
                        (0..=256).map(|i| {
                            let t = from + (to - from) * i as f64 / 256.0;
                            to_view(segment.state_at(&world.bodies, t).position, rig.focus)
                        }),
                        color,
                    );
                }
                Segment::Integrated { samples, burn, .. } => {
                    let color = if burn.is_some() {
                        after_burn = true;
                        if segment.start() <= now && now < segment.end() {
                            draw_momentary_orbit(&mut gizmos, world, segment, now, rig.focus);
                        }
                        BURN_COLOR
                    } else {
                        ATMOSPHERE_PATH_COLOR
                    };
                    draw_samples(&mut gizmos, world, segment, samples, from, rig.focus, color);
                }
                Segment::Surface { .. } => {}
            }
        }
    }
}

/// Während des Brennens: die Bahn, die das Schiff hätte, wenn das Triebwerk
/// jetzt abschaltete (wie in KSP). Sie verformt sich sichtbar von der alten zur
/// neuen Bahn. Abgeschnitten am Eintritt in die Luft; in der Luft gar nicht,
/// dort zählt der berechnete Weg.
fn draw_momentary_orbit(
    gizmos: &mut Gizmos,
    world: &World,
    segment: &Segment,
    now: f64,
    focus: DVec3,
) {
    let body = &world.bodies[segment.body()];
    let state = segment.state_at(&world.bodies, now);
    let boundary = body.boundary_radius();
    if state.position.length() < boundary {
        return;
    }
    let span = kepler::period(body.gm, &state).unwrap_or(3.0 * 3600.0);
    let to =
        kepler::time_to_radius_inbound(body.gm, &state, boundary).map_or(span, |dt| dt.min(span));
    gizmos.linestrip(
        (0..=256).map(|i| {
            let dt = to * i as f64 / 256.0;
            to_view(kepler::propagate(body.gm, state, dt).position, focus)
        }),
        ORBIT_COLOR,
    );
}

fn draw_samples(
    gizmos: &mut Gizmos,
    world: &World,
    segment: &Segment,
    samples: &[Sample],
    from: f64,
    focus: DVec3,
    color: Color,
) {
    let start = segment.state_at(&world.bodies, from).position;
    let rest = samples.iter().filter(|s| s.time > from);
    let step = (samples.len() / 400).max(1);
    gizmos.linestrip(
        std::iter::once(start)
            .chain(rest.step_by(step).map(|s| s.state.position))
            .chain(std::iter::once(samples[samples.len() - 1].state.position))
            .map(|p| to_view(p, focus)),
        color,
    );
}

/// Gelbe Ringe auf der Bahn, wo ein geplantes Brennen beginnt und endet. Für
/// alle Schiffe mit Node, damit man geplante Manöver auf einen Blick sieht.
pub fn draw_burn_points(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    let eye = camera.translation;
    for vessel in &world.vessels {
        let Some(plan) = vessel.trajectory.plan() else {
            continue;
        };
        for t in [plan.burn.start, plan.burn.end] {
            if t <= world.time {
                continue;
            }
            let Some(segment) = vessel
                .trajectory
                .segments()
                .find(|s| s.start() <= t && t <= s.end())
            else {
                continue;
            };
            let p = to_view(segment.state_at(&world.bodies, t).position, rig.focus);
            let rotation = Quat::from_rotation_arc(Vec3::Z, (eye - p).normalize());
            gizmos
                .circle(
                    Isometry3d::new(p, rotation),
                    p.distance(eye) * 0.006,
                    BURN_COLOR,
                )
                .resolution(16);
        }
    }
}

/// Rotes Kreuz auf der Oberfläche, wo ein Schiff laut Vorausberechnung
/// aufschlagen wird. Es dreht sich mit dem Planeten.
pub fn draw_impacts(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    for vessel in &world.vessels {
        let Some((time, body_id, point)) = vessel.trajectory.predicted_impact() else {
            continue;
        };
        if time <= world.time {
            continue; // schon abgestürzt, das Wrack ist markiert
        }
        let body = &world.bodies[body_id];
        let up = body.body_fixed_to_inertial(point.body_fixed_direction(), world.time);
        let east = DVec3::Z.cross(up).try_normalize().unwrap_or(DVec3::X);
        let north = up.cross(east);
        let center = to_view(up * body.radius * 1.002, rig.focus);
        let arm = center.distance(camera.translation) * 0.015;
        for d in [east + north, east - north] {
            let d = camera::dir_to_view(d.normalize()) * arm;
            gizmos.line(center - d, center + d, IMPACT_COLOR);
        }
    }
}
