//! Kamera, die um einen Fokuspunkt kreist.
//!
//! Der Fokus ist ein Objekt (Planet, Mond, Schiff, Node, Zielgebiet), auf das die
//! Kamera mit `F` gerichtet wird; sie folgt ihm (siehe `focus.rs`). In der
//! Karte ist "oben" Norden, in der Nahansicht (`N`) zeigt "oben" vom Planeten
//! weg, der Planet liegt also unten.
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
use sim::BodyId;

/// Ohne anderes Ziel: der Planet.
pub const PLANET: Focus = Focus::Body(0);

/// Worauf die Kamera gerichtet ist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    /// Himmelskörper: 0 der Planet, 1 der Mond.
    Body(BodyId),
    Vessel(usize),
    /// Node eines Schiffs, erkannt an seinem gesetzten Zeitpunkt.
    Node {
        vessel: usize,
        time: f64,
    },
    Zone(usize),
}

/// In der Karte kommt die Kamera einem Objekt höchstens so nahe (m).
pub const MAP_MIN_DISTANCE: f64 = 20_000.0;
/// In der Nahansicht (m).
pub const NEAR_MIN_DISTANCE: f64 = 10.0;
/// Abstand beim Wechsel auf ein Objekt in der Karte (m).
pub const MAP_OBJECT_DISTANCE: f64 = 50_000.0;
/// Abstand beim Einschalten der Nahansicht (höchstens, m).
pub const NEAR_DISTANCE: f64 = 2_000.0;
/// Weiter als so viele Planetenradien zoomt die Kamera nicht heraus (der
/// äußere Mond und der Rand des Heimatsystems passen ins Bild).
const MAX_DISTANCE_RADII: f64 = 250.0;

#[derive(Resource)]
pub struct CameraRig {
    /// Worauf die Kamera gerichtet ist.
    pub target: Focus,
    /// Fokuspunkt im Simulationssystem (m), jedes Bild aus `target` berechnet.
    pub focus: DVec3,
    /// Mittelpunkt des Körpers, bei dem der Fokus ist: In der Nahansicht
    /// zeigt "oben" von ihm weg.
    pub center: DVec3,
    /// Nahansicht: näher heran, Planet unten.
    pub near: bool,
    /// Abstand der Kamera vom Fokus in m.
    pub distance: f64,
    pub yaw: f32,
    pub pitch: f32,
    /// Abstand für den Planeten, wenn die Kamera zu ihm zurückkehrt.
    pub planet_distance: f64,
    /// Radien der Himmelskörper (m), für die Abstände zu ihnen.
    radii: Vec<f64>,
}

impl CameraRig {
    pub fn new(radii: Vec<f64>) -> Self {
        let planet = radii[0] * 4.0;
        Self {
            target: PLANET,
            focus: DVec3::ZERO,
            center: DVec3::ZERO,
            near: false,
            distance: planet,
            yaw: 0.6,
            pitch: 0.45,
            planet_distance: planet,
            radii,
        }
    }

    /// Erlaubter Abstand zum Fokus.
    pub fn limits(&self) -> (f64, f64) {
        let min = match (self.target, self.near) {
            (Focus::Body(b), _) => self.radii[b] * 1.01,
            (_, false) => MAP_MIN_DISTANCE,
            (_, true) => NEAR_MIN_DISTANCE,
        };
        (min, self.radii[0] * MAX_DISTANCE_RADII)
    }

    pub fn clamp_distance(&mut self) {
        let (min, max) = self.limits();
        self.distance = self.distance.clamp(min, max);
    }

    /// Richtet die Kamera auf ein anderes Ziel und wählt einen passenden
    /// Abstand: zurück zum Planeten der alte, auf den Mond das Vierfache
    /// seines Radius, auf ein Objekt in der Karte `MAP_OBJECT_DISTANCE`, in
    /// der Nahansicht bleibt er.
    pub fn set_target(&mut self, target: Focus) {
        if target == self.target {
            return;
        }
        if self.target == PLANET {
            self.planet_distance = self.distance;
        }
        self.target = target;
        if let Focus::Body(b) = target {
            self.near = false;
            self.distance = if b == 0 {
                self.planet_distance
            } else {
                self.radii[b] * 4.0
            };
        } else if !self.near {
            self.distance = MAP_OBJECT_DISTANCE;
        }
        self.clamp_distance();
    }

    /// Nahansicht an oder aus (nur mit einem Objekt im Fokus, nicht mit
    /// einem Himmelskörper).
    pub fn set_near(&mut self, near: bool) {
        if matches!(self.target, Focus::Body(_)) {
            return;
        }
        self.near = near;
        if near {
            self.distance = self.distance.min(NEAR_DISTANCE);
        }
        self.clamp_distance();
    }

    /// Achsen der Kamera in Bevy-Koordinaten: oben, vorn (Gierwinkel 0) und
    /// seitlich, gleich orientiert wie in der Karte (seitlich = oben × vorn),
    /// damit die Maus in beiden Ansichten gleich dreht. In der Nahansicht
    /// zeigt "oben" vom Planeten (im Ursprung) weg, "vorn" nach Norden.
    fn frame(&self) -> (Vec3, Vec3, Vec3) {
        let up = match (self.focus - self.center).try_normalize() {
            Some(up) if self.near => dir_to_view(up),
            _ => return (Vec3::Y, Vec3::Z, Vec3::X),
        };
        let forward = (Vec3::Y - up * up.dot(Vec3::Y))
            .try_normalize()
            .unwrap_or(Vec3::Z);
        (up, forward, up.cross(forward))
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
        rig.distance *= 0.85f64.powf(notches);
        rig.clamp_distance();
    }
}

pub fn apply(rig: Res<CameraRig>, mut cameras: Query<&mut Transform, With<Camera3d>>) {
    let (up, forward, side) = rig.frame();
    let direction =
        up * rig.pitch.sin() + (forward * rig.yaw.cos() + side * rig.yaw.sin()) * rig.pitch.cos();
    let transform =
        Transform::from_translation(direction * rig.distance as f32).looking_at(Vec3::ZERO, up);
    for mut camera in &mut cameras {
        *camera = transform;
    }
}
