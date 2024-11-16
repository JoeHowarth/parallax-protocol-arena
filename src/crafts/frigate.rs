use flight_controller::Engines;

use crate::{circle_bundle, prelude::*, Health};

pub struct FrigatePlugin;

impl Plugin for FrigatePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Frigate>();
    }
}

impl Frigate {
    pub fn bundle(asset_server: &AssetServer, loc: Vec2) -> impl Bundle {
        let radius = 15.;
        let px = 32.;
        let color = Color::srgb(1.0, 0.0, 0.1);
        let script_path = "scripts/frigate.lua".to_string();
        let handle = asset_server.load(&script_path);
        (
            Frigate,
            ScriptCollection::<LuaFile> {
                scripts: vec![Script::new(script_path, handle)],
            },
            LuaHooks::one("on_update"),
            CraftKind::Frigate,
            Engines { max_accel: 0.4 },
            Health(50.),
            ship_bundle(radius, px, color, loc, asset_server),
        )
    }
}

#[derive(Component, Reflect)]
pub struct Frigate;

////////// Utils
