use crate::prelude::*;

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
