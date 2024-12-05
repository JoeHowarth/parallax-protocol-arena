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

use bevy::utils::warn;
use collisions::{
    calculate_impact_energy,
    calculate_inelastic_collision,
    Collider,
    Collision,
    EntityCollisionResult,
    SpatialIndex,
    SpatialItem,
};

use crate::prelude::*;

pub mod collisions;

/// Physical properties and control state of a simulated entity
#[derive(Component, Clone, Debug, Default)]
#[require(Transform, Timeline)]
pub struct PhysicsState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,
    pub angular_velocity: f32,
    pub mass: f32,
    // start with this for basic thrust, but can move them out later
    pub current_thrust: f32, // -1.0 to 1.0
    pub max_thrust: f32,     // newtons
    // TODO: come up with a better way to model destruction
    pub alive: bool,
}

#[derive(Event, Debug)]
pub struct TimelineEventRequest {
    /// Entity to apply to
    pub entity: Entity,
    /// Simulation tick when this input takes effect
    pub tick: u64,
    /// The control input to apply
    pub input: ControlInput,
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
    SetThrustAndRotation(f32, f32),
}

#[derive(Clone, Debug)]
pub enum PhysicsEvent {
    Collision(Collision),
}

#[derive(Clone, Debug)]
pub enum TimelineEvent {
    Control(ControlInput),
    Physics(PhysicsEvent),
}

/// Stores scheduled inputs and computed future states for an entity
#[derive(Component, Default)]
pub struct Timeline {
    /// Computed physics states for future simulation ticks
    pub future_states: BTreeMap<u64, PhysicsState>,
    /// Ordered list of future control inputs
    pub events: BTreeMap<u64, TimelineEvent>,
    /// Last tick that has valid computed states
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
            current_tick: 1,
            ticks_per_second: 60,
            time_dilation: 1.0,
            paused: false,
        }
    }
}

#[derive(Resource)]
pub struct TrajectoryPreview {
    pub entity: Entity,
    pub start_tick: u64,
    pub timeline: Timeline,
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
                    process_timeline_events,
                    compute_future_states,
                    sync_physics_state_transform,
                    update_simulation_time,
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
    for TimelineEventRequest {
        tick,
        input,
        entity,
    } in timeline_events.read()
    {
        info!(?tick, ?input, ?entity, "Got timeline event request");
        let Ok(mut timeline) = timelines.get_mut(*entity) else {
            warn!("Timeline component for given request");
            continue;
        };
        // just insert with binary search in the future
        let prev_last_computed_tick = timeline.last_computed_tick;
        timeline.last_computed_tick = timeline.last_computed_tick.min(*tick);
        timeline
            .events
            .insert(*tick, TimelineEvent::Control(*input));

        info!(
            prev_last_computed_tick,
            last_computed_tick = timeline.last_computed_tick,
            "processing timeline event"
        );
    }
}

impl PhysicsState {
    fn integrate(&self, delta_seconds: f32) -> Self {
        // TODO: replace this with rk4 integration method to reduce errors
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
            alive: self.alive,
        }
    }

    fn apply_collision(&mut self, this: Entity, collision: &Collision) {
        let result = if this == collision.this {
            &collision.this_result
        } else {
            &collision.other_result
        };
        match result {
            collisions::EntityCollisionResult::Destroyed => {
                // TODO: clean this up
                self.alive = false;
            }
            collisions::EntityCollisionResult::Survives {
                post_pos,
                post_vel,
            } => {
                self.position = *post_pos;
                self.velocity = *post_vel;
            }
        }
    }

    fn collision_result(
        &self,
        other_aabb: RRect,
        other: &SpatialItem,
    ) -> (EntityCollisionResult, EntityCollisionResult) {
        let (q, q_other) = calculate_impact_energy(
            self.mass,
            other.mass,
            other.vel - self.velocity,
        );
        let post_vel = calculate_inelastic_collision(
            self.mass,
            self.velocity,
            other.mass,
            other.vel,
        );
        if q > q_other {
            (
                EntityCollisionResult::Destroyed,
                EntityCollisionResult::Survives {
                    post_pos: other.pos,
                    post_vel,
                },
            )
        } else {
            (
                EntityCollisionResult::Survives {
                    post_pos: self.position,
                    post_vel,
                },
                EntityCollisionResult::Destroyed,
            )
        }
    }
}

