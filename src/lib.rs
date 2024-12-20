#![allow(unused_imports, unused_variables)]
#![feature(duration_constructors)]

pub mod crafts;
pub mod input_handler;
// pub mod keyboard_controller;
pub mod physics;
pub mod prelude;
pub mod subsystems;
pub mod trajectory;
pub mod utils;

use std::borrow::Cow;

use bevy::ecs::world::Command;
use input_handler::InputHandlerPlugin;
use physics::{PhysicsSimulationPlugin, SimulationConfig};
use trajectory::TrajectoryPlugin;

use crate::prelude::*;

pub struct ParallaxProtocolArenaPlugin {
    pub config: SimulationConfig,
}

impl Plugin for ParallaxProtocolArenaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone()).insert_resource(
            Time::<Fixed>::from_hz(
                self.config.ticks_per_second as f64
                    * self.config.time_dilation as f64,
            ),
        );
        app.add_plugins((
            Shape2dPlugin::default(),
            InputHandlerPlugin,
            PhysicsSimulationPlugin {
                schedule: FixedUpdate,
                should_keep_alive: false,
            },
            TrajectoryPlugin,
        ));
    }
}

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

#[derive(Resource, Debug)]
pub struct Selected(pub Entity);
