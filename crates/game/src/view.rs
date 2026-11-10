//! Darstellung: Planet, Mond, Atmosphäre, Basen, Schiffe und ihre Bahnen.
//!
//! Bahnen um den Mond werden wie in KSP gezeichnet (`anchors`): Ein Stück
//! der Bahn um einen Körper liegt um die Stelle, an der er zu Beginn des
//! Stücks steht (läuft das Stück schon: wo er jetzt steht). Es ist so eine
//! echte Kegelschnittbahn um den Mond; dort steht ein blasser "Geistermond".
//! Beim Austritt klafft deshalb eine Lücke: Der Mond ist inzwischen
//! weitergezogen. Schiffe, Nodes und Klicks rechnen mit demselben Versatz.

use crate::Sim;
use crate::camera::{self, CameraRig, MainCamera, dir_to_view, to_view};
use crate::node_ui::{SelectedBody, SelectedZone, Selection};
use crate::rendezvous::TARGET_COLOR;
use bevy::math::DVec3;
use bevy::prelude::*;
use sim::body::{body_state, sphere_of_influence};
use sim::integrator::Sample;
use sim::soi;
use sim::{BodyId, Event, Segment, SurfacePoint, Trajectory, VesselStatus, World, kepler};

/// Aktuelle Bahn.
const ORBIT_COLOR: Color = Color::srgb(0.35, 0.75, 1.0);
/// Bahnstücke durch die Luft heben sich farblich ab.
const ATMOSPHERE_PATH_COLOR: Color = Color::srgb(1.0, 0.55, 0.15);
/// Brennmanöver.
const BURN_COLOR: Color = Color::srgb(1.0, 0.95, 0.45);
/// Bahn nach einem geplanten Manöver.
const PLANNED_COLOR: Color = Color::srgb(0.8, 0.5, 1.0);
const IMPACT_COLOR: Color = Color::srgb(1.0, 0.25, 0.25);
/// Zielgebiete und Landung darin.
const ZONE_COLOR: Color = Color::srgb(0.3, 1.0, 0.45);
/// Aufsetzen außerhalb eines Zielgebiets.
const OUTSIDE_COLOR: Color = Color::srgb(1.0, 0.9, 0.2);
/// Markierung des gewählten Zielgebiets (wie ein gewähltes Schiff).
const SELECTED_ZONE_COLOR: Color = Color::srgb(1.0, 0.85, 0.2);
const GRID_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
/// Bahn des Monds und seine Einflusssphäre (gewählt: wie ein gewähltes
/// Zielgebiet).
const BODY_ORBIT_COLOR: Color = Color::srgba(0.8, 0.8, 0.85, 0.35);
const SOI_COLOR: Color = Color::srgba(0.8, 0.8, 0.85, 0.2);
/// Wechsel der Einflusssphäre auf der Bahn, und der Mond, wo er dann steht.
const SOI_CHANGE_COLOR: Color = Color::srgb(0.85, 0.85, 0.9);
const GHOST_COLOR: Color = Color::srgba(0.8, 0.8, 0.85, 0.3);
/// Stützpunkte für Mondbahn und Einflusssphäre.
const BODY_ORBIT_POINTS: usize = 720;
/// Größe der Markierungen relativ zum Abstand zur Kamera (bleibt auf dem
/// Bildschirm gleich groß, egal wie weit man herauszoomt).
const MARKER_SCALE: f32 = 0.007;
/// Höhe (m) von Zielgebieten und Aufsetzkreuz über der Oberfläche: knapp
/// darüber, damit sie aus der Nähe auf dem Boden liegen.
pub const SURFACE_LIFT: f64 = 2.0;
/// Höhe (m) des Gitters über der Oberfläche; die Sehnen zwischen den
/// Stützpunkten hängen bis etwa 1,5 m durch.
const GRID_LIFT: f64 = 10.0;
/// Auflösung der Planetenkugel: Die Dreiecke liegen höchstens etwa 6 m unter
/// der echten Oberfläche (aus der Nähe wichtig, aus der Ferne unsichtbar).
const PLANET_SEGMENTS: (u32, u32) = (720, 360);
/// Stützpunkte pro Umlauf einer Bahnlinie: in der Karte 256 (bis 50 m neben
/// der echten Bahn, aus der Ferne unsichtbar), in der Nahansicht 4096 (unter
/// 0,2 m).
const ORBIT_POINTS: usize = 256;
const NEAR_ORBIT_POINTS: usize = 4096;

