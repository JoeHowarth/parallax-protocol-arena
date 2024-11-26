use std::f32::consts::PI;

use bevy::color::palettes::css;
use bevy_mod_scripting::prelude::{mlua, FromLua};

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
        app.register_type::<Engines>()
            .register_type::<EngineInput>()
            .add_lua_provider(EnginesPlugin)
            .add_systems(FixedPostUpdate, apply_engine_inputs);
    }
}

impl LuaProvider for EnginesPlugin {
    fn attach_lua_api(&mut self, lua: &mut Lua) -> mlua::Result<()> {
        Ok(())
    }

    fn setup_lua_script(
        &mut self,
        sd: &ScriptData,
        lua: &mut Lua,
    ) -> mlua::Result<()> {
        let craft_entity = sd.entity;
        let table = lua.create_table()?;
        table.set(
            "engine_info",
            lua.create_function(move |lua, _: Value| {
                let world = lua.get_world()?;
                let world = world.read();
                let entity_ref = world.get_entity(craft_entity).ok_or(
                    LuaError::RuntimeError(
                        "Failed to get entity from world".into(),
                    ),
                )?;
                let engines = entity_ref.get::<Engines>().ok_or(
                    LuaError::RuntimeError(
                        "Entity does not have engines".into(),
                    ),
                )?;

                let table = lua.create_table_with_capacity(0, 2)?;
                table.set("max_accel", engines.max_accel)?;
                table.set("max_rot", engines.max_rot)?;

                Ok(table)
            })?,
        )?;
        table.set(
            "set_engine_input",
            lua.create_function(
                move |lua, (accel, target_ang): (f32, f32)| {
                    let world = lua.get_world()?;
                    let mut world = world.write();
                    world
                        .get_entity_mut(craft_entity)
                        .ok_or(LuaError::RuntimeError(
                            "Failed to get entity from world".into(),
                        ))?
                        .insert(EngineInput { accel, target_ang });

                    Ok(())
                },
            )?,
        )?;
        lua.globals().set("engines", table)?;
        // let globals = lua.globals();
        // for p in globals.pairs::<Value, Value>() {
        //     dbg!(p?);
        // }
        Ok(())
    }
}

fn apply_engine_inputs(
    mut query: Query<(
        Entity,
        &EngineInput,
        &Engines,
        &mut Transform,
        &mut LinearVelocity,
        &mut AngularVelocity,
    )>,
    mut painter: ShapePainter,
) {
    for inputs in query.iter_mut() {
        let (_entity, input, engines, mut transform, mut vel, mut ang_vel) =
            inputs;
        // dbg!(_entity);
        painter.set_translation(transform.translation);
        painter.set_color(css::PINK);
        painter.line(Vec3::ZERO, transform.local_y() * 50.);

        ang_vel.0 = 0.;
        apply_engine_inputs_inner((
            _entity,
            input,
            engines,
            &mut transform,
            &mut vel,
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
    let rot_to_apply = angle_diff.clamp(-engines.max_rot, engines.max_rot);
    // dbg!(current_angle, angle_diff, rot_to_apply);

    transform.rotate_z(rot_to_apply);

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
