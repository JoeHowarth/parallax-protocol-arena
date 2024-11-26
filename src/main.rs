#![allow(unused_imports)]

use std::time::Duration;

use anyhow::Result;
use avian2d::prelude::*;
use bevy::{prelude::*, time::common_conditions::on_timer, window::WindowMode};
use bevy_mod_picking::{
    debug::DebugPickingMode,
    events::Click,
    prelude::{On, *},
    DefaultPickingPlugins,
    PickableBundle,
};
use bevy_mod_scripting::prelude::*;
use bevy_vector_shapes::prelude::*;
use engines::EngineInput;
use flight_controller::KeyboardFlightController;
use frigate::Frigate;
use lua_bevy_interop::{circle_bundle, health_despawn, prelude::*, Health};
use plasma_drone::PlasmaDrone;
use subsystems::missile::FireMissile;

fn main() -> Result<()> {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: bevy::window::WindowResolution::new(
                        1700., 1100.,
                    ),
                    ..default()
                }),
                ..default()
            }),
            bevy_pancam::PanCamPlugin,
            Shape2dPlugin::default(),
            PhysicsPlugins::new(FixedPostUpdate),
            DefaultPickingPlugins,
            ScriptingPlugin,
            LuaManagerPlugin,
        ))
        .add_plugins((
            keyboard_controller::KeyboardControllerPlugin,
            crafts::CraftsPlugin,
        ))
        .add_plugins((
            subsystems::missile::MissilePlugin,
            subsystems::sensors::SensorPlugin,
            subsystems::flight_controller::FlightControllerPlugin,
            subsystems::engines::EnginesPlugin,
            crafts::frigate::FrigatePlugin,
            crafts::mining_drone::MiningDronePlugin,
            crafts::asteroid::AsteroidPlugin,
        ))
        .insert_resource(Time::<Fixed>::from_duration(Duration::from_millis(
            50,
        )))
        .insert_resource(Gravity::ZERO)
        .insert_resource(DebugPickingMode::Normal)
        .add_systems(Startup, setup)
        .add_systems(Update, (health_despawn, exit_system))
        .run();

    Ok(())
}

fn exit_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.all_pressed([KeyCode::SuperLeft, KeyCode::KeyW]) {
        exit.send(AppExit::Success);
    }
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut painter: ShapePainter,
) {
    commands.spawn((
        Camera2dBundle::default(),
        bevy_pancam::PanCam {
            move_keys: bevy_pancam::DirectionKeys::NONE,
            ..default()
        },
    ));

    commands.spawn((
        LinearVelocity(Vec2::new(90., 0.)),
        PlasmaDrone::bundle(
            &asset_server,
            Vec2::new(-1010., -15.),
            Faction::Red,
        ),
    ));
    commands.spawn((
        LinearVelocity(Vec2::new(60., 0.)),
        PlasmaDrone::bundle(
            &asset_server,
            Vec2::new(-310., -15.),
            Faction::Red,
        ),
    ));
    commands.spawn((
        LinearVelocity(Vec2::new(2., 0.)),
        PlasmaDrone::bundle(
            &asset_server,
            Vec2::new(-112., -2.),
            Faction::Unaligned,
        ),
    ));
    commands.spawn(PlasmaDrone::bundle(
        &asset_server,
        Vec2::new(7., -20.),
        Faction::Blue,
    ));

    let script_path = "scripts/flight_controller.lua".to_string();
    let handle = asset_server.load(&script_path);
    commands.spawn((
        PlasmaDrone::bundle(
            &asset_server,
            Vec2::new(100., -10.),
            Faction::Unaligned,
        ),
        LinearVelocity(Vec2::new(20., 30.)),
        // EngineInput {
        //     accel: 1.,
        //     target_ang: PI,
        // },
        ScriptCollection::<LuaFile> {
            scripts: vec![Script::new(script_path, handle)],
        },
        LuaHooks::one("on_update"),
    ));
    commands.add(Frigate::spawn(200., -10., Faction::Blue));
    commands.add(Frigate::spawn(200., -10., Faction::Blue));
    commands.add(Frigate::spawn(-302., 1., Faction::Red));
    commands.add(Frigate::spawn(305., -100., Faction::Red));
    commands.add(Frigate::spawn(5., -400., Faction::Blue));

    painter.set_2d();
}
