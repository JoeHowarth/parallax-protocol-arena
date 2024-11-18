#![allow(unused_imports, unused_variables)]

pub mod cmd_server;
pub mod crafts;
pub mod keyboard_controller;
pub mod lua_utils;
pub mod prelude;
pub mod subsystems;
pub mod utils;

use std::borrow::Cow;

use bevy::ecs::world::Command;
use bevy_mod_picking::prelude::*;
pub use crafts::*;
pub use subsystems::*;

use crate::prelude::*;

#[derive(Component, Reflect)]
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

pub fn circle_bundle(
    radius: f32,
    px: f32,
    color: Color,
    loc: Vec2,
    asset_server: &AssetServer,
) -> impl Bundle {
    (
        SpriteBundle {
            texture: asset_server.load("circle-32.png"),
            transform:
                Transform::from_translation(Vec3::from2(loc)) //
                    .with_scale(Vec3::new(
                        2. * radius / px,
                        2. * radius / px,
                        1.,
                    )),
            sprite: Sprite { color, ..default() },
            ..default()
        },
        RigidBody::Dynamic,
        Collider::circle(radius),
        PickableBundle::default(),
    )
}

pub fn ship_bundle(
    sprite_name: &'static str,
    radius: f32,
    px: f32,
    faction: Faction,
    loc: Vec2,
    asset_server: &AssetServer,
) -> impl Bundle {
    (
        faction,
        SpriteBundle {
            texture: asset_server.load(sprite_name),
            transform:
                Transform::from_translation(Vec3::from2(loc)) //
                    .with_scale(Vec3::new(
                        2. * radius / px,
                        2. * radius / px,
                        1.,
                    )),
            sprite: Sprite {
                color: faction.sprite_color(),
                ..default()
            },
            ..default()
        },
        RigidBody::Dynamic,
        Collider::circle(radius),
        PickableBundle::default(),
    )
}
