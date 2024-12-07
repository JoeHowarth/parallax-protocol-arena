use std::ops::RangeInclusive;

use bevy::color::palettes::css;
use physics::{PhysicsState, Timeline};
use rtree_rs::RTree;
use utils::intersect_ray_aabb;

use crate::prelude::*;

#[derive(Component, Debug, Clone, Deref, Copy)]
pub struct Collider(pub BRect);

impl Collider {
    pub fn from_dim(dim: Vec2) -> Self {
        Self(BRect::from_corners(-dim / 2., dim / 2.))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Collision {
    pub tick: u64,
    pub this: Entity,
    pub this_result: EntityCollisionResult,
    pub other: Entity,
    pub other_result: EntityCollisionResult,
}

#[derive(Clone, PartialEq, Debug)]
pub enum EntityCollisionResult {
    Destroyed,
    Survives { post_pos: Vec2, post_vel: Vec2 },
}

impl EntityCollisionResult {
    pub fn pos_equiv(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Survives { post_pos, .. },
                Self::Survives {
                    post_pos: other_pos,
                    ..
                },
            ) => post_pos == other_pos,
            (a, b) => a == b,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct SpatialItem {
    pub entity: Entity,
    pub pos: Vec2,
    pub vel: Vec2,
    pub mass: f32,
}

impl SpatialItem {
    pub fn from_state(entity: Entity, state: &PhysicsState) -> SpatialItem {
        SpatialItem {
            entity,
            pos: state.pos,
            vel: state.vel,
            mass: state.mass,
        }
    }
}

#[derive(Resource, Default)]
// pub struct SpatialIndex(pub EntityHashMap<BTreeMap<u64, BoundingBox>>);
pub struct SpatialIndex(pub BTreeMap<u64, SpatialIndexPerTick>);

pub struct SpatialIndexPerTick {
    e_map: EntityHashMap<(RRect, SpatialItem)>,
    rtree: RTree<2, f32, Entity>,
}

impl Default for SpatialIndexPerTick {
    fn default() -> Self {
        Self {
            e_map: default(),
            rtree: RTree::new(),
        }
    }
}

impl SpatialIndexPerTick {
    fn remove(&mut self, entity: &Entity) {
        let Some((rect, item)) = self.e_map.remove(entity) else {
            return;
        };
        self.rtree.remove(rect, entity);
    }

    pub fn collides(
        &self,
        entity: Entity,
        pos: Vec2,
        collider: &Collider,
    ) -> Option<(RRect, SpatialItem)> {
        // info!("Checking collisions...");
        let rect = collider.transalate(pos).to_rtree();
        self.rtree
            .search(rect)
            .filter(|e| e.data != &entity)
            .next()
            .and_then(|e| self.e_map.get(e.data).cloned())
    }

    pub fn insert(&mut self, collider: &Collider, item: SpatialItem) {
        self.remove(&item.entity);

        let rect = collider.0.transalate(item.pos).to_rtree();
        self.rtree.insert(rect, item.entity);
        self.e_map.insert(item.entity, (rect, item));
    }
}

impl SpatialIndex {
    pub fn collides(
        &self,
        entity: Entity,
        tick: u64,
        pos: Vec2,
        collider: &Collider,
    ) -> Option<(RRect, SpatialItem)> {
        self.0
            .get(&tick)
            .and_then(|index| index.collides(entity, pos, collider))
    }

    pub fn insert(
        &mut self,
        tick: u64,
        collider: &Collider,
        item: SpatialItem,
    ) {
        let index = self.0.entry(tick).or_insert_with(default);
        index.insert(collider, item);
    }

    pub fn remove(&mut self, tick: u64, entity: &Entity) {
        let Some(index) = self.0.get_mut(&tick) else {
            return;
        };
        index.remove(entity);
        if index.e_map.is_empty() {
            self.0.remove(&tick);
        }
    }

    pub fn patch(
        &mut self,
        e: Entity,
        timeline: &Timeline,
        collider: &Collider,
        updated: RangeInclusive<u64>,
        removed: Option<RangeInclusive<u64>>,
    ) {
        if let Some(removed) = removed {
            for tick in removed {
                self.remove(tick, &e);
            }
        }

        for tick in updated {
            // eprintln!("Inserting into spatial index {tick}");
            let state = timeline.future_states.get(&tick).unwrap();
            self.insert(
                tick,
                collider,
                SpatialItem {
                    entity: e,
                    pos: state.pos,
                    vel: state.vel,
                    mass: state.mass,
                },
            );
        }
    }
}

impl std::fmt::Debug for SpatialIndexPerTick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")?;
        for item in self.rtree.iter() {
            write!(
                f,
                "(Rect: [{}, {}], [{}, {}], e: {}), ",
                item.rect.min[0],
                item.rect.min[1],
                item.rect.max[0],
                item.rect.max[1],
                item.data.index()
            )?;
        }
        writeln!(f, "}}")
    }
}

/// Calculate specific impact energy Q (J/kg) for a collision between two masses
/// given velocity in m/s
///
/// # Returns
/// * Tuple of specific impact energies (J/kg)
pub fn calculate_impact_energy(
    m1: f32,
    m2: f32,
    rel_velocity: Vec2,
) -> (f32, f32) {
    // Calculate v² in (m/s)²
    let v_squared = rel_velocity.length_squared();

    // Calculate mass ratio μ = m2/m1
    let mu = m2 / m1;

    // Q = ½μv²
    let q1 = 0.5 * mu * v_squared;
    let q2 = 0.5 * (1.0 / mu) * v_squared;
    (q1, q2)
}

#[derive(Debug, PartialEq)]
pub enum CollisionOutcome {
    SurfaceEffects,
    Cratering,
    Fracturing,
    MajorRestructuring,
    Disruption,
}

impl CollisionOutcome {
    pub fn is_destoyed(q: f32) -> bool {
        if q < 100. {
            false
        } else {
            true
        }
    }

