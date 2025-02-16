#![allow(unused_imports)]

use std::{collections::BTreeMap, time::Duration};

use asteroid::{AsteroidPlugin, SmallAsteroid};
use bevy::{
    app::AppExit,
    color::palettes::css,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    text::FontStyle,
    time::{
        common_conditions::{on_real_timer, on_timer},
        Timer,
    },
    ui::{
        AlignItems,
        BackgroundColor,
        JustifyContent,
        Node,
        PositionType,
        UiRect,
        Val,
    },
    utils::{HashMap, HashSet},
};
use bevy_rand::{
    plugin::EntropyPlugin,
    prelude::{GlobalEntropy, WyRand},
};
use collisions::{Collider, SpatialIndex};
use parallax_protocol_arena::{
    client::{ClientPlugin, GraphicsEnabled},
    crafts::{asteroid::AsteroidAssets, Faction},
    health_despawn,
    physics::*,
    prelude::*,
    subsystems::{
        plasma_cannon::{PlasmaCannon, PlasmaCannonPlugin},
        unguided_missile::{UnguidedMissile, UnguidedMissilePlugin},
    },
    ParallaxProtocolArenaPlugin,
    Selected,
};
use rand::Rng;

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
enum GameState {
    #[default]
    Loading,
    Playing,
    DeathScreen,
    Reset,
}

#[derive(Event)]
struct GameOver {
    victory: bool,
}

#[derive(Component)]
struct GameEntity;

#[derive(Resource)]
struct DeathScreenTimer(Timer);

#[derive(Component)]
struct DeathScreenUI;

#[derive(Resource, Default)]
struct BestTime(Option<f32>);

#[derive(Component)]
struct TimeDisplayUI;

#[derive(Component)]
struct StartPopupUI;

#[derive(Resource)]
struct StartPopupTimer(Timer);

#[derive(Resource, Default)]
struct SlowMotionTimer(Option<Timer>);

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
            EntropyPlugin::<WyRand>::with_seed(123u64.to_ne_bytes()),
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_plugins((
            ParallaxProtocolArenaPlugin {
                config: (|| {
                    let tps = 10;
                    SimulationConfig {
                        ticks_per_second: tps,
                        time_dilation: 1.,
                        prediction_ticks: tps * 30,
                        ..default()
                    }
                })(),
                ..default()
            },
            AsteroidPlugin,
            PlasmaCannonPlugin,
            UnguidedMissilePlugin,
        ))
        .insert_state(GameState::Loading)
        .add_event::<GameOver>()
        .add_systems(Startup, startup)
        .add_systems(
            Update,
            (
                exit_system,
                fps_ui.run_if(on_real_timer(Duration::from_millis(200))),
                update_time_display,
                handle_game_over,
                handle_death_screen.run_if(in_state(GameState::DeathScreen)),
                handle_start_popup.run_if(in_state(GameState::Loading)),
                handle_slow_motion,
            ),
        )
        .add_systems(
            FixedUpdate,
            (
                health_despawn,
                (check_victory, check_ship_death)
                    .run_if(in_state(GameState::Playing)),
            ),
        )
        .add_systems(OnEnter(GameState::Loading), setup_start_popup)
        .add_systems(OnEnter(GameState::Playing), (setup_game, reset_camera))
        .add_systems(OnEnter(GameState::DeathScreen), setup_death_screen)
        .add_systems(OnEnter(GameState::Reset), cleanup_all_state)
        .init_resource::<BestTime>()
        .init_resource::<SlowMotionTimer>()
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

fn startup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        bevy_pancam::PanCam {
            move_keys: bevy_pancam::DirectionKeys::arrows(),
            grab_buttons: vec![MouseButton::Right],
            ..default()
        },
    ));

    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.),
            right: Val::Px(10.),
            ..default()
        })
        .with_child((Text("fps".into()), FpsUiMarker));

    commands
        .spawn((Node {
            position_type: PositionType::Absolute,
            top: Val::Px(40.),
            right: Val::Px(10.),
            ..default()
        },))
        .with_child((Text::new("Time: 0.0s\nBest: --"), TimeDisplayUI));

    commands.init_resource::<BestTime>();
}

fn setup_game(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    sim_config: Res<SimulationConfig>,
    asteroid_assets: Res<AsteroidAssets>,
) {
    eprintln!("Setting up game");
    commands.insert_resource(GraphicsEnabled);
    commands.insert_resource(PhysicsEnabled);

    let current_tick = sim_config.current_tick;
    let ship_e = commands
        .spawn(ship_bundle(
            "Ship_rotated.png",
            10.,
            32.,
            Faction::Red,
            Vec2::new(0., 0.),
            &asset_server,
            current_tick,
        ))
        .insert(GameEntity)
        .id();
    info!(ship_entity = ship_e.index(), "Ship Entity");
    commands.insert_resource(Selected(ship_e));

    // Generate initial asteroid field with GameEntity marker
    generate_asteroid_field_with_marker(
        &mut commands,
        sim_config,
        asteroid_assets,
    );
}

