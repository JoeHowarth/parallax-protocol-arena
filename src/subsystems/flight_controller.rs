use crate::prelude::*;

#[derive(Component, Reflect, Debug)]
pub struct FlightController;

pub struct FlightControllerPlugin;

impl Plugin for FlightControllerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FlightController>();
    }
}
