use bevy::ecs::{component, world::DeferredWorld};

use super::{ensure_added, EntityTimeline, ScreenLenToWorld};
use crate::{
    physics::{
        timeline::apply_inputs_and_integrte_phys,
        ControlInput,
        SimulationConfig,
    },
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

impl TrajectorySegment {
    pub fn bundle(self) -> impl Bundle {
        let center_pos = (self.start_pos + self.end_pos) / 2.0;
        (
            Sprite::from_color(
                Color::srgba(0.0, 0.0, 0.0, 0.0),
                Vec2::new(1., 5.),
            ),
            Transform::from_translation(center_pos.to3()),
            self,
        )
    }

    pub fn spawn(self, commands: &mut Commands) -> Entity {
        let tick = self.end_tick;
        commands
            .spawn(self.bundle())
            .with_child(Sprite::from_color(
                Color::srgba(0.5, 0.5, 0.5, (tick % 2) as f32 * 0.5),
                Vec2::new(1., 1.),
            ))
            .id()
    }
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
        app //
            .add_systems(
                Update,
                (
                    ensure_added::<Timeline, TrajectorySegmentTimeline>,
                    preview_lookahead,
                    (
                        sync_preview_segments,
                        render_trajectory_segments,
                        update_segment_visuals,
                    )
                        .chain(),
                ),
            )
            .add_systems(FixedPostUpdate, (sync_trajectory_segments).chain());
    }
}

fn preview_lookahead(
    colliders: Query<&crate::physics::collisions::Collider>,
    mut preview: ResMut<TrajectoryPreview>,
    simulation_config: Res<SimulationConfig>,
    spatial_index: Res<crate::physics::collisions::SpatialIndex>,
) {
    let entity = preview.entity;
    let seconds_per_tick = 1.0 / simulation_config.ticks_per_second as f32;
    let collider = colliders.get(preview.entity).unwrap();

    let timeline = &mut preview.timeline;

    let start_tick = timeline.last_computed_tick + 1;
    assert!(
        start_tick >= simulation_config.current_tick,
        "Expected last_computed_tick + 1 >= current_tick"
    );
    let end_tick =
        simulation_config.current_tick + simulation_config.prediction_ticks;

    for tick in start_tick..=end_tick {
        apply_inputs_and_integrte_phys(
            tick,
            seconds_per_tick,
            entity,
            timeline,
            collider,
            None,
        );
    }
}

#[derive(Component, Deref, DerefMut, Default)]
#[component(on_remove = TrajectorySegmentTimeline::on_remove)]
struct TrajectorySegmentTimeline(EntityTimeline<TrajectorySegment>);

impl TrajectorySegmentTimeline {
    pub fn on_remove(
        mut world: DeferredWorld,
        entity: Entity,
        _: component::ComponentId,
    ) {
        let timeline = world
            .entity(entity)
            .get::<TrajectorySegmentTimeline>()
            .unwrap();
        // we allocate here to avoid world aliasing with commands
        let entities =
            timeline.0.map.values().copied().collect::<Vec<Entity>>();

        let mut commands = world.commands();
        for e in entities {
            commands.entity(e).despawn_recursive();
        }
    }
}

