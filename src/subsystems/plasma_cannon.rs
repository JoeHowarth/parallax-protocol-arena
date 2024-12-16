use bevy::color::palettes::css;
use physics::{PhysicsBundle, PhysicsState, SimulationConfig};

use crate::prelude::*;

pub struct PlasmaCannonPlugin;

impl Plugin for PlasmaCannonPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PlasmaCannon>();
        app.add_event::<FirePlasmaCannon>();
        app.add_systems(Update, debug_keyboard_input);
        app.add_systems(Update, fire);
    }
}

#[derive(Component, Reflect, Debug, Default)]
pub struct PlasmaCannon {
    /// Tick when this cannon will be able to fire again
    pub ready_tick: u64,
}

#[derive(Event)]
pub struct FirePlasmaCannon(pub Entity);

#[derive(Component, Reflect, Debug)]
struct PlasmaBurst;

impl PlasmaBurst {
    pub fn bundle(shooter: &PhysicsState) -> impl Bundle {
        (
            PlasmaBurst,
            PhysicsBundle::new_basic(
                shooter.pos + 20. * shooter.dir(),
                // add an impulse in the forwards direction to account for
                // firing the burst
                shooter.vel + 100. * shooter.dir(),
                shooter.rotation,
                0.,
                // high mass makes the collision system always destroy other
                // object
                1000.,
                Vec2::splat(1.),
            ),
            Sprite::from_color(css::AQUA, Vec2::splat(1.)),
        )
    }
}

fn fire(
    mut commands: Commands,
    sim_config: Res<SimulationConfig>, // TODO: replace with 'tick' resource
    mut cannons: Query<(&mut PlasmaCannon, &PhysicsState)>,
    mut fire_events: EventReader<FirePlasmaCannon>,
) {
    for FirePlasmaCannon(shooter) in fire_events.read() {
        let Ok((mut cannon, phys)) = cannons.get_mut(*shooter) else {
            warn!("FirePlasmaCannon event with invalid entity target");
            continue;
        };
        if cannon.ready_tick <= sim_config.current_tick {
            info!(shooter = shooter.index(), "Firing PlasmaCannon");
            commands.spawn(PlasmaBurst::bundle(phys));
            // add 5 second cooldown for firing
            cannon.ready_tick =
                sim_config.current_tick + sim_config.ticks_per_second * 2;
        }
    }
}

fn debug_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut fire_events: EventWriter<FirePlasmaCannon>,
    selected: ResMut<Selected>,
) {
    if keys.just_pressed(KeyCode::KeyF) {
        fire_events.send(FirePlasmaCannon(selected.0));
    }
}
