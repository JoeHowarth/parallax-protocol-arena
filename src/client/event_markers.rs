use super::{EntityTimeline, ScreenLenToWorld};
use crate::{
    client::trajectory::TrajectoryPreview,
    physics::{
        ControlInput,
        SimulationConfig,
        TimelineEventRemovalRequest,
        TimelineEventRequest,
    },
    prelude::*,
};

#[derive(Default, Clone, Copy)]
pub struct EventMarkerPlugin;

#[derive(Component, Clone, Reflect)]
pub struct TimelineEventMarker {
    tick: u64,
    craft: Entity,
    input: ControlInput,
    pos: Vec2,
    rot: f32,
}

impl Plugin for EventMarkerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<TimelineEventMarker>()
            .add_systems(PreUpdate, add_marker_map)
            .add_systems(
                FixedPostUpdate,
                (
                    EntityTimeline::<TimelineEventMarker>::clear_system,
                    sync_timeline_markers,
                ),
            )
            .add_systems(Update, render_timeline_events);
    }
}

type MarkerEntityTimeline = EntityTimeline<TimelineEventMarker>;

fn add_marker_map(
    mut commands: Commands,
    timelines: Query<
        Entity,
        (Without<EntityTimeline<TimelineEventMarker>>, With<Timeline>),
    >,
) {
    for entity in timelines.iter() {
        commands
            .entity(entity)
            .insert(MarkerEntityTimeline::default());
    }
}

#[derive(Component)]
struct Old<C>(pub C);

impl TimelineEventMarker {
    pub fn bundle(
        phys: &PhysicsState,
        craft: Entity,
        input: ControlInput,
        tick: u64,
    ) -> impl Bundle {
        (
            TimelineEventMarker {
                tick,
                craft,
                input,
                pos: phys.pos,
                rot: phys.rotation,
            },
            Sprite::from_color(Srgba::new(0.1, 0.1, 0.1, 0.9), vec2(1., 1.)),
            Transform::from_translation(phys.pos.extend(10.)),
        )
    }
}

/// Sync timeline events with timeline event marker entities
/// Should be run after simulation update
fn sync_timeline_markers(
    mut commands: Commands,
    mut timelines: Query<(
        Entity,
        &Timeline,
        &mut EntityTimeline<TimelineEventMarker>,
    )>,
    mut markers: Query<(Entity, &mut TimelineEventMarker)>,
    mut alive: Local<EntityHashSet>,
) {
    alive.clear();

    // ensure marker exists for each event in timeline
    for (craft_entity, timeline, mut marker_entity_timeline) in
        timelines.iter_mut()
    {
        for (&tick, input) in timeline.input_events.iter() {
            let mut spawn =
                |marker_entity_timeline: &mut MarkerEntityTimeline| {
                    let Some(phys) = timeline.future_states.get(&tick) else {
                        warn!(
                            "Trying to create event marker entity w/o state \
                             for tick"
                        );
                        return;
                    };

                    let mut entity_commands =
                        commands.spawn(TimelineEventMarker::bundle(
                            phys,
                            craft_entity,
                            input.clone(),
                            tick,
                        ));

                    // add click handlers if
                    // event is a control event
                    configure_marker_observers(
                        craft_entity,
                        input.clone(),
                        &mut entity_commands,
                    );
                    let marker_e = entity_commands.id();
                    alive.insert(marker_e);
                    marker_entity_timeline.insert(tick, marker_e);
                };

            let Some(marker_e) = marker_entity_timeline.get(tick) else {
                spawn(&mut marker_entity_timeline);
                continue;
            };

            let Ok((_, mut marker)) = markers.get_mut(*marker_e) else {
                spawn(&mut marker_entity_timeline);
                continue;
            };

            alive.insert(*marker_e);
            let Some(phys) = timeline.future_states.get(&tick) else {
                warn!("Event marker exists, but state does not");
                panic!("Event marker exists, but state does not");
            };
            if marker.input != *input {
                marker.input = input.clone();
                marker.pos = phys.pos;
                marker.rot = phys.rotation;
            }
        }
    }

    // garbage collect markers that don't correspond to an event
    for (marker_e, _) in markers.iter() {
        if !alive.contains(&marker_e) {
            info!("Removing marker");
            commands.entity(marker_e).despawn_recursive();
        }
    }
}

