use std::marker::PhantomData;

use crate::{physics::collisions::Collider, prelude::*};

pub mod event_markers;
pub mod input_handler;
pub mod trajectory;

pub use event_markers::EventMarkerPlugin;
pub use input_handler::InputHandlerPlugin;
pub use trajectory::TrajectoryPlugin;

#[derive(Default, Clone)]
pub struct ClientPlugin {
    pub event_marker: EventMarkerPlugin,
    pub input_handler: InputHandlerPlugin,
    pub trajectory: TrajectoryPlugin,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            self.event_marker,
            self.input_handler,
            self.trajectory,
        ))
        .insert_resource(ScreenLenToWorld(1.))
        .add_systems(PreUpdate, calc_screen_length_to_world);
    }
}

#[derive(Resource, Deref)]
pub struct ScreenLenToWorld(pub f32);

fn calc_screen_length_to_world(
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut screen_length_to_world: ResMut<ScreenLenToWorld>,
) {
    let _ =
        (|| -> Result<(), bevy::render::camera::ViewportConversionError> {
            let (camera, camera_transform) = camera_q.single();
            let world_diff = camera
                .viewport_to_world_2d(camera_transform, Vec2::new(1., 0.))?
                - camera.viewport_to_world_2d(
                    camera_transform,
                    Vec2::new(0., 0.),
                )?;
            screen_length_to_world.0 = world_diff.x;
            Ok(())
        })();
}

pub type EntityTimeline<T> = GenericSparseTimeline<Entity, T>;

#[derive(Component, Debug)]
pub struct GenericSparseTimeline<C, Marker = C> {
    pub map: BTreeMap<u64, C>,
    _d: PhantomData<Marker>,
}

impl<C, Marker> Default for GenericSparseTimeline<C, Marker> {
    fn default() -> Self {
        Self {
            map: default(),
            _d: PhantomData,
        }
    }
}

impl<C: Send + Sync + 'static, Marker: Send + Sync + 'static>
    GenericSparseTimeline<C, Marker>
{
    pub fn insert(&mut self, tick: u64, c: C) -> Option<C> {
        self.map.insert(tick, c)
    }

    pub fn get(&self, tick: u64) -> Option<&C> {
        self.map.get(&tick)
    }

    pub fn get_mut(&mut self, tick: u64) -> Option<&mut C> {
        self.map.get_mut(&tick)
    }

    pub fn contains(&self, tick: u64) -> bool {
        self.map.contains_key(&tick)
    }

    pub fn clear_system(
        mut query: Query<&mut GenericSparseTimeline<C, Marker>>,
        sim_config: Res<crate::physics::SimulationConfig>,
    ) {
        for mut timeline in query.iter_mut() {
            timeline.map.retain(|k, v| *k >= sim_config.current_tick);
        }
    }
}

#[derive(Default)]
pub struct GenericDenseTimeline<C> {
    pub base_tick: u64,
    pub ring: VecDeque<C>,
}

impl<C> GenericDenseTimeline<C> {
    pub fn push(&mut self, c: C) {
        self.ring.push_back(c);
    }

    pub fn get(&self, tick: u64) -> Option<&C> {
        self.ring.get((tick - self.base_tick) as usize)
    }

    pub fn get_mut(&mut self, tick: u64) -> Option<&mut C> {
        self.ring
            .get_mut((tick.checked_sub(self.base_tick)?) as usize)
    }

    /// Sets the value at tick to val if within domain returning
    /// existing val, otherwise returns None
    pub fn set(&mut self, mut val: C, tick: u64) -> Option<C> {
        let idx = (tick.checked_sub(self.base_tick)?) as usize;
        if idx < self.ring.len() {
            let old = self.ring.get_mut(idx)?;
            std::mem::swap(old, &mut val);
            return Some(val);
        } else {
            None
        }
    }
}
