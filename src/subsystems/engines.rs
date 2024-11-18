use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct Engines {
    pub max_accel: f32,
    pub max_ang_accel: f32,
}

#[derive(Component, Reflect, Debug)]
pub struct EngineInput {
    pub accel: f32,
    pub target_ang: f32,
    pub max_ang_vel: f32,
}

pub struct EnginesPlugin;

impl Plugin for EnginesPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Engines>();
    }
}

use std::f32::consts::PI;
fn apply_engine_inputs(
    mut query: Query<(
        Entity,
        &EngineInput,
        &Engines,
        &Transform,
        &mut LinearVelocity,
        &mut AngularVelocity,
    )>,
) {
    for inputs in query.iter_mut() {
        let (_entity, input, engines, transform, mut _vel, mut ang_vel) =
            inputs;
        apply_engine_inputs_inner((
            _entity,
            input,
            engines,
            transform,
            &mut _vel,
            &mut ang_vel,
        ));
    }
}

fn apply_engine_inputs_inner(
    inputs: (
        Entity,
        &EngineInput,
        &Engines,
        &Transform,
        &mut LinearVelocity,
        &mut AngularVelocity,
    ),
) {
    let (_entity, input, engines, transform, _vel, mut ang_vel) = inputs;
    // Get current angle from transform
    let current_angle = transform.rotation.to_euler(EulerRot::ZYX).0;

    // Calculate angle difference (shortest path)
    let mut angle_diff = input.target_ang - current_angle;
    while angle_diff > PI {
        angle_diff -= 2.0 * PI;
    }
    while angle_diff < -PI {
        angle_diff += 2.0 * PI;
    }

    // Calculate stopping distance at current velocity
    let stopping_distance =
        (ang_vel.0 * ang_vel.0).abs() / (2.0 * engines.max_ang_accel);

    // Determine required acceleration
    let ang_accel = if angle_diff.abs() <= 0.01 && ang_vel.0.abs() <= 0.01 {
        // Already at target and stopped
        0.0
    } else if stopping_distance >= angle_diff.abs() {
        // Need to brake
        if ang_vel.0 > 0.0 {
            -engines.max_ang_accel
        } else {
            engines.max_ang_accel
        }
    } else {
        // Can accelerate towards target
        if angle_diff > 0.0 {
            engines.max_ang_accel
        } else {
            -engines.max_ang_accel
        }
    };

    // Apply acceleration while respecting max angular velocity
    let new_ang_vel = ang_vel.0 + ang_accel;
    ang_vel.0 = new_ang_vel.clamp(-input.max_ang_vel, input.max_ang_vel);
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use bevy::math::Vec2;

    use super::*;

    // Helper function to create test entity with required components
    fn setup_test_entity() -> (
        Engines,
        EngineInput,
        Transform,
        LinearVelocity,
        AngularVelocity,
    ) {
        let engines = Engines {
            max_accel: 10.0,
            max_ang_accel: PI / 4.0, // 45 degrees/s²
        };

        let engine_input = EngineInput {
            accel: 0.0,
            target_ang: 0.0,
            max_ang_vel: PI / 2.0, // 90 degrees/s
        };

        let transform = Transform::from_xyz(0.0, 0.0, 0.0);
        let linear_velocity = LinearVelocity(Vec2::ZERO);
        let angular_velocity = AngularVelocity(0.0);

        (
            engines,
            engine_input,
            transform,
            linear_velocity,
            angular_velocity,
        )
    }

    #[test]
    fn test_rotation_towards_target() {
        let (
            engines,
            mut input,
            mut transform,
            mut linear_vel,
            mut angular_vel,
        ) = setup_test_entity();

        // Set initial conditions
        transform.rotation = Quat::from_rotation_z(0.0); // Facing right (0 degrees)
        input.target_ang = PI / 2.0; // Target is 90 degrees

        // Step simulation multiple times
        for _ in 0..10 {
            apply_engine_inputs_inner((
                Entity::from_raw(0),
                &input,
                &engines,
                &transform,
                &mut linear_vel,
                &mut angular_vel,
            ));

            // Apply angular velocity to transform (simulating physics step)
            transform.rotate_z(angular_vel.0);
        }

        // Should reach target without overshooting
        let final_angle = transform.rotation.to_euler(EulerRot::ZYX).0;
        assert!(
            (final_angle - PI / 2.0).abs() < 0.01,
            "Should reach target angle without overshooting"
        );
    }

    #[test]
    fn test_rotation_braking() {
        let (
            engines,
            mut input,
            mut transform,
            mut linear_vel,
            mut angular_vel,
        ) = setup_test_entity();

        // Set initial conditions
        transform.rotation = Quat::from_rotation_z(0.0);
        input.target_ang = PI / 4.0; // Target is 45 degrees
        angular_vel.0 = PI; // Starting with high angular velocity

        // Step simulation multiple times
        for _ in 0..10 {
            apply_engine_inputs_inner((
                Entity::from_raw(0),
                &input,
                &engines,
                &transform,
                &mut linear_vel,
                &mut angular_vel,
            ));
            transform.rotate_z(angular_vel.0);
        }

        // Should brake and reach target without overshooting
        let final_angle = transform.rotation.to_euler(EulerRot::ZYX).0;
        assert!(
            (final_angle - PI / 4.0).abs() < 0.01,
            "Should brake and reach target precisely"
        );
    }

    #[test]
    fn test_overshooting_correction() {
        let (
            engines,
            mut input,
            mut transform,
            mut linear_vel,
            mut angular_vel,
        ) = setup_test_entity();

        // Set initial conditions - already moving away from target
        transform.rotation = Quat::from_rotation_z(0.0);
        input.target_ang = PI / 4.0; // Target is 45 degrees
        angular_vel.0 = -PI; // Moving in wrong direction

        // Step simulation multiple times
        for _ in 0..20 {
            apply_engine_inputs_inner((
                Entity::from_raw(0),
                &input,
                &engines,
                &transform,
                &mut linear_vel,
                &mut angular_vel,
            ));
            transform.rotate_z(angular_vel.0);
        }

        // Should correct course and reach target
        let final_angle = transform.rotation.to_euler(EulerRot::ZYX).0;
        assert!(
            (final_angle - PI / 4.0).abs() < 0.01,
            "Should correct overshooting and reach target"
        );
    }

    #[test]
    fn test_linear_acceleration() {
        let (engines, mut input, transform, mut linear_vel, mut angular_vel) =
            setup_test_entity();

        // Set acceleration to full forward
        input.accel = 1.0;

        apply_engine_inputs_inner((
            Entity::from_raw(0),
            &input,
            &engines,
            &transform,
            &mut linear_vel,
            &mut angular_vel,
        ));

        // Velocity should be in facing direction with magnitude of max_accel
        assert!(
            (linear_vel.0.length() - engines.max_accel).abs() < 0.01,
            "Should apply full acceleration in facing direction"
        );
    }
}
