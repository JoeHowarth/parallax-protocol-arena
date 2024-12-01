use std::collections::BTreeMap;

use bevy_mod_picking::{
    debug::DebugPickingMode,
    DefaultPickingPlugins,
    PickableBundle,
};
use parallax_protocol_arena::{physics::*, prelude::*};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: bevy::window::WindowResolution::new(
                        1700., 1100.,
                    ),
                    ..default()
                }),
                ..default()
            }),
            bevy_pancam::PanCamPlugin,
            DefaultPickingPlugins,
            Shape2dPlugin::default(),
        ))
        .add_plugins(PhysicsSimulationPlugin)
        .insert_resource(SimulationConfig {
            ticks_per_second: 5,
            time_dilation: 3.,
            ..default()
        })
        .insert_resource(Gravity::ZERO)
        .insert_resource(DebugPickingMode::Normal)
        .add_event::<TrajectoryClicked>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                exit_system,
                (
                    handle_trajectory_clicks,
                    update_segment_visuals,
                    update_event_markers,
                    update_trajectory_segments,
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
        Camera2dBundle::default(),
        bevy_pancam::PanCam {
            move_keys: bevy_pancam::DirectionKeys::NONE,
            ..default()
        },
    ));

    commands.spawn(ship_bundle(
        "Ship_rotated.png",
        10.,
        32.,
        Faction::Red,
        Vec2::new(10., 10.),
        &asset_server,
    ));
}

pub fn ship_bundle(
    sprite_name: &'static str,
    radius: f32,
    px: f32,
    faction: Faction,
    loc: Vec2,
    asset_server: &AssetServer,
) -> impl Bundle {
    (
        faction,
        SpriteBundle {
            texture: asset_server.load(sprite_name),
            transform:
                Transform::from_translation(Vec3::from2(loc)) //
                    .with_scale(Vec3::new(
                        2. * radius / px,
                        2. * radius / px,
                        1.,
                    )),
            sprite: Sprite {
                color: faction.sprite_color(),
                ..default()
            },
            ..default()
        },
        PickableBundle::default(),
        PhysicsState {
            position: loc,
            velocity: Vec2::ZERO,
            angular_velocity: 0.,
            rotation: 0.,
            mass: 1.,
            current_thrust: 0.,
            max_thrust: 100.,
        },
        Timeline {
            events: vec![
                TimelineEvent {
                    tick: 5,
                    input: ControlInput::SetThrust(1.),
                },
                TimelineEvent {
                    tick: 10,
                    input: ControlInput::SetThrust(0.),
                },
                TimelineEvent {
                    tick: 30,
                    input: ControlInput::SetRotation(PI),
                },
                TimelineEvent {
                    tick: 31,
                    input: ControlInput::SetAngVel(0.1),
                },
                TimelineEvent {
                    tick: 35,
                    input: ControlInput::SetThrust(1.),
                },
                TimelineEvent {
                    tick: 40,
                    input: ControlInput::SetThrust(0.1),
                },
            ],
            future_states: BTreeMap::new(),
            last_computed_tick: 0,
        },
    )
}

// Add trajectory visualization system
// fn update_trajectory_lines(
//     mut commands: Commands,
//     query: Query<(Entity, &Timeline)>,
//     mut gizmos: Gizmos,
// ) {
//     for (_entity, timeline) in query.iter() {
//         let positions = timeline
//             .future_states
//             .iter()
//             .map(|(tick, state)| (tick, state.position))
//             .collect::<Vec<_>>();
//         // positions.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
//
//         // Draw the trajectory line
//         if positions.len() >= 2 {
//             for positions in positions.windows(2) {
//                 commands.spawn(
//                  TrajectorySegmentBundle {
//                     sprite_bundle: todo!(),
//                     segment: todo!(),
//                     pickable: todo!(),
//                 }
//                 );
//
//                 gizmos.line_2d(
//                     positions[0].1,
//                     positions[1].1,
//                     Color::srgba(1.0, 1.0, 1.0, 0.5),
//                 );
//             }
//         }
//     }
// }

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};
use bevy_mod_picking::prelude::*;

#[derive(Component)]
struct TimelineEventMarker {
    tick: u64,
    event: TimelineEvent,
}

