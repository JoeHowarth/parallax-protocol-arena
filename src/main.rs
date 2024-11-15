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
use crafts::missile_bot::MissileBot;
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
        ))
        .add_script_host::<LuaScriptHost<()>>(PostUpdate)
        .add_api_provider::<LuaScriptHost<()>>(Box::new(LuaCoreBevyAPIProvider))
        .add_api_provider::<LuaScriptHost<()>>(Box::new(LuaBevyAPIProvider))
        .add_plugins((
            crafts::CraftsPlugin,
            subsystems::missile::MissilePlugin,
            subsystems::sensors::SensorPlugin,
            crafts::missile_bot::MissileBotPlugin,
            crafts::frigate::FrigatePlugin,
            crafts::mining_drone::MiningDronePlugin,
            crafts::asteroid::AsteroidPlugin,
        ))
        // .add_api_provider::<LuaScriptHost<()>>(Box::new(LifeAPI))
        .insert_resource(Gravity::ZERO)
        // .insert_resource(Selected::<MissileBot>(None))
        .insert_resource(DebugPickingMode::Normal)
        // .insert_resource(Time::<Fixed>::from_seconds(0.250))
        // .register_type::<PlasmaBot>()
        .add_systems(Startup, setup)
        .add_systems(
            FixedUpdate,
            send_on_update.run_if(on_timer(Duration::from_millis(500))),
        )
        .add_script_handler::<LuaScriptHost<()>, 0, 0>(FixedPostUpdate)
        .add_systems(Update, health_despawn)
        .run();

    Ok(())
}

#[derive(Resource)]
pub struct Selected<Comp>(pub Option<(Entity, Comp)>);

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut painter: ShapePainter,
) {
    commands.spawn((
        Camera2dBundle::default(),
        bevy_pancam::PanCam { ..default() },
    ));

    commands
        .spawn((
            SpatialBundle::default(),
            On::<Pointer<Click>>::run(
                |e: Listener<Pointer<Click>>,
                 selected: ResMut<Selected<MissileBot>>,
                 mut tx: EventWriter<FireMissile>| {
                    let Some((missile_bot_e, _)) = &selected.0 else {
                        return;
                    };
                    tx.send(FireMissile {
                        from: *missile_bot_e,
                        target: e.target(),
                    });
                },
            ),
        ))
        .with_children(|commands| {
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
            commands
                .spawn(PlasmaDrone::bundle(&asset_server, Vec2::new(7., -20.)));
        });

    commands
        .spawn((
            SpatialBundle::default(),
            On::<Pointer<Click>>::run(
                |listener: Listener<Pointer<Click>>,
                 mut selected: ResMut<Selected<MissileBot>>| {
                    selected.0 = Some((listener.target(), MissileBot));
                },
            ),
        ))
        .with_children(|commands| {
            let missile_bot =
                |x, y| MissileBot::bundle(&asset_server, Vec2::new(x, y));
            commands.spawn(missile_bot(200., -10.));
            commands.spawn(missile_bot(-302., 1.));
            commands.spawn(missile_bot(305., -100.));
            commands.spawn(missile_bot(5., -400.));
        });

    painter.set_2d();
}

/// Sends events allowing scripts to drive update logic
pub fn send_on_update(mut events: PriorityEventWriter<LuaEvent<()>>) {
    events.send(
        LuaEvent {
            hook_name: "on_update".to_owned(),
            args: (),
            recipients: Recipients::All,
        },
        0,
    )
}
