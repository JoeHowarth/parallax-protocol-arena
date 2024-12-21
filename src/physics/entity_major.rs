use super::*;
use crate::prelude::*;

/// Stores scheduled inputs and computed future states for an entity
#[derive(Component, Default, Debug, Clone)]
struct Timeline {
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
}

fn compute_future_states_2(
    mut sim_config: ResMut<SimulationConfig>,
    mut spatial_index: ResMut<SpatialIndex>,
    mut query: Query<(Entity, &Collider, &mut Timeline)>,
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

    for (entity, _, timeline) in query.iter() {
        last_updated_sets
            .entry(timeline.last_computed_tick)
            .or_default()
            .insert(entity);
        min_tick = min_tick.min(timeline.last_computed_tick);
    }

    // set invalid set to min tick in map
    debug_assert!(
        min_tick >= sim_config.current_tick - 1,
        "min_tick must be >= current tick"
    );
    dbg!(&last_updated_sets);
    let mut invalid_set = last_updated_sets.remove(&min_tick).unwrap();

    for tick in (min_tick + 1)..=end_tick {
        // add entities that were last computed the previous tick
        // (thus invalid this tick)
        if let Some(set) = last_updated_sets.remove(&(tick - 1)) {
            invalid_set.extend(set);
        }
        eprintln!("tick {tick}, &invalid_set: {:?}", &invalid_set);

        // add pre-dependencies (e.g. elastic beam pairs) to invalid set

        // for each in invalid set:
        for &entity in &invalid_set {
            let Ok((_, collider, mut timeline)) = query.get_mut(entity) else {
                panic!("whoops");
            };

            // clear sim events since these should be regenerated
            timeline.sim_events.remove(&tick);

            let mut state = timeline
                .state_mut(tick - 1)
                .expect(
                    "Previous tick's state must exist bc of last_updated_sets \
                     invariant",
                )
                .clone();

            let event = timeline.input_events.get(&tick);

            // - apply control input events
            state.apply_input_event(event);

            // - integrate physics
            state = state.integrate(seconds_per_tick);

            spatial_index.insert(
                tick,
                collider,
                SpatialItem::from_state(entity, &state),
            );
            timeline.future_states.insert(tick, state);
            timeline.last_computed_tick = tick;
        }

        // gather collision pairs
        let mut collisions: HashSet<InteractionGroup> = default();
        for &entity in &invalid_set {
            let (_, collider, timeline) = query.get(entity).unwrap();
            let state = timeline.state(tick).expect("Just added");

            if let Some(collision) =
                spatial_index.collides(entity, tick, state.pos, collider)
            {
                collisions.insert((collision.1.entity, entity).into());
            };
        }

        // resolve broad-phase collisions
        for group in collisions {
            let [mut a, mut b] = match query.get_many_mut(group.0) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("{e:?}");
                    panic!("whoops");
                }
            };

            resolve_collisions(
                tick,
                (a.0, a.1, &mut a.2),
                (b.0, b.1, &mut b.2),
                seconds_per_tick,
                &mut spatial_index,
            );

            // all collision participants are invalidated
            invalid_set.extend(group.0);
        }
    }
}

fn resolve_collisions(
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

        // TODO: rethink why we're storing this
        let collision = Collision {
            tick,
            this: a_e,
            this_result: a_result,
            other: b_e,
            other_result: b_result,
        };

        a_tl.sim_events.insert(tick, collision.clone());
        b_tl.sim_events.insert(tick, collision);

        spatial_index.insert(tick, a_col, SpatialItem::from_state(a_e, a_st));
        spatial_index.insert(tick, b_col, SpatialItem::from_state(b_e, b_st));

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

    #[derive(Bundle)]
    struct PhysicsBundle {
        pub state: PhysicsState,
        pub timeline: Timeline,
        pub collider: Collider,
    }

    impl PhysicsBundle {
        fn from_state(state: PhysicsState, dim: Vec2) -> PhysicsBundle {
            let collider = Collider(BRect::from_corners(-dim / 2., dim / 2.));
            PhysicsBundle {
                state,
                timeline: Timeline::default(),
                collider,
            }
        }

        fn new_with_events(
            state: PhysicsState,
            dim: Vec2,
            state_tick: u64,
            events: impl IntoIterator<Item = (u64, ControlInput)>,
        ) -> PhysicsBundle {
            let mut bundle = PhysicsBundle::from_state(state, dim);
            bundle.timeline.input_events.extend(events);
            bundle
                .timeline
                .future_states
                .insert(state_tick, bundle.state.clone());
            bundle.timeline.last_computed_tick = state_tick;
            bundle
        }
    }

    fn spawn_entity_with_states(
        world: &mut World,
        dim: Vec2,
        states: impl IntoIterator<Item = (u64, PhysicsState)>,
        events: impl IntoIterator<Item = (u64, ControlInput)>,
    ) -> Entity {
        let collider = Collider(BRect::from_corners(-dim / 2., dim / 2.));
        // let mut states = states.peekable();
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
            .add_systems(Update, compute_future_states_2);

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
            tl.future_states.get(&tick).unwrap()
        }

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
    fn test_collision_invalidates() {
        let mut app = App::new();
        app.init_resource::<SpatialIndex>()
            .insert_resource(SimulationConfig {
                current_tick: 1,
                prediction_ticks: 4,
                ..TEST_CONFIG
            })
            .add_systems(Update, compute_future_states_2);

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
        app.insert_resource(SpatialIndex::default())
            .insert_resource(SimulationConfig {
                current_tick: 1,
                ..TEST_CONFIG
            })
            .add_systems(Update, compute_future_states_2);

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
