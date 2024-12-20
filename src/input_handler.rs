use std::collections::VecDeque;

use bevy::{color::palettes::css, math::vec2};
use physics::{
    collisions::{Collider, SpatialIndex},
    ControlInput,
    SimulationConfig,
    TimelineEventRequest,
};
use trajectory::{TrajectoryPreview, TrajectorySegment};

use crate::prelude::*;

pub struct InputHandlerPlugin;

#[derive(Resource, Deref, DerefMut, Reflect)]
struct SelectedCraft(pub Entity);

impl Plugin for InputHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SelectedCraft>()
            .insert_resource(ScreenLenToWorld(1.))
            .insert_resource(InputMode::ThrustAndRotation)
            .add_systems(
                FixedPostUpdate,
                GenericSparseTimeline::<TimelineEventMarker>::clear_system,
            )
            .add_systems(PreUpdate, calc_screen_length_to_world)
            .add_systems(
                Update,
                (
                    (
                        handle_input_mode,
                        (handle_engine_input.run_if(|mode: Res<InputMode>| {
                            matches!(*mode, InputMode::ThrustAndRotation)
                        })),
                        render_timeline_events,
                    )
                        .chain(),
                    time_dilation_control,
                ),
            );
    }
}

// #[derive(Default)]
// enum SelectionState {
//     #[default]
//     Default,
//     Target(
//         Box<
//             dyn FnOnce(Entity, &mut Commands, &mut SelectionState)
//                 + 'static
//                 + Send,
//         >,
//     ),
// }

#[derive(Resource, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    ThrustAndRotation,
    PlasmaCannon,
}

fn handle_input_mode(
    mut input_mode: ResMut<InputMode>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    for pressed in keys.get_just_pressed() {
        match pressed {
            KeyCode::Digit1 => *input_mode = InputMode::ThrustAndRotation,
            KeyCode::Digit2 => *input_mode = InputMode::PlasmaCannon,
            _ => {}
        }
    }
}

fn handle_plasma_cannon_mode() {}

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

fn handle_engine_input(
    mut drag_start_r: EventReader<Pointer<DragStart>>,
    mut drag_end_r: EventReader<Pointer<DragEnd>>,
    mut drag_r: EventReader<Pointer<Drag>>,
    segments: Query<&TrajectorySegment>,
    timelines: Query<(&Collider, &Timeline)>,
    mut preview: Option<ResMut<TrajectoryPreview>>,
    mut timeline_event_writer: EventWriter<TimelineEventRequest>,
    mut commands: Commands,
    simulation_config: Res<SimulationConfig>,
    spatial_index: Res<SpatialIndex>,
) {
    for drag_start in drag_start_r.read() {
        println!("Drag start");
        let Ok(seg) = segments.get(drag_start.target) else {
            warn!("Segment being dragged doesn't exist");
            continue;
        };
        let Ok((_, timeline)) = timelines.get(seg.craft_entity) else {
            warn!("Timeline for craft being dragged doesn't exist");
            continue;
        };

        // Create preview timeline starting from segment's end tick
        commands.insert_resource(TrajectoryPreview {
            entity: seg.craft_entity,
            start_tick: seg.end_tick,
            timeline: Timeline {
                events: timeline.events.clone(),
                future_states: BTreeMap::from_iter(
                    timeline
                        .future_states
                        .range(0..=seg.end_tick)
                        .map(|(k, v)| (k.clone(), v.clone())),
                ),
                // TODO: is this right??
                last_computed_tick: seg.start_tick,
            },
        });

        info!(pos = ?drag_start.pointer_location.position, "Drag start");
    }

    for drag in drag_r.read() {
        println!("Drag continue");
        let Ok(seg) = segments.get(drag.target) else {
            info!("Drag segment doesn't exist");
            continue;
        };
        let Some(preview) = preview.as_mut() else {
            info!("Preview doesn't exist");
            continue;
        };
        let craft_entity = seg.craft_entity;

        // convert to world orientation
        let mut world_drag = drag.distance;
        world_drag.y *= -1.;

        let (collider, _) = timelines.get(seg.craft_entity).unwrap();

        // Patch preview timeline
        preview.timeline.events.insert(
            seg.end_tick,
            TimelineEvent::Control(ControlInput::SetThrustAndRotation(
                (world_drag.length() / 50.).min(1.),
                world_drag.to_angle(),
            )),
        );
        preview.timeline.last_computed_tick = preview.start_tick - 1;

        preview.timeline.lookahead(
            craft_entity,
            simulation_config.current_tick,
            1.0 / simulation_config.ticks_per_second as f32,
            simulation_config.prediction_ticks,
            collider,
            &spatial_index,
        );
        info!("drag loop over");
    }

    for drag_end in drag_end_r.read() {
        let Ok(seg) = segments.get(drag_end.target) else {
            info!("Drag target no longer exists, removing preview...");
            commands.remove_resource::<TrajectoryPreview>();
            continue;
        };
        // convert to world orientation
        let mut world_drag = drag_end.distance;
        world_drag.y *= -1.;

        info!(
            ?world_drag,
            len = world_drag.length(),
            angle = world_drag.to_angle(),
            ?seg,
            "Drag end"
        );

        // Send the actual timeline events
        timeline_event_writer.send(TimelineEventRequest {
            entity: seg.craft_entity,
            tick: seg.end_tick,
            input: ControlInput::SetThrustAndRotation(
                (world_drag.length() / 50.).min(1.),
                world_drag.to_angle(),
            ),
        });

        // Remove preview
        commands.remove_resource::<TrajectoryPreview>();
    }
}

