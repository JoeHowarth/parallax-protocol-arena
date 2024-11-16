use flight_controller::Engines;

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
    pub fn bundle(asset_server: &AssetServer, loc: Vec2) -> impl Bundle {
        let radius = 10.;
        let px = 32.;
        let color = Color::srgb(0.0, 1.0, 0.1);
        (
            PlasmaDrone,
            Health(20.),
            Engines { max_accel: 1.0 },
            CraftKind::PlasmaDrone,
            circle_bundle(radius, px, color, loc, asset_server),
        )
    }
}
