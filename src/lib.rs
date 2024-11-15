#![allow(unused_imports)]

use std::sync::Mutex;

use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;

pub mod cmd_server;
pub mod missile_bot;
pub mod sensor;

pub use missile_bot::*;
// pub use sensor::*;

#[derive(Component, Reflect)]
pub struct PlasmaBot;

#[derive(Component, Reflect)]
pub struct Health(pub f64);

pub fn health_despawn(mut commands: Commands, query: Query<(Entity, &Health)>) {
    for (e, h) in query.iter() {
        if h.0 <= 0.0001 {
            debug!("Despawning entity {e}");
            commands.entity(e).despawn();
        }
    }
}

pub trait LuaProvider {
    fn attach_lua_api(&mut self, ctx: &mut Lua) -> mlua::Result<()>;
    fn setup_lua_script(
        &mut self,
        sd: &ScriptData,
        ctx: &mut Lua,
    ) -> mlua::Result<()>;
}

pub struct LuaApiProviderWrapper<T>(pub T);

impl<T: LuaProvider + Send + Sync + 'static> APIProvider
    for LuaApiProviderWrapper<T>
{
    type APITarget = Mutex<Lua>;
    type DocTarget = LuaDocFragment;
    type ScriptContext = Mutex<Lua>;

    fn attach_api(
        &mut self,
        api: &mut Self::APITarget,
    ) -> std::result::Result<(), ScriptError> {
        let ctx = api.get_mut().unwrap();
        self.0.attach_lua_api(ctx).map_err(ScriptError::new_other)
    }

    fn setup_script(
        &mut self,
        sd: &ScriptData,
        api: &mut Self::ScriptContext,
    ) -> std::result::Result<(), ScriptError> {
        let ctx = api.get_mut().unwrap();
        self.0
            .setup_lua_script(sd, ctx)
            .map_err(ScriptError::new_other)
    }
}
