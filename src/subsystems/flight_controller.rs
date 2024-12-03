use std::{
    f32::consts::PI,
    ops::{Add, Mul},
};

use bevy::time::Stopwatch;

use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct KeyboardFlightController;

pub struct FlightControllerPlugin;

impl Plugin for FlightControllerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<KeyboardFlightController>().add_systems(
            FixedUpdate,
            (flight_controller, keyboard_flight_controller),
        );
    }
}

fn keyboard_flight_controller(
    mut crafts: Query<
        (
            Entity,
            &Position,
            &mut LinearVelocity,
            &mut AngularVelocity,
            &mut Transform,
            &Engines,
        ),
        With<KeyboardFlightController>,
    >,
    mut elapsed: Local<Stopwatch>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let dt = elapsed.elapsed_secs_f64();
    elapsed.reset();

    let (e, pos, mut vel, mut ang_vel, mut trans, engines) =
        match crafts.get_single_mut() {
            Ok(v) => v,
            Err(bevy::ecs::query::QuerySingleError::MultipleEntities(s)) => {
                error!(
                    "expected only 1 craft to have keyboard flight \
                     controller, found multiple. Err: {s}"
                );
                return;
            }
            _ => {
                return;
            }
        };

    if keys.pressed(KeyCode::KeyW) {
        let dv = trans.local_y().xy() * engines.max_accel;
        // info!(?vel, ?dv, "W pressed");
        vel.0 += dv;
    }
    if keys.pressed(KeyCode::KeyS) {
        let dv = trans.local_y().xy() * engines.max_accel;
        // info!(?vel, ?dv, "S pressed");
        vel.0 -= dv;
    }
    if keys.pressed(KeyCode::KeyA) {
        let angle = trans.local_y().angle_between(Vec3::Y);
        // info!(angle, "A pressed");
        trans.rotate_z(PI / 100.);
        ang_vel.0 = 0.;
    }
    if keys.pressed(KeyCode::KeyD) {
        let angle = trans.local_y().angle_between(Vec3::Y);
        // info!(angle, "D pressed");
        trans.rotate_z(-PI / 100.);
        ang_vel.0 = 0.;
    }
}

fn flight_controller(
    mut commands: Commands,
    mut crafts: Query<
        (
            Entity,
            &Position,
            &mut LinearVelocity,
            &Engines,
            &FlightControllerTarget,
            &CraftKind,
        ),
        With<FlightController>,
    >,
    mut elapsed: Local<Stopwatch>,
) {
    let dt = elapsed.elapsed_secs_f64();
    elapsed.reset();

    for (e, pos, vel, engines, target, kind) in crafts.iter_mut() {
        //
        let dist = pos.distance(**target);
        if dist < 0.01 {
            info!(?target, kind = kind.to_string(), "Entity reached target");
            commands.entity(e).remove::<FlightControllerTarget>();
        }

        let travelled_in_dt = vel.length() as f64 * dt;
        let v = vel.length();
        let max_accel_vec = vel.normalize() * engines.max_accel;

        let dist_s = dist_to_stop(v, engines.max_accel);
        let dp = pos_at_t(pos.0, vel.0, max_accel_vec, dt as f32);

        //
    }

    //
}

fn pos_at_t<T: VecLike>(p: T, v: T, a: T, t: f32) -> T {
    p + (v + a * 0.5 * t) * t
}

fn vel_at_t<T: VecLike>(v: T, a: T, t: f32) -> T {
    v + a * t
}

trait VecLike:
    Sized + Add<Output = Self> + Mul<Output = Self> + Mul<f32, Output = Self>
{
}

impl<T> VecLike for T where
    T: Add<Output = T> + Mul<Output = T> + Mul<f32, Output = T>
{
}

fn dist_to_stop(v0: f32, a: f32) -> f32 {
    v0 * v0 / (a * 2.)
}

// fn disp_at_t(v: Vec2, a: Vec2, t: f32) -> Vec2 {
//     ()
// }
