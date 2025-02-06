use bevy::color::palettes::css;

use crate::{
    physics::{PhysicsBundle, PhysicsState, SimulationConfig},
    prelude::*,
    Selected,
};

pub struct UnguidedMissilePlugin;

impl Plugin for UnguidedMissilePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<UnguidedMissile>()
            .register_type::<MissileProjectile>()
            .add_event::<FireUnguidedMissile>()
            .add_systems(Update, (debug_keyboard_input, fire, apply_missile_thrust));
    }
}

#[derive(Component, Reflect, Debug, Default)]
pub struct UnguidedMissile {
    /// Tick when this launcher will be able to fire again
    pub ready_tick: u64,
}

#[derive(Event)]
pub struct FireUnguidedMissile(pub Entity);

#[derive(Component, Reflect, Debug)]
struct MissileProjectile {
    /// Constant thrust force applied each tick
    thrust: f32,
    /// How long the missile will live (in ticks)
    lifetime: u64,
    /// Tick when missile was spawned
    spawn_tick: u64,
}

impl MissileProjectile {
    pub fn bundle(tick: u64, shooter: &PhysicsState) -> impl Bundle {
        let thrust = 50.0; // Units per tick of constant thrust
        let lifetime = 180; // 3 seconds at 60 ticks per second
        
        (
            MissileProjectile {
                thrust,
                lifetime,
                spawn_tick: tick,
            },
            PhysicsBundle::new_basic(
                tick,
                shooter.pos + 20. * shooter.dir(), // Spawn in front of shooter
                shooter.vel + 50. * shooter.dir(), // Initial velocity boost
                shooter.rotation,
                0.,           // No rotation
                10.0,        // Lower mass than PlasmaCannon
                Vec2::new(2.0, 0.5), // Elongated hitbox
            ),
            Sprite::from_color(css::ORANGE_RED, Vec2::new(4.0, 1.0)), // Elongated sprite
        )
    }
}

fn fire(
    mut commands: Commands,
    sim_config: Res<SimulationConfig>,
    mut launchers: Query<(&mut UnguidedMissile, &PhysicsState)>,
    mut fire_events: EventReader<FireUnguidedMissile>,
) {
    for FireUnguidedMissile(shooter) in fire_events.read() {
        let Ok((mut launcher, phys)) = launchers.get_mut(*shooter) else {
            warn!("FireUnguidedMissile event with invalid entity target");
            continue;
        };
        if launcher.ready_tick <= sim_config.current_tick {
            info!(shooter = shooter.index(), "Firing UnguidedMissile");
            commands.spawn(MissileProjectile::bundle(sim_config.current_tick, phys));
            // 3 second cooldown
            launcher.ready_tick = 
                sim_config.current_tick + sim_config.ticks_per_second * 3;
        }
    }
}

fn apply_missile_thrust(
    mut commands: Commands,
    sim_config: Res<SimulationConfig>,
    mut missiles: Query<(Entity, &MissileProjectile, &mut PhysicsState)>,
) {
    for (entity, missile, mut physics) in missiles.iter_mut() {
        // Check if missile should be destroyed
        if sim_config.current_tick >= missile.spawn_tick + missile.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Apply constant thrust in missile's forward direction
        let thrust_force = missile.thrust * physics.dir();
        physics.vel += thrust_force;
    }
}

fn debug_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut fire_events: EventWriter<FireUnguidedMissile>,
    selected: ResMut<Selected>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        fire_events.send(FireUnguidedMissile(selected.0));
    }
}