use super::*;
use crate::prelude::*;

/// Stores scheduled inputs and computed future states for an entity
#[derive(Component, Debug, Clone)]
pub struct Timeline {
    /// Computed physics states for future simulation ticks
    pub future_states: BTreeMap<u64, PhysicsState>,
    /// Ordered list of future control inputs
    /// Future states and sim_events are a function of
    /// prev state and input events
    pub input_events: BTreeMap<u64, ControlInput>,
    /// Ordered list of future sim events
    /// These are created by computing future states
    pub sim_events: BTreeMap<u64, Collision>,
    /// Last tick that has valid computed states
    pub last_computed_tick: u64,
    /// Tick range that was modified most recently
    pub last_updated_range: Option<RangeInclusive<u64>>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            future_states: default(),
            input_events: default(),
            sim_events: default(),
            last_computed_tick: default(),
            last_updated_range: None,
        }
    }
}

impl Timeline {
    pub fn state(&self, tick: u64) -> Option<&PhysicsState> {
        self.future_states.get(&tick)
    }

    pub fn state_mut(&mut self, tick: u64) -> Option<&mut PhysicsState> {
        self.future_states.get_mut(&tick)
    }

    pub fn add_input_event(&mut self, tick: u64, event: ControlInput) {
        self.input_events.insert(tick, event);
        self.last_computed_tick = self.last_computed_tick.min(tick - 1);
    }

    pub fn remove_input_event(
        &mut self,
        tick: u64,
        event: ControlInput,
    ) -> bool {
        let existing = self.input_events.get(&tick);
        if existing != Some(&event) {
            return false;
        }
        self.input_events.remove(&tick);
        self.last_computed_tick = self.last_computed_tick.min(tick - 1);
        true
    }
}

pub fn compute_future_states(
    sim_config: Res<SimulationConfig>,
    mut spatial_index: ResMut<SpatialIndex>,
    mut query: Query<(Entity, &Collider, &mut Timeline)>,
    mut invalid_set: Local<EntityHashMap<u64>>,
) {
    eprintln!("\n\n--------");

    if query.is_empty() {
        warn!("No entities match compute future states");
        return;
    }
    let end_tick = sim_config.current_tick + sim_config.prediction_ticks;
    let seconds_per_tick = 1.0 / sim_config.ticks_per_second as f32;

    // construct map of tick to set{entities | last_updated == tick}
    let mut last_updated_sets = HashMap::<u64, EntityHashSet>::new();
    let mut min_tick = u64::MAX;
    invalid_set.clear();

    for (entity, _, mut timeline) in query.iter_mut() {
        let last_computed_tick = timeline.last_computed_tick;
        last_updated_sets
            .entry(last_computed_tick)
            .or_default()
            .insert(entity);
        min_tick = min_tick.min(timeline.last_computed_tick);
        timeline.last_updated_range = None;
    }

    debug_assert!(
        min_tick >= sim_config.current_tick - 1,
        "min_tick must be >= current tick"
    );

    let mut entities_to_invalidate = Vec::new();
    for tick in (min_tick + 1)..=end_tick {
        // Add entities that were last computed the previous tick
        // (thus invalid this tick)
        if let Some(set) = last_updated_sets.remove(&(tick - 1)) {
            set.into_iter().for_each(|e| {
                invalid_set.entry(e).or_insert(tick);
            });
        }

        // Add pre-dependencies (e.g. elastic beam pairs) to invalid set
        // Note: when more than one sim_event per tick is supported, this must
        // be done iteratively
        for &entity in invalid_set.keys() {
            let (_, _, mut timeline) = query.get_mut(entity).unwrap();
            if let Some(event) = timeline.sim_events.remove(&tick) {
                entities_to_invalidate.push(event.other);
            }
        }
        for entity in entities_to_invalidate.drain(..) {
            let Ok((_, _, mut timeline)) = query.get_mut(entity) else {
                warn!("Entity not found in query");
                continue;
            };
            // Remove sim event from other entity
            timeline.sim_events.remove(&tick);
            // Add to invalid set if not already present
            invalid_set.entry(entity).or_insert(tick);
        }

        // For each in invalid set:
        for &entity in invalid_set.keys() {
            let (_, collider, mut timeline) = query.get_mut(entity).unwrap();
            if timeline.last_updated_range.is_none() {
                timeline.last_updated_range = Some(tick..=end_tick);
            }

            apply_inputs_and_integrate_phys(
                tick,
                seconds_per_tick,
                entity,
                &mut timeline,
                collider,
                Some(&mut spatial_index),
            );
        }

        resolve_collisions(
            tick,
            seconds_per_tick,
            &mut spatial_index,
            &mut query,
            &mut invalid_set,
        );
    }

    for (entity, start_tick) in invalid_set.drain() {
        let (_, _, mut timeline) = query.get_mut(entity).unwrap();
        timeline.last_updated_range = Some(start_tick..=end_tick);
    }
}