/// Wo ein Schiff jetzt wirklich ist (relativ zum Planeten).
pub fn vessel_position(world: &World, i: usize) -> DVec3 {
    let (body, state) = world.vessel_state(i);
    body_state(&world.bodies, body, world.time).position + state.position
}

/// Zeichen-Versatz für jeden Abschnitt der Zeitleiste: wo sein Körper beim
/// Zeichnen steht. Aufeinanderfolgende Abschnitte um denselben Körper bilden
/// ein Stück; es liegt um den Körper zu Beginn des Stücks, frühestens jetzt.
/// Beim Planeten ist das der Ursprung.
pub fn anchors(world: &World, trajectory: &Trajectory) -> Vec<DVec3> {
    let mut patch: Option<(BodyId, f64)> = None;
    trajectory
        .segments()
        .map(|segment| {
            if patch.is_none_or(|(body, _)| body != segment.body()) {
                patch = Some((segment.body(), segment.start()));
            }
            let (body, start) = patch.expect("gesetzt");
            body_state(&world.bodies, body, start.max(world.time)).position
        })
        .collect()
}

/// Versatz des Abschnitts, in dem der Zeitpunkt `t` liegt (siehe `anchors`).
pub fn anchor_at(world: &World, trajectory: &Trajectory, t: f64) -> DVec3 {
    trajectory
        .segments()
        .zip(anchors(world, trajectory))
        .find(|(s, _)| s.start() <= t && t <= s.end())
        .map_or(DVec3::ZERO, |(_, anchor)| anchor)
}

/// Stützpunkte pro Umlauf für Bahnlinien (und das Anklicken der Bahn).
pub fn orbit_points(near: bool) -> usize {
    if near {
        NEAR_ORBIT_POINTS
    } else {
        ORBIT_POINTS
    }
}

/// Kugel (und Atmosphäre) eines Himmelskörpers.
#[derive(Component)]
pub struct BodyMarker(BodyId);

#[derive(Component)]
pub struct BaseMarker(usize);

#[derive(Component)]
pub struct VesselMarker(usize);

#[derive(Resource)]
pub struct MarkerMaterials {
    vessel: Handle<StandardMaterial>,
    selected: Handle<StandardMaterial>,
    /// Ziel des gewählten Schiffs.
    target: Handle<StandardMaterial>,
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
    let planet_sphere = meshes.add(
        Sphere::new(1.0)
            .mesh()
            .uv(PLANET_SEGMENTS.0, PLANET_SEGMENTS.1),
    );
    let unlit = |color: Color| StandardMaterial {
        base_color: color,
        unlit: true,
        ..default()
    };

