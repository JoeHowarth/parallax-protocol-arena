use bevy::prelude::*;

/// Component to track if an entity is within camera view
#[derive(Component, Reflect, Deref)]
pub struct NDC(pub bool);

impl Default for NDC {
    fn default() -> Self {
        Self(false)
    }
}

/// Checks if a world position is visible within the camera's view
fn is_position_visible(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    world_position: Vec2,
) -> bool {
    // Convert world position to NDC with depth
    let ndc = camera.world_to_ndc(camera_transform, world_position.extend(0.0));

    // Check if the position is within NDC bounds (-1 to 1 for both x and y)
    match ndc {
        Some(coords) => {
            coords.x >= -1.0
                && coords.x <= 1.0
                && coords.y >= -1.0
                && coords.y <= 1.0
        }
        None => false,
    }
}

fn update_visibility_system(
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut visible_entities: Query<(&GlobalTransform, &mut NDC)>,
) {
    // Early return if no camera is found
    let Ok((camera, camera_transform)) = camera_q.get_single() else {
        return;
    };

    // Update visibility for each entity with a Visible component
    for (transform, mut visible) in visible_entities.iter_mut() {
        let position = transform.translation().truncate();
        visible.0 = is_position_visible(camera, camera_transform, position);
    }
}

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<NDC>() // Enable reflection for debugging
            .add_systems(Update, update_visibility_system);
    }
}
