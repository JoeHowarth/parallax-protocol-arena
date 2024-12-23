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
                Update,
                (
                    (
                        handle_input_mode,
                        (handle_engine_input.run_if(|mode: Res<InputMode>| {
                            matches!(*mode, InputMode::ThrustAndRotation)
                        })),
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
                input_events: timeline.input_events.clone(),
                sim_events: default(),
                future_states: BTreeMap::from_iter(
                    timeline
                        .future_states
                        .range(0..=seg.end_tick)
                        .map(|(k, v)| (k.clone(), v.clone())),
                ),
                last_computed_tick: seg.start_tick,
                last_updated_range: None,
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
        preview.timeline.add_input_event(
            seg.end_tick,
            ControlInput::SetThrustAndRotation(
                (world_drag.length() / 50.).min(1.),
                world_drag.to_angle(),
            ),
        );
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
