//! Physics simulation module for predictive spacecraft movement
//!
//! This module implements a specialized 2D physics system focused on:
//! - Deterministic simulation for trajectory prediction
//! - Timeline-based control inputs
//! - Efficient state computation for visualization
//!
//! The core components are:
//! - `PhysicsState`: Current physical properties of an entity
//! - `Timeline`: Scheduled control inputs and computed future states
//! - `SimulationState`: Global simulation parameters and time control
//!
//! The simulation works by:
//! 1. Storing control inputs (thrust, rotation, etc) with their activation
//!    ticks
//! 2. Computing future physics states by integrating from the current state
//! 3. Invalidating and recomputing states when new inputs are added
//! 4. Synchronizing entity transforms with the current simulation tick

use std::collections::{BTreeMap, VecDeque};

use crate::prelude::*;

/// Physical properties and control state of a simulated entity
#[derive(Component, Clone, Debug)]
pub struct PhysicsState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,
    pub angular_velocity: f32,
    pub mass: f32,
    // start with this for basic thrust, but can move them out later
    pub current_thrust: f32, // -1.0 to 1.0
    pub max_thrust: f32,     // newtons
}

#[derive(Event, Debug)]
pub struct TimelineEventRequest {
    pub entity: Entity,
    pub event: TimelineEvent,
}

/// Control inputs that can be scheduled to modify entity behavior
#[derive(Clone, Copy, Debug)]
pub enum ControlInput {
    /// Set thrust level between -1.0 and 1.0
    SetThrust(f32),
    /// Set absolute rotation in radians
    SetRotation(f32),
    /// Set angular velocity in radians/second
    SetAngVel(f32),
}

/// A scheduled control input at a specific simulation tick
#[derive(Clone, Debug)]
pub struct TimelineEvent {
    /// Simulation tick when this input takes effect
    pub tick: u64,
    /// The control input to apply
    pub input: ControlInput,
}

/// Stores scheduled inputs and computed future states for an entity
#[derive(Component)]
pub struct Timeline {
    /// Ordered list of future control inputs
    /// Computed physics states for future simulation ticks
    pub future_states: BTreeMap<u64, PhysicsState>,
    /// Last tick that has valid computed states
    pub events: Vec<TimelineEvent>,
    pub last_computed_tick: u64,
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
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            current_tick: 0,
            ticks_per_second: 60,
            time_dilation: 1.0,
            paused: false,
        }
    }
}

/// Plugin that sets up the physics simulation systems
#[derive(Clone, Default, Debug)]
pub struct PhysicsSimulationPlugin {
    pub config: SimulationConfig,
}

impl Plugin for PhysicsSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<TimelineEventRequest>()
            .insert_resource(self.config.clone())
            .insert_resource(Time::<Fixed>::from_hz(
                self.config.ticks_per_second as f64
                    * self.config.time_dilation as f64,
            ))
            .add_systems(
                FixedUpdate,
                (
                    update_simulation_time,
                    process_timeline_events,
                    compute_future_states,
                    sync_physics_state_transform,
                )
                    .chain(),
            )
            .add_systems(Update, time_dilation_control);
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
    mut timelines: Query<&mut Timeline>,
) {
    for TimelineEventRequest { event, entity } in timeline_events.read() {
        info!(?event, ?entity, "Got timeline event request");
        let Ok(mut timeline) = timelines.get_mut(*entity) else {
            warn!("Timeline component for given request");
            continue;
        };
        // just insert with binary search in the future
        timeline.events.push(event.clone());
        timeline.events.sort_by_key(|e| e.tick);
        timeline.last_computed_tick =
            timeline.last_computed_tick.min(event.tick);
    }
}

impl PhysicsState {
    fn integrate(&self, delta_seconds: f32) -> Self {
        let thrust_direction = Vec2::from_angle(self.rotation);
        let thrust_force =
            thrust_direction * (self.current_thrust * self.max_thrust);
        let acceleration = thrust_force / self.mass;

        PhysicsState {
            position: self.position + self.velocity * delta_seconds,
            velocity: self.velocity + acceleration * delta_seconds,
            rotation: self.rotation + self.angular_velocity * delta_seconds,
            // Assuming no torque for now
            angular_velocity: self.angular_velocity,
            mass: self.mass,
            current_thrust: self.current_thrust,
            max_thrust: self.max_thrust,
        }
    }
}

