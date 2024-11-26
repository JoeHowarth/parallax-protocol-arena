use engines::Engines;
use missile::MissileBay;

use crate::{circle_bundle, prelude::*, Health};

pub struct FrigatePlugin;

impl Plugin for FrigatePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Frigate>();
    }
}

impl Frigate {
    pub fn spawn(x: f32, y: f32, faction: Faction) -> impl Command {
        move |world: &mut World| {
            world.spawn(Frigate::bundle(
                world.resource::<AssetServer>(),
                Vec2::new(x, y),
                faction,
            ));
        }
    }

    pub fn bundle(
        asset_server: &AssetServer,
        loc: Vec2,
        faction: Faction,
    ) -> impl Bundle {
        let radius = 15.;
        let px = 32.;
        let script_path = "scripts/frigate.lua".to_string();
        let handle = asset_server.load(&script_path);
        (
            Frigate,
            ScriptCollection::<LuaFile> {
                scripts: vec![Script::new(script_path, handle)],
            },
            LuaHooks::one("on_update"),
            CraftKind::Frigate,
            Engines {
                max_accel: 0.4,
                max_rot: PI / 12.,
            },
            Health(50.),
            MissileBay {
                last_fired: -100.,
                reload_time: 5.,
            },
            ship_bundle("Ship.png", radius, px, faction, loc, asset_server),
        )
    }
}

#[derive(Component, Reflect)]
pub struct Frigate;

////////// Utils
