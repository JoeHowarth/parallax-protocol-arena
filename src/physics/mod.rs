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

use crate::prelude::*;

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
        events: impl IntoIterator<Item = (u64, TimelineEvent)>,
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

/// Represents the complete physical state of a simulated entity at a point in
/// time
///
/// This struct contains all the physical properties needed to simulate an
/// entity's movement and interactions. It tracks both linear and angular
/// motion, as well as thrust capabilities.
///
/// # Properties
///
/// * `pos` - Position in world space (meters)
/// * `vel` - Velocity vector (meters/second)
/// * `rotation` - Orientation in radians (0 = facing +X)
/// * `ang_vel` - Angular velocity (radians/second)
/// * `mass` - Mass of entity in kilograms
/// * `current_thrust` - Current thrust level (-1.0 to 1.0)
/// * `max_thrust` - Maximum thrust force in Newtons
/// * `alive` - Whether entity still exists (false = destroyed)
///
/// # Requirements
///
/// Entities with PhysicsState must also have Transform and Timeline components
/// Physical state of an entity at a point in time, including position, motion
/// and thrust capabilities
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

#[derive(Event, Debug)]
pub struct TimelineEventRequest {
    /// Entity to apply to
    pub entity: Entity,
    /// Simulation tick when this input takes effect
    pub tick: u64,
    /// The control input to apply
    pub input: ControlInput,
}

/// Control inputs that can be scheduled to modify entity behavior at specific
/// ticks
///
/// These inputs represent discrete changes to an entity's movement parameters.
/// They can be scheduled in advance to create complex movement patterns.
#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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
    pub should_keep_alive: bool,
}

impl<Label: ScheduleLabel + Clone> Plugin for PhysicsSimulationPlugin<Label> {
    fn build(&self, app: &mut App) {
        let should_keep_alive = self.should_keep_alive;
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
                    compute_future_states,
                    sync_physics_state_transform,
                    despawn_not_alive.run_if(move || !should_keep_alive),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    time_dilation_control,
                    viz_colliders,
                    process_timeline_events,
                ),
            );
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
        timeline.last_computed_tick =
            timeline.last_computed_tick.min(*tick - 1);
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
}

fn compute_future_states(
    simulation_config: Res<SimulationConfig>,
    mut spatial_index: ResMut<SpatialIndex>,
    mut query: Query<(Entity, &Collider, &PhysicsState, &mut Timeline)>,
) {
    let seconds_per_tick = 1.0 / simulation_config.ticks_per_second as f32;

    let mut interaction = None;

    for i in 0..3 {
        let tick = simulation_config.current_tick;
        eprintln!("\n[{i}th Iteration]\n");
        for (e, collider, current_state, mut timeline) in query.iter_mut() {
            // ensure timline has value for current tick
            if !timeline.future_states.contains_key(&(tick - 1)) {
                info!(?e, tick, "Found missing state, inserting...");
                timeline
                    .future_states
                    .insert(tick - 1, current_state.clone());
            }

            let ret = timeline.lookahead(
                e,
                tick,
                seconds_per_tick,
                simulation_config.prediction_ticks,
                collider,
                &spatial_index,
            );

            // Patch spatial index
            spatial_index.patch(
                e,
                &timeline,
                collider,
                ret.updated,
                ret.removed,
            );

            interaction = ret.interaction;
            if interaction.is_some() {
                // Eject and resolve interaction
                break;
            }
        }

        let Some(interaction) = interaction else {
            info!("Loop finished with no interaction to resolve. Done");
            return;
        };

        let Ok([ (a_e, a_col, _, mut a_tl), // fmt
          (b_e, b_col, _, mut b_tl) // fmt
        ]) = query.get_many_mut(interaction.entities) else {
            dbg!(&interaction);
            panic!("whoops")
        };
        resolve_collisions(
            interaction.tick,
            (a_e, a_col, &mut a_tl),
            (b_e, b_col, &mut b_tl),
            seconds_per_tick,
            &mut spatial_index,
        );
    }
    panic!(
        "Exited loop without resolving all interactions. Suggests infinite \
         loop bug"
    );
}