#[derive(Bundle)]
struct TimelineEventMarkerBundle {
    sprite_bundle: SpriteBundle,
    marker: TimelineEventMarker,
    pickable: Pickable,
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
        for event in timeline.events.iter() {
            let tick = event.tick;

            let Some(state) = timeline.future_states.get(&tick) else {
                continue;
            };
            let position = state.position;

            used_keys.insert((timeline_entity, tick));

            let (color, shaft_length, rotation) = match event.input {
                ControlInput::SetThrust(thrust) => {
                    let magnitude = (thrust.abs() * 20.0).max(20.0);
                    (
                        Color::srgba(1.0, 0.0, 0.0, 0.8),
                        magnitude,
                        state.rotation,
                    )
                }
                ControlInput::SetRotation(_rotation) => {
                    (Color::srgba(0.0, 1.0, 0.0, 0.8), 20.0, _rotation)
                }
                ControlInput::SetAngVel(ang_vel) => {
                    let magnitude = (ang_vel.abs() * 8.0).max(20.0);
                    (
                        Color::srgba(0.0, 0.0, 1.0, 0.8),
                        magnitude,
                        state.rotation,
                    )
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
                    marker.event = event.clone();
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
                        sprite_bundle: SpriteBundle {
                            sprite: Sprite {
                                color,
                                custom_size: Some(Vec2::new(
                                    shaft_length - head_size,
                                    shaft_width,
                                )),
                                ..default()
                            },
                            transform: Transform::from_translation(
                                Vec3::from2(shaft_position),
                            )
                            .with_rotation(Quat::from_rotation_z(rotation)),
                            ..default()
                        },
                        marker: TimelineEventMarker {
                            tick,
                            event: event.clone(),
                        },
                        pickable: default(),
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

#[derive(Component)]
struct TrajectorySegment {
    start_tick: u64,
    end_tick: u64,
    start_pos: Vec2,
    end_pos: Vec2,
}

// Bundle to create a trajectory segment entity
#[derive(Bundle)]
struct TrajectorySegmentBundle {
    sprite_bundle: SpriteBundle,
    segment: TrajectorySegment,
    // Add picking components
    pickable: PickableBundle,
    // Highlight effect on hover (optional)
    // highlight: HighlightableObject,
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
) {
    let mut used_keys = HashSet::with_capacity(segments_map.len());
    for (_entity, timeline) in query.iter() {
        let positions = timeline
            .future_states
            .iter()
            .map(|(tick, state)| (*tick, state.position))
            .collect::<Vec<_>>();

        if positions.len() >= 2 {
            for window in positions.windows(2) {
                let (start_tick, start_pos) = window[0];
                let (end_tick, end_pos) = window[1];

                let length = (end_pos - start_pos).length();
                let angle =
                    (end_pos - start_pos).y.atan2((end_pos - start_pos).x);

                let center_pos = (start_pos + end_pos) / 2.0;

                used_keys.insert((_entity, start_tick));
                let Some(seg_ent) = segments_map.get(&(_entity, start_tick))
                else {
                    segments_map.insert(
                        (_entity, start_tick),
                        commands
                            .spawn((
                                TrajectorySegmentBundle {
                                    sprite_bundle: SpriteBundle {
                                        sprite: Sprite {
                                            color: Color::srgba(
                                                1.0, 1.0, 1.0, 0.5,
                                            ),
                                            custom_size: Some(Vec2::new(
                                                length, 2.0,
                                            )),
                                            ..default()
                                        },
                                        transform: Transform::from_translation(
                                            Vec3::from2(center_pos),
                                        )
                                        .with_rotation(Quat::from_rotation_z(
                                            angle,
                                        )),
                                        ..default()
                                    },
                                    segment: TrajectorySegment {
                                        start_tick,
                                        end_tick,
                                        start_pos,
                                        end_pos,
                                    },
                                    pickable: default(),
                                },
                                On::<Pointer<Click>>::send_event::<
                                    TrajectoryClicked,
                                >(),
                            ))
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
                sprite.custom_size = Some(Vec2::new(length, 2.0));
                // Create new segment if we don't have enough
            }
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

#[derive(Event)]
pub struct TrajectoryClicked(pub Entity, pub f32);

impl From<ListenerInput<Pointer<Click>>> for TrajectoryClicked {
    fn from(event: ListenerInput<Pointer<Click>>) -> Self {
        TrajectoryClicked(event.target, event.hit.depth)
    }
}

fn handle_trajectory_clicks(
    mut clicks: EventReader<TrajectoryClicked>,
    query: Query<&TrajectorySegment>,
) {
    for click in clicks.read() {
        let Ok(segment) = query.get(click.0) else {
            error!("oops");
            continue;
        };
        info!(
            entity = ?click.0,
            "Clicked trajectory segment: tick {} to {}",
            segment.start_tick, segment.end_tick
        );
        // Here you can emit an event or trigger your action planning logic
    }
}

fn handle_engine_input(
    mut drag_start: EventReader<Pointer<DragStart>>,
    mut drag_end: EventReader<Pointer<DragEnd>>,
    // mut query: Query<(&mut Sprite, With<TrajectorySegment>>,
) {
}

fn update_segment_visuals(
    mut out: EventReader<Pointer<Out>>,
    mut over: EventReader<Pointer<Over>>,
    mut query: Query<&mut Sprite, With<TrajectorySegment>>,
) {
    for e in out.read() {
        let Ok(mut sprite) = query.get_mut(e.target) else {
            continue;
        };
        sprite.color = Color::srgba(1.0, 1.0, 1.0, 0.5);
    }
    for e in over.read() {
        let Ok(mut sprite) = query.get_mut(e.target) else {
            continue;
        };
        sprite.color = Color::srgba(1.0, 1.0, 1.0, 1.0);
    }
}