#[derive(Component)]
struct TimelineEventMarker {
    tick: u64,
    craft: Entity,
    input: TimelineEvent,
}

#[derive(Component, Debug)]
pub struct GenericSparseTimeline<C> {
    pub map: BTreeMap<u64, C>,
}

impl<C: Send + Sync + 'static> GenericSparseTimeline<C> {
    pub fn insert(&mut self, tick: u64, c: C) -> Option<C> {
        self.map.insert(tick, c)
    }

    pub fn get(&self, tick: u64) -> Option<&C> {
        self.map.get(&tick)
    }

    pub fn get_mut(&mut self, tick: u64) -> Option<&mut C> {
        self.map.get_mut(&tick)
    }

    pub fn clear_system(
        mut query: Query<&mut GenericSparseTimeline<C>>,
        sim_config: Res<SimulationConfig>,
    ) {
        for mut timeline in query.iter_mut() {
            timeline.map.retain(|k, v| *k >= sim_config.current_tick);
        }
    }
}

#[derive(Default)]
pub struct GenericDenseTimeline<C> {
    pub base_tick: u64,
    pub ring: VecDeque<C>,
}

impl<C> GenericDenseTimeline<C> {
    pub fn push(&mut self, c: C) {
        self.ring.push_back(c);
    }

    pub fn get(&self, tick: u64) -> Option<&C> {
        self.ring.get((tick - self.base_tick) as usize)
    }

    pub fn get_mut(&mut self, tick: u64) -> Option<&mut C> {
        self.ring
            .get_mut((tick.checked_sub(self.base_tick)?) as usize)
    }

    /// Sets the value at tick to val if within domain returning
    /// existing val, otherwise returns None
    pub fn set(&mut self, mut val: C, tick: u64) -> Option<C> {
        let idx = (tick.checked_sub(self.base_tick)?) as usize;
        if idx < self.ring.len() {
            let old = self.ring.get_mut(idx)?;
            std::mem::swap(old, &mut val);
            return Some(val);
        } else {
            None
        }
    }
}

fn sync_timeline_markers(
    mut commands: Commands,
    mut timelines: Query<(
        Entity,
        &Timeline,
        &mut GenericSparseTimeline<Entity>,
    )>,
    mut markers: Query<(Entity, &mut TimelineEventMarker)>,
) {
    // ensure marker exists for each event in timeline
    for (craft_entity, timeline, mut marker_entity_timeline) in
        timelines.iter_mut()
    {
        for (&tick, event) in timeline.events.iter() {
            if let Some(marker_e) = marker_entity_timeline.get(tick) {
                continue;
            };
            let Some(phys) = timeline.future_states.get(&tick) else {
                error!("Bad");
                panic!("bad!");
            };

            let marker_e = commands
                .spawn((
                    TimelineEventMarker {
                        tick,
                        craft: craft_entity,
                        input: event.clone(),
                    },
                    Sprite::from_color(css::OLIVE, vec2(1., 1.)),
                    Transform::from_translation(phys.pos.to3()),
                ))
                .id();
            marker_entity_timeline.insert(tick, marker_e);
        }
    }

    for (marker_e, mut marker) in 
}

fn render_timeline_events(
    mut commands: Commands,
    timeline_events: Query<(Entity, &Timeline)>,
    mut painter: ShapePainter,
    screen_len_to_world: Res<ScreenLenToWorld>,
) {
    for (craft_e, timeline) in timeline_events.iter() {
        //
        for (tick, event) in &timeline.events {
            let Some(phys) = timeline.future_states.get(tick) else {
                warn!(
                    entity = ?craft_e,
                    tick,
                    "Each timeline event must have a corresponding state"
                );
                continue;
            };
            let rect_shaft = Vec2::new(5., 20.);

            render_base_and_arrow(
                phys.pos,
                rect_shaft,
                craft_e,
                &mut painter,
                &screen_len_to_world,
            );
        }
    }
}

fn render_base_and_arrow(
    pos: Vec2,
    rect_shaft: Vec2,
    craft_entity: Entity,
    painter: &mut ShapePainter,
    screen_len_to_world: &f32,
) {
    let px = screen_len_to_world.sqrt();
    // let px = 1.;
    painter.set_translation(pos.to3());
    painter.set_color(css::OLIVE);

    painter.circle(rect_shaft.x * 1.5 * px);

    painter.set_color(css::GREEN);
    painter.translate(Vec3::from2((0., rect_shaft.y / 2.)) * px);
    painter.rect(rect_shaft * px);
    painter.translate(Vec3::from2((0., rect_shaft.y / 2.)) * px);
    painter.triangle(
        Vec2::new(-4. / 3. * rect_shaft.x, -rect_shaft.y / 6.) * px,
        Vec2::new(0., rect_shaft.y / 4.) * px,
        Vec2::new(0., 0.) * px,
    );
    painter.triangle(
        Vec2::new(4. / 3. * rect_shaft.x, -rect_shaft.y / 6.) * px,
        Vec2::new(0., rect_shaft.y / 4.) * px,
        Vec2::new(0., 0.) * px,
    );
}

#[derive(Resource, Deref)]
pub struct ScreenLenToWorld(pub f32);

fn calc_screen_length_to_world(
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut screen_length_to_world: ResMut<ScreenLenToWorld>,
) {
    let (camera, camera_transform) = camera_q.single();
    let world_diff = camera
        .viewport_to_world_2d(camera_transform, Vec2::new(1., 0.))
        .unwrap()
        - camera
            .viewport_to_world_2d(camera_transform, Vec2::new(0., 0.))
            .unwrap();
    screen_length_to_world.0 = world_diff.x;
    println!("screen_len_to_world: {}", world_diff.x);
}
