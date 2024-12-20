use parallax_protocol_arena::prelude::*;

fn main() {
    App::new()
        .add_plugins(Shape2dPlugin::default())
        .add_systems(Update, draw_event_marker);
}

fn draw_event_marker(mut painter: ShapePainter) {}
