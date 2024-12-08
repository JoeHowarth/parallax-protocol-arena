#![allow(unused_imports, unused_variables)]
#![feature(duration_constructors)]

pub mod crafts;
// pub mod keyboard_controller;
pub mod physics;
pub mod prelude;
// pub mod subsystems;
pub mod utils;

use std::borrow::Cow;

use bevy::ecs::world::Command;

use crate::prelude::*;

#[derive(Component, Reflect, Clone, Debug)]
pub struct Health(pub f64);

pub fn send_event<E: Event>(e: E) -> impl Command {
    move |world: &mut World| {
        world.send_event(e);
    }
}

pub fn health_despawn(mut commands: Commands, query: Query<(Entity, &Health)>) {
    for (e, h) in query.iter() {
        if h.0 <= 0.0001 {
            debug!("Despawning entity {e}");
            commands.entity(e).despawn();
        }
    }
}
