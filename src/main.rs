#![allow(unused_imports)]

use std::collections::BTreeMap;

use asteroid::{AsteroidPlugin, SmallAsteroid};
use bevy::{
    color::palettes::css,
    utils::{HashMap, HashSet},
};
use collisions::{Collider, SpatialIndex};
use crafts::Faction;
use parallax_protocol_arena::{physics::*, prelude::*};

use crate::utils::screen_to_world;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: bevy::window::WindowResolution::new(
                            1700., 1100.,
                        ),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
            bevy_pancam::PanCamPlugin,
            Shape2dPlugin::default(),
        ))
        .add_plugins((
            PhysicsSimulationPlugin {
                config: (|| {
                    let tps = 30;
                    SimulationConfig {
                        ticks_per_second: tps,
                        time_dilation: 1.0,
                        prediction_ticks: tps * 10,
                        ..default()
                    }
                })(),
                schedule: FixedUpdate,
            },
            AsteroidPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, health_despawn)
        .add_systems(
            Update,
            (
                exit_system,
                (
                    (handle_engine_input),
                    update_event_markers,
                    (update_trajectory_segments, update_segment_visuals)
                        .chain(),
                )
                    .chain(),
            ),
        )
        .run();
}

pub fn exit_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.all_pressed([KeyCode::ControlLeft, KeyCode::KeyC]) {
        exit.send(AppExit::Success);
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera2d,
        bevy_pancam::PanCam {
            move_keys: bevy_pancam::DirectionKeys::arrows(),
            grab_buttons: vec![MouseButton::Right],
            ..default()
        },
    ));

    let ship_e = commands
        .spawn(ship_bundle(
            "Ship_rotated.png",
            10.,
            32.,
            Faction::Red,
            Vec2::new(10., 10.),
            &asset_server,
        ))
        .id();
    info!(ship_entity = ship_e.index(), "Ship Entity");
    // commands.spawn(ship_bundle(
    //     "Ship_rotated.png",
    //     10.,
    //     32.,
    //     Faction::Red,
    //     Vec2::new(-10., -10.),
    //     &asset_server,
    // ));
    // commands.queue(SmallAsteroid::spawn(
    //     Vec2::new(150., 20.),
    //     Vec2::new(1., -2.),
    // ));

    commands.queue(SmallAsteroid::spawn(
        Vec2::new(150., 50.),
        Vec2::new(100., -2.),
    ));
}

pub fn ship_bundle(
    sprite_name: &'static str,
    radius: f32,
    px: f32,
    faction: Faction,
    pos: Vec2,
    asset_server: &AssetServer,
) -> impl Bundle {
    (
        faction,
        Transform::from_translation(Vec3::from2(pos)).with_scale(Vec3::new(
            2. * radius / px,
            2. * radius / px,
            1.,
        )),
        Sprite {
            image: asset_server.load(sprite_name),
            color: faction.sprite_color(),
            ..default()
        },
        PhysicsBundle::new_with_events(
            PhysicsState {
                pos,
                vel: Vec2::ZERO,
                ang_vel: 0.,
                rotation: 0.,
                mass: 1.,
                current_thrust: 0.,
                max_thrust: 100.,
                alive: true,
            },
            Vec2::new(px, px),
            [
                (2, TimelineEvent::Control(ControlInput::SetThrust(1.))),
                (20, TimelineEvent::Control(ControlInput::SetThrust(0.))),
                (60, TimelineEvent::Control(ControlInput::SetRotation(PI))),
                (61, TimelineEvent::Control(ControlInput::SetAngVel(0.1))),
                (65, TimelineEvent::Control(ControlInput::SetThrust(1.))),
                (80, TimelineEvent::Control(ControlInput::SetThrust(0.1))),
            ]
            .into_iter(),
        ),
    )
}

#[derive(Component)]
struct TimelineEventMarker {
    tick: u64,
    input: TimelineEvent,
}

#[derive(Bundle)]
struct TimelineEventMarkerBundle {
    sprite_bundle: Sprite,
    transform: Transform,
    marker: TimelineEventMarker,
    // pickable: Pickable,
}