fn configure_marker_observers(
    craft_entity: Entity,
    input: ControlInput,
    cmds: &mut EntityCommands,
) {
    cmds.observe(
        move |mut trigger: Trigger<Pointer<Click>>,
              markers: Query<&TimelineEventMarker>,
              mut removals: EventWriter<TimelineEventRemovalRequest>| {
            // Get the underlying event type
            let click_event: &Pointer<Click> = trigger.event();
            if click_event.event.button == PointerButton::Secondary {
                info!("Got right click on marker, sending removal request...");
                removals.send(TimelineEventRemovalRequest {
                    input,
                    entity: craft_entity,
                    tick: markers.get(trigger.entity()).unwrap().tick,
                });
            }
            trigger.propagate(false);
        },
    );
    cmds.observe(
        move |mut trigger: Trigger<Pointer<DragStart>>,
              mut commands: Commands,
              markers: Query<&TimelineEventMarker>,
              timelines: Query<&Timeline>,
              sim_config: Res<SimulationConfig>| {
            trigger.propagate(false);

            let marker = markers.get(trigger.entity()).unwrap();
            let tick = marker.tick;
            let start_tick =
                sim_config.current_tick.max(tick.saturating_sub(10));
            let timeline = timelines
                .get(craft_entity)
                .expect("Craft entity must have timeline");

            let last_computed_tick =
                sim_config.current_tick.max(tick.saturating_sub(10));
            commands
                .entity(trigger.entity())
                .insert(Old(marker.clone()));
            commands.insert_resource(TrajectoryPreview {
                entity: craft_entity,
                start_tick: last_computed_tick,
                timeline: Timeline {
                    input_events: timeline.input_events.clone(),
                    sim_events: default(),
                    future_states: BTreeMap::from_iter(
                        timeline
                            .future_states
                            .range(0..=last_computed_tick)
                            .map(|(k, v)| (k.clone(), v.clone())),
                    ),
                    last_computed_tick,
                    last_updated_range: None,
                },
            });
        },
    );
    cmds.observe(
        move |mut trigger: Trigger<Pointer<Drag>>,
              mut preview: ResMut<TrajectoryPreview>,
              camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
              mut markers: Query<&mut TimelineEventMarker>,
              simulation_config: Res<SimulationConfig>,
              sim_config: Res<SimulationConfig>| {
            trigger.propagate(false);

            // let (timeline, mut tick_to_marker_e) = timelines
            //     .get_mut(craft_entity)
            //     .expect("Craft entity must have timeline");

            let (camera, camera_transform) = camera_q.single();
            let Ok(new_marker_pos) = camera.viewport_to_world_2d(
                camera_transform,
                trigger.event().pointer_location.position,
            ) else {
                return;
            };

            let mut marker = markers.get_mut(trigger.entity()).unwrap();
            let old_tick = marker.tick;
            let (new_tick, err_dist) =
                preview.timeline.future_states.iter().fold(
                    (marker.tick, f32::INFINITY),
                    |(best_tick, shortest_dist), (tick, phys)| {
                        let dist = phys.pos.distance_squared(new_marker_pos);
                        if dist < shortest_dist {
                            (*tick, dist)
                        } else {
                            (best_tick, shortest_dist)
                        }
                    },
                );

            preview.timeline.input_events.remove(&old_tick);
            preview
                .timeline
                .input_events
                .insert(new_tick, marker.input.clone());

            // preview.timeline.lookahead(
            //     craft_entity,
            //     simulation_config.current_tick,
            //     1.0 / simulation_config.ticks_per_second as f32,
            //     simulation_config.prediction_ticks,
            // );

            let phys = preview.timeline.future_states.get(&new_tick).unwrap();
            marker.tick = new_tick;
            marker.pos = phys.pos;
            marker.rot = phys.rotation;

            preview.timeline.last_computed_tick = (new_tick.min(old_tick)) - 1;
        },
    );
    cmds.observe(
        move |mut trigger: Trigger<Pointer<DragEnd>>,
              mut commands: Commands,
              mut timelines: Query<(
            &Timeline,
            &mut EntityTimeline<TimelineEventMarker>,
        )>,
              camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
              mut markers: Query<(
            &mut TimelineEventMarker,
            &Old<TimelineEventMarker>,
        )>,
              sim_config: Res<SimulationConfig>| {
            trigger.propagate(false);

            let (timeline, mut tick_to_marker_e) = timelines
                .get_mut(craft_entity)
                .expect("Craft entity must have timeline");

            let (camera, camera_transform) = camera_q.single();
            let Ok(new_marker_pos) = camera.viewport_to_world_2d(
                camera_transform,
                trigger.event().pointer_location.position,
            ) else {
                return;
            };

            let (marker, old) = markers.get_mut(trigger.entity()).unwrap();
            let tick = old.0.tick;
            // let (new_tick, err_dist) = timeline.future_states.iter().fold(
            //     (marker.tick, f32::INFINITY),
            //     |(best_tick, shortest_dist), (tick, phys)| {
            //         let dist = phys.pos.distance_squared(new_marker_pos);
            //         if dist < shortest_dist {
            //             (*tick, dist)
            //         } else {
            //             (best_tick, shortest_dist)
            //         }
            //     },
            // );

            // let phys = timeline.future_states.get(&new_tick).unwrap();

            debug_assert_eq!(
                tick_to_marker_e.map.remove(&tick),
                Some(trigger.entity())
            );
            // TODO: this is error prone, we should come up with something
            // better abstracted
            // marker.tick = new_tick;
            // marker.pos = phys.pos;
            // marker.rot = phys.rotation;
            tick_to_marker_e.insert(marker.tick, trigger.entity());
            commands.send_event(TimelineEventRemovalRequest {
                input,
                entity: craft_entity,
                tick,
            });
            commands.send_event(TimelineEventRequest {
                input,
                entity: craft_entity,
                tick: marker.tick,
            });
            commands.remove_resource::<TrajectoryPreview>();
        },
    );
}

