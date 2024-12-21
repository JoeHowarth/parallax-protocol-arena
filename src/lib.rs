#![allow(unused_imports, unused_variables)]
#![feature(duration_constructors)]

pub mod client;
pub mod crafts;
pub mod physics;
pub mod prelude;
pub mod subsystems;
pub mod utils;

use std::borrow::Cow;

use bevy::ecs::world::Command;
use client::ClientPlugin;

use crate::{
    client::{InputHandlerPlugin, TrajectoryPlugin},
    physics::{PhysicsSimulationPlugin, SimulationConfig},
    prelude::*,
};

pub struct ParallaxProtocolArenaPlugin<Label = FixedUpdate> {
    pub config: SimulationConfig,
    pub physics: PhysicsSimulationPlugin<Label>,
    pub client: Option<ClientPlugin>,
}

impl<Label: Send + Sync + 'static> Plugin
    for ParallaxProtocolArenaPlugin<Label>
{
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone()).insert_resource(
            Time::<Fixed>::from_hz(
                self.config.ticks_per_second as f64
                    * self.config.time_dilation as f64,
            ),
        );
        app.add_plugins((
            Shape2dPlugin::default(),
            PhysicsSimulationPlugin {
                schedule: FixedUpdate,
                should_keep_alive: false,
            },
        ));
        if let Some(client) = &self.client {
            app.add_plugins(client.clone());
        }
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