fn compute_future_states(
    simulation_config: Res<SimulationConfig>,
    mut spatial_index: ResMut<SpatialIndex>,
    mut query: Query<(Entity, &Collider, &PhysicsState, &mut Timeline)>,
    mut invalidations: Local<EntityHashMap<Collision>>,
) {
    let seconds_per_tick = 1.0 / simulation_config.ticks_per_second as f32;
    let tick = simulation_config.current_tick;
    invalidations.clear();

    loop {
        for (e, collider, current_state, mut timeline) in query.iter_mut() {
            // ensure timline has value for current tick
            if !timeline.future_states.contains_key(&tick) {
                timeline.future_states.insert(tick, current_state.clone());
            }

            if let Some(collision) = invalidations.remove(&e) {
                // TODO: should this be tick - 1?
                timeline.last_computed_tick =
                    dbg!(dbg!(timeline.last_computed_tick).min(collision.tick));

                let ejected = timeline.events.insert(
                    collision.tick,
                    TimelineEvent::Physics(PhysicsEvent::Collision(collision)),
                );
                if let Some(ejected) = ejected {
                    warn!(
                        ?ejected,
                        ?e,
                        "we ejected a valid event when handling collision!"
                    );
                }
            }

            timeline.lookahead(
                e,
                tick,
                seconds_per_tick,
                &collider,
                &mut spatial_index,
                &mut invalidations,
            );
        }

        if invalidations.is_empty() {
            info!("No more invalidations, breaking...");
            break;
        }
    }
}

// fn compute_future_states(
//     simulation_config: Res<SimulationConfig>,
//     mut query: Query<(&PhysicsState, &mut Timeline)>,
// ) {
//     let seconds_per_tick = 1.0 / simulation_config.ticks_per_second as f32;
//     let tick = simulation_config.current_tick;
//
//     for (current_state, mut timeline) in query.iter_mut() {
//         // ensure timline has value for current tick
//         if !timeline.future_states.contains_key(&tick) {
//             timeline.future_states.insert(tick, current_state.clone());
//         }
//         timeline.lookahead(tick, seconds_per_tick);
//     }
// }

impl Timeline {
    pub fn lookahead(
        &mut self,
        e: Entity,
        current_tick: u64,
        seconds_per_tick: f32,
        collider: &Collider,
        spatial_index: &SpatialIndex,
        invalidations: &mut EntityHashMap<Collision>,
    ) {
        // Start computation from the earliest invalid state
        let start_tick = self.last_computed_tick.max(current_tick + 1);
        let mut end_tick = current_tick + PREDICTION_TICKS;

        let mut state =
            self.future_states.get(&(start_tick - 1)).unwrap().clone();

        for tick in start_tick..=end_tick {
            // Apply any control inputs that occur at this tick
            if let Some(event) = self.events.get(&tick) {
                info!(?event, tick, ?state, "Found input");

                use TimelineEvent::{Control, Physics};

                match event {
                    Control(control_event) => {
                        match control_event {
                            ControlInput::SetThrust(thrust) => {
                                state.current_thrust = *thrust;
                            }
                            ControlInput::SetRotation(rotation) => {
                                state.rotation = *rotation;
                                state.angular_velocity = 0.;
                            }
                            ControlInput::SetThrustAndRotation(
                                thrust,
                                rotation,
                            ) => {
                                state.current_thrust = *thrust;
                                state.rotation = *rotation;
                                state.angular_velocity = 0.;
                            }
                            ControlInput::SetAngVel(ang_vel) => {
                                state.angular_velocity = *ang_vel;
                            }
                        }

                        // Integrate physics with time scale
                        state = state.integrate(seconds_per_tick);
                    }
                    Physics(physics_event) => {
                        // Integrate state before applying physics event
                        // TODO: is this for all physics events or just
                        // collisions?
                        state = state.integrate(seconds_per_tick);

                        match physics_event {
                            PhysicsEvent::Collision(collision) => {
                                state.apply_collision(e, collision);
                            }
                        }
                    }
                }
            } else {
                // Integrate physics with time scale
                state = state.integrate(seconds_per_tick);
            }

            if let Some((other_aabb, other)) =
                spatial_index.collides(tick, state.position, collider)
            {
                let (this_result, other_result) =
                    state.collision_result(other_aabb, &other);
                let collision = Collision {
                    tick,
                    this: e,
                    this_result,
                    other: other.entity,
                    other_result,
                };
                state.apply_collision(e, &collision);
                invalidations.insert(other.entity, collision);
            }

            // Store the new state
            self.future_states.insert(tick, state.clone());
            if !state.alive {
                end_tick = tick;
                break;
            }
        }

        self.last_computed_tick = end_tick;
    }

