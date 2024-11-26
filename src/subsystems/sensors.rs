use std::{str::FromStr, sync::Mutex};

use anyhow::{Context, Result};
use avian2d::prelude::{LinearVelocity, Position};
// use avian2d::prelude::*;
use bevy::{math::NormedVectorSpace, prelude::*};
use bevy_mod_picking::{
    debug::DebugPickingMode,
    events::Click,
    prelude::{On, *},
    DefaultPickingPlugins,
    PickableBundle,
};
use bevy_mod_scripting::{
    api as bevy_script_api,
    prelude as bevy_mod_scripting_core,
    prelude as bevy_mod_scripting_lua,
    prelude::*,
};
// use bevy_vector_shapes::prelude::*;
use strum::{EnumIter, EnumString};

use crate::{lua_utils::impl_from_lua_enum, prelude::*, CraftKind};

pub struct SensorPlugin;

impl Plugin for SensorPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Action>()
            .register_type::<CraftKind>()
            .register_type::<CraftState>()
            .register_type::<SensorReading>()
            .add_lua_provider(SensorPlugin);
    }
}

impl LuaProvider for SensorPlugin {
    fn attach_lua_api(&mut self, ctx: &mut Lua) -> mlua::Result<()> {
        setup_enum_registries(ctx)?;
        Ok(())
    }

    fn setup_lua_script(
        &mut self,
        sd: &ScriptData,
        lua: &mut Lua,
    ) -> mlua::Result<()> {
        let table = lua.create_table()?;
        let craft_entity = sd.entity;
        table.set("craft_entity", Entity::to_lua_proxy(sd.entity, lua)?)?;
        table.set("range", 500_f32)?;

        table.set(
            "faction",
            lua.create_function(move |lua, _: Value| {
                let world = lua.get_world()?;
                let world = world.read();
                let faction =
                    world.entity(craft_entity).get::<Faction>().unwrap();
                Ok(*faction)
            })?,
        )?;

        table.set(
            "craft_state",
            lua.create_function(move |lua, sensors: Value| {
                let world = lua.get_world()?;
                let world = world.read();
                let e = LuaError::RuntimeError(
                    "Failed to get component for craft_entity".into(),
                );
                let trans = world
                    .entity(craft_entity)
                    .get::<Transform>()
                    .ok_or(e.clone())?;
                let vel = world
                    .entity(craft_entity)
                    .get::<LinearVelocity>()
                    .ok_or(e.clone())?;
                let health = world
                    .entity(craft_entity)
                    .get::<Health>()
                    .ok_or(e.clone())?
                    .clone();
                let kind = world
                    .entity(craft_entity)
                    .get::<CraftKind>()
                    .ok_or(e)?
                    .clone();

                Ok(CraftState {
                    pos: trans.translation.xy(),
                    vel: vel.0,
                    forwards: trans.local_y().xy(),
                    health,
                    kind,
                })
            })?,
        )?;

        table.set(
            "contacts",
            lua.create_function(move |ctx, sensors: Value| {
                let world = ctx.get_world()?;
                let mut world = world.write();

                let craft_pos = world
                    .entity(craft_entity)
                    .get::<Position>()
                    .ok_or(LuaError::RuntimeError(
                        "Failed to get entity".into(),
                    ))?
                    .clone();

                let mut query = world.query::<(
                    Entity,
                    &CraftKind,
                    &Position,
                    &LinearVelocity,
                    &Faction,
                )>();

                // let mut sensor_range: f32 = sensors.get("range")?;
                // if let Ok(limited_range) = _opts.get("range") {
                //     sensor_range = sensor_range.min(limited_range);
                // }
                let mut results_vec = Vec::new();

                for (e, kind, pos, vel, faction) in query.iter(&world) {
                    let dist = pos.distance(craft_pos.xy());
                    // if dist < sensor_range {
                    if dist < 500. {
                        results_vec.push(SensorReading {
                            entity: e,
                            pos: pos.xy(),
                            vel: vel.xy(),
                            dist,
                            kind: *kind,
                            faction: *faction,
                        });
                    }
                }
                results_vec.sort_by_key(|s| (s.dist * 1000.) as i32);

                let results =
                    ctx.create_table_with_capacity(results_vec.len(), 0)?;
                for r in results_vec {
                    results.push(r)?;
                }

                Ok(results)
            })?,
        )?;

        lua.globals().set("sensors", table)?;

        Ok(())
    }
}

