use crate::prelude::*;

pub mod asteroid;
pub mod frigate;
pub mod mining_drone;
pub mod missile_bot;
pub mod plasma_drone;

pub struct CraftsPlugin;

impl Plugin for CraftsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CraftKind>()
            .add_lua_provider(CraftsPlugin);
    }
}

impl LuaProvider for CraftsPlugin {
    fn attach_lua_api(&mut self, lua: &mut Lua) -> mlua::Result<()> {
        setup_string_enum_kind_registry::<CraftKind>(lua)?;
        Ok(())
    }

    fn setup_lua_script(
        &mut self,
        sd: &ScriptData,
        lua: &mut Lua,
    ) -> mlua::Result<()> {
        Ok(())
    }
}

#[derive(
    Component,
    Default,
    Reflect,
    Copy,
    Clone,
    Debug,
    strum::Display,
    EnumString,
    EnumIter,
)]
pub enum CraftKind {
    #[default]
    Asteroid,
    MissileBot,
    PlasmaDrone,
    Missile,
}

impl<'lua> IntoLua<'lua> for CraftKind {
    fn into_lua(self, lua: &'lua Lua) -> LuaResult<LuaValue<'lua>> {
        impl_into_lua_enum(self, lua)
    }
}

impl<'lua> FromLua<'lua> for CraftKind {
    fn from_lua(value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        impl_from_lua_enum(value, lua)
    }
}
