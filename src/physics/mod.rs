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

use std::ops::{RangeBounds, RangeInclusive};

use bevy::{ecs::schedule::ScheduleLabel, utils::warn};
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

#[derive(Bundle)]
pub struct PhysicsBundle {
    pub state: PhysicsState,
    pub timeline: Timeline,
    pub collider: Collider,
}

impl PhysicsBundle {
    pub fn from_state(state: PhysicsState, dim: Vec2) -> PhysicsBundle {
        let collider = Collider(BRect::from_corners(-dim / 2., dim / 2.));
        PhysicsBundle {
            state,
            timeline: Timeline::default(),
            collider,
        }
    }

    pub fn new_with_events(
        state: PhysicsState,
        dim: Vec2,
        events: impl Iterator<Item = (u64, TimelineEvent)>,
    ) -> PhysicsBundle {
        let mut bundle = PhysicsBundle::from_state(state, dim);
        bundle.timeline.events.extend(events);
        bundle
    }

    pub fn new_basic(
        pos: Vec2,
        vel: Vec2,
        rotation: f32,
        max_thrust: f32,
        mass: f32,
        dim: Vec2,
    ) -> PhysicsBundle {
        PhysicsBundle::from_state(
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

/// Physical properties and control state of a simulated entity
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Transform, Timeline)]
pub struct PhysicsState {
    pub pos: Vec2,
    pub vel: Vec2,
    pub rotation: f32,
    pub ang_vel: f32,
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
pub enum TimelineEvent {
    Control(ControlInput),
    Collision(Collision),
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

#[derive(Resource)]
pub struct TrajectoryPreview {
    pub entity: Entity,
    pub start_tick: u64,
    pub timeline: Timeline,
}

/// Plugin that sets up the physics simulation systems
#[derive(Clone, Default, Debug)]
pub struct PhysicsSimulationPlugin<Label = FixedUpdate> {
    pub config: SimulationConfig,
    pub schedule: Label,
}

impl<Label: ScheduleLabel + Clone> Plugin for PhysicsSimulationPlugin<Label> {
    fn build(&self, app: &mut App) {
        app.add_event::<TimelineEventRequest>()
            .insert_resource(self.config.clone())
            .insert_resource(SpatialIndex::default())
            .insert_resource(Time::<Fixed>::from_hz(
                self.config.ticks_per_second as f64
                    * self.config.time_dilation as f64,
            ))
            .add_systems(
                self.schedule.clone(),
                (
                    update_simulation_time,
                    process_timeline_events,
                    compute_future_states,
                    sync_physics_state_transform,
                    despawn_not_alive,
                )
                    .chain(),
            )
            .add_systems(Update, (time_dilation_control, viz_colliders));
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

fn viz_colliders(
    mut gizmos: Gizmos,
    colliders: Query<(&PhysicsState, &Collider)>,
) {
    for (phys, collider) in colliders.iter() {
        // let world_aabb = collider.0.transalate(phys.pos);
        gizmos.rect_2d(
            Isometry2d::new(phys.pos, Rot2::radians(0.)),
            collider.0.size(),
            bevy::color::palettes::css::TOMATO,
        );
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

    fn apply_control_event(&mut self, event: Option<&TimelineEvent>) {
        use TimelineEvent::{Collision, Control};
        let Some(event) = event else {
            return;
        };
        match event {
            Control(control_event) => match control_event {
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
            },
            Collision(physics_event) => {}
        }
    }

    fn apply_collision_event(
        &mut self,
        e: Entity,
        event: Option<&TimelineEvent>,
    ) {
        use TimelineEvent::{Collision, Control};
        let Some(event) = event else {
            return;
        };
        match event {
            Control(control_event) => {}
            Collision(collision) => {
                self.apply_collision(e, collision);
            }
        }
    }

    fn apply_collision(&mut self, e: Entity, collision: &Collision) {
        let result = if dbg!(e == collision.this) {
            dbg!(&collision.this_result)
        } else {
            dbg!(&collision.other_result)
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
                self.pos = *post_pos;
                self.vel = *post_vel;
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
            other.vel - self.vel,
        );
        let post_vel = calculate_inelastic_collision(
            self.mass, self.vel, other.mass, other.vel,
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
                    post_pos: self.pos,
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
    mut query: Query<(Entity, Option<&Collider>, &PhysicsState, &mut Timeline)>,
    mut new_collisions: Local<EntityHashMap<Collision>>,
    mut invalid_collisions: Local<EntityHashMap<Collision>>,
) {
    let seconds_per_tick = 1.0 / simulation_config.ticks_per_second as f32;
    let tick = simulation_config.current_tick;
    new_collisions.clear();
    invalid_collisions.clear();

    for i in 0..5 {
        eprintln!("{i}th iter");
        for (e, collider, current_state, mut timeline) in query.iter_mut() {
            // ensure timline has value for current tick
            if !timeline.future_states.contains_key(&(tick - 1)) {
                info!(?e, tick, "Found missing state, inserting...");
                timeline
                    .future_states
                    .insert(tick - 1, current_state.clone());
            }

            if let Some(collision) = new_collisions.remove(&e) {
                // TODO: should this be tick - 1?
                timeline.last_computed_tick =
                    dbg!(dbg!(timeline.last_computed_tick).min(collision.tick));

                let ejected = timeline.events.insert(
                    collision.tick,
                    TimelineEvent::Collision(collision),
                );
                if let Some(ejected) = ejected {
                    warn!(
                        ?ejected,
                        ?e,
                        "we ejected a valid event when handling collision!"
                    );
                }
            }

            let updated_range = timeline.lookahead(
                e,
                tick,
                seconds_per_tick,
                simulation_config.prediction_ticks,
                collider,
                &spatial_index,
                &mut new_collisions,
                &mut invalid_collisions,
            );

            // patch spatial index
            if let Some(collider) = collider {
                for tick in updated_range {
                    // eprintln!("Inserting into spatial index {tick}");
                    let state = timeline.future_states.get(&tick).unwrap();
                    spatial_index.insert(
                        tick,
                        collider,
                        SpatialItem {
                            entity: e,
                            pos: state.pos,
                            vel: state.vel,
                            mass: state.mass,
                        },
                    );
                }
            }
        }

        if new_collisions.is_empty() {
            info!("No more invalidations, breaking...");
            dbg!("No more invalidations, breaking...");
            break;
        }
    }
}

impl Timeline {
    pub fn lookahead(
        &mut self,
        e: Entity,
        current_tick: u64,
        seconds_per_tick: f32,
        prediction_ticks: u64,
        collider: Option<&Collider>,
        spatial_index: &SpatialIndex,
        // new collision events generated by this timeline that need to be
        // applied to other entity
        new_collisions: &mut EntityHashMap<Collision>,
        // other entities that we shared a collision event with, but that now
        // doesn't happen
        invalid_collisions: &mut EntityHashMap<Collision>,
    ) -> RangeInclusive<u64> {
        // Start computation from the earliest invalid state
        let start_tick = current_tick.max(self.last_computed_tick);
        let mut end_tick = current_tick + prediction_ticks;
        // dbg!(start_tick, end_tick);

        let mut state =
            self.future_states.get(&(start_tick - 1)).unwrap().clone();

        // let mut has_collided = false;

        for tick in start_tick..=end_tick {
            let mut next_state = state.clone();

            // Apply any control inputs that occur at this tick
            let event = self.events.get(&tick);
            let mut event_to_insert = None;

            next_state.apply_control_event(event);

            // Integrate after controls are applied
            next_state = next_state.integrate(seconds_per_tick);

            // Check if we collide
            if let Some(collision) = check_for_collision(
                e,
                tick,
                &next_state,
                collider,
                spatial_index,
            ) {
                dbg!(&event, &collision, e, tick, &next_state);
                match event {
                    None => {
                        // apply collision, add to new_collisions and add new
                        // event to our timeline
                        dbg!("Applying new collision");
                        dbg!(&event, &collision, e, tick, &next_state);
                        next_state.apply_collision(e, &collision);
                        event_to_insert =
                            Some(TimelineEvent::Collision(collision.clone()));
                        new_collisions.insert(collision.other, collision);
                    }
                    Some(TimelineEvent::Collision(existing_collision)) => {
                        // Check if collision events match up
                        // if collision is the same as event, apply the
                        // collision and do not add to
                        // new_collisions since we're already expecting it

                        if collisions_equiv(&collision, existing_collision) {
                            next_state.apply_collision_event(e, event);
                        } else {
                            // Let's assume old event is invalid now
                            let other = if existing_collision.this == e {
                                existing_collision.other
                            } else {
                                e
                            };
                            invalid_collisions
                                .insert(other, existing_collision.clone());
                        }
                    }
                    Some(_) => {
                        warn!(
                            "Adding collision event to tick with existing \
                             event, will overwrite!"
                        );
                        panic!(
                            "Adding collision event to tick with existing \
                             non-collision event, will overwrite!"
                        );
                    }
                }
            }

            // TODO: remove this hack
            if let Some(event) = event_to_insert {
                self.events.insert(tick, event);
            }

            // Store the new state
            state = next_state;
            self.future_states.insert(tick, state.clone());
            if !state.alive {
                end_tick = tick;
                let max_tick = *self.future_states.last_key_value().unwrap().0;
                for tick in end_tick + 1..=max_tick {
                    self.future_states.remove(&tick);
                }
                break;
            }
        }

        self.last_computed_tick = end_tick;
        start_tick..=end_tick
    }
}

fn collisions_equiv(a: &Collision, b: &Collision) -> bool {
    if (a.this != b.this && a.this != b.other)
        || (a.other != b.other && a.other != b.this)
    {
        dbg!("Not same entities for collision", a, b);
        return false;
    }

    if a.tick != b.tick {
        dbg!("ticks don't match", a, b);
        return false;
    }

    let (b_this, b_other) = if a.this == b.this {
        // Proceed with normal comparison
        (&b.this_result, &b.other_result)
    } else {
        // Switch other and this
        (&b.other_result, &b.this_result)
    };

    // if &a.this_result != b_this {
    //     dbg!("This Results don't match", a, b);
    //     return false;
    // }
    //
    // if &a.other_result != b_other {
    //     dbg!("Other Results don't match", a, b);
    //     return false;
    // }
    if !a.this_result.pos_equiv(b_this) {
        dbg!("This Results don't match", a, b);
        return false;
    }

    if !a.other_result.pos_equiv(b_other) {
        dbg!("Other Results don't match", a, b);
        return false;
    }

    true
}

fn check_for_collision(
    e: Entity,
    tick: u64,
    state: &PhysicsState,
    collider: Option<&Collider>,
    spatial_index: &SpatialIndex,
) -> Option<Collision> {
    let Some(collider) = collider else {
        dbg!("No collider");
        return None;
    };
    let Some(res) = spatial_index.collides(e, tick, state.pos, collider) else {
        dbg!("doesn't collide", tick);
        // eprintln!(
        //     "Doesn't collide tick: {tick}, {:?}",
        //     &spatial_index.0.get(&tick)
        // );
        return None;
    };
    let (other_aabb, other) = res;

    let (this_result, other_result) =
        state.collision_result(other_aabb, &other);

    // info!(
    //     this = e.index(),
    //     other = other.entity.index(),
    //     // other_aabb = ?other_aabb.to_bevy(),
    //     ?this_result,
    //     ?other_result,
    //     "Found collision"
    // );
    return Some(Collision {
        tick,
        this: e,
        this_result,
        other: other.entity,
        other_result,
    });
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
        timeline.future_states.remove(&(sim_state.current_tick - 1));
    }
}

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
            .add_plugins(PhysicsSimulationPlugin {
                config: default(),
                schedule: Update,
            });
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
        assert_approx_eq!(next_state.pos.x, 10.0 + 2.0 * delta, 1e-6);
        assert_approx_eq!(next_state.pos.y, 5.0 + 1.0 * delta, 1e-6);
        // Velocity should remain unchanged (no forces)
        assert_approx_eq!(next_state.vel.x, 2.0, 1e-6);
        assert_approx_eq!(next_state.vel.y, 1.0, 1e-6);
        // Rotation should change based on angular velocity
        assert_approx_eq!(next_state.rotation, 0.0 + 0.5 * delta, 1e-6);

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
        assert_approx_eq!(next_state.vel.x, 50.0 * delta, 1e-6);
        assert_approx_eq!(next_state.vel.y, 0.0, 1e-6);
        assert_approx_eq!(next_state.pos.x, 0.0, 1e-6); // Fixed: position doesn't change first frame
        assert_approx_eq!(next_state.pos.y, 0.0, 1e-6);

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
        assert_approx_eq!(next_state.vel.x, expected_accel * delta, 1e-6);
        assert_approx_eq!(next_state.vel.y, expected_accel * delta, 1e-6);
        assert_approx_eq!(next_state.pos.x, 0.0, 1e-6); // Fixed: position doesn't change first frame
        assert_approx_eq!(next_state.pos.y, 0.0, 1e-6);

        // Let's verify position changek after a second integration step
        let third_state = next_state.integrate(delta);
        assert_approx_eq!(
            third_state.pos.x,
            (expected_accel * delta) * delta, /* Using velocity from
                                               * previous state */
            1e-6
        );
        assert_approx_eq!(
            third_state.pos.y,
            (expected_accel * delta) * delta,
            1e-6
        );
    }

    #[test]
    fn test_lookahead_collision_survives() {
        let mut prev_state = create_test_physics_state();
        prev_state.vel.x = 10.;
        prev_state.mass = 9.;

        let mut timeline = Timeline {
            future_states: BTreeMap::from_iter(
                [(0, prev_state.clone())].into_iter(),
            ),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let entity = Entity::from_raw(5);
        let current_tick = 1;
        let seconds_per_tick = 1.;
        let dim = Vec2::new(2., 2.);
        let collider = Some(Collider(BRect::from_corners(dim / 2., -dim / 2.)));
        let mut spatial_index = SpatialIndex::default();
        let other_entity = Entity::from_raw(1009001);
        spatial_index.insert(
            2,
            &Collider(BRect::from_corners(dim / 2., -dim / 2.)),
            SpatialItem {
                entity: other_entity,
                pos: Vec2::new(20., 0.),
                vel: Vec2::new(0., 0.),
                mass: 1.,
            },
        );
        let mut new_collisions = EntityHashMap::default();
        let mut invalidations = EntityHashMap::default();

        timeline.lookahead(
            entity,
            current_tick,
            seconds_per_tick,
            10,
            collider.as_ref(),
            &spatial_index,
            &mut new_collisions,
            &mut invalidations,
        );

        dbg!(&new_collisions);
        dbg!(&timeline.events);

        let next_state = timeline.future_states.get(&(current_tick)).unwrap();
        let expected = PhysicsState {
            pos: Vec2::new(10., 0.),
            ..prev_state
        };
        assert_eq!(next_state, &expected);

        let others_collision = new_collisions.get(&other_entity).unwrap();
        assert_eq!(
            others_collision,
            &Collision {
                tick: 2,
                this: entity,
                this_result: EntityCollisionResult::Survives {
                    post_pos: Vec2::new(20., 0.),
                    post_vel: Vec2::new(9., 0.),
                },
                other: other_entity,
                other_result: EntityCollisionResult::Destroyed
            }
        );

        let expected = PhysicsState {
            pos: Vec2::new(29., 0.),
            vel: Vec2::new(9., 0.),
            ..prev_state
        };
        assert_eq!(timeline.future_states.get(&3), Some(&expected));
        assert_eq!(timeline.future_states.get(&2).unwrap().alive, true);
    }

    #[test]
    fn test_lookahead_collision_destroyed() {
        let mut prev_state = create_test_physics_state();
        prev_state.vel.x = 10.;

        let mut timeline = Timeline {
            future_states: BTreeMap::from_iter(
                [(0, prev_state.clone())].into_iter(),
            ),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let entity = Entity::from_raw(5);
        let current_tick = 1;
        let seconds_per_tick = 1.;
        let dim = Vec2::new(2., 2.);
        let collider = Some(Collider(BRect::from_corners(dim / 2., -dim / 2.)));
        let mut spatial_index = SpatialIndex::default();
        let other_entity = Entity::from_raw(1009001);
        spatial_index.insert(
            2,
            &Collider(BRect::from_corners(dim / 2., -dim / 2.)),
            SpatialItem {
                entity: other_entity,
                pos: Vec2::new(20., 0.),
                vel: Vec2::new(0., 0.),
                mass: 2.,
            },
        );
        let mut new_collisions = EntityHashMap::default();
        let mut invalidations = EntityHashMap::default();

        timeline.lookahead(
            entity,
            current_tick,
            seconds_per_tick,
            10,
            collider.as_ref(),
            &spatial_index,
            &mut new_collisions,
            &mut invalidations,
        );

        dbg!(&new_collisions);
        dbg!(&timeline.events);

        let next_state = timeline.future_states.get(&(current_tick)).unwrap();
        let expected = PhysicsState {
            pos: Vec2::new(10., 0.),
            ..prev_state
        };
        assert_eq!(next_state, &expected);

        let others_collision = new_collisions.get(&other_entity).unwrap();
        assert_eq!(
            others_collision,
            &Collision {
                tick: 2,
                this: entity,
                this_result: EntityCollisionResult::Destroyed,
                other: other_entity,
                other_result: EntityCollisionResult::Survives {
                    post_pos: Vec2::new(20., 0.),
                    post_vel: Vec2::new(10. / 3., 0.),
                }
            }
        );

        assert_eq!(timeline.future_states.get(&3), None);
        assert_eq!(timeline.future_states.get(&2).unwrap().alive, false);
    }

    #[test]
    fn test_lookahead() {
        let mut prev_state = create_test_physics_state();
        prev_state.vel.x = 10.;

        let mut timeline = Timeline {
            future_states: BTreeMap::from_iter(
                [(0, prev_state.clone())].into_iter(),
            ),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let entity = Entity::from_raw(5);
        let current_tick = 1;
        let seconds_per_tick = 1.;
        let collider = None;
        let spatial_index = SpatialIndex::default();
        let mut new_collisions = EntityHashMap::default();
        let mut invalidations = EntityHashMap::default();

        timeline.lookahead(
            entity,
            current_tick,
            seconds_per_tick,
            120,
            collider,
            &spatial_index,
            &mut new_collisions,
            &mut invalidations,
        );

        let next_state = timeline.future_states.get(&(current_tick)).unwrap();
        let expected = PhysicsState {
            pos: Vec2::new(10., 0.),
            ..prev_state
        };
        assert_eq!(next_state, &expected);
    }

    #[test]
    fn test_compute_future_states_system_with_collision() {
        let mut app = create_test_app();
        app.world_mut()
            .resource_mut::<SimulationConfig>()
            .prediction_ticks = 4;
        app.world_mut()
            .resource_mut::<SimulationConfig>()
            .ticks_per_second = 1;

        // Spawn an entity with physics components

        let entity = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(
                PhysicsState {
                    vel: Vec2::new(10., 0.),
                    ..create_test_physics_state()
                },
                Vec2::splat(2.),
                // [(2, TimelineEvent::Control(ControlInput::SetThrust(1.0)))]
                //     .into_iter(),
            ))
            .id();

        let other_entity = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(
                PhysicsState {
                    pos: Vec2::new(30., 0.),
                    mass: 3.,
                    ..create_test_physics_state()
                },
                Vec2::splat(2.),
            ))
            .id();

        // Run the system once
        app.update();
        let world = app.world();

        // Get the resulting timeline component
        let timeline = world
            .entity(entity)
            .get::<Timeline>()
            .expect("Timeline component should exist");

        dbg!(&timeline.events);

        let other_timeline = world
            .entity(other_entity)
            .get::<Timeline>()
            .expect("Timeline component should exist");

        dbg!(&other_timeline.events);

        dbg!(timeline
            .future_states
            .iter()
            .map(|s| s.1.pos.x)
            .collect::<Vec<_>>());
        dbg!(other_timeline
            .future_states
            .iter()
            .map(|s| s.1.pos.x)
            .collect::<Vec<_>>());

        // Verify states were computed
        assert!(!timeline.future_states.is_empty());
        assert_eq!(timeline.last_computed_tick, 3);
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
                [(30, TimelineEvent::Control(ControlInput::SetThrust(1.0)))]
                    .into_iter(),
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
        assert!(mid_state.vel.x > 0.0);

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
        assert!(mid_state.vel.x > 0.0);

        // Check state at tick 25 (after both events)
        let final_state = timeline
            .future_states
            .get(&25)
            .expect("Should have state after rotation");
        assert_eq!(final_state.rotation, std::f32::consts::FRAC_PI_2);
    }
}
