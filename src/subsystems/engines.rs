use std::f32::consts::PI;

use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct Engines {
    pub max_accel: f32,
    pub max_rot: f32,
}

#[derive(Component, Reflect, Debug)]
pub struct EngineInput {
    pub accel: f32,
    pub target_ang: f32,
}

pub struct EnginesPlugin;

impl Plugin for EnginesPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Engines>();
    }
}

fn apply_engine_inputs(
    mut query: Query<(
        Entity,
        &EngineInput,
        &Engines,
        &mut Transform,
        &mut LinearVelocity,
    )>,
) {
    for inputs in query.iter_mut() {
        let (_entity, input, engines, mut transform, mut _vel) = inputs;
        apply_engine_inputs_inner((
            _entity,
            input,
            engines,
            &mut transform,
            &mut _vel,
        ));
    }
}

fn apply_engine_inputs_inner(
    inputs: (
        Entity,
        &EngineInput,
        &Engines,
        &mut Transform,
        &mut LinearVelocity,
    ),
) {
    let (_entity, input, engines, transform, vel) = inputs;
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

    transform.rotate_z(angle_diff.clamp(-engines.max_rot, engines.max_rot));

    vel.0 += transform.local_y().xy() * input.accel.min(engines.max_accel);
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use bevy::math::Vec2;

    use super::*;

    // Helper function to create test entity with required components
    fn setup_test_entity() -> (Engines, EngineInput, Transform, LinearVelocity)
    {
        let engines = Engines {
            max_accel: 10.0,
            max_rot: PI / 12., // 15 degress per tick
        };

        let engine_input = EngineInput {
            accel: 0.0,
            target_ang: 0.0,
        };

        let transform = Transform::from_xyz(0.0, 0.0, 0.0);
        let linear_velocity = LinearVelocity(Vec2::ZERO);

        (engines, engine_input, transform, linear_velocity)
    }

    #[test]
    fn test_linear_acceleration() {
        let (engines, mut input, mut transform, mut linear_vel) =
            setup_test_entity();

        // Set acceleration to full forward
        input.accel = 1.0;

        apply_engine_inputs_inner((
            Entity::from_raw(0),
            &input,
            &engines,
            &mut transform,
            &mut linear_vel,
        ));

        dbg!(transform.translation.xy());
        dbg!(transform.rotation.to_euler(EulerRot::ZYX).0);
        dbg!(&linear_vel);

        // Velocity should be in facing direction with magnitude of max_accel
        assert!(
            (linear_vel.0.length() - engines.max_accel.min(input.accel)).abs()
                < 0.01,
            "Should apply full acceleration in facing direction"
        );

        let (engines, mut input, mut transform, mut linear_vel) =
            setup_test_entity();
        // Set acceleration to full forward
        input.accel = 100.0;

        apply_engine_inputs_inner((
            Entity::from_raw(0),
            &input,
            &engines,
            &mut transform,
            &mut linear_vel,
        ));

        dbg!(transform.translation.xy());
        dbg!(transform.rotation.to_euler(EulerRot::ZYX).0);
        dbg!(&linear_vel);

        // Velocity should be in facing direction with magnitude of max_accel
        assert!(
            (linear_vel.0.length() - engines.max_accel.min(input.accel)).abs()
                < 0.01,
            "Should apply full acceleration in facing direction"
        );
    }

    #[test]
    fn test_rotation_towards_target() {
        let (engines, mut input, mut transform, mut linear_vel) =
            setup_test_entity();

        // Set initial conditions
        transform.rotation = Quat::from_rotation_z(0.0); // Facing right (0 degrees)
        input.target_ang = PI / 2.0; // Target is 90 degrees

        // Step simulation multiple times
        for _ in 0..10 {
            apply_engine_inputs_inner((
                Entity::from_raw(0),
                &input,
                &engines,
                &mut transform,
                &mut linear_vel,
            ));
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
        let (engines, mut input, mut transform, mut linear_vel) =
            setup_test_entity();

        // Set initial conditions
        transform.rotation = Quat::from_rotation_z(0.0);
        input.target_ang = PI / 4.0; // Target is 45 degrees

        // Step simulation multiple times
        for _ in 0..10 {
            apply_engine_inputs_inner((
                Entity::from_raw(0),
                &input,
                &engines,
                &mut transform,
                &mut linear_vel,
            ));
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
        let (engines, mut input, mut transform, mut linear_vel) =
            setup_test_entity();

        // Set initial conditions - already moving away from target
        transform.rotation = Quat::from_rotation_z(0.0);
        input.target_ang = PI / 4.0; // Target is 45 degrees

        // Step simulation multiple times
        for _ in 0..20 {
            apply_engine_inputs_inner((
                Entity::from_raw(0),
                &input,
                &engines,
                &mut transform,
                &mut linear_vel,
            ));
        }

        // Should correct course and reach target
        let final_angle = transform.rotation.to_euler(EulerRot::ZYX).0;
        assert!(
            (final_angle - PI / 4.0).abs() < 0.01,
            "Should correct overshooting and reach target"
        );
    }
}