fn compute_future_states(
    simulation_config: Res<SimulationConfig>,
    mut query: Query<(&PhysicsState, &mut Timeline)>,
) {
    let seconds_per_tick = 1.0 / simulation_config.ticks_per_second as f32;

    for (current_state, mut timeline) in query.iter_mut() {
        // If everything is up to date, skip
        if timeline.last_computed_tick
            >= simulation_config.current_tick + PREDICTION_TICKS
        {
            info!("Nothing to compute...");
            continue;
        }

        // Start computation from the earliest invalid state
        let start_tick = timeline
            .last_computed_tick
            .min(simulation_config.current_tick);
        let mut state = if start_tick == simulation_config.current_tick {
            current_state.clone()
        } else {
            timeline
                .future_states
                .get(&start_tick)
                .cloned()
                .unwrap_or_else(|| current_state.clone())
        };
        // info!(start_tick, ?state, "Computing future states");

        // Compute future states up to PREDICTION_TICKS into the future
        for tick in
            start_tick..=simulation_config.current_tick + PREDICTION_TICKS
        {
            // Apply any control inputs that occur at this tick
            if let Some(event) =
                timeline.events.iter().find(|event| event.tick == tick)
            {
                // info!(?event, "Found event");
                match event.input {
                    ControlInput::SetThrust(thrust) => {
                        state.current_thrust = thrust;
                    }
                    ControlInput::SetRotation(rotation) => {
                        state.rotation = rotation;
                        state.angular_velocity = 0.;
                    }
                    ControlInput::SetAngVel(ang_vel) => {
                        state.angular_velocity = ang_vel;
                    }
                }
            }

            // Integrate physics with time scale
            state = state.integrate(seconds_per_tick);

            // Store the new state
            timeline.future_states.insert(tick, state.clone());
        }

        let mut future_pos = timeline
            .future_states
            .iter()
            .map(|(t, s)| (t, s.position))
            .collect::<Vec<_>>();
        future_pos.sort_by_key(|x| x.0);
        let future_pos = &future_pos[0..5];
        // info!(?future_pos, "future positions");
        // dbg!(future_pos);

        timeline.last_computed_tick =
            simulation_config.current_tick + PREDICTION_TICKS;
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

        transform.translation = Vec3::from2(phys_state.position);
        transform.rotation = Quat::from_rotation_z(phys_state.rotation);
        timeline.future_states.remove(&(sim_state.current_tick - 1));
    }
}

// Constants
const PREDICTION_TICKS: u64 = 120; // 2 seconds at 60 ticks/second