fn update_event_markers(
    mut commands: Commands,
    query: Query<(Entity, &Timeline)>,
    mut markers_query: Query<(
        Entity,
        &mut TimelineEventMarker,
        &mut Transform,
        &mut Sprite,
    )>,
    mut markers_map: Local<HashMap<(Entity, u64), Entity>>,
    mut gizmos: Gizmos,
) {
    let mut used_keys = HashSet::with_capacity(markers_map.len());

    for (timeline_entity, timeline) in query.iter() {
        for (tick, input) in timeline.events.iter() {
            let tick = *tick;
            let input = input.clone();

            let Some(state) = timeline.future_states.get(&tick) else {
                continue;
            };
            let position = state.pos;

            used_keys.insert((timeline_entity, tick));

            let (color, shaft_length, rotation) = match input {
                TimelineEvent::Control(control_input) => match control_input {
                    ControlInput::SetThrust(thrust) => {
                        let magnitude = (thrust.abs() * 50.0).min(50.0);
                        (
                            Color::srgba(1.0, 0.0, 0.0, 0.8),
                            magnitude,
                            state.rotation,
                        )
                    }
                    ControlInput::SetThrustAndRotation(thrust, rotation) => {
                        let magnitude = (thrust.abs() * 50.0).min(50.0);
                        (Color::srgba(1.0, 0.0, 0.0, 0.8), magnitude, rotation)
                    }
                    ControlInput::SetRotation(rotation) => {
                        (Color::srgba(0.0, 1.0, 0.0, 0.8), 20.0, rotation)
                    }
                    ControlInput::SetAngVel(ang_vel) => {
                        let magnitude = (ang_vel.abs() * 8.0).min(20.0);
                        (
                            Color::srgba(0.0, 0.0, 1.0, 0.8),
                            magnitude,
                            state.rotation,
                        )
                    }
                },
                TimelineEvent::Collision(ref _collision) => {
                    (css::DARK_SALMON.into(), 20_f32, state.rotation)
                }
            };

            let shaft_width = shaft_length / 6.0;
            let head_size = shaft_width * 2.0;
            let dir = Vec2::from_angle(rotation);
            let shaft_position =
                position + dir * (shaft_length - head_size) / 2.0;

            // Calculate arrowhead points
            let head_center = position + dir * (shaft_length - head_size / 2.0);
            let head_left = head_center
                + Vec2::from_angle(rotation + std::f32::consts::PI * 2.0 / 3.0)
                    * head_size;
            let head_right = head_center
                + Vec2::from_angle(rotation - std::f32::consts::PI * 2.0 / 3.0)
                    * head_size;

            // Draw arrowhead with gizmos
            gizmos.line_2d(head_center, head_left, color);
            gizmos.line_2d(head_center, head_right, color);
            gizmos.line_2d(head_left, head_right, color);

            // Either update existing shaft or create new one
            if let Some(&marker_entity) =
                markers_map.get(&(timeline_entity, tick))
            {
                if let Ok((_entity, mut marker, mut transform, mut sprite)) =
                    markers_query.get_mut(marker_entity)
                {
                    marker.input = input.clone();
                    transform.translation = Vec3::from2(shaft_position);
                    transform.rotation = Quat::from_rotation_z(rotation);
                    sprite.color = color;
                    sprite.custom_size =
                        Some(Vec2::new(shaft_length - head_size, shaft_width));
                }
            } else {
                // Create new shaft
                let marker_entity = commands
                    .spawn(TimelineEventMarkerBundle {
                        sprite_bundle: Sprite {
                            color,
                            custom_size: Some(Vec2::new(
                                shaft_length - head_size,
                                shaft_width,
                            )),
                            ..default()
                        },
                        transform: Transform::from_translation(Vec3::from2(
                            shaft_position,
                        ))
                        .with_rotation(Quat::from_rotation_z(rotation)),
                        marker: TimelineEventMarker { tick, input },
                    })
                    .id();

                markers_map.insert((timeline_entity, tick), marker_entity);
            }
        }
    }

    // Cleanup unused markers
    let mut to_delete = Vec::new();
    for (k, e) in markers_map.iter() {
        if !used_keys.contains(k) {
            commands.entity(*e).despawn();
            to_delete.push(*k);
        }
    }

    for k in to_delete {
        markers_map.remove(&k);
    }
}

#[derive(Component, Debug)]
struct TrajectorySegment {
    craft_entity: Entity,
    start_tick: u64,
    end_tick: u64,
    start_pos: Vec2,
    end_pos: Vec2,
    is_preview: bool,
}

// Bundle to create a trajectory segment entity
#[derive(Bundle)]
struct TrajectorySegmentBundle {
    sprite_bundle: Sprite,
    transform: Transform,
    segment: TrajectorySegment,
}

