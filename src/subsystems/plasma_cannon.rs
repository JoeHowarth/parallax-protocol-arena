use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct PlasmaCannon;

pub struct PlasmaCannonPlugin;

impl Plugin for PlasmaCannonPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PlasmaCannon>();
    }
}
