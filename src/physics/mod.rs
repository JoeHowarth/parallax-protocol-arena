//! Physics simulation module for predictive spacecraft movement in a 2D space
//! environment
//!
//! This module implements a specialized physics system that enables both
//! real-time simulation and future state prediction. The system is designed
//! around these key goals:
//!
//! - Deterministic simulation for reliable trajectory prediction
//! - Timeline-based control inputs for planning complex maneuvers
//! - Efficient state computation and visualization of future paths
//! - Realistic collision detection and resolution between spacecraft and
//!   obstacles
//!
//! # Core Components
//!
//! - `PhysicsState`: Represents the physical properties and state of an entity
//!   at a point in time
//! - `Timeline`: Manages scheduled control inputs and computed future states
//!   for an entity
//! - `SimulationConfig`: Controls global simulation parameters and time flow
//!
//! # How It Works
//!
//! 1. Control inputs (thrust, rotation changes, etc.) are scheduled at specific
//!    simulation ticks
//! 2. The system computes future states by integrating physics from the current
//!    state
//! 3. When new inputs are added, affected future states are invalidated and
//!    recomputed
//! 4. Entity transforms are synchronized with the current simulation tick
//!
//! # Coordinate System
//!
//! - Origin (0,0) is at the center of the world
//! - +X extends right, +Y extends up
//! - Rotations are in radians, clockwise from the +X axis
//! - Distances are in meters
//! - Time is measured in simulation ticks (default: 60 ticks/second)
//!
//! # Physics Model
//!
//! The simulation uses a simplified 2D physics model with these properties:
//! - No gravity or orbital mechanics
//! - Constant mass (no fuel consumption)
//! - Instant thrust response
//! - Perfect rigid body collisions
//!
//! # Limitations
//!
//! - No continuous collision detection (may miss collisions at high velocities)
//! - Limited accuracy from simple Euler integration
//! - No support for non-rigid body deformation

pub mod collisions;
#[cfg(test)]
mod test_utils;
pub mod timeline;

use std::{
    ops::{RangeBounds, RangeInclusive},
    time::Duration,
};

use bevy::{ecs::schedule::ScheduleLabel, utils::warn};
use collisions::{
    calculate_collision_result,
    calculate_impact_energy,
    viz_colliders,
    Collider,
    Collision,
    EntityCollisionResult,
    SpatialIndex,
    SpatialItem,
};
use timeline::compute_future_states;
pub use timeline::Timeline;

use crate::prelude::*;

#[derive(Bundle)]
pub struct PhysicsBundle {
    pub state: PhysicsState,
    pub timeline: Timeline,
    pub collider: Collider,
}

impl PhysicsBundle {
    pub fn from_state(
        tick: u64,
        state: PhysicsState,
        dim: Vec2,
    ) -> PhysicsBundle {
        let collider = Collider(BRect::from_corners(-dim / 2., dim / 2.));
        let mut timeline = Timeline::default();
        timeline.future_states.insert(tick, state.clone());
        timeline.last_computed_tick = tick;
        PhysicsBundle {
            state,
            timeline,
            collider,
        }
    }

    pub fn new_with_events(
        state: PhysicsState,
        dim: Vec2,
        state_tick: u64,
        events: impl IntoIterator<Item = (u64, ControlInput)>,
    ) -> PhysicsBundle {
        let mut bundle = PhysicsBundle::from_state(state_tick, state, dim);
        bundle.timeline.input_events.extend(events);
        bundle
    }

    pub fn new_basic(
        tick: u64,
        pos: Vec2,
        vel: Vec2,
        rotation: f32,
        max_thrust: f32,
        mass: f32,
        dim: Vec2,
    ) -> PhysicsBundle {
        PhysicsBundle::from_state(
            tick,
            PhysicsState {
                pos,
                vel,
                rotation,
                ang_vel: 0.,
                mass,
                current_thrust: 0.,
                max_thrust,
                alive: true,
            },
            dim,
        )
    }
}

/// Represents the complete physical state of a simulated entity at a point in
/// time
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Transform, Timeline)]
pub struct PhysicsState {
    /// Position in world space (meters)
    /// Origin at center, +X right, +Y up
    pub pos: Vec2,

    /// Velocity vector (meters/second)
    pub vel: Vec2,

    /// Orientation angle in radians
    /// 0 = facing +X axis, increases clockwise
    pub rotation: f32,

    /// Angular velocity in radians/second
    /// Positive = clockwise rotation
    pub ang_vel: f32,

