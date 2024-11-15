use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct MiningDrone;

pub struct MiningDronePlugin;

impl Plugin for MiningDronePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<MiningDrone>();
    }
}
