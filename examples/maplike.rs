#![feature(associated_type_defaults, mem_copy_fn)]
#![allow(unused_imports)]

use bevy::{
    ecs::{
        entity::EntityHashMap,
        query::{QueryData, ROQueryItem, WorldQuery},
    },
    prelude::*,
};

pub trait MapLike<'a, V: QueryData> {
    fn mget(&'a self, entity: Entity) -> Option<ROQueryItem<'a, V>>;
    fn miter(&'a self) -> impl Iterator<Item = ROQueryItem<'a, V>>;
}

pub trait MapLikeMut<'a, V: QueryData> {
    fn mget_mut(&'a mut self, entity: Entity) -> Option<V::Item<'a>>;
    fn miter_mut(&'a mut self) -> impl Iterator<Item = V::Item<'a>>;
}

impl<'a, V: QueryData> MapLike<'a, V> for EntityHashMap<ROQueryItem<'a, V>>
where
    ROQueryItem<'a, V>: Copy,
{
    fn mget(&'a self, entity: Entity) -> Option<ROQueryItem<'a, V>> {
        self.get(&entity).copied()
    }

    fn miter(&'a self) -> impl Iterator<Item = ROQueryItem<'a, V>> {
        self.values().copied()
    }
}

impl<'w, 's, V: QueryData> MapLike<'w, V> for Query<'w, 's, V, ()> {
    fn miter(&'w self) -> impl Iterator<Item = ROQueryItem<'w, V>> {
        self.iter()
    }

    fn mget(&'w self, entity: Entity) -> Option<ROQueryItem<'w, V>> {
        self.get(entity).ok()
    }
}

impl<'w, 's, V: QueryData> MapLikeMut<'w, V> for Query<'w, 's, V, ()> {
    fn mget_mut(&'w mut self, entity: Entity) -> Option<<V>::Item<'w>> {
        self.get_mut(entity).ok()
    }

    fn miter_mut(&'w mut self) -> impl Iterator<Item = <V>::Item<'w>> {
        self.iter_mut()
    }
}

#[derive(Component)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

#[derive(Component)]
pub struct Health(pub f32);

fn hi<'map, 'item, T: MapLike<'map, (&'item Pos, &'item Health)>>(
    map: &'map T,
    e: Entity,
) {
    let _p: (&Pos, &Health) = map.mget(e).unwrap();
}

pub fn bye<'a>(query: Query<(&'a Pos, &'a Health)>) {
    let e = Entity::from_raw(1);
    hi(&query, e);

    let p = Pos { x: 1., y: 2. };
    let health = Health(2.);

    let map = EntityHashMap::from_iter([(e, (&p, &health))]);
    hi(&map, e);
}

fn main() {}
