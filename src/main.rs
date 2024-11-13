#![allow(unused_imports)]

use anyhow::Result;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_mod_picking::{
    debug::DebugPickingMode,
    events::Click,
    prelude::{On, *},
    DefaultPickingPlugins,
    PickableBundle,
};
use bevy_vector_shapes::prelude::*;
use deno_bevy_interop::{
    agent_runtime::{FromJs, ToJs, *},
    missile_bot::*,
};

fn main() -> Result<()> {
    let scripts = ScriptManager::new();
    let missile_bot_script = "MissileBotScript";

    scripts.run(missile_bot_script.to_owned(), "./ts/missile_bot.ts")?;

    App::new()
        .add_plugins((
            DefaultPlugins,
            bevy_pancam::PanCamPlugin,
            Shape2dPlugin::default(),
            PhysicsPlugins::default(),
            DefaultPickingPlugins,
        ))
        .insert_resource(scripts)
        .insert_resource(Gravity::ZERO)
        .insert_resource(Selected::<MissleBot>(None))
        .insert_resource(DebugPickingMode::Normal)
        .add_event::<FireMissile>()
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, handle_scripts)
        .add_systems(Update, (handle_fire_missile, update_missiles))
        .run();

    Ok(())
}

pub fn handle_scripts(scripts: ResMut<ScriptManager>) {}

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
                 selected: ResMut<Selected<MissleBot>>,
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
                plasma_bot_bundle(&asset_server, Vec2::new(-1010., -15.)),
            ));
            commands.spawn((
                LinearVelocity(Vec2::new(60., 0.)),
                plasma_bot_bundle(&asset_server, Vec2::new(-310., -15.)),
            ));
            commands.spawn((
                LinearVelocity(Vec2::new(2., 0.)),
                plasma_bot_bundle(&asset_server, Vec2::new(-110., -2.)),
            ));
            commands
                .spawn(plasma_bot_bundle(&asset_server, Vec2::new(7., -20.)));
        });

    commands
        .spawn((
            SpatialBundle::default(),
            On::<Pointer<Click>>::run(
                |listener: Listener<Pointer<Click>>,
                 mut selected: ResMut<Selected<MissleBot>>| {
                    selected.0 = Some((listener.target(), MissleBot));
                },
            ),
        ))
        .with_children(|commands| {
            let missile_bot =
                |x, y| missile_bot_bundle(&asset_server, Vec2::new(x, y));
            commands.spawn(missile_bot(200., -10.));
            commands.spawn(missile_bot(-302., 1.));
            commands.spawn(missile_bot(305., -100.));
            commands.spawn(missile_bot(5., -400.));
        });

    painter.set_2d();
}

pub fn plasma_bot_bundle(asset_server: &AssetServer, loc: Vec2) -> impl Bundle {
    let radius = 10.;
    let px = 32.;
    let color = Color::srgb(0.0, 1.0, 0.1);
    (
        PlasmaBot,
        circle_bundle(radius, px, color, loc, asset_server),
    )
}

#[derive(Component, Reflect)]
pub struct PlasmaBot;
