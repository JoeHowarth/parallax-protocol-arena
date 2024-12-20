use std::f32::consts::PI;

use bevy::color::palettes::css;
use input_handler::ScreenLenToWorld;
use physics::ControlInput;

use crate::prelude::*;

#[derive(Component, Debug)]
pub struct TrajectorySegment {
    pub craft_entity: Entity,
    pub start_tick: u64,
    pub end_tick: u64,
    pub start_pos: Vec2,
    pub end_pos: Vec2,
    pub is_preview: bool,
}

// Bundle to create a trajectory segment entity
#[derive(Bundle)]
struct TrajectorySegmentBundle {
    sprite_bundle: Sprite,
    transform: Transform,
    segment: TrajectorySegment,
}

#[derive(Resource)]
pub struct TrajectoryPreview {
    pub entity: Entity,
    pub start_tick: u64,
    pub timeline: Timeline,
}

pub struct TrajectoryPlugin;

impl Plugin for TrajectoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // update_event_markers,
                (update_trajectory_segments, update_segment_visuals).chain(),
            ),
        );
    }
}

fn update_trajectory_segments(
    mut commands: Commands,
    query: Query<(Entity, &Timeline)>,
    mut segments_query: Query<
        (
            Entity,
            &mut TrajectorySegment,
            &mut Transform,
            &mut Sprite,
            &Children,
        ),
        Without<Camera>,
    >,
    mut visual_lines: Query<&mut Sprite, Without<TrajectorySegment>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    // windows: Query<&Window>,
    preview: Option<Res<TrajectoryPreview>>,
    screen_len_to_world: Res<ScreenLenToWorld>,
    mut segments_map: Local<HashMap<(Entity, u64), Entity>>,
    mut preview_segments: Local<HashMap<(Entity, u64), Entity>>,
) {
    let pixel_width = 3.;
    // Calculate pixel-perfect line width in world space
    let line_width = **screen_len_to_world * pixel_width;

    let mut used_keys = HashSet::with_capacity(segments_map.len());
    for (craft_entity, timeline) in query.iter() {
        update_trajectory(
            timeline,
            craft_entity,
            &mut commands,
            &mut segments_query,
            &mut visual_lines,
            &mut segments_map,
            Some(&mut used_keys),
            line_width,
        );
    }

    match preview {
        Some(preview) => update_trajectory(
            &preview.timeline,
            preview.entity,
            &mut commands,
            &mut segments_query,
            &mut visual_lines,
            &mut preview_segments,
            None,
            line_width,
        ),
        None => {
            preview_segments
                .values()
                .for_each(|e| commands.entity(*e).despawn_recursive());
            preview_segments.clear();
        }
    }

    // Clean up unused segments
    let mut to_delete = Vec::new();
    for (k, e) in segments_map.iter() {
        if !used_keys.contains(k) {
            commands.entity(*e).despawn_recursive();
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
    segments_query: &mut Query<
        (
            Entity,
            &mut TrajectorySegment,
            &mut Transform,
            &mut Sprite,
            &Children,
        ),
        Without<Camera>,
    >,
    visual_lines: &mut Query<&mut Sprite, Without<TrajectorySegment>>,
    segments_map: &mut HashMap<(Entity, u64), Entity>,
    mut used_keys_or_is_preview: Option<&mut HashSet<(Entity, u64)>>,
    line_width: f32,
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
                                color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                                custom_size: Some(Vec2::new(
                                    length,
                                    line_width * 5.,
                                )),
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
                        .with_child(Sprite::from_color(
                            Color::srgba(
                                0.5,
                                0.5,
                                0.5,
                                (end_tick % 2) as f32 * 0.5,
                            ),
                            Vec2::new(length, line_width),
                        ))
                        .id(),
                );
                continue;
            };

            let Ok((_entity, mut segment, mut transform, mut sprite, children)) =
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

            sprite.custom_size = Some(Vec2::new(length, line_width * 5.));
            if let Ok(mut sprite) = visual_lines.get_mut(children[0]) {
                let Some(size) = sprite.custom_size.as_mut() else {
                    continue;
                };
                size.x = length;
                size.y = line_width;
            }
        }
    }
}

fn update_segment_visuals(
    mut out: EventReader<Pointer<Out>>,
    mut over: EventReader<Pointer<Over>>,
    query: Query<(&Children, &TrajectorySegment)>,
    mut visual_lines: Query<&mut Sprite, Without<TrajectorySegment>>,
) {
    for e in out.read() {
        let Ok((children, segment)) = query.get(e.target) else {
            continue;
        };
        let alpha = if segment.is_preview { 0.25 } else { 0.5 };
        let Ok(mut sprite) = visual_lines.get_mut(children[0]) else {
            error!("Trajectory segment does not have a visual line child");
            continue;
        };
        sprite.color =
            Color::srgba(0.5, 0.5, 0.5, (segment.end_tick % 2) as f32 * alpha);
        // sprite.custom_size.as_mut().unwrap().y = 2.0;
    }

    for e in over.read() {
        let Ok((children, segment)) = query.get(e.target) else {
            continue;
        };
        // TODO: is hovering a preview even something we should support??
        let alpha = if segment.is_preview { 0.5 } else { 1.0 };
        let Ok(mut sprite) = visual_lines.get_mut(children[0]) else {
            error!("Trajectory segment does not have a visual line child");
            continue;
        };
        sprite.color = Color::srgba(0.5, 1.0, 0.5, alpha);
        // sprite.custom_size.as_mut().unwrap().y = 5.0;
    }
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
                + Vec2::from_angle(rotation + PI * 2.0 / 3.0) * head_size;
            let head_right = head_center
                + Vec2::from_angle(rotation - PI * 2.0 / 3.0) * head_size;

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