fn sync_trajectory_segments(
    mut commands: Commands,
    mut crafts: Query<(Entity, &Timeline, &mut TrajectorySegmentTimeline)>,
    mut segments: Query<(Entity, &mut TrajectorySegment, &mut Transform)>,
    sim_config: Res<SimulationConfig>,
    // TODO: create multi-tick segments based off ticks_per_second
) {
    for (craft_entity, timeline, mut segment_timeline) in crafts.iter_mut() {
        // STEP 1: ensure there is a segment for each updated tick
        let Some(range) = timeline.last_updated_range.clone() else {
            continue;
        };
        // dbg!(&timeline);
        let mut first_dead = None;
        for tick in range.clone() {
            if !timeline.state(tick).unwrap().alive {
                first_dead = Some(tick);
                break;
            }

            let mut spawn =
                |segment_timeline: &mut TrajectorySegmentTimeline| {
                    let segment = TrajectorySegment {
                        craft_entity,
                        start_tick: tick - 1,
                        end_tick: tick,
                        start_pos: timeline.state(tick - 1).unwrap().pos,
                        end_pos: timeline.state(tick).unwrap().pos,
                        is_preview: false,
                    };
                    segment_timeline.insert(tick, segment.spawn(&mut commands));
                };

            let Some(seg_e) = segment_timeline.get(tick) else {
                spawn(&mut segment_timeline);
                continue;
            };

            let Ok((_, mut segment, mut transform)) = segments.get_mut(*seg_e)
            else {
                spawn(&mut segment_timeline);
                continue;
            };

            segment.start_pos = timeline.state(tick - 1).unwrap().pos;
            segment.end_pos = timeline.state(tick).unwrap().pos;
            transform.translation =
                ((segment.start_pos + segment.end_pos) / 2.0).to3();
        }

        // STEP 2: Remove all segments since fist_dead
        if let Some(first_dead) = first_dead {
            for tick in first_dead..=*range.end() {
                let Some(seg_e) = segment_timeline.map.remove(&tick) else {
                    continue;
                };
                commands.entity(seg_e).despawn_recursive();
            }
        }

        // STEP 3: Remove all segments older than current tick
        for (&tick, &seg_e) in segment_timeline.0.map.iter() {
            if tick >= sim_config.current_tick {
                break;
            }
            commands.entity(seg_e).despawn_recursive();
        }
        segment_timeline
            .0
            .map
            .retain(|tick, _| *tick >= sim_config.current_tick);

        // Note: handling despawning a craft is done through segment_timeline
        //       component hooks
    }
}

fn sync_preview_segments(
    mut commands: Commands,
    preview: Option<Res<TrajectoryPreview>>,
    mut seg_ents: Local<EntityHashSet>,
) {
    // despawn all preview segments
    for e in seg_ents.drain() {
        commands.entity(e).despawn_recursive();
    }

    let Some(preview) = preview else {
        return;
    };

    let mut iter = preview
        .timeline
        .future_states
        .iter()
        .map(|(t, s)| (t, s.pos))
        .peekable();

    while let Some((&start_tick, start_pos)) = iter.next() {
        let Some((end_tick, end_pos)) = iter.peek().copied() else {
            break;
        };

        let segment = TrajectorySegment {
            craft_entity: preview.entity,
            start_tick,
            end_tick: *end_tick,
            start_pos,
            end_pos,
            is_preview: true,
        };

        seg_ents.insert(segment.spawn(&mut commands));
    }
}

fn render_trajectory_segments(
    mut segments: Query<(
        Entity,
        &TrajectorySegment,
        &mut Sprite,
        &mut Transform,
        &Children,
        Option<&ViewVisibility>,
    )>,
    mut visual_lines: Query<&mut Sprite, Without<TrajectorySegment>>,
    screen_len_to_world: Res<ScreenLenToWorld>,
) {
    let pixel_width = 3.;
    // Calculate pixel-perfect line width in world space
    let line_width = **screen_len_to_world * pixel_width;

    for (seg_e, seg, mut hitbox, mut transform, children, view_visibility) in
        segments.iter_mut()
    {
        if let None = view_visibility {
            eprintln!("No visibility");
        }

        if !seg.is_preview
            && view_visibility.is_some()
            && !view_visibility.unwrap().get()
        {
            continue;
        }

        let diff = seg.end_pos - seg.start_pos;
        let length = diff.length();
        let center_pos = (seg.start_pos + seg.end_pos) / 2.0;
        let angle = diff.y.atan2(diff.x);

        transform.translation = center_pos.to3();
        transform.rotation = Quat::from_rotation_z(angle);
        hitbox.custom_size = Some(Vec2::new(length, line_width * 5.));

        let mut sprite = visual_lines
            .get_mut(children[0])
            .expect("Expected trajectory segment to have child");
        sprite.custom_size = Some(Vec2::new(length, line_width));
    }
}

fn check_close_to_viewport(
    (camera, camera_transform): (&Camera, &GlobalTransform),
    pos: Vec2,
    cutoff: f32, // 1.0 for screen visibility, >1.0 includes offscreen space
) -> bool {
    let ndc = camera.world_to_ndc(camera_transform, pos.extend(0.0));

    // if ndc.is_some() {
    //     eprintln!("NDC ({:>2.2}, {:>2.2})", ndc.unwrap().x, ndc.unwrap().y,);
    // }

    // Check if the position is within NDC
    // bounds
    match ndc {
        Some(coords) => {
            coords.x >= -cutoff
                && coords.x <= cutoff
                && coords.y >= -cutoff
                && coords.y <= cutoff
        }
        None => false,
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
