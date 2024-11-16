#![allow(unused_imports)]

use std::time::Duration;

use anyhow::Result;
use avian2d::prelude::*;
use bevy::{prelude::*, time::common_conditions::on_timer};
use bevy_mod_picking::{
    debug::DebugPickingMode,
    events::Click,
    prelude::{On, *},
    DefaultPickingPlugins,
    PickableBundle,
};
use bevy_mod_scripting::prelude::*;
use bevy_vector_shapes::prelude::*;
use flight_controller::KeyboardFlightController;
use frigate::Frigate;
use lua_bevy_interop::{circle_bundle, health_despawn, prelude::*, Health};
use plasma_drone::PlasmaDrone;
use subsystems::missile::FireMissile;

fn main() -> Result<()> {
    App::new()
        .add_plugins((
            DefaultPlugins,
            bevy_pancam::PanCamPlugin,
            Shape2dPlugin::default(),
            PhysicsPlugins::default(),
            DefaultPickingPlugins,
            ScriptingPlugin,
            LuaManagerPlugin,
        ))
        .add_plugins((
            keyboard_controller::KeyboardControllerPlugin,
            crafts::CraftsPlugin,
            subsystems::missile::MissilePlugin,
            subsystems::sensors::SensorPlugin,
            subsystems::flight_controller::FlightControllerPlugin,
            crafts::frigate::FrigatePlugin,
            crafts::mining_drone::MiningDronePlugin,
            crafts::asteroid::AsteroidPlugin,
        ))
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

    // commands
    // .spawn((
    //     SpatialBundle::default(),
    //     On::<Pointer<Click>>::run(
    //         |e: Listener<Pointer<Click>>,
    //          selected: ResMut<Selected<Frigate>>,
    //          mut tx: EventWriter<FireMissile>| {
    //             let Some((missile_bot_e, _)) = &selected.0 else {
    //                 return;
    //             };
    //             tx.send(FireMissile {
    //                 from: *missile_bot_e,
    //                 target: e.target(),
    //             });
    //         },
    //     ),
    // ))
    // .with_children(|commands| {
    commands.spawn((
        LinearVelocity(Vec2::new(90., 0.)),
        PlasmaDrone::bundle(&asset_server, Vec2::new(-1010., -15.)),
    ));
    commands.spawn((
        LinearVelocity(Vec2::new(60., 0.)),
        PlasmaDrone::bundle(&asset_server, Vec2::new(-310., -15.)),
    ));
    commands.spawn((
        LinearVelocity(Vec2::new(2., 0.)),
        PlasmaDrone::bundle(&asset_server, Vec2::new(-110., -2.)),
    ));
    commands.spawn(PlasmaDrone::bundle(&asset_server, Vec2::new(7., -20.)));
    // });

    // commands
    //     .spawn((
    //         SpatialBundle::default(),
    //         On::<Pointer<Click>>::run(
    //             |listener: Listener<Pointer<Click>>,
    //              mut selected: ResMut<Selected<Frigate>>| {
    //                 selected.0 = Some((listener.target(), Frigate));
    //             },
    //         ),
    //     ))
    //     .with_children(|commands| {
    let frigate = |x, y| Frigate::bundle(&asset_server, Vec2::new(x, y));
    commands.spawn(frigate(200., -10.));
    commands.spawn(frigate(-302., 1.));
    commands.spawn(frigate(305., -100.));
    commands.spawn(frigate(5., -400.));
    // });

    painter.set_2d();
}