fn update_trajectory_segments(
    mut commands: Commands,
    query: Query<(Entity, &Timeline)>,
    mut segments_query: Query<(
        Entity,
        &mut TrajectorySegment,
        &mut Transform,
        &mut Sprite,
    )>,
    mut segments_map: Local<HashMap<(Entity, u64), Entity>>,
    mut preview_segments: Local<HashMap<(Entity, u64), Entity>>,
    preview: Option<Res<TrajectoryPreview>>,
) {
    let mut used_keys = HashSet::with_capacity(segments_map.len());
    for (craft_entity, timeline) in query.iter() {
        update_trajectory(
            timeline,
            craft_entity,
            &mut commands,
            &mut segments_query,
            &mut segments_map,
            Some(&mut used_keys),
        );
    }

    match preview {
        Some(preview) => update_trajectory(
            &preview.timeline,
            preview.entity,
            &mut commands,
            &mut segments_query,
            &mut preview_segments,
            None,
        ),
        None => {
            preview_segments
                .values()
                .for_each(|e| commands.entity(*e).despawn());
            preview_segments.clear();
        }
    }

    // Is this the best way to clean this up???
    let mut to_delete = Vec::new();
    for (k, e) in segments_map.iter() {
        if !used_keys.contains(k) {
            commands.entity(*e).despawn();
            to_delete.push(*k);
        }
    }

    for e in to_delete {
        segments_map.remove(&e);
    }
}

fn update_trajectory(
    timeline: &Timeline,
    craft_entity: Entity,
    commands: &mut Commands,
    segments_query: &mut Query<(
        Entity,
        &mut TrajectorySegment,
        &mut Transform,
        &mut Sprite,
    )>,
    segments_map: &mut HashMap<(Entity, u64), Entity>,
    mut used_keys_or_is_preview: Option<&mut HashSet<(Entity, u64)>>,
) {
    let positions = timeline
        .future_states
        .iter()
        .take_while(|(_, s)| s.alive)
        .map(|(tick, state)| (*tick, state.pos))
        .collect::<Vec<_>>();

    if positions.len() >= 2 {
        for window in positions.windows(2) {
            let (start_tick, start_pos) = window[0];
            let (end_tick, end_pos) = window[1];

            let length = (end_pos - start_pos).length();
            let angle = (end_pos - start_pos).y.atan2((end_pos - start_pos).x);

            let center_pos = (start_pos + end_pos) / 2.0;

            used_keys_or_is_preview
                .as_mut()
                .map(|used_keys| used_keys.insert((craft_entity, start_tick)));

            let Some(seg_ent) = segments_map.get(&(craft_entity, start_tick))
            else {
                segments_map.insert(
                    (craft_entity, start_tick),
                    commands
                        .spawn((TrajectorySegmentBundle {
                            sprite_bundle: Sprite {
                                color: Color::srgba(1.0, 1.0, 1.0, 0.5),
                                custom_size: Some(Vec2::new(length, 2.0)),
                                ..default()
                            },
                            transform: Transform::from_translation(
                                Vec3::from2(center_pos),
                            )
                            .with_rotation(Quat::from_rotation_z(angle)),
                            segment: TrajectorySegment {
                                craft_entity,
                                start_tick,
                                end_tick,
                                start_pos,
                                end_pos,
                                is_preview: used_keys_or_is_preview.is_none(),
                            },
                        },))
                        .id(),
                );
                continue;
            };

            let Ok((_entity, mut segment, mut transform, mut sprite)) =
                segments_query.get_mut(*seg_ent)
            else {
                panic!("oops");
            };

            // Update existing segment
            segment.start_tick = start_tick;
            segment.end_tick = end_tick;
            segment.start_pos = start_pos;
            segment.end_pos = end_pos;

            transform.translation = Vec3::from2(center_pos);
            transform.rotation = Quat::from_rotation_z(angle);

            sprite.custom_size.as_mut().unwrap().x = length;
        }
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
        let Ok(seg) = segments.get(drag.target) else {
            continue;
        };
        let Some(preview) = preview.as_mut() else {
            continue;
        };
        let craft_entity = seg.craft_entity;

        let world_drag = screen_to_world(drag.distance);
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
        let world_drag = screen_to_world(drag_end.distance);

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

fn update_segment_visuals(
    mut out: EventReader<Pointer<Out>>,
    mut over: EventReader<Pointer<Over>>,
    mut query: Query<(&mut Sprite, &TrajectorySegment)>,
) {
    for e in out.read() {
        let Ok((mut sprite, segment)) = query.get_mut(e.target) else {
            continue;
        };
        let alpha = if segment.is_preview { 0.25 } else { 0.5 };
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        sprite.custom_size.as_mut().unwrap().y = 2.0;
    }

    for e in over.read() {
        let Ok((mut sprite, segment)) = query.get_mut(e.target) else {
            continue;
        };
        // TODO: is hovering a preview even something we should support??
        let alpha = if segment.is_preview { 0.5 } else { 1.0 };
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        sprite.custom_size.as_mut().unwrap().y = 5.0;
    }
}
