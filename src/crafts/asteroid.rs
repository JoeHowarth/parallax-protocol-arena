use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct Asteroid;

pub struct AsteroidPlugin;

impl Plugin for AsteroidPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Asteroid>();
    }
}