    for (b, body) in sim.0.bodies.iter().enumerate() {
        // Der Planet grün, Monde grau.
        let color = if b == 0 {
            Color::srgb(0.18, 0.42, 0.32)
        } else {
            Color::srgb(0.5, 0.5, 0.52)
        };
        commands.spawn((
            BodyMarker(b),
            Mesh3d(planet_sphere.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.9,
                ..default()
            })),
            Transform::from_scale(Vec3::splat(body.radius as f32)),
        ));
        if body.atmosphere.exists() {
            let outer = (body.radius + body.atmosphere.top_altitude) as f32;
            commands.spawn((
                BodyMarker(b),
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
        target: materials.add(unlit(TARGET_COLOR)),
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
    mut planets: Query<(&BodyMarker, &mut Transform), Without<MainCamera>>,
    mut bases: Query<(&BaseMarker, &mut Transform), (Without<MainCamera>, Without<BodyMarker>)>,
    mut vessels: Query<
        (
            &VesselMarker,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (
            Without<MainCamera>,
            Without<BodyMarker>,
            Without<BaseMarker>,
        ),
    >,
) {
    let world = &sim.0;
    let camera_position = camera.translation;
    let target = selection.0.and_then(|i| world.vessels[i].target);

    for (BodyMarker(b), mut transform) in &mut planets {
        let center = body_state(&world.bodies, *b, world.time).position;
        transform.translation = to_view(center, rig.focus);
    }

    for (BaseMarker(i), mut transform) in &mut bases {
        let base = &world.bases[*i];
        let body = &world.bodies[base.body];
        let p = to_view(
            body_state(&world.bodies, base.body, world.time).position
                + body.surface_to_inertial(base.location, 0.0, world.time),
            rig.focus,
        );
        transform.translation = p;
        transform.scale = marker_scale(p, camera_position);
    }

    for (VesselMarker(i), mut transform, mut material) in &mut vessels {
        let status = world.vessel_status(*i);
        let p = to_view(vessel_position(world, *i), rig.focus);
        transform.translation = p;
        // Aus dem Heimatsystem hinaus: nicht mehr zu sehen.
        transform.scale = if status == VesselStatus::Escaped {
            Vec3::ZERO
        } else {
            marker_scale(p, camera_position)
        };
        let wanted = match status {
            VesselStatus::Wreck { .. } => &materials.wreck,
            _ if selection.0 == Some(*i) => &materials.selected,
            _ if target == Some(*i) => &materials.target,
            _ => &materials.vessel,
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

/// Breiten- und Längenlinien, die sich mit Planet und Mond drehen. Ohne sie
/// wäre die Rotation einer einfarbigen Kugel nicht zu sehen.
pub fn draw_planet_grid(sim: Res<Sim>, rig: Res<CameraRig>, mut gizmos: Gizmos) {
    let world = &sim.0;
    for (b, body) in world.bodies.iter().enumerate() {
        let center = body_state(&world.bodies, b, world.time).position;
        let radius = body.radius + GRID_LIFT;
        let point = |lat: f64, lon: f64| {
            let p = SurfacePoint::from_degrees(lat, lon);
            to_view(
                center + body.body_fixed_to_inertial(p.body_fixed_direction() * radius, world.time),
                rig.focus,
            )
        };
        // Alle 0,25°: Die Linien liegen auch aus der Nähe auf dem Boden.
        for lat in [-60.0, -30.0, 0.0, 30.0, 60.0] {
            gizmos.linestrip((0..=1440).map(|i| point(lat, i as f64 * 0.25)), GRID_COLOR);
        }
        for lon in (0..12).map(|i| i as f64 * 30.0) {
            gizmos.linestrip(
                (0..=720).map(|i| point(-90.0 + i as f64 * 0.25, lon)),
                GRID_COLOR,
            );
        }
    }
}

/// Bahn des Monds um den Planeten und seine Einflusssphäre, beides Kreise in
/// der Äquatorebene. Die Einflusssphäre ist eine Kugel; gezeichnet wird ihr
/// Umriss in der Bahnebene, beim gewählten Mond in dessen Farbe.
pub fn draw_body_orbits(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    chosen: Res<SelectedBody>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    let circle = |center: DVec3, radius: f64| {
        (0..=BODY_ORBIT_POINTS).map(move |i| {
            let angle = std::f64::consts::TAU * i as f64 / BODY_ORBIT_POINTS as f64;
            let (sin, cos) = angle.sin_cos();
            center + DVec3::new(cos, sin, 0.0) * radius
        })
    };
    for (b, body) in world.bodies.iter().enumerate() {
        let Some(orbit) = &body.orbit else { continue };
        let parent = body_state(&world.bodies, orbit.parent, world.time).position;
        gizmos.linestrip(
            circle(parent, orbit.radius).map(|p| to_view(p, rig.focus)),
            BODY_ORBIT_COLOR,
        );
        let center = body_state(&world.bodies, b, world.time).position;
        let color = if chosen.0 == Some(b) {
            SELECTED_ZONE_COLOR
        } else {
            SOI_COLOR
        };
        gizmos.linestrip(
            circle(center, sphere_of_influence(&world.bodies, b)).map(|p| to_view(p, rig.focus)),
            color,
        );
    }
}

/// Umlaufzeit eines Keplerabschnitts; bei offenen Bahnen bis zum Wechsel der
/// Einflusssphäre, ohne einen drei Stunden.
fn span(world: &World, segment: Option<&Segment>) -> f64 {
    match segment {
        Some(s @ Segment::Kepler { state, .. }) => kepler_span(world, s, state),
        _ => 3.0 * 3600.0,
    }
}

/// Wie weit ein Keplerabschnitt gezeichnet wird: ein Umlauf, offen bis zu
/// seinem Ende, ohne Ende drei Stunden.
pub fn kepler_span(world: &World, segment: &Segment, state: &sim::StateVector) -> f64 {
    kepler::period(world.bodies[segment.body()].gm, state).unwrap_or_else(|| {
        let length = segment.end() - segment.start();
        if length.is_finite() {
            length
        } else {
            3.0 * 3600.0
        }
    })
}

/// Zeichnet für jedes Schiff die vorausberechnete Bahn ab jetzt.
///
/// Blau die aktuelle Bahn (beim Ziel des gewählten Schiffs rosa), orange
/// Stücke durch die Luft, gelb das Brennen, violett die Bahn danach. Wie in KSP wird die aktuelle Bahn bei einem
/// geplanten Manöver trotzdem ganz gezeigt (bis zum Eintritt in die Luft).
/// Gezeichnet wird ein Umlauf voraus, mit Manövern bis zum letzten und dann
/// noch ein Umlauf der Bahn danach; sonst stapeln sich bei zerfallenden Bahnen
/// die Ellipsen.
pub fn draw_trajectories(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    selection: Res<Selection>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    let now = world.time;
    let target = selection
        .0
        .and_then(|i| world.vessels[i].target)
        .map(|t| world.host(t));
    let points = orbit_points(rig.near);
    for (i, vessel) in world.vessels.iter().enumerate() {
        // Angedockte Schiffe fliegen mit ihrem Hauptschiff; aus dem
        // Heimatsystem hinaus wird nichts mehr gezeichnet.
        if vessel.docked_to.is_some() || world.vessel_status(i) == VesselStatus::Escaped {
            continue;
        }
        let orbit_color = if target == Some(i) {
            TARGET_COLOR
        } else {
            ORBIT_COLOR
        };
        let trajectory = &vessel.trajectory;
        let first_kepler = trajectory
            .segments()
            .find(|s| matches!(s, Segment::Kepler { .. }));
        let mut horizon = now + span(world, first_kepler);
        if let Some(burn) = trajectory.burns().last() {
            let after = span(world, trajectory.segment_after(burn));
            horizon = horizon.max(burn.end + after);
        }

        let mut after_burn = false;
        let mut switched = false;
        for (segment, anchor) in trajectory.segments().zip(anchors(world, trajectory)) {
            // Nach einem Wechsel der Einflusssphäre das neue Stück noch ganz.
            if switched {
                horizon = horizon.max(segment.start() + span(world, Some(segment)));
            }
            switched = matches!(
                segment.end_event(),
                Some(Event::SoiEntry(_) | Event::SoiExit)
            );
            if segment.start() >= horizon {
                break;
            }
            let from = segment.start().max(now);
            let place = |p: DVec3| to_view(anchor + p, rig.focus);
            match segment {
                Segment::Kepler {
                    start, state, end, ..
                } => {
                    let body = &world.bodies[segment.body()];
                    // Eine geschlossene Bahn einmal ganz, eine offene bis zum
                    // Wechsel der Einflusssphäre (ohne: drei Stunden weit).
                    let span = kepler_span(world, segment, state);
                    // Vor einem geplanten Brennen die alte Bahn trotzdem
                    // ganz (nicht, wenn sie den Mond erreichen kann: Die
                    // Begegnung stünde nicht in diesem Abschnitt).
                    let end = match end {
                        Some((_, Event::BurnStart))
                            if !soi::reaches_moon(&world.bodies, segment.body(), state) =>
                        {
                            let inbound = kepler::time_to_radius_inbound(
                                body.gm,
                                state,
                                body.boundary_radius(),
                            )
                            .map_or(f64::INFINITY, |dt| start + dt);
                            soi::exit_time(&world.bodies, segment.body(), *start, state)
                                .map_or(inbound, |exit| exit.min(inbound))
                        }
                        _ => segment.end(),
                    };
                    let to = end.min(from + span).min(horizon.max(from));
                    let color = if after_burn {
                        PLANNED_COLOR
                    } else {
                        orbit_color
                    };
                    // Pro Umlauf `points` Stützpunkte, für Teilstücke anteilig.
                    let n = ((points as f64 * (to - from) / span).ceil() as usize).max(16);
                    gizmos.linestrip(
                        (0..=n).map(|i| {
                            let t = from + (to - from) * i as f64 / n as f64;
                            place(segment.state_at(&world.bodies, t).position)
                        }),
                        color,
                    );
                }
                Segment::Integrated { samples, burn, .. } => {
                    let color = if let Some(burn) = burn {
                        after_burn = true;
                        // Beim Landebrennen ginge die momentane Bahn nur in
                        // den Boden und verdeckte den Weg nach unten.
                        if segment.start() <= now && now < segment.end() && !burn.is_landing() {
                            draw_momentary_orbit(
                                &mut gizmos,
                                world,
                                segment,
                                now,
                                anchor - rig.focus,
                                points,
                            );
                        }
                        BURN_COLOR
                    } else {
                        ATMOSPHERE_PATH_COLOR
                    };
                    let every = if rig.near { 1 } else { samples.len() / 400 };
                    draw_samples(
                        &mut gizmos,
                        world,
                        segment,
                        samples,
                        from,
                        anchor - rig.focus,
                        color,
                        every,
                    );
                }
                Segment::Surface { .. } => {}
                Segment::Approach { .. } => {
                    // Anflug zum Andocken: kurz und langsam, fein gezeichnet.
                    let (a, b) = (from, segment.end());
                    gizmos.linestrip(
                        (0..=64).map(|i| {
                            let t = a + (b - a) * i as f64 / 64.0;
                            place(segment.state_at(&world.bodies, t).position)
                        }),
                        TARGET_COLOR,
                    );
                }
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
    offset: DVec3,
    points: usize,
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
    let n = ((points as f64 * to / span).ceil() as usize).max(16);
    gizmos.linestrip(
        (0..=n).map(|i| {
            let dt = to * i as f64 / n as f64;
            dir_to_view(kepler::propagate(body.gm, state, dt).position + offset)
        }),
        ORBIT_COLOR,
    );
}

/// Stützpunkte eines vorausberechneten Abschnitts, jeder `every`-te.
#[allow(clippy::too_many_arguments)]
fn draw_samples(
    gizmos: &mut Gizmos,
    world: &World,
    segment: &Segment,
    samples: &[Sample],
    from: f64,
    offset: DVec3,
    color: Color,
    every: usize,
) {
    let start = segment.state_at(&world.bodies, from).position;
    let rest = samples.iter().filter(|s| s.time > from);
    let step = every.max(1);
    gizmos.linestrip(
        std::iter::once(start)
            .chain(rest.step_by(step).map(|s| s.state.position))
            .chain(std::iter::once(samples[samples.len() - 1].state.position))
            .map(|p| dir_to_view(p + offset)),
        color,
    );
}

/// Gelbe Ringe auf der Bahn, wo ein geplantes Brennen beginnt und endet. Für
/// alle Schiffe und alle gültigen Nodes, damit man geplante Manöver auf einen
/// Blick sieht.
pub fn draw_burn_points(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    let eye = camera.translation;
    for vessel in world.vessels.iter().filter(|v| v.docked_to.is_none()) {
        let burns = vessel.trajectory.burns();
        let times = burns.iter().flat_map(|b| [b.start, b.end]);
        let anchors = anchors(world, &vessel.trajectory);
        for t in times {
            if t <= world.time {
                continue;
            }
            let Some((segment, anchor)) = vessel
                .trajectory
                .segments()
                .zip(&anchors)
                .find(|(s, _)| s.start() <= t && t <= s.end())
            else {
                continue;
            };
            let p = to_view(
                *anchor + segment.state_at(&world.bodies, t).position,
                rig.focus,
            );
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

/// Wechsel der Einflusssphäre: kleine Ringe auf der Bahn (beim Austritt
/// zwei, auf beiden Seiten der Lücke), und wo ein künftiges Stück um den Mond
/// beginnt, der Mond selbst blass ("Geistermond"), wie in KSP.
pub fn draw_soi_changes(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    let eye = camera.translation;
    for (i, vessel) in world.vessels.iter().enumerate() {
        if vessel.docked_to.is_some() || world.vessel_status(i) == VesselStatus::Escaped {
            continue;
        }
        let anchors = anchors(world, &vessel.trajectory);
        let segments: Vec<&Segment> = vessel.trajectory.segments().collect();
        for k in 1..segments.len() {
            let (done, next) = (segments[k - 1], segments[k]);
            let t = done.end();
            if t <= world.time
                || !matches!(done.end_event(), Some(Event::SoiEntry(_) | Event::SoiExit))
            {
                continue;
            }
            for p in [
                anchors[k - 1] + done.state_at(&world.bodies, t).position,
                anchors[k] + next.state_at(&world.bodies, t).position,
            ] {
                let p = to_view(p, rig.focus);
                let rotation = Quat::from_rotation_arc(Vec3::Z, (eye - p).normalize());
                gizmos
                    .circle(
                        Isometry3d::new(p, rotation),
                        p.distance(eye) * 0.008,
                        SOI_CHANGE_COLOR,
                    )
                    .resolution(20);
            }
            if let Some(Event::SoiEntry(moon)) = done.end_event() {
                let center = to_view(anchors[k], rig.focus);
                gizmos
                    .sphere(
                        Isometry3d::from_translation(center),
                        world.bodies[moon].radius as f32,
                        GHOST_COLOR,
                    )
                    .resolution(48);
            }
        }
    }
}

/// Vorausgesagter Bodenkontakt als Kreuz auf der Oberfläche: grün im
/// Zielgebiet, gelb außerhalb (danach Hop), rot bei einem Absturz. Das Kreuz
/// dreht sich mit dem Planeten (die Bahn selbst ist ohne Drehung gezeichnet).
pub fn draw_touchdowns(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    for i in (0..world.vessels.len()).filter(|&i| world.vessels[i].docked_to.is_none()) {
        let Some(t) = world.touchdown(i) else {
            continue;
        };
        let color = match (t.landing, t.zone) {
            (Ok(_), Some(_)) => ZONE_COLOR,
            (Ok(_), None) => OUTSIDE_COLOR,
            (Err(_), _) => IMPACT_COLOR,
        };
        let body = &world.bodies[t.body];
        let up = body.body_fixed_to_inertial(t.point.body_fixed_direction(), world.time);
        let east = DVec3::Z.cross(up).try_normalize().unwrap_or(DVec3::X);
        let north = up.cross(east);
        // Auf dem echten Körper (ein Punkt seiner Oberfläche).
        let center = to_view(
            body_state(&world.bodies, t.body, world.time).position
                + up * (body.radius + SURFACE_LIFT),
            rig.focus,
        );
        let arm = center.distance(camera.translation) * 0.015;
        for d in [east + north, east - north] {
            let d = camera::dir_to_view(d.normalize()) * arm;
            gizmos.line(center - d, center + d, color);
        }
    }
}

/// Zielgebiete als Kreise auf der Oberfläche, mit dem Planeten drehend. Aus
/// der Ferne mindestens so groß wie ein Markierungspunkt, damit man sie findet.
/// In der Mitte eine kleine Raute zum Anklicken (gewählt: gelb).
pub fn draw_zones(
    sim: Res<Sim>,
    rig: Res<CameraRig>,
    zone_selection: Res<SelectedZone>,
    camera: Single<&Transform, With<MainCamera>>,
    mut gizmos: Gizmos,
) {
    let world = &sim.0;
    let eye = camera.translation;
    for (k, zone) in world.zones.iter().enumerate() {
        let body = &world.bodies[zone.body];
        let up = body.body_fixed_to_inertial(zone.center.body_fixed_direction(), world.time);
        let center = to_view(
            body_state(&world.bodies, zone.body, world.time).position
                + up * (body.radius + SURFACE_LIFT),
            rig.focus,
        );
        let rotation = Quat::from_rotation_arc(Vec3::Z, camera::dir_to_view(up));
        let distance = center.distance(eye);
        let radius = (zone.radius as f32).max(distance * 0.012);
        gizmos
            .circle(Isometry3d::new(center, rotation), radius, ZONE_COLOR)
            .resolution(96);
        let color = if zone_selection.0 == Some(k) {
            SELECTED_ZONE_COLOR
        } else {
            ZONE_COLOR
        };
        // Flach auf dem Boden wie der Kreis (zur Kamera gedreht steckte die
        // untere Hälfte im Boden).
        gizmos
            .circle(Isometry3d::new(center, rotation), distance * 0.01, color)
            .resolution(4);
    }
}
