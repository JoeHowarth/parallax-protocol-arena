use bevy::color::palettes;

use crate::prelude::*;

pub mod asteroid;
pub mod frigate;

pub struct CraftsPlugin;

impl Plugin for CraftsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Faction>().register_type::<CraftKind>();
    }
}

#[derive(
    Component, Reflect, Copy, Clone, Debug, strum::Display, EnumString, EnumIter,
)]
pub enum Faction {
    Unaligned,
    Unknown,
    Blue,
    Red,
}

impl Faction {
    pub fn sprite_color(&self) -> Color {
        use palettes::basic;
        Color::Srgba(match self {
            Faction::Unaligned => basic::WHITE,
            Faction::Unknown => basic::GRAY,
            Faction::Blue => basic::BLUE,
            Faction::Red => basic::RED,
        })
    }
}

#[derive(
    Component, Reflect, Copy, Clone, Debug, strum::Display, EnumString, EnumIter,
)]
pub enum CraftKind {
    Asteroid,
    Frigate,
    Missile,
}
