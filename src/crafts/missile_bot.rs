use crate::prelude::*;
// use std::{sync::Mutex, time::Instant};
//
// use anyhow::Result;
// use avian2d::prelude::*;
// use bevy::prelude::*;
// use bevy_mod_picking::{
//     debug::DebugPickingMode,
//     events::Click,
//     prelude::{On, *},
//     DefaultPickingPlugins,
//     PickableBundle,
// };
// use bevy_mod_scripting::{
//     api::{prelude::*, providers::bevy_ecs::LuaEntity},
//     prelude::*,
// };
// use bevy_vector_shapes::prelude::*;
use crate::{circle_bundle, prelude::*, Health};

pub struct MissileBotPlugin;

impl Plugin for MissileBotPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<MissileBot>();
    }
}

impl MissileBot {
    pub fn bundle(asset_server: &AssetServer, loc: Vec2) -> impl Bundle {
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
}

#[derive(Component, Reflect)]
pub struct MissileBot;

////////// Utils
