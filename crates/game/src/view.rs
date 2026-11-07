//! Darstellung: Planet, Atmosphäre, Basen, Schiffe und ihre Bahnen.

use crate::Sim;
use crate::camera::{self, CameraRig, to_view};
use bevy::math::DVec3;
use bevy::prelude::*;
use sim::{Segment, SurfacePoint, VesselStatus};

const ORBIT_COLOR: Color = Color::srgb(0.35, 0.75, 1.0);
/// Bahnstücke durch die Luft heben sich farblich ab.
const ATMOSPHERE_PATH_COLOR: Color = Color::srgb(1.0, 0.55, 0.15);
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
#[allow(clippy::type_complexity)]
pub fn sync_markers(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    materials: Res<MarkerMaterials>,
    camera: Single<&Transform, With<Camera3d>>,
    mut planets: Query<(&PlanetMarker, &mut Transform), Without<Camera3d>>,
    mut bases: Query<(&BaseMarker, &mut Transform), (Without<Camera3d>, Without<PlanetMarker>)>,
    mut vessels: Query<
        (
            &VesselMarker,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (
            Without<Camera3d>,
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

/// Zeichnet für jedes Schiff die Bahn ab jetzt: den aktuellen Abschnitt und,
/// falls schon bekannt, den nächsten. Stücke durch die Luft in Orange.
pub fn draw_trajectories(sim: Res<Sim>, rig: Res<CameraRig>, mut gizmos: Gizmos) {
    let world = &sim.0;
    let now = world.time;
    for vessel in &world.vessels {
        for segment in vessel.trajectory.segments() {
            let body = &world.bodies[segment.body()];
            let from = segment.start().max(now);
            match segment {
                Segment::Kepler { state, .. } => {
                    // Eine geschlossene Bahn einmal ganz, eine offene drei Stunden weit.
                    let span = sim::kepler::period(body.gm, state).unwrap_or(3.0 * 3600.0);
                    let to = segment.end().min(from + span);
                    let points = 256;
                    gizmos.linestrip(
                        (0..=points).map(|i| {
                            let t = from + (to - from) * i as f64 / points as f64;
                            to_view(segment.state_at(&world.bodies, t).position, rig.focus)
                        }),
                        ORBIT_COLOR,
                    );
                }
                Segment::Integrated { samples, .. } => {
                    let start = segment.state_at(&world.bodies, from).position;
                    let rest = samples.iter().filter(|s| s.time > from);
                    let step = (samples.len() / 400).max(1);
                    gizmos.linestrip(
                        std::iter::once(start)
                            .chain(rest.step_by(step).map(|s| s.state.position))
                            .chain(std::iter::once(samples[samples.len() - 1].state.position))
                            .map(|p| to_view(p, rig.focus)),
                        ATMOSPHERE_PATH_COLOR,
                    );
                }
                Segment::Surface { .. } => {}
            }
        }
    }
}
