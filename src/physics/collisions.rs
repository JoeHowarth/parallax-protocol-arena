use rtree_rs::RTree;
use utils::intersect_ray_aabb;

use crate::prelude::*;

#[derive(Resource, Reflect, Debug)]
pub struct BoundingBox {
    entity: Entity,
    aabb: Rect,
    pos: Vec2,
    dim: Vec2,
}

// impl BoundingBox {
//     pub fn from(entity: Entity, pos: Vec2, dim: Vec2) -> Self {
//         Self {
//             entity,
//             pos,
//             dim,
//             aabb: Rect::from_corners(pos - dim * 0.5, pos + dim * 0.5),
//         }
//     }
//
//     pub fn collides(&self, other: &Self) -> bool {
//         let m = self.aabb;
//         let n = other.aabb;
//         dbg!(&n, &m);
//         dbg!(m.min.x <= n.max.x)
//             && dbg!(m.max.x >= n.min.x)
//             && dbg!(m.min.y <= n.max.y)
//             && dbg!(m.max.y >= n.min.y)
//     }
//
//     pub fn min_ints(&self) -> (i32, i32) {
//         let aabb = self.aabb;
//         (aabb.min.x as i32, aabb.min.y as i32)
//     }
// }

#[derive(Clone, PartialEq)]
pub struct Collision {
    pub this: Entity,
    pub other: Entity,
    pub other_bbox: RRect,
}

#[derive(Resource, Default)]
// pub struct SpatialIndex(pub EntityHashMap<BTreeMap<u64, BoundingBox>>);
pub struct SpatialIndex(pub BTreeMap<u64, RTree<2, f32, Entity>>);

impl SpatialIndex {
    pub fn collides(&self, tick: u64, rect: Rect) -> Option<(RRect, Entity)> {
        self.0.get(&tick).and_then(|index| {
            index
                .search(rect.to_rtree())
                .next()
                .map(|item| (item.rect, *item.data))
        })
    }

    pub fn insert(&mut self, tick: u64, rect: Rect, entity: Entity) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;

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
