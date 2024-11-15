use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct Frigate;

pub struct FrigatePlugin;

impl Plugin for FrigatePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Frigate>();
    }
}
