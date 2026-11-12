//! Kepler Industries: Kartenansicht.
//!
//! Dieses Crate stellt nur dar und nimmt Eingaben an. Die Simulation lebt
//! vollständig im Crate `sim`; hier wird sie nur gelesen und vorangetrieben.

mod booster_card;
mod camera;
mod depot;
mod dv_bar;
mod focus;
mod fracht;
mod hud;
mod left_bar;
mod maneuver_card;
mod node_ui;
mod rendezvous;
mod savegame;
mod side;
mod start_card;
mod status_bar;
mod target_card;
mod top_bar;
mod ui;
mod verbund;
mod view;
mod wirtschaft;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::WindowResolution;

/// Die Simulation als Bevy-Ressource.
#[derive(Resource)]
pub struct Sim(pub sim::World);

/// Zeitraffer-Stufen wie in KSP.
pub const WARP_LEVELS: [f64; 9] = [
    1.0,
    5.0,
    10.0,
    50.0,
    100.0,
    1_000.0,
    10_000.0,
    100_000.0,
    1_000_000.0,
];

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
    /// Diesen Spielstand laden statt neu zu beginnen.
    load: Option<std::path::PathBuf>,
    /// Statt des Spielbeginns die Demo mit Schiffen laden (`scenario::demo`).
    demo: bool,
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
    /// Start; der Wendewinkel `None`: Voreinstellung des Körpers.
    launch: Option<(usize, sim::Launch, Option<f64>)>,
    /// Ziel: Schiff, Zielschiff.
    targets: Vec<(usize, usize)>,
    target_bodies: Vec<(usize, usize)>,
    /// Andocken einschalten (Schiff mit Ziel).
    dock: Vec<usize>,
    /// Kamera auf dieses Schiff bzw. Zielgebiet richten.
    focus: Option<camera::Focus>,
    /// Nahansicht einschalten.
    near: bool,
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
        // Schalter ohne Wert.
        if arg == "--near" {
            options.near = true;
            continue;
        }
        if arg == "--demo" {
            options.demo = true;
            continue;
        }
        let value = args.next();
        match (arg.as_str(), value) {
            ("--screenshot", Some(path)) => options.screenshot = Some(path),
            ("--load", Some(path)) => options.load = Some(path.into()),
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
                let p = numbers(&l, "--launch SCHIFF,ZEIT,KURS,WENDE_KM,AP_KM[,WENDEWINKEL]");
                let profile = sim::LaunchProfile {
                    heading: p[2],
                    turn_altitude: p[3] * 1000.0,
                    target_apoapsis: p[4] * 1000.0,
                    ..sim::LaunchProfile::default()
                };
                options.launch = Some((
                    p[0] as usize,
                    sim::Launch {
                        time: p[1],
                        profile,
                    },
                    p.get(5).copied(),
                ));
            }
            ("--focus", Some(v)) => {
                options.focus = Some(camera::Focus::Vessel(
                    v.parse().expect("--focus erwartet eine Schiffsnummer"),
                ))
            }
            ("--focus-body", Some(v)) => {
                options.focus = Some(camera::Focus::Body(
                    v.parse()
                        .expect("--focus-body erwartet die Nummer eines Himmelskörpers"),
                ))
            }
            ("--focus-zone", Some(v)) => {
                options.focus = Some(camera::Focus::Zone(
                    v.parse()
                        .expect("--focus-zone erwartet die Nummer eines Zielgebiets"),
                ))
            }
            ("--dock", Some(v)) => options
                .dock
                .push(v.parse().expect("--dock erwartet eine Schiffsnummer")),
            ("--target", Some(t)) => {
                let p = numbers(&t, "--target SCHIFF,ZIEL");
                options.targets.push((p[0] as usize, p[1] as usize));
            }
            ("--target-body", Some(t)) => {
                let p = numbers(&t, "--target-body SCHIFF,KÖRPER");
                options.target_bodies.push((p[0] as usize, p[1] as usize));
            }
            (other, _) => {
                eprintln!("Unbekannte Option: {other}");
                eprintln!(
                    "Optionen: --screenshot DATEI.png  --time SEKUNDEN  --camera GIER,NEIGUNG,KM\n\
                     \x20         --select SCHIFF  --node SCHIFF,ZEIT,PROGRAD,NORMAL,RADIAL[,SCHARF]\n\
                     \x20         --launch SCHIFF,ZEIT,KURS,WENDE_KM,AP_KM  --target SCHIFF,ZIEL\n\
                     \x20         --target-body SCHIFF,KÖRPER\n\
                     \x20         --focus SCHIFF  --focus-zone ZIELGEBIET  --focus-body KÖRPER\n\
                     \x20         --near  --dock SCHIFF\n\
                     \x20         --load DATEI (Spielstand laden; F5 speichert, F9 lädt)\n\
                     \x20         --demo (Schiffe und Zielgebiete zum Ausprobieren; die\n\
                     \x20         Schiffsnummern der anderen Optionen gelten dafür)"
                );
                std::process::exit(2);
            }
        }
    }
    options
}