    // pub fn lookahead(&mut self, current_tick: u64, seconds_per_tick: f32) {
    //     // Start computation from the earliest invalid state
    //     let start_tick = self.last_computed_tick.max(current_tick + 1);
    //     let end_tick = current_tick + PREDICTION_TICKS;
    //
    //     let mut state =
    //         self.future_states.get(&(start_tick - 1)).unwrap().clone();
    //
    //     for tick in start_tick..=end_tick {
    //         // Apply any control inputs that occur at this tick
    //         if let Some(input) = self.events.get(&tick) {
    //             info!(?input, tick, ?state, "Found input");
    //             match input {
    //                 ControlInput::SetThrust(thrust) => {
    //                     state.current_thrust = *thrust;
    //                 }
    //                 ControlInput::SetRotation(rotation) => {
    //                     state.rotation = *rotation;
    //                     state.angular_velocity = 0.;
    //                 }
    //                 ControlInput::SetThrustAndRotation(thrust, rotation) => {
    //                     state.current_thrust = *thrust;
    //                     state.rotation = *rotation;
    //                     state.angular_velocity = 0.;
    //                 }
    //                 ControlInput::SetAngVel(ang_vel) => {
    //                     state.angular_velocity = *ang_vel;
    //                 }
    //             }
    //         }
    //
    //         // Integrate physics with time scale
    //         state = state.integrate(seconds_per_tick);
    //
    //         // Store the new state
    //         self.future_states.insert(tick, state.clone());
    //     }
    //
    //     self.last_computed_tick = end_tick;
    // }
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
            .add_event::<TimelineEventRequest>()
            .add_systems(
                Update,
                (process_timeline_events, compute_future_states).chain(),
            );
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
            alive: true,
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
            alive: true,
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
            alive: true,
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
            alive: true,
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

        let state = create_test_physics_state();
        let mut future_states = BTreeMap::new();
        future_states.insert(1, state.clone());
        let entity = app
            .world_mut()
            .spawn((
                state,
                Timeline {
                    events: BTreeMap::from_iter(
                        [(
                            30,
                            TimelineEvent::Control(ControlInput::SetThrust(
                                1.0,
                            )),
                        )]
                        .into_iter(),
                    ),
                    future_states,
                    last_computed_tick: 1,
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
        assert_eq!(timeline.last_computed_tick, 1 + PREDICTION_TICKS);

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
    fn test_timeline_event_processing_required_components() {
        let mut app = create_test_app();
        bevy::log::tracing_subscriber::fmt::init();

        // Set up entity with multiple control inputs
        let entity = app.world_mut().spawn(create_test_physics_state()).id();
        app.world_mut().send_event(TimelineEventRequest {
            entity,
            tick: 10,
            input: ControlInput::SetThrust(1.0),
        });
        app.world_mut().send_event(TimelineEventRequest {
            entity,
            tick: 20,
            input: ControlInput::SetRotation(std::f32::consts::FRAC_PI_2),
        });

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

    #[test]
    fn test_timeline_event_processing() {
        let mut app = create_test_app();

        // Set up entity with multiple control inputs
        let state = create_test_physics_state();
        let mut future_states = BTreeMap::new();
        future_states.insert(1, state.clone());
        let entity = app
            .world_mut()
            .spawn((
                state,
                Timeline {
                    events: BTreeMap::from_iter(
                        [
                            (
                                10,
                                TimelineEvent::Control(
                                    ControlInput::SetThrust(1.0),
                                ),
                            ),
                            (
                                20,
                                TimelineEvent::Control(
                                    ControlInput::SetRotation(
                                        std::f32::consts::FRAC_PI_2,
                                    ),
                                ),
                            ),
                        ]
                        .into_iter(),
                    ),
                    future_states,
                    last_computed_tick: 1,
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
