pub use std::f32::consts::PI;

pub use ::bevy::prelude::*;
pub use ::std::{
    str::FromStr,
    sync::Mutex,
    time::{Duration, Instant},
};
pub use bevy_vector_shapes::prelude::*;
pub use strum::{EnumIter, EnumString, IntoEnumIterator};

pub use crate::{crafts::*, *};

/////// SMALL UTILS //////////

pub trait Vec3Ext {
    fn new2(x: impl Into<f32>, y: impl Into<f32>) -> Vec3;
    fn from2(vec2: impl Into<Vec2>) -> Vec3;
}

impl Vec3Ext for Vec3 {
    fn new2(x: impl Into<f32>, y: impl Into<f32>) -> Vec3 {
        Vec3::new(x.into(), y.into(), 0.)
    }

    fn from2(vec2: impl Into<Vec2>) -> Vec3 {
        Vec3::from((vec2.into(), 0.))
    }
}
