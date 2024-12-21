use core::f32;
use std::marker::PhantomData;

use bevy::{
    color::palettes::css,
    math::{vec2, NormedVectorSpace},
    render::camera::ViewportConversionError,
};

use super::{
    trajectory::{TrajectoryPreview, TrajectorySegment},
    EntityTimeline,
    ScreenLenToWorld,
};
use crate::{
    physics::{
        collisions::{Collider, SpatialIndex},
        ControlInput,
        SimulationConfig,
        TimelineEventRemovalRequest,
        TimelineEventRequest,
    },
    prelude::*,
};

#[derive(Default, Clone, Copy)]
pub struct InputHandlerPlugin;

#[derive(Resource, Deref, DerefMut, Reflect)]
struct SelectedCraft(pub Entity);

impl Plugin for InputHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SelectedCraft>()
            .insert_resource(InputMode::ThrustAndRotation)
            .add_systems(Startup, build_input_mode_ui)
            .add_systems(
                FixedPostUpdate,
                (
                    EntityTimeline::<TimelineEventMarker>::clear_system,
                    sync_timeline_markers,
                ),
            )
            .add_systems(
                Update,
                (
                    (
                        handle_input_mode,
                        (handle_engine_input.run_if(|mode: Res<InputMode>| {
                            matches!(*mode, InputMode::ThrustAndRotation)
                        })),
                        render_timeline_events,
                        update_input_mode_ui,
                    )
                        .chain(),
                    time_dilation_control,
                ),
            );
    }
}

fn build_input_mode_ui(mut commands: Commands) {
    commands
        .spawn(Node {
            // width: Val::Px(100.),
            // height: Val::Px(300.),
            bottom: Val::Px(10.),
            left: Val::Px(10.),
            position_type: PositionType::Absolute,
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new(
                    "Modes: 1: ThrustAndRotation, 2: MoveNode, 3: PlasmaCannon",
                ),
                Marker::<InputMode>::default(),
            ));
        });
}

fn update_input_mode_ui(
    mut input_mode_ui: Query<&mut Text, With<Marker<InputMode>>>,
    input_mode: Res<InputMode>,
) {
    let mut ui = input_mode_ui.single_mut();

    use std::fmt::Write;

    let s = &mut ui.0;
    s.clear();

    write!(s, "Modes: ").unwrap();
    let mut to_pop = 0;
    for (idx, variant) in InputMode::iter().enumerate() {
        let key = idx + 1;

        if *input_mode == variant {
            write!(s, "<").unwrap();
        } else {
            write!(s, " ").unwrap();
        }
        write!(s, "{}: {}", key, variant).unwrap();
        if *input_mode == variant {
            write!(s, ">, ").unwrap();
            to_pop = 2;
        } else {
            write!(s, ",  ").unwrap();
            to_pop = 3;
        }
    }
    while to_pop > 0 {
        s.pop();
        to_pop -= 1;
    }
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, EnumIter, strum::Display)]
enum InputMode {
    ThrustAndRotation,
    FireMissle,
    PlasmaCannon,
}

fn handle_input_mode(
    mut input_mode: ResMut<InputMode>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    for pressed in keys.get_just_pressed() {
        match pressed {
            KeyCode::Digit1 => *input_mode = InputMode::ThrustAndRotation,
            KeyCode::Digit2 => *input_mode = InputMode::FireMissle,
            KeyCode::Digit3 => *input_mode = InputMode::PlasmaCannon,
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
    timelines: Query<&Timeline>,
    mut preview: Option<ResMut<TrajectoryPreview>>,
    mut timeline_event_writer: EventWriter<TimelineEventRequest>,
    mut commands: Commands,
) {
    for drag_start in drag_start_r.read() {
        if drag_start.button != PointerButton::Primary {
            continue;
        }
        println!("Drag start");
        let Ok(seg) = segments.get(drag_start.target) else {
            continue;
        };
        let Ok(timeline) = timelines.get(seg.craft_entity) else {
            warn!("Timeline for craft being dragged doesn't exist");
            continue;
        };

        // Create preview timeline starting from segment's end tick
        commands.insert_resource(TrajectoryPreview {
            entity: seg.craft_entity,
            start_tick: seg.start_tick,
            timeline: Timeline {
                events: timeline.events.clone(),

                future_states: BTreeMap::from_iter(
                    timeline
                        .future_states
                        .range(0..=seg.end_tick)
                        .map(|(k, v)| (k.clone(), v.clone())),
                ),
                last_computed_tick: seg.start_tick,
            },
        });

        info!(pos = ?drag_start.pointer_location.position, "Drag start");
    }

    for drag in drag_r.read() {
        if drag.button != PointerButton::Primary {
            continue;
        }
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

        // Patch preview timeline
        preview.timeline.events.insert(
            seg.end_tick,
            TimelineEvent::Control(ControlInput::SetThrustAndRotation(
                (world_drag.length() / 50.).min(1.),
                world_drag.to_angle(),
            )),
        );
        preview.timeline.last_computed_tick = preview.start_tick - 1;
        info!("drag loop over");
    }

    for drag_end in drag_end_r.read() {
        if drag_end.button != PointerButton::Primary {
            continue;
        }
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

#[derive(Component, Clone)]
pub struct TimelineEventMarker {
    tick: u64,
    craft: Entity,
    input: TimelineEvent,
    pos: Vec2,
    rot: f32,
}

#[derive(Component)]
struct Old<C>(pub C);

impl TimelineEventMarker {
    pub fn bundle(
        phys: &PhysicsState,
        craft: Entity,
        input: TimelineEvent,
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
        for (&tick, event) in timeline.events.iter() {
            let mut spawn = |marker_entity_timeline: &mut EntityTimeline<
                TimelineEventMarker,
            >| {
                let Some(phys) = timeline.future_states.get(&tick) else {
                    error!("Bad");
                    panic!("bad!");
                };

                let mut entity_commands =
                    commands.spawn(TimelineEventMarker::bundle(
                        phys,
                        craft_entity,
                        event.clone(),
                        tick,
                    ));

                // add click handlers if
                // event is a control event
                match event {
                    TimelineEvent::Control(input) => {
                        let input = input.clone();
                        configure_marker_observers(
                            craft_entity,
                            input,
                            &mut entity_commands,
                        );
                    }
                    _ => {}
                }
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
                error!("Bad");
                panic!("bad!");
            };
            if marker.input != *event {
                marker.input = event.clone();
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
                    events: timeline.events.clone(),
                    future_states: BTreeMap::from_iter(
                        timeline
                            .future_states
                            .range(0..=last_computed_tick)
                            .map(|(k, v)| (k.clone(), v.clone())),
                    ),
                    last_computed_tick,
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

            preview.timeline.events.remove(&old_tick);
            preview
                .timeline
                .events
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
            Control(SetThrust(thrust)) => Arrow {
                length: *thrust,
                relative_rot: 0.,
                color: css::PALE_GREEN,
            },
            Control(SetRotation(new_rot)) => ArcArrow {
                sweep: (new_rot - event.rot) % (2. * PI),
                color: css::DARK_BLUE,
            },
            Control(SetAngVel(ang_vel)) => ArcArrow {
                sweep: (ang_vel - event.rot) % (2. * PI),
                color: css::MIDNIGHT_BLUE,
            },
            Control(SetThrustAndRotation(thrust, new_rot)) => Arrow {
                length: *thrust,
                relative_rot: new_rot - event.rot,
                color: css::LIGHT_GREEN,
            },
            Collision(collision) => Cross { color: css::RED },
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