#[derive(EnumDiscriminants, Component, Reflect, Debug, Copy, Clone)]
#[strum_discriminants(derive(EnumString, Display, EnumIter, Reflect))]
pub enum Action {
    MoveTo(Vec2),
    FireMissile(Entity),
}

impl<'lua> IntoLua<'lua> for ActionDiscriminants {
    fn into_lua(self, lua: &'lua Lua) -> LuaResult<Value<'lua>> {
        impl_into_lua_enum(self, lua)
    }
}

impl<'lua> FromLua<'lua> for ActionDiscriminants {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        impl_from_lua_enum(value, lua)
    }
}

impl<'lua> IntoLua<'lua> for Action {
    fn into_lua(self, lua: &'lua Lua) -> LuaResult<Value<'lua>> {
        let table = lua.create_table()?;

        let kind = ActionDiscriminants::from(self);
        table.set("kind", kind.into_lua(lua)?)?;

        let v = match self {
            Action::MoveTo(vec2) => vec2.to_lua_proxy(lua),
            Action::FireMissile(entity) => entity.to_lua_proxy(lua),
        };
        table.set("v", v?)?;
        table.into_lua(lua)
    }
}

impl<'lua> FromLua<'lua> for Action {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        let table = LuaTable::from_lua(value, lua)?;
        let kind = ActionDiscriminants::from_lua(table.get("kind")?, lua)?;
        match kind {
            ActionDiscriminants::MoveTo => {
                Ok(Action::MoveTo(Vec2::from_lua_proxy(table.get("v")?, lua)?))
            }
            ActionDiscriminants::FireMissile => Ok(Action::FireMissile(
                Entity::from_lua_proxy(table.get("v")?, lua)?,
            )),
        }
    }
}

use strum::{Display, EnumDiscriminants, IntoEnumIterator};
// use strum::*;

pub fn setup_enum_registries(lua: &Lua) -> LuaResult<()> {
    setup_string_enum_kind_registry::<ActionDiscriminants>(lua)
}

#[derive(Debug, Clone, Reflect, Component)]
// #[reflect(Component, LuaProxyable)]
pub struct SensorReading {
    pub entity: Entity,
    pub pos: Vec2,
    pub vel: Vec2,
    pub dist: f32,
    pub kind: CraftKind,
    pub faction: Faction,
}

impl<'lua> IntoLua<'lua> for SensorReading {
    fn into_lua(self, lua: &'lua Lua) -> LuaResult<Value<'lua>> {
        let table = lua.create_table()?;
        table.set("entity", Entity::to_lua_proxy(self.entity, lua)?)?;
        table.set("pos", Vec2::to_lua_proxy(self.pos, lua)?)?;
        table.set("vel", Vec2::to_lua_proxy(self.vel, lua)?)?;
        table.set("dist", self.dist)?;
        table.set("kind", self.kind)?;
        table.set("faction", self.faction)?;

        table.into_lua(lua)
    }
}

#[derive(Debug, Clone, Reflect, Component)]
pub struct CraftState {
    pub pos: Vec2,
    pub vel: Vec2,
    pub forwards: Vec2,
    pub kind: CraftKind,
    pub health: Health,
}

impl<'lua> IntoLua<'lua> for CraftState {
    fn into_lua(self, lua: &'lua Lua) -> LuaResult<Value<'lua>> {
        let table = lua.create_table()?;
        table.set("pos", self.pos.to_lua_proxy(lua)?)?;
        table.set("vel", self.vel.to_lua_proxy(lua)?)?;
        table.set("forwards", self.forwards.to_lua_proxy(lua)?)?;
        table.set("kind", self.kind)?;
        table.set("health", self.health.0)?;
        table.into_lua(lua)
    }
}
