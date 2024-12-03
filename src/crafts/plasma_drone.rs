use std::f32::consts::PI;

use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct PlasmaDrone;

pub struct PlasmaDronePlugin;

impl Plugin for PlasmaDronePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PlasmaDrone>();
    }
}

impl PlasmaDrone {
    // pub fn bundle(
    //     asset_server: &AssetServer,
    //     loc: Vec2,
    //     faction: Faction,
    // ) -> impl Bundle {
    //     let radius = 10.;
    //     let px = 32.;
    //     let color = Color::srgb(0.0, 1.0, 0.1);
    //     (
    //         PlasmaDrone,
    //         Health(20.),
    //         Engines {
    //             max_accel: 100.0,
    //             max_rot: PI / 12.,
    //         },
    //         CraftKind::PlasmaDrone,
    //         ship_bundle(
    //             "circle-32.png",
    //             radius,
    //             px,
    //             faction,
    //             loc,
    //             asset_server,
    //         ),
    //     )
    // }
}
