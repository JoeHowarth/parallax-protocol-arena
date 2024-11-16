use bevy::color::palettes;

use crate::prelude::*;

pub mod asteroid;
pub mod frigate;
pub mod mining_drone;
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
        dbg!("setting up faction");
        setup_string_enum_kind_registry::<Faction>(lua)?;
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

macro_rules! lua_enum {
    (pub enum $name:ident { $($variant:ident),* $(,)? }) => {
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
        pub enum $name {
            #[default]
            $($variant),*
        }

        impl<'lua> IntoLua<'lua> for $name {
            fn into_lua(self, lua: &'lua Lua) -> LuaResult<LuaValue<'lua>> {
                impl_into_lua_enum(self, lua)
            }
        }

        impl<'lua> FromLua<'lua> for $name {
            fn from_lua(value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
                impl_from_lua_enum(value, lua)
            }
        }
    }
}

lua_enum! {
    pub enum Faction {
        Unaligned,
        Unknown,
        Blue,
        Red,
    }
}

impl Faction {
    pub fn sprite_color(&self) -> Color {
        use palettes::basic;
        Color::Srgba(match self {
            Faction::Unaligned => basic::WHITE,
            Faction::Unknown => basic::GRAY,
            Faction::Blue => basic::BLUE,
            Faction::Red => basic::RED,
        })
    }
}

lua_enum! {
    pub enum CraftKind {
        Asteroid,
        Frigate,
        PlasmaDrone,
        Missile,
    }
}

// #[derive(
//     Component,
//     Default,
//     Reflect,
//     Copy,
//     Clone,
//     Debug,
//     strum::Display,
//     EnumString,
//     EnumIter,
// )]
// pub enum CraftKind {
//     #[default]
//     Asteroid,
//     Frigate,
//     PlasmaDrone,
//     Missile,
// }
//
// impl<'lua> IntoLua<'lua> for CraftKind {
//     fn into_lua(self, lua: &'lua Lua) -> LuaResult<LuaValue<'lua>> {
//         impl_into_lua_enum(self, lua)
//     }
// }
//
// impl<'lua> FromLua<'lua> for CraftKind {
//     fn from_lua(value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
//         impl_from_lua_enum(value, lua)
//     }
// }