pub fn apply_inputs_and_integrate_phys(
    tick: u64,
    seconds_per_tick: f32,
    entity: Entity,
    timeline: &mut Timeline,
    collider: &Collider,
    spatial_index: Option<&mut SpatialIndex>,
) {
    // clear sim events since these should be regenerated
    timeline.sim_events.remove(&tick);

    let mut state = timeline
        .state(tick - 1)
        .expect(
            "Previous tick's state must exist bc of last_updated_sets \
             invariant",
        )
        .clone();

    let event = timeline.input_events.get(&tick);

    // Apply control input events
    state.apply_input_event(event);

    // Integrate physics
    state = state.integrate(seconds_per_tick);

    if state.alive {
        if let Some(spatial_index) = spatial_index {
            spatial_index.insert(
                tick,
                collider,
                SpatialItem::from_state(entity, &state),
            );
        }
    }
    timeline.future_states.insert(tick, state);
    timeline.last_computed_tick = tick;
}

fn resolve_collisions(
    tick: u64,
    seconds_per_tick: f32,
    spatial_index: &mut SpatialIndex,
    query: &mut Query<(Entity, &Collider, &mut Timeline)>,
    invalid_set: &mut EntityHashMap<u64>,
) {
    // Gather collision pairs
    let mut collisions: HashSet<InteractionGroup> = default();
    for &entity in invalid_set.keys() {
        let (_, collider, timeline) = query.get(entity).unwrap();
        let state = timeline.state(tick).expect("Just added");

        if let Some(collision) =
            spatial_index.collides(entity, tick, state.pos, collider)
        {
            collisions.insert((collision.1.entity, entity).into());
        };
    }

    // Resolve broad-phase collisions
    for group in collisions {
        let [mut a, mut b] = match query.get_many_mut(group.0) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("{e:?}");
                panic!("whoops");
            }
        };

        resolve_collision(
            tick,
            (a.0, a.1, &mut a.2),
            (b.0, b.1, &mut b.2),
            seconds_per_tick,
            spatial_index,
        );

        // All collision participants are invalidated
        group.0.into_iter().for_each(|e| {
            invalid_set.entry(e).or_insert(tick);
        });
    }
}

