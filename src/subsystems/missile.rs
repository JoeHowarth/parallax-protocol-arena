use utils::intersect_ray_aabb;

use crate::{circle_bundle, prelude::*};

#[derive(Component, Reflect, Debug)]
pub struct MissileBay {
    pub last_fired: f64,
    pub reload_time: f64,
}

#[derive(Event, Clone, Copy)]
pub struct FireMissile {
    pub from: Entity,
    pub target: Entity,
}

#[derive(Component, Reflect)]
pub struct Missile {
    pub target: Entity,
}

pub struct MissilePlugin;

impl Plugin for MissilePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<FireMissile>()
            .add_systems(
                FixedUpdate,
                (
                    handle_fire_missile,
                    update_missiles,
                    handle_missile_collision,
                ),
            )
            .add_lua_provider(MissilePlugin);
    }
}

impl LuaProvider for MissilePlugin {
    fn attach_lua_api(&mut self, lua: &mut Lua) -> mlua::Result<()> {
        let table = lua.create_table()?;
        table.set(
            "can_fire",
            lua.create_function(|lua, _: Value| {
                let world = lua.get_world()?;
                let world = world.read();

                let from =
                    lua.globals().get::<_, LuaEntity>("entity")?.inner()?;
                Ok(can_fire_world(&world, from))
            })?,
        )?;
        table.set(
            "fire",
            lua.create_function(
                |lua, (this, target): (LuaTable, LuaEntity)| {
                    // retrieve the world pointer
                    let world = lua.get_world()?;
                    let mut world = world.write();

                    let from =
                        lua.globals().get::<_, LuaEntity>("entity")?.inner()?;

                    // check if we can fire
                    if !can_fire_world(&world, from) {
                        return Ok(false);
                    }

                    let mut events: Mut<Events<FireMissile>> =
                        world.get_resource_mut().unwrap();
                    events.send(FireMissile {
                        from,
                        target: target.inner()?,
                    });

                    Ok(true)
                },
            )?,
        )?;

        lua.globals().set("missiles", table)
    }

    fn setup_lua_script(
        &mut self,
        sd: &ScriptData,
        ctx: &mut Lua,
    ) -> mlua::Result<()> {
        Ok(())
    }
}

impl MissileBay {
    pub fn can_fire(&self, now: &Time<Virtual>) -> bool {
        self.last_fired + self.reload_time < now.elapsed_seconds_f64()
    }
}

pub fn can_fire_world(world: &World, from: Entity) -> bool {
    let now = world.resource::<Time<Virtual>>();
    let last_fired = world.entity(from).get::<MissileBay>();

    last_fired.map(|bay| bay.can_fire(now)).unwrap_or(false)
}

fn handle_missile_collision(
    mut commands: Commands,
    missiles: Query<(Entity, &CollidingEntities, &Missile)>,
    mut health: Query<&mut Health, Without<Missile>>,
) {
    for (e, colliding_entities, missile) in missiles.iter() {
        if colliding_entities.0.contains(&missile.target) {
            info!("Collision");
            commands.entity(e).despawn();
            health.get_mut(missile.target).unwrap().0 -= 10.;
        }
    }
}

fn update_missiles(
    mut commands: Commands,
    missiles: Query<(Entity, &Missile)>,
    mut p: ParamSet<(
        Query<&Transform>,
        Query<&mut LinearVelocity, With<Missile>>,
    )>,
    mut painter: ShapePainter,
) {
    // Apply a scaled impulse
    // Adjust this value as needed
    let impulse_strength = 1.1;

    for (e, missile) in missiles.iter() {
        let missile_trans = p.p0().get(e).unwrap().translation;
        let target_trans = {
            let p0 = p.p0();
            let Ok(target_trans) = p0.get(missile.target) else {
                // if target is not there anymore, despawn missile
                commands.entity(e).despawn();
                continue;
            };
            target_trans.translation
        };

        painter.set_translation(missile_trans);

        let dir = (target_trans - missile_trans).normalize();
        let mut p1 = p.p1();
        let mut v = p1.get_mut(e).unwrap();
        let v3 = Vec3::from2(v.0);

        painter.set_color(bevy::color::palettes::basic::AQUA);
        painter.line(Vec3::ZERO, dir * 30.);
        painter.set_color(bevy::color::palettes::basic::LIME);
        painter.line(Vec3::ZERO, v3 * 0.1);

        // First, ensure v3 is not zero
        if v3.length_squared() < f32::EPSILON {
            v.0 += dir.xy();
            info!("v3 < epsilon");
            continue;
        }

        let v_dir = v3.dot(dir);
        let v_not_dir = v3.length() - v_dir;
        let dx = if v_dir < 0. {
            dir * impulse_strength
        } else if v_not_dir > impulse_strength {
            let dx = (v3 - dir * v_dir) * -impulse_strength;

            painter.set_color(bevy::color::palettes::basic::FUCHSIA);
            painter.line(Vec3::ZERO, dx * 30.);
            // println!("dx: {dx}, dir: {dir}");
            painter.triangle(
                Vec2::new(1., 1.),
                Vec2::new(2., 2.),
                Vec2::new(3., 1.),
            );

            dx
        } else {
            let dx = dir * impulse_strength;

            painter.set_color(bevy::color::palettes::basic::PURPLE);
            painter.line(Vec3::ZERO, dx * 30.);
            // println!("dx: {dx}, dir: {dir}");

            dx
        };

        v.0 += dx.xy();
    }
}

fn handle_fire_missile(
    mut reader: EventReader<FireMissile>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut missile_bays: Query<(
        &mut MissileBay,
        Option<&Collider>,
        &LinearVelocity,
    )>,
    locs: Query<&Transform>,
    now: Res<Time<Virtual>>,
) {
    for FireMissile { from, target } in reader.read().cloned() {
        let Ok((mut missile_bay, collider, vel)) = missile_bays.get_mut(from)
        else {
            continue;
        };
        if !missile_bay.can_fire(&now) {
            continue;
        }

        let starting = locs.get(from).unwrap();
        let starting_pt = starting.translation.xy();
        let forward = starting.local_y().xy();

        let loc = match collider.and_then(|collider| {
            let aabb = collider.aabb(starting_pt, starting.rotation);
            intersect_ray_aabb(aabb.min, aabb.max, starting_pt, forward).ok()
        }) {
            Some(pt) => (pt - starting_pt) * 1.2 + starting_pt,
            None => starting_pt + forward * 15.,
        };

        // we will bump bc of collider, so do so in right direction
        let loc = commands.spawn((
            Missile { target },
            CraftKind::Missile,
            LinearVelocity(vel.0 + forward * 50.),
            circle_bundle(
                1.,
                32.,
                Color::srgb(0., 1., 1.),
                loc.xy(),
                &asset_server,
            ),
        ));

        missile_bay.last_fired = now.elapsed_seconds_f64();
    }
}