    pub fn from_q(q: f32) -> Self {
        match q {
            q if q < 10.0 => CollisionOutcome::SurfaceEffects,
            q if q < 100.0 => CollisionOutcome::Cratering,
            q if q < 1000.0 => CollisionOutcome::Fracturing,
            q if q < 10000.0 => CollisionOutcome::MajorRestructuring,
            _ => CollisionOutcome::Disruption,
        }
    }
}

pub fn calculate_inelastic_collision(
    mass_a: f32,
    vel_a: Vec2,
    mass_b: f32,
    vel_b: Vec2,
) -> Vec2 {
    // Calculate momentum conservation: p1 + p2 = (m1 + m2)v_final
    let total_momentum = (vel_a * mass_a) + (vel_b * mass_b);

    // Final velocity = total momentum / total mass
    total_momentum / (mass_a + mass_b)
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;

    #[test]
    fn test_spatial_index() {
        let collider = Collider::from_dim(Vec2::splat(2.));
        let mut spatial_index = SpatialIndexPerTick::default();
        let e0 = Entity::from_raw(0);
        let e1 = Entity::from_raw(1);
        let pos = Vec2::new(30., 0.);
        spatial_index.insert(
            &collider,
            SpatialItem {
                entity: e0,
                pos,
                vel: Vec2::new(0., 0.),
                mass: 1.,
            },
        );
        spatial_index.insert(
            &collider,
            SpatialItem {
                entity: e1,
                pos,
                vel: Vec2::new(0., 0.),
                mass: 1.,
            },
        );

        let res = spatial_index.collides(e0, pos, &collider);
        assert!(res.is_some());
        let (rect, item) = res.unwrap();
        assert_eq!(
            rect.to_bevy(),
            BRect {
                min: Vec2::new(29.0, -1.0,),
                max: Vec2::new(31.0, 1.0,),
            }
        );
        assert_eq!(item.entity, e1);
    }

    #[test]
    fn test_slow_equal_mass() {
        let v = Vec2::new(50.0, 0.0); // 50 m/s
        let (q1, q2) = calculate_impact_energy(1000.0, 1000.0, v);
        assert!((q1 - 1250.0).abs() < 0.1);
        assert!((q2 - 1250.0).abs() < 0.1);
    }

    #[test]
    fn test_diagonal_velocity() {
        let v = Vec2::new(30.0, 40.0); // 50 m/s magnitude
        let (q1, q2) = calculate_impact_energy(1000.0, 1000.0, v);
        assert!((q1 - 1250.0).abs() < 0.1);
        assert!((q2 - 1250.0).abs() < 0.1);
    }
    // fn create_box(
    //     entity: u32,
    //     pos: (f32, f32),
    //     dim: (f32, f32),
    // ) -> BoundingBox {
    //     BoundingBox::from(
    //         Entity::from_raw(entity),
    //         Vec2::new(pos.0, pos.1),
    //         Vec2::new(dim.0, dim.1),
    //     )
    // }

    // #[test]
    // fn test_same_min_ints() {
    // let e0 = Entity::from_raw(0);
    // let e1 = Entity::from_raw(1);
    // let mut index = SpatialIndex::default();
    // assert_eq!(
    // index.insert(
    // e0,
    // 5,
    // Vec2::new(4., 3.0),
    // Vec2::new(1.0, 1.0),
    // Vec2::new(-1., 4.).normalize(),
    // ),
    // None
    // );
    //
    // let res = index.insert(
    // e1,
    // 5,
    // Vec2::new(4.0, 3.0),
    // Vec2::new(1., 1.),
    // Vec2::new(1., 0.).normalize(),
    // );
    // dbg!(&res);
    // assert_eq!(
    // res,
    // Some(Collision {
    // this: e1,
    // other: e0,
    // new_pos: Vec2::new(3., 3.0)
    // })
    // );
    // }
    //
    // #[test]
    // fn test_complete_overlap() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (0.0, 0.0), (2.0, 2.0));
    // assert!(box1.collides(&box2));
    // }
    //
    // #[test]
    // fn test_partial_overlap() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (1.0, 1.0), (2.0, 2.0));
    // assert!(box1.collides(&box2));
    // }
    //
    // #[test]
    // fn test_edge_touching() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (2.0, 0.0), (2.0, 2.0));
    // assert!(
    // box1.collides(&box2),
    // "Boxes touching on edge should collide"
    // );
    // }
    //
    // #[test]
    // fn test_corner_touching() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (2.0, 2.0), (2.0, 2.0));
    // assert!(
    // box1.collides(&box2),
    // "Boxes touching at corner should collide"
    // );
    // }
    //
    // #[test]
    // fn test_no_collision() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (3.0, 3.0), (2.0, 2.0));
    // assert!(!box1.collides(&box2));
    // }
    //
    // #[test]
    // fn test_different_sizes() {
    // let box1 = create_box(1, (0.0, 0.0), (4.0, 4.0));
    // let box2 = create_box(2, (1.0, 1.0), (1.0, 1.0));
    // assert!(box1.collides(&box2), "Large box should contain small box");
    // }
    //
    // #[test]
    // fn test_self_collision() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // assert!(box1.collides(&box1), "Box should collide with itself");
    // }
    //
    // #[test]
    // fn test_vertical_separation() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (0.0, 3.0), (2.0, 2.0));
    // assert!(
    // !box1.collides(&box2),
    // "Vertically separated boxes should not collide"
    // );
    // }
    //
    // #[test]
    // fn test_horizontal_separation() {
    // let box1 = create_box(1, (0.0, 0.0), (2.0, 2.0));
    // let box2 = create_box(2, (3.0, 0.0), (2.0, 2.0));
    // assert!(
    // !box1.collides(&box2),
    // "Horizontally separated boxes should not collide"
    // );
    // }
}