/// Render event marker entities
fn render_timeline_events(
    mut markers: Query<(&TimelineEventMarker, &mut Sprite, &mut Transform)>,
    mut painter: ShapePainter,
    screen_len_to_world: Res<ScreenLenToWorld>,
) {
    let px = screen_len_to_world.0.sqrt();
    for (marker, mut clickbox, mut transform) in markers.iter_mut() {
        let old_z = transform.translation.z;
        transform.rotation = Quat::from_rotation_z(marker.rot);
        transform.translation.x = marker.pos.x;
        transform.translation.y = marker.pos.y;
        clickbox.custom_size = Some(Vec2::new(14., 14.) * px);

        MarkerVisual::from_event(marker).render(&transform, &mut painter, px);
    }
}

enum MarkerVisual {
    Arrow {
        length: f32,
        relative_rot: f32,
        color: Srgba,
    },
    ArcArrow {
        sweep: f32,
        color: Srgba,
    },
    Cross {
        color: Srgba,
    },
}

impl MarkerVisual {
    fn from_event(event: &TimelineEventMarker) -> Self {
        use ControlInput::*;
        use MarkerVisual::*;
        use TimelineEvent::*;
        match &event.input {
            SetThrust(thrust) => Arrow {
                length: *thrust,
                relative_rot: 0.,
                color: css::PALE_GREEN,
            },
            SetRotation(new_rot) => ArcArrow {
                sweep: (new_rot - event.rot) % (2. * PI),
                color: css::DARK_BLUE,
            },
            SetAngVel(ang_vel) => ArcArrow {
                sweep: (ang_vel - event.rot) % (2. * PI),
                color: css::MIDNIGHT_BLUE,
            },
            SetThrustAndRotation(thrust, new_rot) => Arrow {
                length: *thrust,
                relative_rot: new_rot - event.rot,
                color: css::LIGHT_GREEN,
            },
            // Collision(collision) => Cross { color: css::RED },
        }
    }

    fn render(self, trans: &Transform, painter: &mut ShapePainter, px: f32) {
        painter.set_translation(trans.translation);
        painter.set_rotation(trans.rotation);
        painter.set_color(css::OLIVE);
        painter.circle(6. * px);

        match self {
            MarkerVisual::Arrow {
                length,
                relative_rot,
                color,
            } => {
                let w = 5.;
                let l = length * 20.;
                painter.set_color(color);
                painter.translate(Vec3::from2((l * 0.5, 0.)) * px);
                painter.rect(Vec2::new(l, w) * px);
                painter.translate(Vec3::from2((l * 0.5, 0.)) * px);
                painter.triangle(
                    Vec2::new(-l / 6., 4. / 3. * w) * px,
                    Vec2::new(l / 4., 0.) * px,
                    Vec2::new(0., 0.) * px,
                );
                painter.triangle(
                    Vec2::new(-l / 6., -4. / 3. * w) * px,
                    Vec2::new(l / 4., 0.) * px,
                    Vec2::new(0., 0.) * px,
                );
            }
            MarkerVisual::ArcArrow { sweep, color } => {
                painter.set_color(color);
                painter.arc(15., 0., sweep);
                //
            }
            MarkerVisual::Cross { color } => {
                let old = painter.thickness;
                painter.thickness = 2.;

                painter.set_color(color);
                let mut corner = (Vec2::splat(10.) * px).to3();
                painter.line(-corner, corner);
                corner.x *= -1.;
                painter.line(-corner, corner);

                painter.thickness = old;
            }
        }
    }
}
