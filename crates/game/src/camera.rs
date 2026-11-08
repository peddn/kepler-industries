//! Kamera, die um einen Fokuspunkt kreist.
//!
//! Gegen Ungenauigkeit bei großen Entfernungen (KSPs "Deep Space Kraken"):
//! Der Fokuspunkt liegt in Bevy immer im Ursprung. Alles wird relativ zum
//! Fokus gezeichnet; die Umrechnung von f64 nach f32 passiert erst nach dem
//! Abziehen des Fokus (siehe `to_view`). Die Kamera ist nie weiter vom
//! Ursprung entfernt als der Zoomabstand, daher bleibt alles Sichtbare genau.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{PerspectiveProjection, Projection};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::math::DVec3;
use bevy::prelude::*;

#[derive(Resource)]
pub struct CameraRig {
    /// Fokuspunkt im Simulationssystem (m). In PR 1 immer der Planetenmittelpunkt.
    pub focus: DVec3,
    /// Abstand der Kamera vom Fokus in m.
    pub distance: f64,
    pub yaw: f32,
    pub pitch: f32,
    min_distance: f64,
    max_distance: f64,
}

impl CameraRig {
    pub fn new(planet_radius: f64) -> Self {
        Self {
            focus: DVec3::ZERO,
            distance: planet_radius * 4.0,
            yaw: 0.6,
            pitch: 0.45,
            min_distance: planet_radius * 1.01,
            max_distance: planet_radius * 80.0,
        }
    }
}

/// Simulationssystem (Z nach Norden) → Bevy (Y nach oben), relativ zum Fokus.
pub fn to_view(position: DVec3, focus: DVec3) -> Vec3 {
    dir_to_view(position - focus)
}

/// Wie `to_view`, aber für Richtungen (ohne Fokus).
pub fn dir_to_view(d: DVec3) -> Vec3 {
    Vec3::new(d.x as f32, d.z as f32, -d.y as f32)
}

/// Die Kamera, die die Welt zeigt.
#[derive(Component)]
pub struct MainCamera;

/// Ebene für Dinge, die immer über der Welt liegen (Node und Griffe).
pub const OVERLAY_LAYER: usize = 1;

fn projection() -> Projection {
    Projection::Perspective(PerspectiveProjection {
        near: 1.0,
        near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -1.0),
        // Bevy rendert mit unendlicher Tiefe; `far` begrenzt nur das Aussortieren.
        far: 1e12,
        ..default()
    })
}

pub fn spawn(commands: &mut Commands) {
    commands.spawn((
        MainCamera,
        IsDefaultUiCamera,
        Camera3d::default(),
        projection(),
        Transform::default(),
    ));
    // Zweite Kamera mit gleichem Blick, die nur die Overlay-Ebene zeichnet. Sie
    // hat einen eigenen Tiefenpuffer, deshalb liegt das Overlay immer obenauf.
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        projection(),
        Transform::default(),
        RenderLayers::layer(OVERLAY_LAYER),
    ));
}

/// Ziehen mit der rechten Maustaste dreht (wie in KSP), Mausrad zoomt.
/// Die linke Maustaste gehört der Bedienung (Auswahl, Node).
pub fn input(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut rig: ResMut<CameraRig>,
) {
    if buttons.pressed(MouseButton::Right) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch = (rig.pitch + motion.delta.y * 0.005).clamp(-1.55, 1.55);
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y as f64,
        MouseScrollUnit::Pixel => scroll.delta.y as f64 / 40.0,
    };
    if notches != 0.0 {
        // Multiplikativ zoomen: gleiches Gefühl von der Oberfläche bis zur Fernsicht.
        rig.distance =
            (rig.distance * 0.85f64.powf(notches)).clamp(rig.min_distance, rig.max_distance);
    }
}

pub fn apply(rig: Res<CameraRig>, mut cameras: Query<&mut Transform, With<Camera3d>>) {
    let direction = Vec3::new(
        rig.pitch.cos() * rig.yaw.sin(),
        rig.pitch.sin(),
        rig.pitch.cos() * rig.yaw.cos(),
    );
    let transform = Transform::from_translation(direction * rig.distance as f32)
        .looking_at(Vec3::ZERO, Vec3::Y);
    for mut camera in &mut cameras {
        *camera = transform;
    }
}