fn main() {
    let options = parse_options();

    // Start, Nodes und Ziel gelten ab Spielbeginn; `--time` führt sie dann aus.
    let mut world = if let Some(path) = &options.load {
        savegame::read(path).unwrap_or_else(|e| {
            eprintln!("--load: {e}");
            std::process::exit(2);
        })
    } else if options.demo {
        sim::scenario::demo()
    } else {
        sim::scenario::start()
    };
    let zone = savegame::initial_zone(&world);
    let mut selection = node_ui::Selection(options.select);
    // Zuerst der Start, damit Nodes danach auf seiner Bahn liegen können.
    if let Some((vessel, mut launch, kick)) = options.launch {
        let body = &world.bodies[world.vessel_state(vessel).0];
        launch.profile.kick = kick.unwrap_or(sim::LaunchProfile::for_body(body).kick);
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

    for &(vessel, target) in &options.targets {
        world.set_target(vessel, Some(target));
    }
    for &(vessel, body) in &options.target_bodies {
        world.set_target_body(vessel, Some(body));
    }
    for &vessel in &options.dock {
        if let Err(e) = world.set_docking(vessel, true) {
            eprintln!("--dock: {e}");
            std::process::exit(2);
        }
    }
    // Ein geladener Spielstand kann schon weiter sein.
    if options.time > world.time {
        world.advance_to(options.time);
    }

    if let Some(camera::Focus::Body(b)) = options.focus
        && b >= world.bodies.len()
    {
        eprintln!(
            "--focus-body: es gibt nur {} Himmelskörper",
            world.bodies.len()
        );
        std::process::exit(2);
    }
    let mut rig =
        camera::CameraRig::new(world.bodies.iter().map(|b| b.radius).collect(), world.home);
    if let Some(focus) = options.focus {
        rig.set_target(focus);
        rig.set_near(options.near);
    }
    if let Some((yaw, pitch, km)) = options.camera {
        rig.yaw = yaw.to_radians();
        rig.pitch = pitch.to_radians();
        rig.distance = km * 1000.0;
        rig.clamp_distance();
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
    .add_plugins(bevy_egui::EguiPlugin::default())
    // Die Oberfläche hängt an der Overlay-Kamera (siehe `camera::spawn`).
    .insert_resource(bevy_egui::EguiGlobalSettings {
        auto_create_primary_context: false,
        ..default()
    })
    .add_systems(bevy_egui::EguiPrimaryContextPass, ui::draw)
    .insert_resource(Sim(world))
    .insert_resource(warp)
    .insert_resource(rig)
    .insert_resource(selection)
    .init_resource::<node_ui::SelectedNode>()
    .insert_resource(zone)
    .init_resource::<node_ui::SelectedBody>()
    .init_resource::<ui::WorldLabels>()
    .init_resource::<ui::MapArea>()
    .init_resource::<top_bar::Panels>()
    .init_resource::<rendezvous::Rendezvous>()
    .init_resource::<status_bar::StatusBar>()
    .init_resource::<verbund::UndockChoice>()
    .init_resource::<node_ui::Drag>()
    .init_resource::<node_ui::UiMessage>()
    .init_gizmo_group::<node_ui::NodeGizmos>()
    .insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.025)))
    .add_systems(Startup, ((view::setup, node_ui::configure_gizmos),).chain())
    .add_systems(
        Update,
        (
            controls,
            advance_simulation,
            (
                focus::keys,
                focus::follow,
                // Über der Oberfläche gehören Mausrad und Ziehen dieser.
                camera::input.run_if(ui::pointer_on_map),
                camera::apply,
            )
                .chain(),
            // Eingaben ändern die Simulation; danach wird nur noch gelesen.
            (savegame::keys, node_ui::input, rendezvous::update).chain(),
            (view::spawn_markers, view::sync_markers, view::aim_sunlight).chain(),
            node_ui::draw,
            node_ui::label,
            rendezvous::labels,
            view::draw_planet_grid,
            view::draw_body_orbits,
            view::draw_trajectories,
            view::draw_burn_points,
            view::draw_soi_changes,
            view::draw_zones,
            view::draw_touchdowns,
            rendezvous::draw,
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
