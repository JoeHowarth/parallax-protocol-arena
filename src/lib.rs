#![allow(unused_imports)]

use bevy::prelude::*;

pub mod cmd_server;
pub mod missile_bot;

pub use missile_bot::*;

#[derive(Component, Reflect)]
pub struct PlasmaBot;

#[derive(Component, Reflect)]
pub struct Health(pub f64);

pub fn health_despawn(mut commands: Commands, query: Query<(Entity, &Health)>) {
    for (e, h) in query.iter() {
        if h.0 <= 0.0001 {
            debug!("Despawning entity {e}");
            commands.entity(e).despawn();
        }
    }
}
