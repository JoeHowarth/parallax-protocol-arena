use crate::{prelude::*, Health};

pub struct FrigatePlugin;

impl Plugin for FrigatePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Frigate>();
    }
}

#[derive(Component, Reflect)]
pub struct Frigate;

impl Frigate {
    // pub fn spawn(x: f32, y: f32, faction: Faction) -> impl Command {
    //     todo!()
    // }
}
