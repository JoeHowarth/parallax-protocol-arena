use std::fs;

use bevy::{
    ecs::component::{RequiredComponent, RequiredComponentConstructor},
    sprite::Anchor,
};
use rand::Rng;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    physics::{PhysicsBundle, PhysicsState, SimulationConfig},
    prelude::*,
};

/// Holds sprite sheet data
#[derive(Resource)]
pub struct AsteroidAssets {
    texture: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
    _rects: Vec<URect>,
}

#[derive(Component, Reflect, Debug, Default)]
pub struct Asteroid;

#[derive(Component, Reflect, Debug)]
#[require(Asteroid)]
pub struct SmallAsteroid;

impl SmallAsteroid {
    pub fn bundle(
        tick: u64,
        assets: &AsteroidAssets,
        position: Vec2,
        velocity: Vec2,
    ) -> impl Bundle {
        (
            Self,
            Sprite::from_atlas_image(
                assets.texture.clone(),
                TextureAtlas {
                    layout: assets.layout.clone(),
                    index: 0,
                },
            ),
            Transform::from_scale(Vec3::new(1., 1., 1.))
                .with_translation(position.to3()),
            PhysicsBundle::from_state(
                tick,
                PhysicsState {
                    pos: position,
                    vel: velocity,
                    mass: 10.,
                    alive: true,
                    ..default()
                },
                Vec2::new(28., 28.),
            ),
        )
    }

    pub fn spawn(position: Vec2, velocity: Vec2) -> impl Command {
        move |world: &mut World| {
            let assets = world.resource::<AsteroidAssets>();
            let tick = world.resource::<SimulationConfig>().current_tick;
            world.spawn(Self::bundle(tick, assets, position, velocity));
        }
    }
}

#[derive(Resource, Reflect)]
pub struct AsteroidSpriteLayout(pub Handle<TextureAtlasLayout>);

pub struct AsteroidPlugin;

impl Plugin for AsteroidPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AsteroidSpriteLayout>();
        app.register_type::<Asteroid>();
        app.add_systems(Startup, setup);
    }
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Load and parse JSON file
    let json_content = fs::read_to_string("assets/asteroids.json")
        .expect("Failed to read asteroid JSON file");
    let json: Value = serde_json::from_str(&json_content)
        .expect("Failed to parse asteroid JSON");

    // Load the sprite sheet
    let texture = asset_server.load("asteroids-sheet.png");
    let mut layout = TextureAtlasLayout::new_empty(UVec2::new(502, 503));

    // Extract slice coordinates from JSON
    if let Some(slices) = json["meta"]["slices"].as_array() {
        for slice in slices {
            if let Some(bounds) = slice["keys"][0]["bounds"].as_object() {
                let x = bounds["x"].as_i64().unwrap_or(0) as u32;
                let y = bounds["y"].as_i64().unwrap_or(0) as u32;
                let w = bounds["w"].as_i64().unwrap_or(0) as u32;
                let h = bounds["h"].as_i64().unwrap_or(0) as u32;

                layout.add_texture(URect::new(x, y, x + w, y + h));
            }
        }
    }

    let rects = layout.textures.clone();
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    commands.insert_resource(AsteroidAssets {
        texture,
        layout: texture_atlas_layout,
        _rects: rects,
    });
}