    /// Mass of entity in kilograms
    /// Used for collision momentum calculations
    pub mass: f32,

    /// Current thrust level normalized to [-1.0, 1.0]
    /// Negative = reverse thrust
    pub current_thrust: f32,

    /// Maximum thrust force in Newtons
    /// Actual thrust force = current_thrust * max_thrust
    pub max_thrust: f32,

    /// Whether entity still exists or has been destroyed
    /// False indicates entity should be despawned
    pub alive: bool,
}

#[derive(Event, Debug, Reflect)]
pub struct TimelineEventRequest {
    /// Entity to apply to
    pub entity: Entity,
    /// Simulation tick when this input takes effect
    pub tick: u64,
    /// The control input to apply
    pub input: ControlInput,
}

#[derive(Event, Debug, Reflect)]
pub struct TimelineEventRemovalRequest {
    /// Entity to apply to
    pub entity: Entity,
    /// Simulation tick when this input takes effect
    pub tick: u64,
    /// The control input to remove
    pub input: ControlInput,
}

/// Control inputs that can be scheduled to modify entity behavior at specific
/// ticks
///
/// These inputs represent discrete changes to an entity's movement parameters.
/// They can be scheduled in advance to create complex movement patterns.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum ControlInput {
    /// Set thrust level between -1.0 (full reverse) and 1.0 (full forward)
    SetThrust(f32),

    /// Set absolute rotation in radians (0 = facing +X axis)
    SetRotation(f32),

    /// Set angular velocity in radians/second (positive = clockwise)
    SetAngVel(f32),

    /// Simultaneously set thrust (-1.0 to 1.0) and rotation (radians)
    /// Useful for immediate course corrections
    SetThrustAndRotation(f32, f32),
}

#[derive(Clone, Debug, PartialEq, Reflect)]
pub enum TimelineEvent {
    Control(ControlInput),
    Collision(Collision),
}

/// Global simulation parameters and time control
#[derive(Resource, Clone, Debug)]
pub struct SimulationConfig {
    /// Current simulation tick
    pub current_tick: u64,
    /// How many simulation ticks per virtual second
    pub ticks_per_second: u64,
    /// How many virtual seconds should pass per real second
    pub time_dilation: f32,
    /// Whether simulation is paused
    pub paused: bool,
    /// How many ticks in the future to predict
    pub prediction_ticks: u64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            current_tick: 0,
            ticks_per_second: 60,
            time_dilation: 1.0,
            paused: false,
            prediction_ticks: 120,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum PhysicsSchedule {
    #[default]
    FixedUpdate,
    Update,
}

/// Plugin that sets up the physics simulation systems
#[derive(Clone, Debug, Default)]
pub struct PhysicsSimulationPlugin {
    pub schedule: PhysicsSchedule,
    pub should_keep_alive: bool,
}

impl Plugin for PhysicsSimulationPlugin {
    fn build(&self, app: &mut App) {
        let should_keep_alive = self.should_keep_alive;
        app.add_event::<TimelineEventRequest>()
            .add_event::<TimelineEventRemovalRequest>()
            .insert_resource(SpatialIndex::default())
            .add_systems(Update, (viz_colliders, process_timeline_events));

        let systems = (
            update_simulation_time,
            compute_future_states,
            sync_physics_state_transform,
            despawn_not_alive.run_if(move || !should_keep_alive),
        );
        let systems = systems.chain();

        match self.schedule {
            PhysicsSchedule::FixedUpdate => {
                app.add_systems(FixedUpdate, systems);
            }
            PhysicsSchedule::Update => {
                app.add_systems(Update, systems);
            }
        }
    }
}

fn despawn_not_alive(
    mut commands: Commands,
    states: Query<(Entity, &PhysicsState)>,
) {
    for (entity, state) in states.iter() {
        if !state.alive {
            info!(?entity, "Despawning dead entity");
            commands.entity(entity).despawn();
        }
    }
}

// Increment current_tick when not paused
fn update_simulation_time(mut sim_time: ResMut<SimulationConfig>) {
    if !sim_time.paused {
        sim_time.current_tick += 1;
        info!(tick = sim_time.current_tick, "Updated tick");
    }
}

// When receiving events:
//   1. Update Timeline events
//   2. Set last_computed_tick to invalidate future states
fn process_timeline_events(
    mut timeline_events: EventReader<TimelineEventRequest>,
    mut timeline_removals: EventReader<TimelineEventRemovalRequest>,
    mut timelines: Query<&mut Timeline>,
) {
    for TimelineEventRequest {
        tick,
        input,
        entity,
    } in timeline_events.read()
    {
        info!(?tick, ?input, ?entity, "Got timeline event request");
        let Ok(mut timeline) = timelines.get_mut(*entity) else {
            warn!("Timeline component missing for given request");
            continue;
        };

        timeline.add_input_event(*tick, *input);
    }

    for TimelineEventRemovalRequest {
        tick,
        input,
        entity,
    } in timeline_removals.read()
    {
        info!(?tick, ?input, ?entity, "Got timeline removal request");
        let Ok(mut timeline) = timelines.get_mut(*entity) else {
            warn!("Timeline component missing for given removal");
            continue;
        };

        timeline.remove_input_event(*tick, *input);
    }
}

impl PhysicsState {
    fn integrate(&self, delta_seconds: f32) -> Self {
        if !self.alive {
            return PhysicsState::default();
        }
        // TODO: replace this with rk4 integration method to reduce errors
        let thrust_direction = Vec2::from_angle(self.rotation);
        let thrust_force =
            thrust_direction * (self.current_thrust * self.max_thrust);
        let acceleration = thrust_force / self.mass;

        PhysicsState {
            pos: self.pos + self.vel * delta_seconds,
            vel: self.vel + acceleration * delta_seconds,
            rotation: self.rotation + self.ang_vel * delta_seconds,
            // Assuming no torque for now
            ang_vel: self.ang_vel,
            mass: self.mass,
            current_thrust: self.current_thrust,
            max_thrust: self.max_thrust,
            alive: self.alive,
        }
    }

