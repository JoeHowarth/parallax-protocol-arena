#![allow(unused_imports)]

use std::collections::BTreeMap;

use asteroid::{AsteroidPlugin, SmallAsteroid};
use bevy::{
    color::palettes::css,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    time::common_conditions::{on_real_timer, on_timer},
    utils::{HashMap, HashSet},
};
use bevy_rand::{
    plugin::EntropyPlugin,
    prelude::{GlobalEntropy, WyRand},
};
use collisions::{Collider, SpatialIndex};
use parallax_protocol_arena::{
    client::ClientPlugin,
    crafts::Faction,
    health_despawn,
    physics::*,
    prelude::*,
    subsystems::plasma_cannon::{PlasmaCannon, PlasmaCannonPlugin},
    ParallaxProtocolArenaPlugin,
    Selected,
};
use rand::Rng;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: bevy::window::WindowResolution::new(
                            1700., 1100.,
                        ),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
            bevy_pancam::PanCamPlugin,
            EntropyPlugin::<WyRand>::with_seed(123u64.to_ne_bytes()),
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_plugins((
            ParallaxProtocolArenaPlugin {
                config: (|| {
                    let tps = 10;
                    SimulationConfig {
                        ticks_per_second: tps,
                        time_dilation: 1.,
                        prediction_ticks: tps * 30,
                        ..default()
                    }
                })(),
                ..default()
            },
            AsteroidPlugin,
            PlasmaCannonPlugin,
        ))
        .add_systems(Startup, spawn_fps_ui)
        .add_systems(PostStartup, (setup, generate_asteroid_field).chain())
        // .add_systems(
        //     Update,
        //     generate_asteroid_field.run_if(on_timer(Duration::from_secs(1))),
        // )
        .add_systems(FixedUpdate, health_despawn)
        .add_systems(
            Update,
            (
                exit_system,
                fps_ui.run_if(on_real_timer(Duration::from_millis(200))),
            ),
        )
        .run();
}

pub fn exit_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.all_pressed([KeyCode::ControlLeft, KeyCode::KeyC]) {
        exit.send(AppExit::Success);
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera2d,
        bevy_pancam::PanCam {
            move_keys: bevy_pancam::DirectionKeys::arrows(),
            grab_buttons: vec![MouseButton::Right],
            ..default()
        },
    ));

    let ship_e = commands
        .spawn(ship_bundle(
            "Ship_rotated.png",
            10.,
            32.,
            Faction::Red,
            Vec2::new(10., 10.),
            &asset_server,
            0,
        ))
        .id();
    info!(ship_entity = ship_e.index(), "Ship Entity");
    commands.insert_resource(Selected(ship_e));

    // commands.spawn(ship_bundle(
    //     "Ship_rotated.png",
    //     10.,
    //     32.,
    //     Faction::Red,
    //     Vec2::new(-10., -10.),
    //     &asset_server,
    // ));
    //
    // commands.queue(SmallAsteroid::spawn(
    //     Vec2::new(150., 20.),
    //     Vec2::new(1., -2.),
    // ));

    commands.queue(SmallAsteroid::spawn(
        Vec2::new(150., 50.),
        Vec2::new(100., -2.),
    ));
}

pub fn ship_bundle(
    sprite_name: &'static str,
    radius: f32,
    px: f32,
    faction: Faction,
    pos: Vec2,
    asset_server: &AssetServer,
    tick: u64,
) -> impl Bundle {
    (
        faction,
        Transform::from_translation(Vec3::from2(pos)).with_scale(Vec3::new(
            2. * radius / px,
            2. * radius / px,
            1.,
        )),
        Sprite {
            image: asset_server.load(sprite_name),
            color: faction.sprite_color(),
            ..default()
        },
        PlasmaCannon::default(),
        PhysicsBundle::new_with_events(
            PhysicsState {
                pos,
                vel: Vec2::ZERO,
                ang_vel: 0.,
                rotation: 0.,
                mass: 1.,
                current_thrust: 0.,
                max_thrust: 100.,
                alive: true,
            },
            Vec2::new(px, px),
            tick,
            [
                (2, ControlInput::SetThrust(1.)),
                (20, ControlInput::SetThrust(0.)),
                (60, ControlInput::SetRotation(PI)),
                (61, ControlInput::SetAngVel(0.1)),
                (65, ControlInput::SetThrust(1.)),
                (80, ControlInput::SetThrust(0.1)),
            ]
            .into_iter(),
        ),
    )
}

fn generate_asteroid_field(
    mut commands: Commands,
    mut rng: ResMut<GlobalEntropy<WyRand>>,
) {
    for _ in 0..100 {
        commands.queue(SmallAsteroid::spawn(
            Vec2::new(
                rng.gen_range((-5000.)..(5000.)),
                rng.gen_range((-5000.)..(5000.)),
            ),
            Vec2::new(
                rng.gen_range((-500.)..(500.)),
                rng.gen_range((-500.)..(500.)),
            ),
        ));
    }
}

#[derive(Component)]
struct FpsUiMarker;

fn spawn_fps_ui(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.),
            right: Val::Px(10.),
            ..default()
        })
        .with_child((Text("fps".into()), FpsUiMarker));
}

fn fps_ui(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsUiMarker>>,
) {
    let mut text = query.single_mut();
    let Some(value) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    else {
        return;
    };

    text.0.clear();
    use std::fmt::Write;
    let _ = write!(&mut text.0, "FPS: {value:>3.0}");
}
