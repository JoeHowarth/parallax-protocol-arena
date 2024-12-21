#![allow(unused_imports)]

use std::collections::BTreeMap;

use asteroid::{AsteroidPlugin, SmallAsteroid};
use bevy::{
    color::palettes::css,
    utils::{HashMap, HashSet},
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
        ))
        .add_plugins((
            ParallaxProtocolArenaPlugin::<FixedUpdate> {
                config: (|| {
                    let tps = 30;
                    SimulationConfig {
                        ticks_per_second: tps,
                        time_dilation: 1.0,
                        prediction_ticks: tps * 10,
                        ..default()
                    }
                })(),
                physics: PhysicsSimulationPlugin::<FixedUpdate>::default(),
                client: Some(ClientPlugin::default()),
            },
            AsteroidPlugin,
            PlasmaCannonPlugin,
        ))
        .add_systems(PostStartup, setup)
        .add_systems(FixedUpdate, health_despawn)
        .add_systems(Update, (exit_system,))
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
            [
                (2, TimelineEvent::Control(ControlInput::SetThrust(1.))),
                (20, TimelineEvent::Control(ControlInput::SetThrust(0.))),
                (60, TimelineEvent::Control(ControlInput::SetRotation(PI))),
                (61, TimelineEvent::Control(ControlInput::SetAngVel(0.1))),
                (65, TimelineEvent::Control(ControlInput::SetThrust(1.))),
                (80, TimelineEvent::Control(ControlInput::SetThrust(0.1))),
            ]
            .into_iter(),
        ),
    )
}