    fn apply_input_event(&mut self, event: Option<&ControlInput>) {
        use TimelineEvent::{Collision, Control};
        let Some(event) = event else {
            return;
        };
        match event {
            ControlInput::SetThrust(thrust) => {
                self.current_thrust = *thrust;
            }
            ControlInput::SetRotation(rotation) => {
                self.rotation = *rotation;
                self.ang_vel = 0.;
            }
            ControlInput::SetThrustAndRotation(thrust, rotation) => {
                self.current_thrust = *thrust;
                self.rotation = *rotation;
                self.ang_vel = 0.;
            }
            ControlInput::SetAngVel(ang_vel) => {
                self.ang_vel = *ang_vel;
            }
        }
    }

    fn apply_collision_result(&mut self, result: &EntityCollisionResult) {
        match result {
            EntityCollisionResult::Destroyed => {
                self.alive = false;
            }
            EntityCollisionResult::Survives { post_pos, post_vel } => {
                self.pos = *post_pos;
                self.vel = *post_vel;
            }
        }
    }

    pub fn dir(&self) -> Vec2 {
        Vec2::from_angle(self.rotation)
    }

    pub fn quat(&self) -> Quat {
        Quat::from_rotation_z(self.rotation)
    }
}

/// Update tranform and physics state from timeline
fn sync_physics_state_transform(
    mut query: Query<(&mut Transform, &mut PhysicsState, &mut Timeline)>,
    sim_state: Res<SimulationConfig>,
) {
    for (mut transform, mut phys_state, mut timeline) in query.iter_mut() {
        *phys_state = timeline
            .future_states
            .get(&sim_state.current_tick)
            .expect("current tick not included in timeline")
            .clone();

        transform.translation = Vec3::from2(phys_state.pos);
        transform.rotation = Quat::from_rotation_z(phys_state.rotation);

        if let Some(to_remove) = sim_state.current_tick.checked_sub(2) {
            timeline.future_states.remove(&to_remove);
            timeline.input_events.retain(|k, _v| *k > to_remove + 1);
            timeline.sim_events.retain(|k, _v| *k > to_remove + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{f32::consts::PI, time::Duration};

    use assertables::assert_approx_eq;
    use bevy::{app::App, time::Time};

    use super::{test_utils::*, *};

    fn create_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(
            crate::ParallaxProtocolArenaPlugin {
                config: default(),
                physics: PhysicsSimulationPlugin {
                    schedule: PhysicsSchedule::Update,
                    should_keep_alive: false,
                },
                client: None,
            },
        );
        app
    }

    fn create_test_physics_state() -> PhysicsState {
        PhysicsState {
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            rotation: 0.0,
            ang_vel: 0.0,
            mass: 1.0,
            current_thrust: 0.0,
            max_thrust: 100.0,
            alive: true,
        }
    }

    #[test]
    fn test_physics_state_integration() {
        let delta = 1.0 / 60.0; // Standard 60 FPS timestep

        // Case 1: No forces, only existing velocity
        let state = PhysicsState {
            pos: Vec2::new(10.0, 5.0),
            vel: Vec2::new(2.0, 1.0),
            rotation: 0.0,
            ang_vel: 0.5,
            mass: 1.0,
            current_thrust: 0.0,
            max_thrust: 100.0,
            alive: true,
        };

        let next_state = state.integrate(delta);

        // Position should change based on existing velocity
        assert_approx_eq!(next_state.pos.x, 10.0 + 2.0 * delta);
        assert_approx_eq!(next_state.pos.y, 5.0 + 1.0 * delta);
        // Velocity should remain unchanged (no forces)
        assert_approx_eq!(next_state.vel.x, 2.0);
        assert_approx_eq!(next_state.vel.y, 1.0);
        // Rotation should change based on angular velocity
        assert_approx_eq!(next_state.rotation, 0.0 + 0.5 * delta);

        // Case 2: Full thrust to the right (rotation = 0)
        let state = PhysicsState {
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            rotation: 0.0,
            ang_vel: 0.0,
            mass: 2.0,           // 2kg mass
            current_thrust: 1.0, // Full thrust
            max_thrust: 100.0,   // 100N max thrust
            alive: true,
        };

        let next_state = state.integrate(delta);

        // Calculate expected values:
        // Force = 100N right
        // Acceleration = 100N / 2kg = 50 m/s²
        // Δv = 50 m/s² * (1/60) s = 0.8333... m/s
        // Position shouldn't change yet since initial velocity was zero
        assert_approx_eq!(next_state.vel.x, 50.0 * delta);
        assert_approx_eq!(next_state.vel.y, 0.0);
        assert_approx_eq!(next_state.pos.x, 0.0); // Fixed: position doesn't change first frame
        assert_approx_eq!(next_state.pos.y, 0.0);

        // Case 3: Full thrust at 45 degrees
        let state = PhysicsState {
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            rotation: PI / 4.0, // 45 degrees
            ang_vel: 0.0,
            mass: 2.0,
            current_thrust: 1.0,
            max_thrust: 100.0,
            alive: true,
        };

        let next_state = state.integrate(delta);

        // At 45 degrees, force is split equally between x and y
        // Each component should be 100N * √2/2 = 70.71... N
        // Acceleration per component = 35.355... m/s²
        let expected_accel = 50.0 / 2.0_f32.sqrt();
        assert_approx_eq!(next_state.vel.x, expected_accel * delta);
        assert_approx_eq!(next_state.vel.y, expected_accel * delta);
        assert_approx_eq!(next_state.pos.x, 0.0); // Fixed: position doesn't change first frame
        assert_approx_eq!(next_state.pos.y, 0.0);

        // Let's verify position changek after a second integration step
        let third_state = next_state.integrate(delta);
        assert_approx_eq!(
            third_state.pos.x,
            (expected_accel * delta) * delta, /* Using velocity from
                                               * previous state */
        );
        assert_approx_eq!(third_state.pos.y, (expected_accel * delta) * delta,);
    }

    #[test]
    fn test_compute_future_states_system() {
        let mut app = create_test_app();

        // Spawn an entity with physics components

        let state = create_test_physics_state();
        let entity = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(
                state,
                Vec2::splat(2.),
                0,
                [(30, ControlInput::SetThrust(1.0))],
            ))
            .id();

        // Run the system once
        app.update();

        // Get the resulting timeline component
        let timeline = app
            .world()
            .entity(entity)
            .get::<Timeline>()
            .expect("Timeline component should exist");

        // Verify states were computed
        assert!(!timeline.future_states.is_empty());
        assert_eq!(
            timeline.last_computed_tick,
            1 + app.world().resource::<SimulationConfig>().prediction_ticks
        );

        // Check that thrust was applied at the correct tick
        let state_before = timeline
            .future_states
            .get(&29)
            .expect("Should have state before thrust application");
        let state_after = timeline
            .future_states
            .get(&31)
            .expect("Should have state after thrust application");
        assert!(state_after.vel.length() > state_before.vel.length());
    }

    #[test]
    fn test_rotation_affects_thrust_direction() {
        let mut state = create_test_physics_state();
        state.current_thrust = 1.0;
        state.rotation = std::f32::consts::FRAC_PI_2; // 90 degrees, thrust up

        let next_state = state.integrate(1.0 / 60.0);
        assert!(next_state.vel.x.abs() < f32::EPSILON);
        assert!(next_state.vel.y > 0.0);
    }
}
