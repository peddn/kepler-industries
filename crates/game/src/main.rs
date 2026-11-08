//! Kepler Industries: Kartenansicht.
//!
//! Dieses Crate stellt nur dar und nimmt Eingaben an. Die Simulation lebt
//! vollständig im Crate `sim`; hier wird sie nur gelesen und vorangetrieben.

mod camera;
mod dv_bar;
mod hud;
mod node_ui;
mod panel;
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
    /// Gewähltes Schiff.
    select: Option<usize>,
    /// Nodes: Schiff, Zeitpunkt (s), Δv prograd, normal, radial (m/s), scharf
    /// (ohne Angabe ja, wie im Spiel). Mehrere Nodes eines Schiffs bilden eine
    /// Kette; sie ist scharf, wenn keiner "nicht scharf" verlangt.
    nodes: Vec<(usize, f64, [f64; 3], bool)>,
    /// Start: Schiff, Zeitpunkt (s), Kurs (°), Wendebeginn (km), Ziel-Apoapsis (km).
    launch: Option<(usize, sim::Launch)>,
}

fn numbers(text: &str, what: &str) -> Vec<f64> {
    text.split(',')
        .map(|p| {
            p.parse()
                .unwrap_or_else(|_| panic!("{what}: Zahl erwartet, nicht {p:?}"))
        })
        .collect()
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
                let p = numbers(&c, "--camera GIER,NEIGUNG,KM");
                options.camera = Some((p[0] as f32, p[1] as f32, p[2]));
            }
            ("--select", Some(v)) => {
                options.select = Some(v.parse().expect("--select erwartet eine Schiffsnummer"))
            }
            ("--node", Some(n)) => {
                let p = numbers(&n, "--node SCHIFF,ZEIT,PROGRAD,NORMAL,RADIAL[,SCHARF]");
                let armed = p.get(5).is_none_or(|&a| a != 0.0);
                options
                    .nodes
                    .push((p[0] as usize, p[1], [p[2], p[3], p[4]], armed));
            }
            ("--launch", Some(l)) => {
                let p = numbers(&l, "--launch SCHIFF,ZEIT,KURS,WENDE_KM,AP_KM");
                let profile = sim::LaunchProfile {
                    heading: p[2],
                    turn_altitude: p[3] * 1000.0,
                    target_apoapsis: p[4] * 1000.0,
                };
                options.launch = Some((
                    p[0] as usize,
                    sim::Launch {
                        time: p[1],
                        profile,
                    },
                ));
            }
            (other, _) => {
                eprintln!("Unbekannte Option: {other}");
                eprintln!(
                    "Optionen: --screenshot DATEI.png  --time SEKUNDEN  --camera GIER,NEIGUNG,KM\n\
                     \x20         --select SCHIFF  --node SCHIFF,ZEIT,PROGRAD,NORMAL,RADIAL[,SCHARF]\n\
                     \x20         --launch SCHIFF,ZEIT,KURS,WENDE_KM,AP_KM"
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
    let mut selection = node_ui::Selection(options.select);
    // Zuerst der Start, damit Nodes danach auf seiner Bahn liegen können.
    if let Some((vessel, launch)) = options.launch {
        if let Err(e) = world
            .set_launch(vessel, launch)
            .and_then(|()| world.set_armed(vessel, true))
        {
            eprintln!("--launch: {e}");
            std::process::exit(2);
        }
        selection.0 = Some(vessel);
    }
    for &(vessel, time, [p, n, r], _) in &options.nodes {
        let node = sim::ManeuverNode {
            time,
            delta_v: bevy::math::DVec3::new(p, n, r),
        };
        let armed = options
            .nodes
            .iter()
            .all(|&(v, .., armed)| v != vessel || armed);
        if let Err(e) = world
            .add_node(vessel, node)
            .and_then(|_| world.set_armed(vessel, armed))
        {
            eprintln!("--node: {e}");
            std::process::exit(2);
        }
        selection.0 = Some(vessel);
    }

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
    .insert_resource(selection)
    .init_resource::<node_ui::SelectedNode>()
    .init_resource::<panel::PanelShown>()
    .init_resource::<dv_bar::BarShown>()
    .init_resource::<node_ui::Drag>()
    .init_resource::<node_ui::UiMessage>()
    .init_gizmo_group::<node_ui::NodeGizmos>()
    .insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.025)))
    .add_systems(
        Startup,
        (
            hud::load_font,
            (
                view::setup,
                node_ui::configure_gizmos,
                node_ui::setup_label,
                panel::setup,
                dv_bar::setup,
                hud::setup,
            ),
        )
            .chain(),
    )
    .add_systems(
        Update,
        (
            controls,
            advance_simulation,
            camera::input,
            camera::apply,
            panel::buttons,
            node_ui::input,
            view::sync_markers,
            node_ui::draw,
            node_ui::sync_label,
            panel::sync,
            panel::hover,
            dv_bar::sync,
            view::draw_planet_grid,
            view::draw_trajectories,
            view::draw_burn_points,
            view::draw_impacts,
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

fn advance_simulation(time: Res<Time>, mut warp: ResMut<TimeWarp>, mut sim: ResMut<Sim>) {
    if warp.paused {
        return;
    }
    // Ruckler (z. B. beim Start) nicht in einen Zeitsprung verwandeln.
    let real_dt = time.delta_secs_f64().min(0.1);
    let target = sim.0.time + real_dt * warp.factor();
    // Beginnt ein markiertes Schiff in diesem Schritt zu brennen: genau dort
    // anhalten und auf 1× schalten, damit man das Brennen sieht.
    match sim.0.next_halting_burn() {
        Some(start) if warp.level > 0 && start <= target => {
            sim.0.advance_to(start);
            warp.level = 0;
        }
        _ => sim.0.advance_to(target),
    }
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
