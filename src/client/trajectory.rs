use super::ScreenLenToWorld;
use crate::{
    physics::{ControlInput, SimulationConfig},
    prelude::*,
};

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
    sprite: Sprite,
    transform: Transform,
    segment: TrajectorySegment,
}

#[derive(Resource, Debug)]
pub struct TrajectoryPreview {
    pub entity: Entity,
    pub start_tick: u64,
    pub timeline: Timeline,
}

#[derive(Default, Clone, Copy)]
pub struct TrajectoryPlugin;

impl Plugin for TrajectoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // update_event_markers,
                preview_lookahead,
                (update_trajectory_segments, update_segment_visuals).chain(),
            ),
        );
    }
}

fn preview_lookahead(
    colliders: Query<&crate::physics::collisions::Collider>,
    mut preview: ResMut<TrajectoryPreview>,
    simulation_config: Res<SimulationConfig>,
    spatial_index: Res<crate::physics::collisions::SpatialIndex>,
) {
    let entity = preview.entity;
    preview.timeline.lookahead(
        entity,
        simulation_config.current_tick,
        1.0 / simulation_config.ticks_per_second as f32,
        simulation_config.prediction_ticks,
        colliders.get(entity).unwrap(),
        &spatial_index,
    );
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
                            sprite: Sprite::from_color(
                                Color::srgba(0.0, 0.0, 0.0, 0.0),
                                Vec2::new(length, line_width * 5.),
                            ),
                            transform: Transform::from_translation(
                                center_pos.to3(),
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