pub fn ship_bundle(
    sprite_name: &'static str,
    radius: f32,
    px: f32,
    faction: Faction,
    pos: Vec2,
    asset_server: &AssetServer,
    tick: u64,
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
        PlasmaCannon::default(),
        UnguidedMissile::default(),
        PhysicsBundle::new_with_events(
            PhysicsState {
                pos,
                vel: Vec2::ZERO,
                ang_vel: 0.,
                rotation: 0.,
                mass: 1.,
                current_thrust: 0.,
                max_thrust: 50.,
                alive: true,
            },
            Vec2::new(px, px),
            tick,
            [
                (tick + 2, ControlInput::SetThrust(0.1)),
                (tick + 20, ControlInput::SetThrust(0.)),
            ]
            .into_iter(),
        ),
    )
}

fn generate_asteroid_field(
    mut commands: Commands,
    mut rng: ResMut<GlobalEntropy<WyRand>>,
) {
    for _ in 0..500 {
        commands.queue(SmallAsteroid::spawn(
            Vec2::new(
                rng.gen_range((-3000.)..(10000.)),
                rng.gen_range((-3000.)..(3000.)),
            ),
            Vec2::new(
                bad_normal_distribution(&mut rng, 0., 10.),
                bad_normal_distribution(&mut rng, 0., 5.),
            ),
            bad_log_normal_distribution(&mut rng, 0., 0.5)
                .max(0.1)
                .min(20.),
        ));
    }
}

#[derive(Component)]
struct FpsUiMarker;

fn fps_ui(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsUiMarker>>,
) {
    let mut text = query.single_mut();
    let Some(value) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    else {
        return;
    };

    text.0.clear();
    use std::fmt::Write;
    let _ = write!(&mut text.0, "FPS: {value:>3.0}");
}

fn bad_normal_distribution(
    rng: &mut GlobalEntropy<WyRand>,
    mu: f32,
    sigma: f32,
) -> f32 {
    let mut x = 0.;
    for _ in 0..12 {
        x += rng.gen_range(-1.0..1.0);
    }
    x * sigma + mu
}

fn bad_log_normal_distribution(
    rng: &mut GlobalEntropy<WyRand>,
    mu: f32,
    sigma: f32,
) -> f32 {
    bad_normal_distribution(rng, mu, sigma).exp()
}

fn check_victory(
    selected: Option<Res<Selected>>,
    query: Query<&PhysicsState>,
    mut game_over: EventWriter<GameOver>,
) {
    let Some(selected) = selected else {
        return;
    };

    let Ok(physics) = query.get(selected.0) else {
        return;
    };

    if physics.pos.x >= 10000.0 {
        game_over.send(GameOver { victory: true });
    }
}

fn check_ship_death(
    selected: Option<Res<Selected>>,
    query: Query<&PhysicsState>,
    mut commands: Commands,
    mut game_over: EventWriter<GameOver>,
) {
    let Some(selected) = selected else {
        return;
    };

    // Check if ship is dead via PhysicsState
    if let Ok(physics) = query.get(selected.0) {
        if !physics.alive {
            commands.remove_resource::<Selected>();
            game_over.send(GameOver { victory: false });
        }
    } else {
        // Entity doesn't exist anymore
        commands.remove_resource::<Selected>();
        game_over.send(GameOver { victory: false });
    }
}

fn handle_game_over(
    mut game_over: EventReader<GameOver>,
    mut next_state: ResMut<NextState<GameState>>,
    sim_config: Res<SimulationConfig>,
    mut best_time: ResMut<BestTime>,
) {
    for event in game_over.read() {
        info!("Game Over! Victory: {}", event.victory);
        if event.victory {
            let current_time = sim_config.current_tick as f32
                / sim_config.ticks_per_second as f32;
            best_time.0 = Some(
                best_time
                    .0
                    .map(|best| best.min(current_time))
                    .unwrap_or(current_time),
            );
        }
        next_state.set(GameState::DeathScreen);
    }
}

fn handle_death_screen(
    time: Res<Time>,
    mut timer: ResMut<DeathScreenTimer>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    timer.0.tick(time.delta());

    if timer.0.finished() {
        next_state.set(GameState::Reset);
    }
}

fn cleanup_all_state(
    mut commands: Commands,
    mut next_state: ResMut<NextState<GameState>>,
    query: Query<Entity, Or<(With<GameEntity>, With<DeathScreenUI>)>>,
    mut sim_config: ResMut<SimulationConfig>,
    mut spatial_index: ResMut<SpatialIndex>,
) {
    // Despawn all game entities and UI
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }

    // Reset current tick
    sim_config.current_tick = 0;

    // Clear spatial index
    spatial_index.0.clear();

    // Remove resources except GameTimer
    commands.remove_resource::<DeathScreenTimer>();
    commands.remove_resource::<GraphicsEnabled>();
    commands.remove_resource::<PhysicsEnabled>();

    // Transition to Playing state
    next_state.set(GameState::Playing);
}