fn resolve_collision(
    tick: u64,
    (a_e, a_col, a_tl): (Entity, &Collider, &mut Timeline),
    (b_e, b_col, b_tl): (Entity, &Collider, &mut Timeline),
    seconds_per_tick: f32,
    spatial_index: &mut SpatialIndex,
) {
    // STEP 1: unpack state
    let a_st = a_tl.future_states.get_mut(&tick).unwrap();
    let b_st = b_tl.future_states.get_mut(&tick).unwrap();

    // STEP 2: check for interaction
    if let Some(_) = spatial_index.collides(a_e, tick, a_st.pos, a_col) {
        // STEP 3: resolve interaction
        let (a_result, b_result) = calculate_collision_result(
            &SpatialItem::from_state(a_e, a_st),
            &SpatialItem::from_state(b_e, b_st),
        );

        a_st.apply_collision_result(&a_result);
        b_st.apply_collision_result(&b_result);

        match &a_result {
            EntityCollisionResult::Destroyed => {
                spatial_index.remove(tick, &a_e)
            }
            EntityCollisionResult::Survives { .. } => {
                spatial_index.insert(
                    tick,
                    a_col,
                    SpatialItem::from_state(a_e, a_st),
                );
            }
        }
        match &b_result {
            EntityCollisionResult::Destroyed => {
                spatial_index.remove(tick, &b_e)
            }
            EntityCollisionResult::Survives { .. } => {
                spatial_index.insert(
                    tick,
                    a_col,
                    SpatialItem::from_state(b_e, b_st),
                );
            }
        }

        a_tl.sim_events.insert(tick, Collision { other: b_e });
        b_tl.sim_events.insert(tick, Collision { other: a_e });

        a_tl.last_computed_tick = tick;
        b_tl.last_computed_tick = tick;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct InteractionGroup(pub [Entity; 2]);

impl From<(Entity, Entity)> for InteractionGroup {
    fn from((e1, e2): (Entity, Entity)) -> Self {
        Self([e1.min(e2), e2.max(e1)])
    }
}

#[cfg(test)]
mod tests {
    use std::{f32::consts::PI, time::Duration};

    use assertables::{assert_abs_diff_le_x, assert_approx_eq};
    use bevy::{app::App, time::Time};

    use super::{test_utils::*, *};
    use crate::states_eq;

    fn spawn_entity_with_states(
        world: &mut World,
        dim: Vec2,
        states: impl IntoIterator<Item = (u64, PhysicsState)>,
        events: impl IntoIterator<Item = (u64, ControlInput)>,
    ) -> Entity {
        let collider = Collider(BRect::from_corners(-dim / 2., dim / 2.));
        let mut timeline = Timeline {
            future_states: BTreeMap::from_iter(states),
            input_events: BTreeMap::from_iter(events),
            ..default()
        };

        if let Some((tick, _)) = timeline.future_states.last_key_value() {
            timeline.last_computed_tick = *tick;
        }

        let entity = world
            .spawn(PhysicsBundle {
                state: timeline
                    .future_states
                    .last_key_value()
                    .unwrap()
                    .1
                    .clone(),
                timeline: timeline.clone(),
                collider,
            })
            .id();

        let mut spatial_index = world.resource_mut::<SpatialIndex>();
        for (tick, state) in timeline.future_states.iter() {
            spatial_index.insert(
                *tick,
                &collider,
                SpatialItem::from_state(entity, state),
            );
        }
        entity
    }

    #[test]
    fn test_simple() {
        let mut app = App::new();
        app.init_resource::<SpatialIndex>()
            .insert_resource(SimulationConfig {
                current_tick: 1,
                prediction_ticks: 3,
                ..TEST_CONFIG
            })
            .add_systems(Update, compute_future_states);

        let dim = Vec2::splat(2.);

        let a_st = TestStateBuilder::new().vel(10., 0.).mass(1.).build();
        let a = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(a_st.clone(), dim, 0, []))
            .id();

        let b_st = TestStateBuilder::new().pos(30., 0.).mass(9.).build();
        let b = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(b_st.clone(), dim, 0, []))
            .id();

        app.update();

        let a_tl = app.world().entity(a).get::<Timeline>().unwrap();
        let b_tl = app.world().entity(b).get::<Timeline>().unwrap();

        fn s<'a>(tl: &'a Timeline, tick: u64) -> &'a PhysicsState {
            tl.state(tick).unwrap()
        }

        assert_eq!(a_tl.last_updated_range, Some(1..=4));
        assert_eq!(b_tl.last_updated_range, Some(1..=4));

        states_eq!(s(a_tl, 0), a_st.b().pos(0., 0.).b());
        states_eq!(s(a_tl, 1), a_st.b().pos(10., 0.).b());
        states_eq!(s(a_tl, 2), a_st.b().pos(20., 0.).b());
        states_eq!(s(a_tl, 3), a_st.b().pos(30., 0.).alive(false).b());

        states_eq!(s(b_tl, 0), b_st.b().b());
        states_eq!(s(b_tl, 1), b_st.b().b());
        states_eq!(s(b_tl, 2), b_st.b().b());
        states_eq!(s(b_tl, 3), b_st.b().vel(1., 0.).b());
        states_eq!(s(b_tl, 4), b_st.b().pos(31., 0.).vel(1., 0.).b());
    }

    #[test]
    fn test_collision_invalidation_from_input() {
        let mut app = App::new();
        app.init_resource::<SpatialIndex>()
            .insert_resource(SimulationConfig {
                current_tick: 1,
                prediction_ticks: 4,
                ..TEST_CONFIG
            })
            .add_systems(Update, compute_future_states);

        let dim = Vec2::splat(2.);

        // Craft A starts at origin, moving right
        let a_st = TestStateBuilder::new().vel(10., 0.).mass(1.).build();
        let a = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(a_st.clone(), dim, 0, []))
            .id();

        // Craft B starts at x=30, stationary
        let b_st = TestStateBuilder::new().pos(30., 0.).mass(9.).build();
        let b = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(b_st.clone(), dim, 0, []))
            .id();

        // First update - should compute collision at tick 3
        app.update();

        // Verify initial collision state
        let a_tl = app.world().entity(a).get::<Timeline>().unwrap();
        let b_tl = app.world().entity(b).get::<Timeline>().unwrap();

        assert!(a_tl.sim_events.contains_key(&3));
        assert!(b_tl.sim_events.contains_key(&3));
        assert!(!a_tl.state(3).unwrap().alive);

        // Add input at tick 2 that changes A's trajectory
        let world = app.world_mut();
        let mut entity = world.entity_mut(a);
        let mut a_tl = entity.get_mut::<Timeline>().unwrap();
        a_tl.add_input_event(2, ControlInput::SetThrustAndRotation(1., PI));

        // Second update - should recompute and remove collision
        app.update();

        let a_tl = app.world().entity(a).get::<Timeline>().unwrap();
        let b_tl = app.world().entity(b).get::<Timeline>().unwrap();

        // Verify collision was removed from both timelines
        assert!(!a_tl.sim_events.contains_key(&3));
        assert!(!b_tl.sim_events.contains_key(&3));

        // Verify A is still alive (didn't collide) and B was recomputed
        assert!(a_tl.state(3).unwrap().alive);
        assert_eq!(b_tl.last_updated_range, Some(3..=5));
    }

    #[test]
    fn test_collision_invalidates() {
        let mut app = App::new();
        app.init_resource::<SpatialIndex>()
            .insert_resource(SimulationConfig {
                current_tick: 1,
                prediction_ticks: 4,
                ..TEST_CONFIG
            })
            .add_systems(Update, compute_future_states);

        let dim = Vec2::splat(2.);

        let a_st = TestStateBuilder::new().vel(10., 0.).mass(1.).build();
        let a = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(a_st.clone(), dim, 0, []))
            .id();

        let b_st = TestStateBuilder::new().pos(30., 0.).mass(9.).build();
        let b = spawn_entity_with_states(
            app.world_mut(),
            dim,
            [
                (0, b_st.clone()),
                (1, b_st.clone()),
                (2, b_st.clone()),
                (3, b_st.clone()),
                (4, b_st.clone()),
            ],
            [],
        );

        app.update();

        let a_tl = app.world().entity(a).get::<Timeline>().unwrap();
        let b_tl = app.world().entity(b).get::<Timeline>().unwrap();

        fn s<'a>(tl: &'a Timeline, tick: u64) -> &'a PhysicsState {
            tl.future_states.get(&tick).unwrap()
        }

        assert_eq!(a_tl.last_updated_range, Some(1..=5));
        assert_eq!(b_tl.last_updated_range, Some(3..=5));

        states_eq!(s(a_tl, 0), a_st.b().pos(0., 0.).b());
        states_eq!(s(a_tl, 1), a_st.b().pos(10., 0.).b());
        states_eq!(s(a_tl, 2), a_st.b().pos(20., 0.).b());
        states_eq!(s(a_tl, 3), a_st.b().pos(30., 0.).alive(false).b());

        states_eq!(s(b_tl, 0), b_st.b().b());
        states_eq!(s(b_tl, 1), b_st.b().b());
        states_eq!(s(b_tl, 2), b_st.b().b());
        states_eq!(s(b_tl, 3), b_st.b().vel(1., 0.).b());
        states_eq!(s(b_tl, 4), b_st.b().pos(31., 0.).vel(1., 0.).b());
    }

    #[test]
    fn test_input_events() {
        let mut app = App::new();
        let config = SimulationConfig {
            current_tick: 1,
            ..TEST_CONFIG
        };
        app.insert_resource(SpatialIndex::default())
            .insert_resource(config.clone())
            .add_systems(Update, compute_future_states);
        let end_tick = config.current_tick + config.prediction_ticks;

        let dim = Vec2::splat(2.);

        let a_st = TestStateBuilder::new().vel(10., 0.).mass(1.).build();
        let a = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(
                a_st.clone(),
                dim,
                0,
                [
                    (1, ControlInput::SetThrustAndRotation(1., PI)),
                    (3, ControlInput::SetThrustAndRotation(1., PI / 2.)),
                ],
            ))
            .id();

        let b_st = TestStateBuilder::new().pos(30., 0.).mass(9.).build();
        let b = app
            .world_mut()
            .spawn(PhysicsBundle::new_with_events(b_st.clone(), dim, 0, []))
            .id();

        app.update();

        let a_tl = app.world().entity(a).get::<Timeline>().unwrap();
        let b_tl = app.world().entity(b).get::<Timeline>().unwrap();

        fn s<'a>(tl: &'a Timeline, tick: u64) -> &'a PhysicsState {
            tl.future_states.get(&tick).unwrap()
        }

        assert_eq!(
            a_tl.last_updated_range,
            Some(config.current_tick..=end_tick)
        );
        assert_eq!(
            b_tl.last_updated_range,
            Some(config.current_tick..=end_tick)
        );

        states_eq!(s(a_tl, 0), a_st.b().pos(0., 0.).b());
        states_eq!(
            s(a_tl, 1),
            PhysicsState {
                pos: (10., 0.).into(),
                vel: (-90., 0.).into(),
                current_thrust: 1.0,
                rotation: PI,
                ..a_st
            }
        );
        states_eq!(
            s(a_tl, 2),
            PhysicsState {
                pos: (-80., 0.).into(),
                vel: (-190., 0.).into(),
                current_thrust: 1.0,
                rotation: PI,
                ..a_st
            }
        );
        states_eq!(
            s(a_tl, 3),
            PhysicsState {
                pos: (-270., 0.).into(),
                vel: (-190., 100.).into(),
                current_thrust: 1.0,
                rotation: PI / 2.,
                ..a_st
            }
        );

        states_eq!(s(b_tl, 0), b_st.b().b());
        states_eq!(s(b_tl, 1), b_st.b().b());
        states_eq!(s(b_tl, 2), b_st.b().b());
        states_eq!(s(b_tl, 3), b_st.b().vel(0., 0.).b());
    }
}