fn resolve_collisions(
    tick: u64,
    (a_e, a_col, a_tl): (Entity, &Collider, &mut Timeline),
    (b_e, b_col, b_tl): (Entity, &Collider, &mut Timeline),
    seconds_per_tick: f32,
    spatial_index: &mut SpatialIndex,
) {
    // STEP 1: Remove B's state and replay tick t

    // FIXME: check this is an interaction event
    a_tl.events.remove(&tick);
    b_tl.events.remove(&tick);
    b_tl.last_computed_tick = tick - 1;

    let b_ret =
        b_tl.lookahead(b_e, tick, seconds_per_tick, 0, b_col, &spatial_index);

    assert_eq!(*b_ret.updated.end(), tick);

    // Patch b's spatial index
    spatial_index.patch(b_e, &b_tl, b_col, b_ret.updated, b_ret.removed);

    // NOTE:both must have computed state at tick, but not applied any
    // interaction events
    let a_st = a_tl.future_states.get_mut(&tick).unwrap();
    let b_st = b_tl.future_states.get_mut(&tick).unwrap();

    // STEP 2: check for interaction
    if let Some(collision) = spatial_index.collides(a_e, tick, a_st.pos, a_col)
    {
        // STEP 3: resolve interaction
        let (a_result, b_result) = calculate_collision_result(
            &SpatialItem::from_state(a_e, a_st),
            &SpatialItem::from_state(b_e, b_st),
        );

        a_st.apply_collision_result(&a_result);
        b_st.apply_collision_result(&b_result);

        // TODO: rethink why we're storing this
        let collision = Collision {
            tick,
            this: a_e,
            this_result: a_result,
            other: b_e,
            other_result: b_result,
        };

        a_tl.events
            .insert(tick, TimelineEvent::Collision(collision.clone()));
        b_tl.events
            .insert(tick, TimelineEvent::Collision(collision));
        a_tl.last_computed_tick = tick;
        b_tl.last_computed_tick = tick;
    }
}

