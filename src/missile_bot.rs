use anyhow::Result;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_mod_picking::{
    debug::DebugPickingMode,
    events::Click,
    prelude::{On, *},
    DefaultPickingPlugins,
    PickableBundle,
};
use bevy_vector_shapes::prelude::*;

use crate::agent_runtime::*;

pub struct Name(pub String);

pub enum Action {
    MoveTo(Vec2),
    FireMissile(Name),
}

pub fn missile_bot_bundle(
    asset_server: &AssetServer,
    loc: Vec2,
) -> impl Bundle {
    let radius = 15.;
    let px = 32.;
    let color = Color::srgb(1.0, 0.0, 0.1);
    (
        MissleBot,
        circle_bundle(radius, px, color, loc, asset_server),
    )
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

pub fn update_missiles(
    missiles: Query<(Entity, &Missile)>,
    mut p: ParamSet<(
        Query<&Transform>,
        Query<&mut LinearVelocity, With<Missile>>,
    )>,
    mut painter: ShapePainter,
) {
    // Apply a scaled impulse
    // Adjust this value as needed
    let impulse_strength = 0.1;

    for (e, missile) in missiles.iter() {
        let missile_trans = p.p0().get(e).unwrap().translation;
        let target_trans = p.p0().get(missile.target).unwrap().translation;

        painter.set_translation(missile_trans);

        let dir = (target_trans - missile_trans).normalize();
        let mut p1 = p.p1();
        let mut v = p1.get_mut(e).unwrap();
        let v3 = Vec3::from2(v.0);

        painter.set_color(bevy::color::palettes::basic::AQUA);
        painter.line(Vec3::ZERO, dir * 30.);
        painter.set_color(bevy::color::palettes::basic::LIME);
        painter.line(Vec3::ZERO, v3 * 0.1);

        // First, ensure v3 is not zero
        if v3.length_squared() < f32::EPSILON {
            v.0 += dir.xy();
            info!("v3 < epsilon");
            continue;
        }

        let v_dir = v3.dot(dir);
        let v_not_dir = v3.length() - v_dir;
        let dx = if v_dir < 0. {
            dir * impulse_strength
        } else if v_not_dir > impulse_strength {
            let dx = (v3 - dir * v_dir) * -impulse_strength;

            painter.set_color(bevy::color::palettes::basic::FUCHSIA);
            painter.line(Vec3::ZERO, dx * 30.);
            println!("dx: {dx}, dir: {dir}");
            painter.triangle(
                Vec2::new(1., 1.),
                Vec2::new(2., 2.),
                Vec2::new(3., 1.),
            );

            dx
        } else {
            let dx = dir * impulse_strength;

            painter.set_color(bevy::color::palettes::basic::PURPLE);
            painter.line(Vec3::ZERO, dx * 30.);
            println!("dx: {dx}, dir: {dir}");

            dx
        };

        v.0 += dx.xy();
    }
}

pub fn handle_fire_missile(
    mut reader: EventReader<FireMissile>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    locs: Query<&Transform>,
) {
    for FireMissile { from, target } in reader.read() {
        let starting_loc = locs.get(*from).unwrap();
        let target_loc = locs.get(*target).unwrap();

        // we will bump bc of collider, so do so in right direction
        let dir = (target_loc.translation - starting_loc.translation)
            .normalize()
            .xy();
        let loc = starting_loc.translation.xy() + dir * 5.;

        commands.spawn((
            Missile { target: *target },
            circle_bundle(1., 32., Color::srgb(0., 1., 1.), loc, &asset_server),
        ));
    }
}

#[derive(Event)]
pub struct FireMissile {
    pub from: Entity,
    pub target: Entity,
}

#[derive(Component, Reflect)]
pub struct Missile {
    pub target: Entity,
}

#[derive(Component, Reflect)]
pub struct MissleBot;

////////// Utils

pub trait Vec3Ext {
    fn new2(x: impl Into<f32>, y: impl Into<f32>) -> Vec3;
    fn from2(vec2: impl Into<Vec2>) -> Vec3;
}

impl Vec3Ext for Vec3 {
    fn new2(x: impl Into<f32>, y: impl Into<f32>) -> Vec3 {
        Vec3::new(x.into(), y.into(), 0.)
    }

    fn from2(vec2: impl Into<Vec2>) -> Vec3 {
        Vec3::from((vec2.into(), 0.))
    }
}
