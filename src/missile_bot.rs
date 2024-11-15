use std::{sync::Mutex, time::Instant};

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
use bevy_mod_scripting::{
    api::{prelude::*, providers::bevy_ecs::LuaEntity},
    prelude::*,
};
use bevy_vector_shapes::prelude::*;

use crate::{sensor::CraftKind, Health};

pub struct MissileBotPlugin;

impl Plugin for MissileBotPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<MissileBot>()
            .add_event::<FireMissile>()
            .add_systems(
                Update,
                (
                    handle_fire_missile,
                    update_missiles,
                    handle_missile_collision,
                ),
            )
            .add_api_provider::<LuaScriptHost<()>>(Box::new(MissileBotPlugin));
    }
}

impl APIProvider for MissileBotPlugin {
    type APITarget = Mutex<Lua>;
    type DocTarget = LuaDocFragment;
    type ScriptContext = Mutex<Lua>;

    fn attach_api(
        &mut self,
        ctx: &mut Self::APITarget,
    ) -> Result<(), ScriptError> {
        // callbacks can receive any `ToLuaMulti` arguments, here '()' and
        // return any `FromLuaMulti` arguments, here a `usize`
        // check the Rlua documentation for more details

        let lua = ctx.get_mut().unwrap();

        let table = lua.create_table().map_err(ScriptError::new_other)?;
        table
            .set(
                "can_fire",
                lua.create_function(|lua, _: Value| {
                    let world = lua.get_world()?;
                    let world = world.read();

                    let from =
                        lua.globals().get::<_, LuaEntity>("entity")?.inner()?;
                    can_fire(&world, from)
                })
                .map_err(ScriptError::new_other)?,
            )
            .map_err(ScriptError::new_other)?;
        table
            .set(
                "fire",
                lua.create_function(
                    |lua, (this, target): (LuaTable, LuaEntity)| {
                        // retrieve the world pointer
                        let world = lua.get_world()?;
                        let mut world = world.write();

                        let from = lua
                            .globals()
                            .get::<_, LuaEntity>("entity")?
                            .inner()?;

                        // check if we can fire
                        if !can_fire(&world, from)? {
                            return Ok(false);
                        }

                        let mut events: Mut<Events<FireMissile>> =
                            world.get_resource_mut().unwrap();
                        events.send(FireMissile {
                            from,
                            target: target.inner()?,
                        });

                        Ok(true)
                    },
                )
                .map_err(ScriptError::new_other)?,
            )
            .map_err(ScriptError::new_other)?;

        lua.globals()
            .set("missiles", table)
            .map_err(ScriptError::new_other)?;

        Ok(())
    }

    fn setup_script(
        &mut self,
        _: &ScriptData,
        _: &mut Self::ScriptContext,
    ) -> Result<(), ScriptError> {
        Ok(())
    }
}

fn can_fire(world: &World, from: Entity) -> mlua::Result<bool> {
    if let Some(last_fired) = world.entity(from).get::<MissileLastFiredTime>() {
        let now = world.resource::<Time<Virtual>>();
        return Ok(dbg!(dbg!(last_fired.0) + 5. < now.elapsed_seconds_f64()));
    }
    Ok(dbg!(true))
}

pub struct MissileBotState {}

pub fn missile_bot_bundle(
    asset_server: &AssetServer,
    loc: Vec2,
) -> impl Bundle {
    let radius = 15.;
    let px = 32.;
    let color = Color::srgb(1.0, 0.0, 0.1);
    let script_path = "scripts/missile_bot.lua".to_string();
    let handle = asset_server.load(&script_path);
    (
        MissileBot,
        ScriptCollection::<LuaFile> {
            scripts: vec![Script::new(script_path, handle)],
        },
        CraftKind::MissileBot,
        circle_bundle(radius, px, color, loc, asset_server),
    )
}

pub fn circle_bundle(
    radius: f32,
    px: f32,
    color: Color,
    loc: Vec2,
    asset_server: &AssetServer,
) -> impl Bundle {
    (
        SpriteBundle {
            texture: asset_server.load("circle-32.png"),
            transform:
                Transform::from_translation(Vec3::from2(loc)) //
                    .with_scale(Vec3::new(
                        2. * radius / px,
                        2. * radius / px,
                        1.,
                    )),
            sprite: Sprite { color, ..default() },
            ..default()
        },
        RigidBody::Dynamic,
        Collider::circle(radius),
        PickableBundle::default(),
    )
}