#[derive(Debug)]
pub struct LookaheadRet {
    /// States that changed
    pub updated: RangeInclusive<u64>,
    /// States that were removed
    pub removed: Option<RangeInclusive<u64>>,
    /// Entities that interact and at what tick
    pub interaction: Option<InteractionLocator>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct InteractionLocator {
    pub entities: [Entity; 2],
    pub tick: u64,
}

impl From<(Entity, Entity, u64)> for InteractionLocator {
    fn from(value: (Entity, Entity, u64)) -> Self {
        InteractionLocator {
            entities: [value.0, value.1],
            tick: value.2,
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
        collider: &Collider,
        spatial_index: &SpatialIndex,
    ) -> LookaheadRet {
        // Start computation from the earliest invalid state
        let start_tick = current_tick.max(self.last_computed_tick + 1);
        let mut end_tick = current_tick + prediction_ticks;
        if start_tick > end_tick {
            info!("start_tick > end_tick, returning early");
            return LookaheadRet {
                // will cause `is_empty()` to be true
                updated: start_tick..=end_tick,
                removed: None,
                interaction: None,
            };
        }
        eprintln!(
            "start: {start_tick}, end: {end_tick}, current: {current_tick}, \
             last_computed: {}",
            self.last_computed_tick
        );

        let mut state =
            self.future_states.get(&(start_tick - 1)).unwrap().clone();

        let mut interaction = None;

        for tick in start_tick..=end_tick {
            let event = self.events.get(&tick);

            // Apply any control inputs that occur at this tick
            state.apply_control_event(event);

            // Integrate after controls are applied
            state = state.integrate(seconds_per_tick);

            // Store the new state
            self.future_states.insert(tick, state.clone());

            if let Some(TimelineEvent::Collision(c)) = event {
                eprintln!(
                    "Timeline encountered and interaction event. Ejecting for \
                     resolution. {tick}, {c:?}, {state:?}"
                );
                interaction = Some((e, c.other, tick).into());
                end_tick = tick;
                break;
            }

            // Stop if we're dead
            if !state.alive {
                eprintln!("dead");
                end_tick = tick;
                break;
            }

            // Check if we collide
            if let Some((_, other)) =
                spatial_index.collides(e, tick, state.pos, collider)
            {
                eprintln!("found new collision, {tick}");
                interaction = Some((e, other.entity, tick).into());
                end_tick = tick;
                break;
            }
        }

        self.last_computed_tick = end_tick;

        // Remove invalid trailing states
        // NOTE: Should pruning old states happen before timeline update?
        let max = *self.future_states.last_key_value().unwrap().0;
        let removed = if end_tick != max {
            let removed = (end_tick + 1)..=max;
            for tick in (end_tick + 1)..=max {
                self.future_states.remove(&tick);
            }
            Some(removed)
        } else {
            None
        };

        LookaheadRet {
            updated: start_tick..=end_tick,
            removed,
            interaction,
        }
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
        timeline
            .future_states
            .remove(&(sim_state.current_tick.saturating_sub(2)));
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
    if keys.just_pressed(KeyCode::KeyP) {
        config.paused = !config.paused;
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
                should_keep_alive: false,
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
    fn test_resolve_collision_remove() {
        let tick = 2;
        let mut spatial_index = SpatialIndex::default();
        let col = Collider::from_dim(Vec2::new(2., 2.));

        // Set up b
        let b = Entity::from_raw(1);
        let mut b_st = create_test_physics_state();
        b_st.pos.x = 20.;
        b_st.mass = 1.;
        let mut b_tl = Timeline {
            future_states: BTreeMap::from_iter([(0, b_st.clone())].into_iter()),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let b_ret = b_tl.lookahead(b, 1, 1., 1, &col, &spatial_index);
        dbg!(&b_ret);
        assert_eq!(b_ret.updated, 1..=2);
        assert_eq!(b_ret.interaction, None);
        let b_st_t = b_tl.future_states.get(&tick).unwrap();
        assert_eq!(b_st_t, &PhysicsState { ..b_st });
        assert_eq!(b_tl.events.len(), 0);
        spatial_index.patch(b, &b_tl, &col, b_ret.updated, b_ret.removed);

        // Set up a
        let a = Entity::from_raw(0);
        let mut a_st = create_test_physics_state();
        a_st.vel.x = 10.;
        a_st.mass = 9.;
        let mut a_tl = Timeline {
            future_states: BTreeMap::from_iter([(0, a_st.clone())].into_iter()),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let a_ret = a_tl.lookahead(a, 1, 1., 1, &col, &spatial_index);
        dbg!(&a_ret);
        assert_eq!(a_ret.updated, 1..=2);
        assert_eq!(a_ret.interaction, Some((a, b, tick).into()));
        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        assert_eq!(
            a_st_t,
            &PhysicsState {
                pos: Vec2::new(20., 0.),
                ..a_st
            }
        );
        assert_eq!(a_tl.events.len(), 0);
        spatial_index.patch(a, &a_tl, &col, a_ret.updated, a_ret.removed);

        // We expect a collision at tick t where b is destroyed and a's velocity
        // is reduced
        resolve_collisions(
            tick,
            (a, &col, &mut a_tl),
            (b, &col, &mut b_tl),
            1.,
            &mut spatial_index,
        );
        assert_eq!(a_tl.events.len(), 1);
        assert_eq!(b_tl.events.len(), 1);

        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        let b_st_t = b_tl.future_states.get(&tick).unwrap();
        dbg!(&a_st_t, &b_st_t);
        assert_eq!(
            b_st_t,
            &PhysicsState {
                alive: false,
                ..b_st
            }
        );
        assert_eq!(
            a_st_t,
            &PhysicsState {
                pos: Vec2::new(20., 0.),
                vel: Vec2::new(9., 0.),
                alive: true,
                ..a_st
            }
        );

        // Simulate a user input to avoid the collision at the previous tick
        a_tl.events.insert(
            1,
            // cancel 10 vel/tick
            TimelineEvent::Control(ControlInput::SetThrustAndRotation(0.9, PI)),
        );
        a_tl.last_computed_tick = 0;

        // Re-run lookahead
        // We expect it to find the old, invalid collision event and eject
        let a_ret = a_tl.lookahead(a, 1, 1., 1, &col, &spatial_index);
        dbg!(&a_ret);
        assert_eq!(a_ret.updated, 1..=tick);
        assert_eq!(a_ret.interaction, Some((a, b, tick).into()));
        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        assert_eq!(a_st_t.pos.x, 10.);
        assert_eq!(a_st_t.vel.x, -10.);
        assert_eq!(a_st_t.alive, true);
        assert_eq!(a_tl.events.len(), 2);
        spatial_index.patch(a, &a_tl, &col, a_ret.updated, a_ret.removed);

        // We expect to not collide
        // a and b should have the collision events removed
        resolve_collisions(
            tick,
            (a, &col, &mut a_tl),
            (b, &col, &mut b_tl),
            1.,
            &mut spatial_index,
        );

        dbg!(&a_tl.events, &b_tl.events);

        assert_eq!(a_tl.events.len(), 1);
        assert_eq!(b_tl.events.len(), 0);

        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        let b_st_t = b_tl.future_states.get(&tick).unwrap();
        dbg!(&a_st_t, &b_st_t);
        assert_eq!(b_st_t, &PhysicsState { ..b_st });
        assert_eq!(a_st_t.pos.x, 10.);
        assert_eq!(a_st_t.vel.x, -10.);
        assert_eq!(a_st_t.alive, true);
    }

    #[test]
    fn test_resolve_collision_new() {
        let tick = 1;
        let mut spatial_index = SpatialIndex::default();
        let col = Collider::from_dim(Vec2::new(2., 2.));

        // Set up b
        let b = Entity::from_raw(1);
        let mut b_st = create_test_physics_state();
        b_st.pos.x = 10.;
        b_st.mass = 1.;
        let mut b_tl = Timeline {
            future_states: BTreeMap::from_iter([(0, b_st.clone())].into_iter()),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        // spatial_index.insert(tick, &col, SpatialItem::from_state(b, &b_st));
        let b_ret = b_tl.lookahead(b, 1, 1., 0, &col, &spatial_index);
        dbg!(&b_ret);
        assert_eq!(b_ret.updated, 1..=1);
        assert_eq!(b_ret.interaction, None);
        let b_st_t = b_tl.future_states.get(&tick).unwrap();
        assert_eq!(b_st_t, &PhysicsState { ..b_st });
        assert_eq!(b_tl.events.len(), 0);
        spatial_index.patch(b, &b_tl, &col, b_ret.updated, b_ret.removed);

        // Set up a
        let a = Entity::from_raw(0);
        let mut a_st = create_test_physics_state();
        a_st.vel.x = 10.;
        a_st.mass = 9.;
        let mut a_tl = Timeline {
            future_states: BTreeMap::from_iter([(0, a_st.clone())].into_iter()),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let a_ret = a_tl.lookahead(a, 1, 1., 0, &col, &spatial_index);
        dbg!(&a_ret);
        assert_eq!(a_ret.updated, 1..=1);
        assert_eq!(a_ret.interaction, Some((a, b, 1).into()));
        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        assert_eq!(
            a_st_t,
            &PhysicsState {
                pos: Vec2::new(10., 0.),
                ..a_st
            }
        );
        assert_eq!(a_tl.events.len(), 0);
        spatial_index.patch(a, &a_tl, &col, a_ret.updated, a_ret.removed);

        resolve_collisions(
            tick,
            (a, &col, &mut a_tl),
            (b, &col, &mut b_tl),
            1.,
            &mut spatial_index,
        );
        assert_eq!(a_tl.events.len(), 1);
        assert_eq!(b_tl.events.len(), 1);

        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        let b_st_t = b_tl.future_states.get(&tick).unwrap();
        dbg!(&a_st_t, &b_st_t);
        assert_eq!(
            b_st_t,
            &PhysicsState {
                alive: false,
                ..b_st
            }
        );
        assert_eq!(
            a_st_t,
            &PhysicsState {
                pos: Vec2::new(10., 0.),
                vel: Vec2::new(9., 0.),
                alive: true,
                ..a_st
            }
        );
    }

    #[test]
    fn test_lookahead_collision() {
        let tick = 1;
        let mut spatial_index = SpatialIndex::default();
        let col = Collider::from_dim(Vec2::new(2., 2.));
        let b = Entity::from_raw(1);
        let b_spatial_item = SpatialItem {
            entity: b,
            pos: Vec2::new(10., 0.),
            vel: Vec2::new(0., 0.),
            mass: 1.,
        };
        spatial_index.insert(tick, &col, b_spatial_item.clone());

        let a = Entity::from_raw(0);
        let mut a_st = create_test_physics_state();
        a_st.vel.x = 10.;
        a_st.mass = 9.;

        let mut a_tl = Timeline {
            future_states: BTreeMap::from_iter([(0, a_st.clone())].into_iter()),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let a_ret = a_tl.lookahead(a, 1, 1., 0, &col, &spatial_index);

        dbg!(&a_ret);
        assert_eq!(a_ret.updated, 1..=1);
        assert_eq!(a_ret.interaction, Some((a, b, 1).into()));

        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        assert_eq!(
            a_st_t,
            &PhysicsState {
                pos: Vec2::new(10., 0.),
                ..a_st
            }
        )
    }

    #[test]
    fn test_lookahead_no_collision() {
        let tick = 1;
        let spatial_index = SpatialIndex::default();
        let col = Collider::from_dim(Vec2::new(2., 2.));
        let b = Entity::from_raw(1);
        let b_spatial_item = SpatialItem {
            entity: b,
            pos: Vec2::new(20., 0.),
            vel: Vec2::new(0., 0.),
            mass: 1.,
        };
        // spatial_index.insert(tick, &col, b_spatial_item.clone());

        let a = Entity::from_raw(0);
        let mut a_st = create_test_physics_state();
        a_st.vel.x = 10.;
        a_st.mass = 9.;

        let mut a_tl = Timeline {
            future_states: BTreeMap::from_iter([(0, a_st.clone())].into_iter()),
            events: BTreeMap::default(),
            last_computed_tick: 0,
        };
        let a_ret = a_tl.lookahead(a, 1, 1., 0, &col, &spatial_index);

        dbg!(&a_ret);
        assert_eq!(a_ret.updated, 1..=1);
        assert_eq!(a_ret.interaction, None);

        let a_st_t = a_tl.future_states.get(&tick).unwrap();
        assert_eq!(
            a_st_t,
            &PhysicsState {
                pos: Vec2::new(10., 0.),
                ..a_st
            }
        )
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

    // #[test]
    // fn test_lookahead_collision_survives() {
    // let mut prev_state = create_test_physics_state();
    // prev_state.vel.x = 10.;
    // prev_state.mass = 9.;
    //
    // let mut timeline = Timeline {
    // future_states: BTreeMap::from_iter(
    // [(0, prev_state.clone())].into_iter(),
    // ),
    // events: BTreeMap::default(),
    // last_computed_tick: 0,
    // };
    // let entity = Entity::from_raw(5);
    // let current_tick = 1;
    // let seconds_per_tick = 1.;
    // let dim = Vec2::new(2., 2.);
    // let collider = Some(Collider(BRect::from_corners(dim / 2., -dim / 2.)));
    // let mut spatial_index = SpatialIndex::default();
    // let other_entity = Entity::from_raw(1009001);
    // spatial_index.insert(
    // 2,
    // &Collider(BRect::from_corners(dim / 2., -dim / 2.)),
    // SpatialItem {
    // entity: other_entity,
    // pos: Vec2::new(20., 0.),
    // vel: Vec2::new(0., 0.),
    // mass: 1.,
    // },
    // );
    //
    // let ret = timeline.lookahead(
    // entity,
    // current_tick,
    // seconds_per_tick,
    // 10,
    // collider.as_ref(),
    // &spatial_index,
    // );
    //
    // dbg!(&ret);
    // dbg!(&timeline.events);
    //
    // let next_state = timeline.future_states.get(&(current_tick)).unwrap();
    // let expected = PhysicsState {
    // pos: Vec2::new(10., 0.),
    // ..prev_state
    // };
    // assert_eq!(next_state, &expected);
    //
    // let others_collision = new_collisions.get(&other_entity).unwrap();
    // assert_eq!(
    // others_collision,
    // &Collision {
    // tick: 2,
    // this: entity,
    // this_result: EntityCollisionResult::Survives {
    // post_pos: Vec2::new(20., 0.),
    // post_vel: Vec2::new(9., 0.),
    // },
    // other: other_entity,
    // other_result: EntityCollisionResult::Destroyed
    // }
    // );
    //
    // let expected = PhysicsState {
    // pos: Vec2::new(29., 0.),
    // vel: Vec2::new(9., 0.),
    // ..prev_state
    // };
    // assert_eq!(timeline.future_states.get(&3), Some(&expected));
    // assert_eq!(timeline.future_states.get(&2).unwrap().alive, true);
    // }
    //
    // #[test]
    // fn test_lookahead_collision_destroyed() {
    // let mut prev_state = create_test_physics_state();
    // prev_state.vel.x = 10.;
    //
    // let mut timeline = Timeline {
    // future_states: BTreeMap::from_iter(
    // [(0, prev_state.clone())].into_iter(),
    // ),
    // events: BTreeMap::default(),
    // last_computed_tick: 0,
    // };
    // let entity = Entity::from_raw(5);
    // let current_tick = 1;
    // let seconds_per_tick = 1.;
    // let dim = Vec2::new(2., 2.);
    // let collider = Some(Collider(BRect::from_corners(dim / 2., -dim / 2.)));
    // let mut spatial_index = SpatialIndex::default();
    // let other_entity = Entity::from_raw(1009001);
    // spatial_index.insert(
    // 2,
    // &Collider(BRect::from_corners(dim / 2., -dim / 2.)),
    // SpatialItem {
    // entity: other_entity,
    // pos: Vec2::new(20., 0.),
    // vel: Vec2::new(0., 0.),
    // mass: 2.,
    // },
    // );
    // let mut new_collisions = EntityHashMap::default();
    // let mut invalidations = EntityHashMap::default();
    //
    // timeline.lookahead(
    // entity,
    // current_tick,
    // seconds_per_tick,
    // 10,
    // collider.as_ref(),
    // &spatial_index,
    // &mut new_collisions,
    // &mut invalidations,
    // );
    //
    // dbg!(&new_collisions);
    // dbg!(&timeline.events);
    //
    // let next_state = timeline.future_states.get(&(current_tick)).unwrap();
    // let expected = PhysicsState {
    // pos: Vec2::new(10., 0.),
    // ..prev_state
    // };
    // assert_eq!(next_state, &expected);
    //
    // let others_collision = new_collisions.get(&other_entity).unwrap();
    // assert_eq!(
    // others_collision,
    // &Collision {
    // tick: 2,
    // this: entity,
    // this_result: EntityCollisionResult::Destroyed,
    // other: other_entity,
    // other_result: EntityCollisionResult::Survives {
    // post_pos: Vec2::new(20., 0.),
    // post_vel: Vec2::new(10. / 3., 0.),
    // }
    // }
    // );
    //
    // assert_eq!(timeline.future_states.get(&3), None);
    // assert_eq!(timeline.future_states.get(&2).unwrap().alive, false);
    // }

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
        let collider = Collider::from_wh(2., 2.);
        let spatial_index = SpatialIndex::default();

        let ret = timeline.lookahead(
            entity,
            current_tick,
            seconds_per_tick,
            120,
            &collider,
            &spatial_index,
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
            .prediction_ticks = 2;
        app.world_mut()
            .resource_mut::<SimulationConfig>()
            .ticks_per_second = 1;

        // Spawn an entity with physics components
        let mut b_st = create_test_physics_state();
        b_st.pos.x = 30.;
        b_st.mass = 1.;
        let dim = Vec2::splat(2.);

        let b = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(b_st.clone(), dim))
            .id();

        let mut a_st = create_test_physics_state();
        a_st.vel.x = 10.;
        a_st.mass = 9.;
        let a = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(a_st.clone(), dim))
            .id();

        // Run the system once
        app.update();
        {
            let world = app.world();

            // Get the resulting timeline component
            let a_tl = world
                .entity(a)
                .get::<Timeline>()
                .expect("Timeline component should exist");

            dbg!(&a_tl.events);

            let b_tl = world
                .entity(b)
                .get::<Timeline>()
                .expect("Timeline component should exist");

            dbg!(&b_tl.events);
            assert_eq!(b_tl.events, a_tl.events);
            assert_eq!(b_tl.future_states.get(&3).unwrap().alive, false);
            assert_eq!(a_tl.future_states.get(&3).unwrap().alive, true);
            assert_eq!(
                a_tl.future_states.get(&3).unwrap().pos,
                Vec2::new(30., 0.)
            );
        }

        eprintln!(
            "\n==========================\nNow we add a control input that \
             avoids the collision\n==========================="
        );
        {
            let world = app.world_mut();
            let mut a = world.entity_mut(a);
            let mut a_tl = a.get_mut::<Timeline>().unwrap();
            a_tl.events.insert(
                2,
                TimelineEvent::Control(ControlInput::SetThrustAndRotation(
                    0.9, PI,
                )),
            );
            a_tl.last_computed_tick = 1;
        }
        app.update();

        {
            let world = app.world();

            // Get the resulting timeline component
            let a_tl = world
                .entity(a)
                .get::<Timeline>()
                .expect("Timeline component should exist");

            dbg!(&a_tl.events);

            let b_tl = world
                .entity(b)
                .get::<Timeline>()
                .expect("Timeline component should exist");

            dbg!(&b_tl.events);
            assert_eq!(a_tl.events.len(), 1);
            assert_eq!(
                a_tl.events.get(&2),
                Some(&TimelineEvent::Control(
                    ControlInput::SetThrustAndRotation(0.9, PI,)
                ))
            );
            assert_eq!(b_tl.future_states.get(&3).unwrap().alive, true);
            assert_eq!(a_tl.future_states.get(&3).unwrap().alive, true);
            assert_approx_eq!(a_tl.future_states.get(&3).unwrap().pos.x, 20.);
            assert_approx_eq!(a_tl.future_states.get(&3).unwrap().pos.y, 0.);
        }
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
        let entity = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(
                create_test_physics_state(),
                Vec2::splat(2.),
            ))
            .id();

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
        let entity = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(
                state,
                Vec2::splat(2.),
                [
                    (10, TimelineEvent::Control(ControlInput::SetThrust(1.0))),
                    (
                        20,
                        TimelineEvent::Control(ControlInput::SetRotation(
                            std::f32::consts::FRAC_PI_2,
                        )),
                    ),
                ],
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
