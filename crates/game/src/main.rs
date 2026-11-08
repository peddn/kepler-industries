//! Kepler Industries: Kartenansicht.
//!
//! Dieses Crate stellt nur dar und nimmt Eingaben an. Die Simulation lebt
//! vollständig im Crate `sim`; hier wird sie nur gelesen und vorangetrieben.

mod camera;
mod hud;
mod view;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::WindowResolution;

/// Die Simulation als Bevy-Ressource.
#[derive(Resource)]
pub struct Sim(pub sim::World);

/// Zeitraffer-Stufen wie in KSP.
pub const WARP_LEVELS: [f64; 8] = [1.0, 5.0, 10.0, 50.0, 100.0, 1_000.0, 10_000.0, 100_000.0];

#[derive(Resource, Default)]
pub struct TimeWarp {
    pub level: usize,
    pub paused: bool,
}

impl TimeWarp {
    pub fn factor(&self) -> f64 {
        WARP_LEVELS[self.level]
    }
}

/// Startoptionen von der Kommandozeile.
#[derive(Resource, Default, Clone)]
struct Options {
    /// Bild speichern und beenden.
    screenshot: Option<String>,
    /// Spielzeit in s, bis zu der vor dem Start vorgerechnet wird.
    time: f64,
    /// Kamera: Gierwinkel (°), Neigung (°), Abstand vom Planetenmittelpunkt (km).
    camera: Option<(f32, f32, f64)>,
}

fn parse_options() -> Options {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next();
        match (arg.as_str(), value) {
            ("--screenshot", Some(path)) => options.screenshot = Some(path),
            ("--time", Some(t)) => options.time = t.parse().expect("--time erwartet Sekunden"),
            ("--camera", Some(c)) => {
                let parts: Vec<f64> = c
                    .split(',')
                    .map(|p| p.parse().expect("--camera erwartet Gier,Neigung,km"))
                    .collect();
                options.camera = Some((parts[0] as f32, parts[1] as f32, parts[2]));
            }
            (other, _) => {
                eprintln!("Unbekannte Option: {other}");
                eprintln!(
                    "Optionen: --screenshot DATEI.png  --time SEKUNDEN  --camera GIER,NEIGUNG,KM"
                );
                std::process::exit(2);
            }
        }
    }
    options
}

fn main() {
    let options = parse_options();

    let mut world = sim::scenario::demo();
    world.advance_to(options.time);

    let mut rig = camera::CameraRig::new(world.bodies[0].radius);
    if let Some((yaw, pitch, km)) = options.camera {
        rig.yaw = yaw.to_radians();
        rig.pitch = pitch.to_radians();
        rig.distance = km * 1000.0;
    }

    let warp = TimeWarp {
        level: 0,
        // Für Screenshots die Zeit anhalten, damit das Bild genau `--time` zeigt.
        paused: options.screenshot.is_some(),
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Kepler Industries".into(),
            resolution: WindowResolution::new(1280, 720),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(Sim(world))
    .insert_resource(warp)
    .insert_resource(rig)
    .insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.025)))
    .add_systems(Startup, (view::setup, hud::setup))
    .add_systems(
        Update,
        (
            controls,
            advance_simulation,
            camera::input,
            camera::apply,
            view::sync_markers,
            view::draw_planet_grid,
            view::draw_trajectories,
            hud::update,
        )
            .chain(),
    );

    if options.screenshot.is_some() {
        app.insert_resource(options)
            .add_systems(Update, take_screenshot);
    }
    app.run();
}

/// Zeitraffer: `,` langsamer, `.` schneller, Leertaste Pause.
fn controls(keys: Res<ButtonInput<KeyCode>>, mut warp: ResMut<TimeWarp>) {
    if keys.just_pressed(KeyCode::Period) && warp.level + 1 < WARP_LEVELS.len() {
        warp.level += 1;
    }
    if keys.just_pressed(KeyCode::Comma) && warp.level > 0 {
        warp.level -= 1;
    }
    if keys.just_pressed(KeyCode::Space) {
        warp.paused = !warp.paused;
    }
}

fn advance_simulation(time: Res<Time>, warp: Res<TimeWarp>, mut sim: ResMut<Sim>) {
    if warp.paused {
        return;
    }
    // Ruckler (z. B. beim Start) nicht in einen Zeitsprung verwandeln.
    let real_dt = time.delta_secs_f64().min(0.1);
    sim.0.advance(real_dt * warp.factor());
}

/// Wartet, bis alles geladen und gezeichnet ist, speichert ein Bild und beendet.
fn take_screenshot(
    mut commands: Commands,
    options: Res<Options>,
    time: Res<Time<Real>>,
    mut requested: Local<bool>,
) {
    // Software-Rendering braucht beim ersten Mal ein paar Sekunden für die Shader.
    if *requested || time.elapsed_secs() < 6.0 {
        return;
    }
    *requested = true;
    let path = options.screenshot.clone().unwrap();
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save_to_disk(path.clone())(captured);
            exit.write(AppExit::Success);
        },
    );
}