pub fn handle_missile_collision(
    mut commands: Commands,
    missiles: Query<(Entity, &CollidingEntities, &Missile)>,
    mut health: Query<&mut Health, Without<Missile>>,
) {
    for (e, colliding_entities, missile) in missiles.iter() {
        if colliding_entities.0.len() > 0 {
            dbg!(&colliding_entities.0);
        }
        if colliding_entities.0.contains(&missile.target) {
            info!("Collision");
            commands.entity(e).despawn();
            health.get_mut(missile.target).unwrap().0 -= 10.;
        }
    }
}

pub fn update_missiles(
    mut commands: Commands,
    missiles: Query<(Entity, &Missile)>,
    mut p: ParamSet<(
        Query<&Transform>,
        Query<&mut LinearVelocity, With<Missile>>,
    )>,
    mut painter: ShapePainter,
) {
    // Apply a scaled impulse
    // Adjust this value as needed
    let impulse_strength = 0.1;

    for (e, missile) in missiles.iter() {
        let missile_trans = p.p0().get(e).unwrap().translation;
        let target_trans = {
            let p0 = p.p0();
            let Ok(target_trans) = p0.get(missile.target) else {
                // if target is not there anymore, despawn missile
                commands.entity(e).despawn();
                continue;
            };
            target_trans.translation
        };

        painter.set_translation(missile_trans);

        let dir = (target_trans - missile_trans).normalize();
        let mut p1 = p.p1();
        let mut v = p1.get_mut(e).unwrap();
        let v3 = Vec3::from2(v.0);

        painter.set_color(bevy::color::palettes::basic::AQUA);
        painter.line(Vec3::ZERO, dir * 30.);
        painter.set_color(bevy::color::palettes::basic::LIME);
        painter.line(Vec3::ZERO, v3 * 0.1);

        // First, ensure v3 is not zero
        if v3.length_squared() < f32::EPSILON {
            v.0 += dir.xy();
            info!("v3 < epsilon");
            continue;
        }

        let v_dir = v3.dot(dir);
        let v_not_dir = v3.length() - v_dir;
        let dx = if v_dir < 0. {
            dir * impulse_strength
        } else if v_not_dir > impulse_strength {
            let dx = (v3 - dir * v_dir) * -impulse_strength;

            painter.set_color(bevy::color::palettes::basic::FUCHSIA);
            painter.line(Vec3::ZERO, dx * 30.);
            // println!("dx: {dx}, dir: {dir}");
            painter.triangle(
                Vec2::new(1., 1.),
                Vec2::new(2., 2.),
                Vec2::new(3., 1.),
            );

            dx
        } else {
            let dx = dir * impulse_strength;

            painter.set_color(bevy::color::palettes::basic::PURPLE);
            painter.line(Vec3::ZERO, dx * 30.);
            // println!("dx: {dx}, dir: {dir}");

            dx
        };

        v.0 += dx.xy();
    }
}

pub fn handle_fire_missile(
    mut reader: EventReader<FireMissile>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    locs: Query<&Transform>,
    now: Res<Time<Virtual>>,
) {
    for FireMissile { from, target } in reader.read() {
        let starting_loc = locs.get(*from).unwrap();
        let target_loc = locs.get(*target).unwrap();

        // we will bump bc of collider, so do so in right direction
        let dir = (target_loc.translation - starting_loc.translation)
            .normalize()
            .xy();
        let loc = starting_loc.translation.xy() + dir * 5.;

        commands
            .entity(*from)
            .insert(dbg!(MissileLastFiredTime(now.elapsed_seconds_f64())));

        commands.spawn((
            Missile { target: *target },
            CraftKind::Missile,
            circle_bundle(1., 32., Color::srgb(0., 1., 1.), loc, &asset_server),
        ));
    }
}

#[derive(Component, Reflect, Debug)]
pub struct MissileLastFiredTime(pub f64);

#[derive(Event)]
pub struct FireMissile {
    pub from: Entity,
    pub target: Entity,
}

#[derive(Component, Reflect)]
pub struct Missile {
    pub target: Entity,
}

#[derive(Component, Reflect)]
pub struct MissileBot;

////////// Utils

pub trait Vec3Ext {
    fn new2(x: impl Into<f32>, y: impl Into<f32>) -> Vec3;
    fn from2(vec2: impl Into<Vec2>) -> Vec3;
}

impl Vec3Ext for Vec3 {
    fn new2(x: impl Into<f32>, y: impl Into<f32>) -> Vec3 {
        Vec3::new(x.into(), y.into(), 0.)
    }

    fn from2(vec2: impl Into<Vec2>) -> Vec3 {
        Vec3::from((vec2.into(), 0.))
    }
}