fn generate_asteroid_field_with_marker(
    commands: &mut Commands,
    sim_config: Res<SimulationConfig>,
    asteroid_assets: Res<AsteroidAssets>,
) {
    let mut rng = GlobalEntropy::<WyRand>::default();

    let tick = sim_config.current_tick;

    for _ in 0..1000 {
        commands.spawn((
            SmallAsteroid::bundle(
                tick,
                &asteroid_assets,
                Vec2::new(
                    rng.gen_range((-3000.)..(10000.)),
                    rng.gen_range((-3000.)..(3000.)),
                ),
                Vec2::new(
                    bad_normal_distribution(&mut rng, 0., 15.),
                    bad_normal_distribution(&mut rng, 0., 5.),
                ),
                bad_log_normal_distribution(&mut rng, 0., 0.5)
                    .max(0.1)
                    .min(20.),
            ),
            GameEntity,
        ));
    }
}

fn setup_death_screen(mut commands: Commands) {
    commands.remove_resource::<PhysicsEnabled>();
    commands.insert_resource(DeathScreenTimer(Timer::from_seconds(
        2.0,
        TimerMode::Once,
    )));
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            DeathScreenUI,
        ))
        .with_child((
            Text::new("Game Over!\nRestarting..."),
            Node {
                margin: UiRect::all(Val::Px(8.0)),
                ..default()
            },
        ));
}

fn update_time_display(
    sim_config: Res<SimulationConfig>,
    best_time: Res<BestTime>,
    mut query: Query<&mut Text, With<TimeDisplayUI>>,
    game_state: Res<State<GameState>>,
) {
    if *game_state.get() != GameState::Playing {
        return;
    }

    let Ok(mut text) = query.get_single_mut() else {
        return;
    };

    let current_time =
        sim_config.current_tick as f32 / sim_config.ticks_per_second as f32;

    text.0.clear();
    use std::fmt::Write;
    let _ = write!(&mut text.0, "Time: {:.1}s", current_time);
    if let Some(best) = best_time.0 {
        let _ = write!(&mut text.0, "\nBest: {:.1}s", best);
    } else {
        let _ = write!(&mut text.0, "\nBest: --");
    }
    let _ =
        write!(&mut text.0, "\nDialation: {:.2}x", sim_config.time_dilation);
}

fn reset_camera(mut query: Query<&mut Transform, With<Camera>>) {
    if let Ok(mut transform) = query.get_single_mut() {
        transform.translation = Vec3::new(0., 0., transform.translation.z);
    }
}

fn setup_start_popup(mut commands: Commands) {
    commands.insert_resource(StartPopupTimer(Timer::from_seconds(
        40.0,
        TimerMode::Once,
    )));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            StartPopupUI,
            Interaction::default(),
        ))
        .with_child((
            Text::new(
                "Race through the asteroid field!\nReach the right side to \
                 win!\n\nControls:\n- Drag with left mouse to thrust\n- Arrow \
                 keys to move camera\n- Right mouse to pan camera\n- P to \
                 pause\n- [ to slow, ] to speed up time\n\nClick anywhere to \
                 start",
            ),
            TextLayout::new_with_justify(JustifyText::Center),
            TextColor(Color::WHITE),
            TextFont {
                font_size: 32.0,
                ..default()
            },
        ));
}

fn handle_start_popup(
    time: Res<Time>,
    mut timer: ResMut<StartPopupTimer>,
    mut commands: Commands,
    query: Query<(Entity, &Interaction), With<StartPopupUI>>,
    mut next_state: ResMut<NextState<GameState>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    timer.0.tick(time.delta());

    let should_transition =
        timer.0.finished() || mouse.just_pressed(MouseButton::Left);

    if should_transition {
        for (entity, _) in query.iter() {
            commands.entity(entity).despawn_recursive();
        }
        commands.remove_resource::<StartPopupTimer>();
        next_state.set(GameState::Playing);
    }
}

fn handle_slow_motion(
    mut slow_motion: ResMut<SlowMotionTimer>,
    time: Res<Time>,
    mut fixed_time: ResMut<Time<Fixed>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut sim_config: ResMut<SimulationConfig>,
) {
    // Start slow motion when space is pressed
    if keys.just_pressed(KeyCode::Space) {
        slow_motion.0 = Some(Timer::from_seconds(3.0, TimerMode::Once));
        sim_config.time_dilation = 0.125;
        // Update the fixed timestep
        fixed_time.set_timestep_hz(
            sim_config.ticks_per_second as f64
                * sim_config.time_dilation as f64,
        );
    }

    // Handle timer if it exists
    if let Some(timer) = &mut slow_motion.0 {
        timer.tick(time.delta());

        if timer.finished() {
            sim_config.time_dilation = 1.0;
            // Update the fixed timestep back to normal
            fixed_time.set_timestep_hz(
                sim_config.ticks_per_second as f64
                    * sim_config.time_dilation as f64,
            );
            slow_motion.0 = None;
        }
    }
}
