use bevy::{
    ecs::{system::EntityCommands, world::Command},
    time::common_conditions::on_timer,
};
use ustr::Ustr;

use crate::prelude::*;

/////////////// Commmands ////////////////

pub fn attach_script(
    script_path: impl Into<String>,
    entity: Entity,
) -> impl Command {
    let script_path = script_path.into();
    move |world: &mut World| {
        //
        let asset_server = world.resource::<AssetServer>();
        let handle = asset_server.load(&script_path);
        let commands = world.entity_mut(entity).insert(ScriptCollection::<
            LuaFile,
        > {
            scripts: vec![Script::new(script_path, handle)],
        });
    }
}

enum BoolOrToggle {
    True,
    False,
    Toggle,
}

fn set_enabled_lua_hooks(entity: Entity, val: BoolOrToggle) -> impl Command {
    move |world: &mut World| {
        let mut entity_mut = world.entity_mut(entity);
        let Some(mut hooks) = entity_mut.get_mut::<LuaHooks>() else {
            return;
        };

        match val {
            BoolOrToggle::True => hooks.enabled = true,
            BoolOrToggle::False => hooks.enabled = false,
            BoolOrToggle::Toggle => hooks.enabled = !hooks.enabled,
        }
    }
}

pub fn disable_lua_hooks(entity: Entity) -> impl Command {
    set_enabled_lua_hooks(entity, BoolOrToggle::False)
}

pub fn enable_lua_hooks(entity: Entity) -> impl Command {
    set_enabled_lua_hooks(entity, BoolOrToggle::True)
}

pub fn toggle_lua_hooks(entity: Entity) -> impl Command {
    set_enabled_lua_hooks(entity, BoolOrToggle::Toggle)
}

/////////////// Lua Provider //////////////////

pub trait LuaProvider: Sized {
    fn attach_lua_api(&mut self, lua: &mut Lua) -> mlua::Result<()>;

    fn setup_lua_script(
        &mut self,
        sd: &ScriptData,
        lua: &mut Lua,
    ) -> mlua::Result<()>;

    fn as_api_provider(self) -> Box<LuaApiProviderWrapper<Self>> {
        Box::new(LuaApiProviderWrapper(self))
    }
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

pub trait AddLuaProvider {
    fn add_lua_provider(
        &mut self,
        provider: impl LuaProvider + Send + Sync + 'static,
    ) -> &mut Self;
}

impl AddLuaProvider for App {
    fn add_lua_provider(
        &mut self,
        provider: impl LuaProvider + Send + Sync + 'static,
    ) -> &mut Self {
        self.add_api_provider::<LuaScriptHost<()>>(provider.as_api_provider())
    }
}

///////////// Plugin /////////////////////

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct LuaHooks {
    // TODO: consider using Ustr
    pub hooks: Vec<String>,
    pub enabled: bool,
}

impl LuaHooks {
    pub fn one(hook: impl Into<String>) -> LuaHooks {
        LuaHooks {
            hooks: vec![hook.into()],
            enabled: true,
        }
    }
}

impl Default for LuaHooks {
    fn default() -> Self {
        LuaHooks {
            hooks: vec!["on_update".into()],
            enabled: true,
        }
    }
}

pub struct LuaManagerPlugin;

impl Plugin for LuaManagerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<LuaHooks>()
            .add_script_host::<LuaScriptHost<()>>(PostUpdate)
            .add_api_provider::<LuaScriptHost<()>>(Box::new(
                LuaCoreBevyAPIProvider,
            ))
            .add_api_provider::<LuaScriptHost<()>>(Box::new(LuaBevyAPIProvider))
            .add_script_handler::<LuaScriptHost<()>, 0, 0>(FixedPostUpdate)
            .add_systems(
                FixedUpdate,
                send_lua_hooks.run_if(on_timer(Duration::from_millis(500))),
            );
    }
}

/// Sends events allowing scripts to drive update logic
pub fn send_lua_hooks(
    mut events: PriorityEventWriter<LuaEvent<()>>,
    hooks_q: Query<(Entity, &LuaHooks)>,
) {
    for (entity, hooks) in hooks_q.iter() {
        if !hooks.enabled {
            continue;
        }
        for hook in &hooks.hooks {
            events.send(
                LuaEvent {
                    hook_name: hook.clone(),
                    args: (),
                    recipients: Recipients::Entity(entity),
                },
                0,
            )
        }
    }
}

///////////////// enum utils ///////////////

pub fn setup_string_enum_kind_registry<
    T: Reflect + IntoEnumIterator + ToString + Clone,
>(
    lua: &Lua,
) -> LuaResult<()> {
    let craft_kinds = lua.create_table()?;
    let reverse_lookup = lua.create_table()?;

    // Iterate over all variants automatically using strum
    for (i, variant) in T::iter().enumerate() {
        let variant_str = variant.to_string();
        craft_kinds.set(variant_str.clone(), i)?;
        reverse_lookup.set(i, variant_str)?;
    }

    let short_name = T::short_name();
    lua.set_named_registry_value(
        &format!("{short_name}_reverse"),
        reverse_lookup,
    )?;
    lua.globals().set(short_name, craft_kinds)?;
    Ok(())
}

pub fn impl_into_lua_enum<'lua, T: Reflect + ToString>(
    this: T,
    lua: &'lua Lua,
) -> LuaResult<LuaValue<'lua>> {
    lua.globals()
        .get::<_, LuaTable>(this.reflect_short_type_path())?
        .get(this.to_string())
}

pub fn impl_from_lua_enum<
    'lua,
    T: IntoEnumIterator + Reflect + ToString + FromStr,
>(
    value: LuaValue<'lua>,
    lua: &'lua Lua,
) -> LuaResult<T> {
    let short_name = T::short_name();

    match value {
        LuaValue::Integer(i) => {
            let reverse_lookup: LuaTable =
                lua.named_registry_value(&format!("{short_name}_reverse"))?;
            let variant: String = reverse_lookup.get(i)?;
            variant
                .parse()
                .map_err(move |_| LuaError::FromLuaConversionError {
                    from: "integer",
                    to: short_name,
                    message: Some(format!("Invalid {short_name} index")),
                })
        }
        _ => Err(LuaError::FromLuaConversionError {
            from: value.type_name(),
            to: short_name,
            message: Some("Expected integer".into()),
        }),
    }
}

pub trait EnumShortName: IntoEnumIterator + Reflect {
    fn short_name() -> &'static str {
        let val = Self::iter().next().unwrap();
        ustr::ustr(val.reflect_short_type_path()).as_str()
    }
}

impl<T: IntoEnumIterator + Reflect> EnumShortName for T {}