fn time_dilation_control(
    keys: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<SimulationConfig>,
    mut time: ResMut<Time<Fixed>>,
) {
    let mut changed = false;

    if keys.just_pressed(KeyCode::BracketRight) {
        config.time_dilation *= 2.0;
        changed = true;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        config.time_dilation *= 0.5;
        changed = true;
    }

    if changed {
        time.set_timestep_hz(
            config.ticks_per_second as f64 * config.time_dilation as f64,
        );
        info!(
            "Simulation speed: {:.1}x ({}Hz)",
            config.time_dilation,
            config.ticks_per_second as f64 * config.time_dilation as f64
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{f32::consts::PI, time::Duration};

    use assert_approx_eq::assert_approx_eq;
    use bevy::{app::App, time::Time};

    use super::*;

    fn create_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(SimulationConfig::default())
            .add_systems(Update, compute_future_states);
        app
    }

    fn create_test_physics_state() -> PhysicsState {
        PhysicsState {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            rotation: 0.0,
            angular_velocity: 0.0,
            mass: 1.0,
            current_thrust: 0.0,
            max_thrust: 100.0,
        }
    }

    #[test]
    fn test_physics_state_integration() {
        let delta = 1.0 / 60.0; // Standard 60 FPS timestep

        // Case 1: No forces, only existing velocity
        let state = PhysicsState {
            position: Vec2::new(10.0, 5.0),
            velocity: Vec2::new(2.0, 1.0),
            rotation: 0.0,
            angular_velocity: 0.5,
            mass: 1.0,
            current_thrust: 0.0,
            max_thrust: 100.0,
        };

        let next_state = state.integrate(delta);

        // Position should change based on existing velocity
        assert_approx_eq!(next_state.position.x, 10.0 + 2.0 * delta, 1e-6);
        assert_approx_eq!(next_state.position.y, 5.0 + 1.0 * delta, 1e-6);
        // Velocity should remain unchanged (no forces)
        assert_approx_eq!(next_state.velocity.x, 2.0, 1e-6);
        assert_approx_eq!(next_state.velocity.y, 1.0, 1e-6);
        // Rotation should change based on angular velocity
        assert_approx_eq!(next_state.rotation, 0.0 + 0.5 * delta, 1e-6);

        // Case 2: Full thrust to the right (rotation = 0)
        let state = PhysicsState {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            rotation: 0.0,
            angular_velocity: 0.0,
            mass: 2.0,           // 2kg mass
            current_thrust: 1.0, // Full thrust
            max_thrust: 100.0,   // 100N max thrust
        };

        let next_state = state.integrate(delta);

        // Calculate expected values:
        // Force = 100N right
        // Acceleration = 100N / 2kg = 50 m/s²
        // Δv = 50 m/s² * (1/60) s = 0.8333... m/s
        // Position shouldn't change yet since initial velocity was zero
        assert_approx_eq!(next_state.velocity.x, 50.0 * delta, 1e-6);
        assert_approx_eq!(next_state.velocity.y, 0.0, 1e-6);
        assert_approx_eq!(next_state.position.x, 0.0, 1e-6); // Fixed: position doesn't change first frame
        assert_approx_eq!(next_state.position.y, 0.0, 1e-6);

        // Case 3: Full thrust at 45 degrees
        let state = PhysicsState {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            rotation: PI / 4.0, // 45 degrees
            angular_velocity: 0.0,
            mass: 2.0,
            current_thrust: 1.0,
            max_thrust: 100.0,
        };

        let next_state = state.integrate(delta);

        // At 45 degrees, force is split equally between x and y
        // Each component should be 100N * √2/2 = 70.71... N
        // Acceleration per component = 35.355... m/s²
        let expected_accel = 50.0 / 2.0_f32.sqrt();
        assert_approx_eq!(next_state.velocity.x, expected_accel * delta, 1e-6);
        assert_approx_eq!(next_state.velocity.y, expected_accel * delta, 1e-6);
        assert_approx_eq!(next_state.position.x, 0.0, 1e-6); // Fixed: position doesn't change first frame
        assert_approx_eq!(next_state.position.y, 0.0, 1e-6);

        // Let's verify position changek after a second integration step
        let third_state = next_state.integrate(delta);
        assert_approx_eq!(
            third_state.position.x,
            (expected_accel * delta) * delta, /* Using velocity from
                                               * previous state */
            1e-6
        );
        assert_approx_eq!(
            third_state.position.y,
            (expected_accel * delta) * delta,
            1e-6
        );
    }

    #[test]
    fn test_compute_future_states_system() {
        let mut app = create_test_app();

        // Spawn an entity with physics components
        let entity = app
            .world_mut()
            .spawn((
                create_test_physics_state(),
                Timeline {
                    events: vec![TimelineEvent {
                        tick: 30,
                        input: ControlInput::SetThrust(1.0),
                    }],
                    future_states: BTreeMap::new(),
                    last_computed_tick: 0,
                },
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
        assert_eq!(timeline.last_computed_tick, PREDICTION_TICKS);

        // Check that thrust was applied at the correct tick
        let state_before = timeline
            .future_states
            .get(&29)
            .expect("Should have state before thrust application");
        let state_after = timeline
            .future_states
            .get(&31)
            .expect("Should have state after thrust application");
        assert!(state_after.velocity.length() > state_before.velocity.length());
    }

    #[test]
    fn test_rotation_affects_thrust_direction() {
        let mut state = create_test_physics_state();
        state.current_thrust = 1.0;
        state.rotation = std::f32::consts::FRAC_PI_2; // 90 degrees, thrust up

        let next_state = state.integrate(1.0 / 60.0);
        assert!(next_state.velocity.x.abs() < f32::EPSILON);
        assert!(next_state.velocity.y > 0.0);
    }

    #[test]
    fn test_timeline_event_processing() {
        let mut app = create_test_app();

        // Set up entity with multiple control inputs
        let entity = app
            .world_mut()
            .spawn((
                create_test_physics_state(),
                Timeline {
                    events: vec![
                        TimelineEvent {
                            tick: 10,
                            input: ControlInput::SetThrust(1.0),
                        },
                        TimelineEvent {
                            tick: 20,
                            input: ControlInput::SetRotation(
                                std::f32::consts::FRAC_PI_2,
                            ),
                        },
                    ],
                    future_states: BTreeMap::new(),
                    last_computed_tick: 0,
                },
            ))
            .id();

        app.update();

        let timeline = app
            .world()
            .entity(entity)
            .get::<Timeline>()
            .expect("Timeline component should exist");

        // Check state at tick 15 (after thrust, before rotation)
        let mid_state = timeline
            .future_states
            .get(&15)
            .expect("Should have state after thrust application");
        assert!(mid_state.velocity.x > 0.0);

        // Check state at tick 25 (after both events)
        let final_state = timeline
            .future_states
            .get(&25)
            .expect("Should have state after rotation");
        assert_eq!(final_state.rotation, std::f32::consts::FRAC_PI_2);
    }
}
